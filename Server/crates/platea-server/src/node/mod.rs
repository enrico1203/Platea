//! Modalità nodo: `platea node`.
//!
//! Lo stesso eseguibile del server, senza database né web UI. Il server lo chiama su `/node/v1` (porta 8097) per
//! affidargli transcodifiche (HLS con lo stesso `HlsRuntime` del server) e per leggere i file delle cartelle che il
//! nodo ha in copia (CDN). Tutta la configurazione arriva dal server; il nodo non ha stato proprio.
//! Sicurezza: chiave condivisa (`Authorization: Bearer`), reti ammesse facoltative, sola lettura sotto le radici
//! dei media. Va esposto solo su una rete fidata (Tailscale, LAN).

mod api;

use crate::nodes::proto::{NodeConfig, NodeStatus, RemoteSessionStatus, Slots, PROTOCOL};
use crate::transcode::command::Backend;
use crate::transcode::ffmpeg::FfmpegLocator;
use crate::transcode::{HlsRuntime, TranscodeManager};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

/// Senza heartbeat dal server per questo tempo il nodo chiude tutte le transcodifiche: niente FFmpeg orfani.
const ORPHAN_AFTER_MS: i64 = 60_000;
/// Una sessione mai più richiesta per questo tempo si chiude anche se il server la elenca ancora.
const SESSION_IDLE_MS: i64 = 15 * 60_000;

pub struct NodeSession {
    pub rt: Arc<HlsRuntime>,
    pub ahead: usize,
    pub keep: usize,
    pub created: i64,
    pub last_used: AtomicI64,
}

pub struct NodeState {
    pub name: String,
    pub key: String,
    pub allow: Vec<String>,
    /// Radici dei media (canoniche): il nodo legge solo sotto di esse.
    pub roots: Vec<PathBuf>,
    pub transcode_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub ffmpeg: FfmpegLocator,
    pub transcoder: Arc<TranscodeManager>,
    pub config: RwLock<(i64, NodeConfig)>,
    pub sessions: Mutex<HashMap<String, Arc<NodeSession>>>,
    pub last_heartbeat: AtomicI64,
    pub detecting: AtomicBool,
    pub started_at: i64,
}

pub type NodeShared = Arc<NodeState>;

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// Chiave del nodo: `PLATEA_NODE_KEY`, oppure generata al primo avvio in `<data>/node.key`.
fn load_key(data_dir: &Path) -> anyhow::Result<(String, bool)> {
    if let Some(k) = env("PLATEA_NODE_KEY") {
        if k.trim().len() < 16 {
            anyhow::bail!("PLATEA_NODE_KEY troppo corta: servono almeno 16 caratteri");
        }
        return Ok((k.trim().to_string(), false));
    }
    let path = data_dir.join("node.key");
    if let Ok(k) = std::fs::read_to_string(&path) {
        if k.trim().len() >= 16 {
            return Ok((k.trim().to_string(), false));
        }
    }
    std::fs::create_dir_all(data_dir)?;
    let k = crate::util::random_hex(24);
    std::fs::write(&path, &k)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok((k, true))
}

fn bind_addr() -> String {
    env("PLATEA_NODE_BIND").unwrap_or_else(|| "0.0.0.0:8097".into())
}

/// `platea node [key|--healthcheck]`.
pub fn main(args: &[String]) -> anyhow::Result<()> {
    let data_dir = PathBuf::from(env("PLATEA_DATA_DIR").unwrap_or_else(|| "./data".into()));
    match args.first().map(String::as_str) {
        Some("--healthcheck") => std::process::exit(crate::healthcheck(&bind_addr(), "/node/v1/health")),
        Some("key") => {
            let (k, _) = load_key(&data_dir)?;
            println!("{k}");
            return Ok(());
        }
        Some(other) => anyhow::bail!("comando del nodo sconosciuto: {other} (uso: platea node [key])"),
        None => {}
    }
    crate::init_tracing(&env("PLATEA_LOG").unwrap_or_else(|| "info".into()));
    crate::raise_open_files_limit();
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().max_blocking_threads(128).thread_name("platea-node").build()?;
    rt.block_on(run(data_dir))
}

