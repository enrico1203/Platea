#![allow(clippy::type_complexity, clippy::too_many_arguments)]
//! Platea: open source media server.

/// mimalloc: allocazioni più veloci e memoria restituita al sistema dopo i picchi (scansioni grandi).
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod api;
mod audiobook;
mod auth;
mod backup;
mod boot;
mod cli;
mod db;
mod error;
mod events;
mod ids;
mod jobs;
mod livetv;
mod logs;
mod maintenance;
mod metadata;
mod metrics;
mod mkv;
mod mp4;
mod net;
mod node;
mod nodes;
mod prefs;
mod probe;
mod quickconnect;
mod ratings;
mod scanner;
mod sessions;
mod settings;
mod state;
mod stream;
mod strm;
mod sysinfo;
mod transcode;
mod trickplay;
mod util;
mod watch;
mod webhooks;

use crate::state::{AppState, Shared};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;
use tracing_subscriber::EnvFilter;

/// Controllo di salute per Docker: una richiesta HTTP minima, senza dipendenze nell'immagine.
pub(crate) fn healthcheck(bind: &str, path: &str) -> i32 {
    use std::io::{Read, Write};
    let port = bind.rsplit(':').next().unwrap_or("8096");
    let addr = format!("127.0.0.1:{port}");
    let Ok(addr) = addr.parse::<SocketAddr>() else { return 1 };
    let Ok(mut s) = std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_secs(3)) else { return 1 };
    let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(3)));
    if s.write_all(format!("GET {path} HTTP/1.0\r\nHost: localhost\r\n\r\n").as_bytes()).is_err() {
        return 1;
    }
    let mut buf = [0u8; 64];
    match s.read(&mut buf) {
        Ok(n) if n > 12 && &buf[9..12] == b"200" => 0,
        _ => 1,
    }
}

/// Log su stdout e nella dashboard.
pub(crate) fn init_tracing(level: &str) {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    let filter = EnvFilter::try_new(level).unwrap_or_else(|_| EnvFilter::new("info"));
    let fmt = tracing_subscriber::fmt::layer().with_target(false).with_ansi(std::io::IsTerminal::is_terminal(&std::io::stdout())).compact();
    tracing_subscriber::registry().with(filter).with(fmt).with(logs::DashboardLayer).init();
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    // Modalità nodo: `platea node` (transcodifica e copie delle cartelle per un server Platea).
    if args.get(1).map(String::as_str) == Some("node") {
        return node::main(&args[2..]);
    }
    let boot = boot::BootConfig::from_env();
    if std::env::args().any(|a| a == "--healthcheck") {
        // L'immagine è la stessa per server e nodo: il controllo di salute vale per entrambi.
        if std::env::var("PLATEA_NODE_BIND").is_ok() || std::env::var("PLATEA_ROLE").is_ok_and(|r| r == "node") {
            let bind = std::env::var("PLATEA_NODE_BIND").unwrap_or_else(|_| "0.0.0.0:8097".into());
            std::process::exit(healthcheck(&bind, "/node/v1/health"));
        }
        std::process::exit(healthcheck(&boot.bind, "/api/v1/health"));
    }
    if std::env::args().any(|a| a == "--version") {
        println!("platea {}", state::VERSION);
        return Ok(());
    }
    if let Some(code) = cli::dispatch(&boot) {
        std::process::exit(code);
    }
    init_tracing(&boot.log);

    raise_open_files_limit();
    // Pool di thread bloccanti ampio: ogni mount lento consuma i propri thread, non il runtime.
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().max_blocking_threads(256).thread_name("platea").build()?;
    rt.block_on(run(boot))
}

async fn run(boot: boot::BootConfig) -> anyhow::Result<()> {
    let t0 = std::time::Instant::now();
    // Cartelle o file creati da una versione che girava come root: messaggio chiaro invece di un errore generico.
    if let Err(e) = boot.ensure_dirs().and_then(|_| boot.check_writable()) {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            anyhow::bail!(
                "{e}: i dati non sono accessibili all'utente del server (uid {}). Succede aggiornando da una versione che girava come root. \
                 Sull'host, nella cartella di Platea: sudo chown -R 1000:1000 data cache transcode",
                unsafe { libc::getuid() }
            );
        }
        return Err(e.into());
    }
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
        quick_connect: Default::default(),
        jobs: jobs::JobQueue::new(),
        ffmpeg,
        sessions: Default::default(),
        live: Default::default(),
        nodes: Default::default(),
        metrics: Default::default(),
        server_id,
        setup_code: Mutex::new(None),
        image_downloads: Semaphore::new(4),
        image_resizes: Semaphore::new(std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).clamp(1, 4)),
        extractions: Semaphore::new(2),
        subtitle_extractions: Semaphore::new(2),
        subtitle_jobs: Default::default(),
        started_at: util::now_ms(),
        branding: api::branding::Branding::load(&boot.data_dir).await,
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
    state.nodes.reload(&state.db).await?;
    tokio::spawn(sessions::run_maintenance(state.clone()));
    tokio::spawn(livetv::stream::run_maintenance(state.clone()));
    tokio::spawn(nodes::run_health(state.clone()));
    tokio::spawn(metrics::run(state.clone()));
    tokio::spawn(watch::run(state.clone()));
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

/// Centinaia di riproduzioni e connessioni contemporanee: il limite di file aperti predefinito (spesso 1024)
/// non basta. Si alza il limite "soft" fino al massimo consentito ("hard") del container.
pub(crate) fn raise_open_files_limit() {
    #[cfg(unix)]
    unsafe {
        let mut lim = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) == 0 && lim.rlim_cur < lim.rlim_max {
            let want = lim.rlim_max.min(1_048_576);
            lim.rlim_cur = want;
            let _ = libc::setrlimit(libc::RLIMIT_NOFILE, &lim);
        }
    }
}
