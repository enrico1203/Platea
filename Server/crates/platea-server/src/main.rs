#![allow(clippy::type_complexity, clippy::too_many_arguments)]
//! Platea: open source media server.

/// mimalloc: allocazioni più veloci e memoria restituita al sistema dopo i picchi (scansioni grandi).
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod api;
mod auth;
mod boot;
mod db;
mod error;
mod events;
mod ids;
mod jobs;
mod logs;
mod metadata;
mod mkv;
mod net;
mod probe;
mod ratings;
mod scanner;
mod sessions;
mod settings;
mod state;
mod stream;
mod transcode;
mod util;

use crate::state::{AppState, Shared};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;
use tracing_subscriber::EnvFilter;

/// Controllo di salute per Docker: una richiesta HTTP minima, senza dipendenze nell'immagine.
fn healthcheck(bind: &str) -> i32 {
    use std::io::{Read, Write};
    let port = bind.rsplit(':').next().unwrap_or("8096");
    let addr = format!("127.0.0.1:{port}");
    let Ok(addr) = addr.parse::<SocketAddr>() else { return 1 };
    let Ok(mut s) = std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_secs(3)) else { return 1 };
    let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(3)));
    if s.write_all(b"GET /api/v1/health HTTP/1.0\r\nHost: localhost\r\n\r\n").is_err() {
        return 1;
    }
    let mut buf = [0u8; 64];
    match s.read(&mut buf) {
        Ok(n) if n > 12 && &buf[9..12] == b"200" => 0,
        _ => 1,
    }
}

fn main() -> anyhow::Result<()> {
    let boot = boot::BootConfig::from_env();
    if std::env::args().any(|a| a == "--healthcheck") {
        std::process::exit(healthcheck(&boot.bind));
    }
    if std::env::args().any(|a| a == "--version") {
        println!("platea {}", state::VERSION);
        return Ok(());
    }
    {
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;
        let filter = EnvFilter::try_new(&boot.log).unwrap_or_else(|_| EnvFilter::new("info"));
        let fmt = tracing_subscriber::fmt::layer().with_target(false).with_ansi(std::io::IsTerminal::is_terminal(&std::io::stdout())).compact();
        tracing_subscriber::registry().with(filter).with(fmt).with(logs::DashboardLayer).init();
    }

    // Pool di thread bloccanti ampio: ogni mount lento consuma i propri thread, non il runtime.
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().max_blocking_threads(256).thread_name("platea").build()?;
    rt.block_on(run(boot))
}

async fn run(boot: boot::BootConfig) -> anyhow::Result<()> {
    let t0 = std::time::Instant::now();
    boot.ensure_dirs()?;
    let secret = boot.load_secret()?;
    let db = db::Db::open(&boot).await?;
    let settings = settings::Settings::load(db.clone()).await?;
    let s = settings.get();

    let server_id = match db.state_get("server_id").await? {
        Some(id) => id,
        None => {
            let id = util::random_hex(16);
            db.state_set("server_id", &id).await?;
            id
        }
    };
    let http = reqwest::Client::builder()
        .user_agent(concat!("Platea/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(10))
        .pool_max_idle_per_host(8)
        .build()?;

    let ffmpeg = transcode::ffmpeg::FfmpegLocator::new();
    {
        let f = ffmpeg.clone();
        let (a, b) = (s.transcode.ffmpeg_path.clone(), s.transcode.ffprobe_path.clone());
        tokio::task::spawn_blocking(move || f.resolve(&a, &b)).await?;
    }

    let state: Shared = Arc::new(AppState {
        tmdb: metadata::tmdb::Tmdb::new(http.clone(), db.clone()),
        transcoder: Arc::new(transcode::TranscodeManager::new(s.transcode.max_hw_transcodes, s.transcode.max_sw_transcodes)),
        boot: boot.clone(),
        db,
        settings,
        events: events::EventBus::new(),
        ids: ids::IdCodec::new(&secret),
        http,
        token_cache: Default::default(),
        login_limiter: Default::default(),
        jobs: jobs::JobQueue::new(),
        ffmpeg,
        sessions: Default::default(),
        server_id,
        setup_code: Mutex::new(None),
        image_downloads: Semaphore::new(4),
        image_resizes: Semaphore::new(std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).clamp(1, 4)),
        extractions: Semaphore::new(2),
        started_at: util::now_ms(),
    });

    // Primo avvio: codice nei log, chiesto dalla procedura guidata.
    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&state.db.read).await?;
    if users == 0 {
        let code = util::setup_code();
        tracing::warn!("==========================================================");
        tracing::warn!("Primo avvio. Codice per la procedura guidata: {code}");
        tracing::warn!("==========================================================");
        *state.setup_code.lock().unwrap() = Some(code);
    }

    // Pulizia dei segmenti rimasti da un arresto brusco.
    if let Ok(mut rd) = tokio::fs::read_dir(&state.boot.transcode_dir).await {
        while let Ok(Some(e)) = rd.next_entry().await {
            let _ = tokio::fs::remove_dir_all(e.path()).await;
        }
    }

    jobs::recover(&state).await?;
    tokio::spawn(jobs::run_dispatcher(state.clone()));
    tokio::spawn(jobs::run_scheduler(state.clone()));
    tokio::spawn(sessions::run_maintenance(state.clone()));
    // Prova delle GPU in sottofondo: l'API è pronta subito.
    {
        let st = state.clone();
        tokio::spawn(async move { api::admin::run_hw_detection(&st).await });
    }

    let app = api::router(state.clone());
    let addr: SocketAddr = state.boot.bind.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, version = state::VERSION, ms = t0.elapsed().as_millis() as u64, "Platea in ascolto");
    // Lo strato compatibile Jellyfin non distingue maiuscole e minuscole: l'URI si normalizza prima dell'instradamento.
    let app = tower::Layer::layer(&tower::util::MapRequestLayer::new(api::jellyfin::normalize_uri::<axum::body::Body>), app);
    axum::serve(listener, axum::ServiceExt::<axum::extract::Request>::into_make_service_with_connect_info::<SocketAddr>(app))
        .with_graceful_shutdown(shutdown(state.clone()))
        .await?;
    Ok(())
}

async fn shutdown(state: Shared) {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
    tracing::info!("arresto: chiusura delle sessioni");
    for s in state.sessions.all() {
        sessions::stop_session(&state, &s.id).await;
    }
    let _ = sessions::flush_progress(&state).await;
}