async fn run(data_dir: PathBuf) -> anyhow::Result<()> {
    let (key, generated) = load_key(&data_dir)?;
    if generated {
        tracing::warn!("==========================================================");
        tracing::warn!("Chiave del nodo (da inserire nel server, Amministrazione → Nodi): {key}");
        tracing::warn!("==========================================================");
    } else {
        tracing::info!("chiave del nodo pronta (per vederla: platea node key)");
    }
    let transcode_dir = PathBuf::from(env("PLATEA_TRANSCODE_DIR").unwrap_or_else(|| "./transcode".into()));
    let cache_dir = PathBuf::from(env("PLATEA_CACHE_DIR").unwrap_or_else(|| "./cache".into()));
    std::fs::create_dir_all(&transcode_dir)?;
    std::fs::create_dir_all(&cache_dir)?;
    // Segmenti rimasti da un arresto brusco.
    if let Ok(rd) = std::fs::read_dir(&transcode_dir) {
        for e in rd.flatten() {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
    let roots: Vec<PathBuf> = env("PLATEA_NODE_MEDIA")
        .unwrap_or_else(|| "/media".into())
        .split([',', ':'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|r| match std::fs::canonicalize(r) {
            Ok(p) => Some(p),
            Err(e) => {
                tracing::warn!(root = r, error = %e, "radice dei media non accessibile: ignorata");
                None
            }
        })
        .collect();
    let allow: Vec<String> =
        env("PLATEA_NODE_ALLOW").map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default();
    for a in &allow {
        if crate::net::Cidr::parse(a).is_none() {
            anyhow::bail!("PLATEA_NODE_ALLOW: rete non valida: {a}");
        }
    }
    let name =
        env("PLATEA_NODE_NAME").unwrap_or_else(|| std::fs::read_to_string("/etc/hostname").map(|h| h.trim().to_string()).unwrap_or_else(|_| "nodo".into()));

    let ffmpeg = FfmpegLocator::new();
    {
        let f = ffmpeg.clone();
        tokio::task::spawn_blocking(move || f.resolve("", "")).await?;
    }
    let cfg = NodeConfig::default();
    let state: NodeShared = Arc::new(NodeState {
        name,
        key,
        allow,
        roots,
        transcode_dir,
        cache_dir,
        ffmpeg,
        transcoder: Arc::new(TranscodeManager::new(cfg.max_hw_transcodes, cfg.max_sw_transcodes)),
        config: RwLock::new((0, cfg)),
        sessions: Mutex::new(HashMap::new()),
        last_heartbeat: AtomicI64::new(0),
        detecting: AtomicBool::new(false),
        started_at: crate::util::now_ms(),
    });
    {
        let st = state.clone();
        tokio::spawn(async move { detect(&st).await });
    }
    tokio::spawn(maintenance(state.clone()));

    let addr: std::net::SocketAddr = bind_addr().parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, version = crate::state::VERSION, protocol = PROTOCOL, roots = ?state.roots, "nodo Platea in ascolto");
    let app = api::router(state.clone());
    axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    for (_, s) in state.sessions.lock().unwrap().drain() {
        let rt = s.rt.clone();
        tokio::spawn(async move { rt.stop().await });
    }
    Ok(())
}

/// Prova reale delle GPU con la configurazione corrente (all'avvio, al cambio di backend, su richiesta).
pub async fn detect(state: &NodeShared) {
    if state.detecting.swap(true, Ordering::SeqCst) {
        return;
    }
    let f = state.ffmpeg.get();
    if f.available {
        let cfg = state.config.read().unwrap().1.clone();
        let forced = crate::api::admin::forced_backend(&cfg.hw_backend);
        let mut caps = crate::transcode::hw::detect(&f.ffmpeg, &cfg.vaapi_device, forced, &state.cache_dir).await;
        caps.ffmpeg_version = f.version.clone();
        *state.transcoder.caps.write().unwrap() = caps;
        state.transcoder.reset_failures();
    }
    state.detecting.store(false, Ordering::SeqCst);
}

/// Applica la configurazione spinta dal server. Restituisce se serve una nuova prova delle GPU.
pub fn apply_config(state: &NodeShared, rev: i64, cfg: NodeConfig) -> bool {
    let mut cur = state.config.write().unwrap();
    let redetect = cfg.hw_backend != cur.1.hw_backend || cfg.vaapi_device != cur.1.vaapi_device;
    state.transcoder.apply_limits(cfg.max_hw_transcodes, cfg.max_sw_transcodes);
    *cur = (rev, cfg);
    redetect
}

async fn maintenance(state: NodeShared) {
    let mut tick = tokio::time::interval(Duration::from_secs(2));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut orphan_warned = false;
    loop {
        tick.tick().await;
        let now = crate::util::now_ms();
        let last_hb = state.last_heartbeat.load(Ordering::Relaxed);
        let all: Vec<(String, Arc<NodeSession>)> = state.sessions.lock().unwrap().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let orphaned = !all.is_empty() && last_hb > 0 && now - last_hb > ORPHAN_AFTER_MS;
        if orphaned && !orphan_warned {
            tracing::warn!(sessions = all.len(), "nessun heartbeat dal server da oltre 60 s: chiudo le transcodifiche");
        }
        orphan_warned = orphaned;
        for (id, s) in all {
            if orphaned || now - s.last_used.load(Ordering::Relaxed) > SESSION_IDLE_MS {
                stop_session(&state, &id).await;
                continue;
            }
            s.rt.maintain(s.ahead, s.keep).await;
        }
    }
}

pub async fn stop_session(state: &NodeShared, id: &str) -> bool {
    let s = state.sessions.lock().unwrap().remove(id);
    match s {
        Some(s) => {
            s.rt.stop().await;
            tracing::info!(session = id, "sessione chiusa");
            true
        }
        None => false,
    }
}

pub async fn status(state: &NodeShared, peer: Option<std::net::IpAddr>) -> NodeStatus {
    let f = state.ffmpeg.get();
    let caps = state.transcoder.caps.read().unwrap().clone();
    let (hw_used, hw_max, sw_used, sw_max) = state.transcoder.active_counts();
    let all: Vec<(String, Arc<NodeSession>)> = state.sessions.lock().unwrap().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let mut sessions = Vec::with_capacity(all.len());
    for (id, s) in all {
        sessions.push(RemoteSessionStatus { id, paused: s.rt.is_paused().await, buffered_until_ms: s.rt.buffered_until_ms().await, backend: s.rt.backend() });
    }
    let (rev, cfg) = state.config.read().unwrap().clone();
    let (mem_total, mem_avail) = crate::sysinfo::meminfo();
    NodeStatus {
        version: crate::state::VERSION.to_string(),
        protocol: PROTOCOL,
        name: state.name.clone(),
        ffmpeg_version: f.version.clone(),
        ffmpeg_available: f.available,
        ffmpeg_jellyfin: f.is_jellyfin,
        caps,
        current: Some(state.transcoder.current_profile()).filter(|p| p.backend != Backend::Software || f.available),
        encoders: state.transcoder.server_encoders(),
        slots: Slots { hw_used, hw_max, sw_used, sw_max },
        cpus: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
        load_1m: crate::sysinfo::load_1m(),
        memory_total: mem_total,
        memory_available: mem_avail,
        rss: crate::sysinfo::rss(),
        transcode_free_bytes: crate::sysinfo::free_bytes(&state.transcode_dir),
        sessions,
        config_rev: rev,
        config: Some(cfg),
        peer_ip: peer.map(|p| p.to_string()),
        media_roots: state.roots.iter().map(|r| r.to_string_lossy().to_string()).collect(),
        uptime_ms: crate::util::now_ms() - state.started_at,
    }
}

/// Percorso richiesto dal server: assoluto, senza `..`, e (dopo i collegamenti simbolici) sotto una radice dei media.
/// Restituisce il percorso canonico e la sua radice. Il mount può essere lento: tempo limite.
pub async fn check_path(state: &NodeShared, raw: &str) -> Result<(PathBuf, PathBuf), String> {
    let p = PathBuf::from(raw);
    if !safe_shape(&p) {
        return Err("Percorso non valido".into());
    }
    let roots = state.roots.clone();
    let res = tokio::time::timeout(Duration::from_secs(20), tokio::task::spawn_blocking(move || std::fs::canonicalize(&p)))
        .await
        .map_err(|_| "Lo storage non risponde".to_string())?
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    under_roots(&res, &roots).map(|root| (res.clone(), root)).ok_or_else(|| "Fuori dalle cartelle dei media del nodo".to_string())
}

pub fn safe_shape(p: &Path) -> bool {
    p.is_absolute() && !p.components().any(|c| matches!(c, std::path::Component::ParentDir))
}

pub fn under_roots(p: &Path, roots: &[PathBuf]) -> Option<PathBuf> {
    roots.iter().find(|r| p.starts_with(r)).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_under_roots() {
        let roots = vec![PathBuf::from("/media/film"), PathBuf::from("/srv/serie")];
        assert!(safe_shape(Path::new("/media/film/a.mkv")));
        assert!(!safe_shape(Path::new("/media/film/../../etc/passwd")));
        assert!(!safe_shape(Path::new("media/film/a.mkv")));
        assert_eq!(under_roots(Path::new("/media/film/A/a.mkv"), &roots), Some(PathBuf::from("/media/film")));
        assert_eq!(under_roots(Path::new("/media/filmati/a.mkv"), &roots), None);
        assert_eq!(under_roots(Path::new("/etc/passwd"), &roots), None);
    }

    #[test]
    fn key_is_generated_once() {
        let d = tempfile::tempdir().unwrap();
        let (k1, g1) = load_key(d.path()).unwrap();
        let (k2, g2) = load_key(d.path()).unwrap();
        assert!(g1 && !g2);
        assert_eq!(k1, k2);
        assert_eq!(k1.len(), 48);
    }
}
