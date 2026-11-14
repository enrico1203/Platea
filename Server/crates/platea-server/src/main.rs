//! Platea: open source media server.

mod api;
mod auth;
mod boot;
mod db;
mod error;
mod events;
mod ids;
mod jobs;
mod metadata;
mod net;
mod probe;
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

fn main() -> anyhow::Result<()> {
    let boot = boot::BootConfig::from_env();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_new(&boot.log).unwrap_or_else(|_| EnvFilter::new("info")))
        .with_target(false)
        .compact()
        .init();

    // Pool di thread bloccanti ampio: ogni mount lento consuma i propri thread, non il runtime.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(256)
        .thread_name("platea")
        .build()?;
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
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
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
