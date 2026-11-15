//! Sessione HLS con esecutore: questo server (`HlsRuntime` locale) o un nodo. La playlist resta del server e i
//! confini dei segmenti sono fissi, quindi se l'esecutore cade a metà film un altro riparte dallo stesso segmento
//! e il player non se ne accorge. Eccezione: una transcodifica in fMP4 (HEVC/AV1 in uscita) ha un init proprio di
//! ogni encoder, quindi lì la sessione finisce (410) e il player riapre alla posizione corrente.

use super::client::{NodeCallError, NodeClient};
use super::proto::{CreateHls, Source};
use super::route::Job;
use crate::error::ApiError;
use crate::state::Shared;
use crate::transcode::command::{Backend, HwProfile, JobSpec};
use crate::transcode::{HlsError, HlsRuntime};
use axum::http::StatusCode;
use platea_playback::VideoAction;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

/// Limite della transcodifica senza GPU: in software un 4K non regge il tempo reale.
pub const SOFTWARE_MAX_WIDTH: u32 = 1920;
pub const SOFTWARE_MAX_HEIGHT: u32 = 1080;

/// Tutto ciò che serve per (ri)avviare la sessione su un esecutore qualsiasi.
#[derive(Clone)]
pub struct Plan {
    pub session_id: String,
    /// Specifica di FFmpeg: `input` è l'ingresso del server (file o .strm); `out_dir` lo decide l'esecutore.
    pub spec: JobSpec,
    pub job: Job,
    pub file_id: i64,
    pub file_size: i64,
    pub library_path_id: i64,
    /// Percorso relativo alla cartella della libreria.
    pub rel: String,
    pub cuts: Vec<i64>,
    pub duration_ms: i64,
    pub ahead: usize,
    pub keep: usize,
}

#[derive(Clone)]
enum Exec {
    Local(Arc<HlsRuntime>),
    Remote { node_id: i64, client: NodeClient, remote_id: String },
}

pub struct HlsHandle {
    pub session_id: String,
    pub cuts: Vec<i64>,
    pub duration_ms: i64,
    fmp4: bool,
    transcode: bool,
    plan: StdMutex<Plan>,
    exec: tokio::sync::Mutex<Exec>,
    exec_id: AtomicI64,
    exec_name: StdMutex<String>,
    remote: StdMutex<Option<(i64, String)>>,
    remote_backend: StdMutex<String>,
    gen: AtomicU32,
    tried: StdMutex<Vec<i64>>,
    stopped: AtomicBool,
}

/// Esito dell'avvio: la sessione, l'azione video effettiva (dimensioni ridotte se finisce in software) e chi la esegue.
pub struct Started {
    pub handle: Arc<HlsHandle>,
    pub video: VideoAction,
    pub backend: String,
    pub executor: String,
}

fn node_err(status: StatusCode, code: &'static str, msg: String) -> ApiError {
    ApiError::coded(status, code, msg)
}

/// Ordine degli esecutori da provare, con il server in fondo come ripiego se consentito.
fn executors(state: &Shared, plan: &Plan, exclude: &[i64]) -> Result<Vec<i64>, ApiError> {
    let local = state.nodes.local_id();
    if !state.nodes.has_any_remote() {
        return Ok(if exclude.contains(&local) { vec![] } else { vec![local] });
    }
    let mut order = super::transcode_order(state, &plan.job, plan.library_path_id, plan.file_id, exclude);
    let fallback = state.settings.get().nodes.fallback_local;
    if fallback && !order.contains(&local) && !exclude.contains(&local) {
        order.push(local);
    }
    if order.is_empty() && exclude.is_empty() {
        return Err(node_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "TranscodeUnavailable",
            "Nessun nodo disponibile per questa transcodifica (regole, nodi spenti o pieni) e il ripiego sul server è spento".into(),
        ));
    }
    Ok(order)
}

pub async fn start(state: &Shared, plan: Plan) -> Result<Started, ApiError> {
    let order = executors(state, &plan, &[])?;
    let mut plan = plan;
    let mut last_err = String::new();
    let mut tried = Vec::new();
    for id in order {
        match launch(state, &mut plan, id, 0, true).await {
            Ok((exec, backend)) => {
                let name = exec_name(state, &exec);
                let transcode = matches!(plan.spec.video_action, VideoAction::Transcode { .. });
                let handle = HlsHandle {
                    session_id: plan.session_id.clone(),
                    cuts: plan.cuts.clone(),
                    duration_ms: plan.duration_ms,
                    fmp4: plan.spec.fmp4,
                    transcode,
                    exec_id: AtomicI64::new(exec_node(state, &exec)),
                    exec_name: StdMutex::new(name.clone()),
                    remote: StdMutex::new(remote_of(&exec)),
                    remote_backend: StdMutex::new(backend.clone()),
                    exec: tokio::sync::Mutex::new(exec),
                    gen: AtomicU32::new(0),
                    tried: StdMutex::new(tried),
                    stopped: AtomicBool::new(false),
                    plan: StdMutex::new(plan.clone()),
                };
                if state.nodes.has_remote() {
                    tracing::info!(session = %plan.session_id, executor = %name, backend = %backend, "sessione HLS assegnata");
                }
                return Ok(Started { handle: Arc::new(handle), video: plan.spec.video_action.clone(), backend, executor: name });
            }
            Err(e) => {
                tracing::warn!(session = %plan.session_id, node = id, error = %e, "esecutore non disponibile: provo il successivo");
                if id != state.nodes.local_id() {
                    state.nodes.record_failure(id, &e);
                }
                tried.push(id);
                last_err = e;
            }
        }
    }
    Err(node_err(StatusCode::SERVICE_UNAVAILABLE, "TranscodeUnavailable", format!("Nessun esecutore ha accettato la transcodifica: {last_err}")))
}

fn exec_node(state: &Shared, e: &Exec) -> i64 {
    match e {
        Exec::Local(_) => state.nodes.local_id(),
        Exec::Remote { node_id, .. } => *node_id,
    }
}

fn exec_name(state: &Shared, e: &Exec) -> String {
    match e {
        Exec::Local(_) => String::new(),
        Exec::Remote { node_id, .. } => state.nodes.row(*node_id).map(|r| r.name).unwrap_or_else(|| format!("nodo {node_id}")),
    }
}

fn remote_of(e: &Exec) -> Option<(i64, String)> {
    match e {
        Exec::Local(_) => None,
        Exec::Remote { node_id, remote_id, .. } => Some((*node_id, remote_id.clone())),
    }
}

/// Profilo hardware per il codec d'uscita: quello del dispositivo se lo codifica, altrimenti software.
fn profile_for(spec: &JobSpec, current: Option<HwProfile>) -> HwProfile {
    match &spec.video_action {
        VideoAction::Transcode { codec, .. } => match current {
            Some(p) if p.encode.contains(codec) => p,
            _ => HwProfile::software(),
        },
        VideoAction::Copy => HwProfile::software(),
    }
}

/// Al primo avvio, se si finisce in software, l'uscita si limita a 1080p (ai riavvii no: la risoluzione non
/// deve cambiare a metà film).
fn cap_for_software(spec: &mut JobSpec, hw: &HwProfile) {
    if hw.backend == Backend::Software {
        if let VideoAction::Transcode { max_width, max_height, .. } = &mut spec.video_action {
            *max_width = (*max_width).min(SOFTWARE_MAX_WIDTH);
            *max_height = (*max_height).min(SOFTWARE_MAX_HEIGHT);
        }
    }
}

/// Avvia la sessione sull'esecutore `node_id`. Restituisce l'esecutore e l'etichetta del backend.
async fn launch(state: &Shared, plan: &mut Plan, node_id: i64, gen: u32, first: bool) -> Result<(Exec, String), String> {
    let row = state.nodes.row(node_id).ok_or("nodo sconosciuto")?;
    let dir_name = if gen == 0 { plan.session_id.clone() } else { format!("{}-{gen}", plan.session_id) };
    if row.is_local {
        let hw = profile_for(&plan.spec, Some(state.transcoder.current_profile()));
        if first {
            cap_for_software(&mut plan.spec, &hw);
        }
        let mut spec = plan.spec.clone();
        spec.out_dir = state.boot.transcode_dir.join(&dir_name);
        tokio::fs::create_dir_all(&spec.out_dir).await.map_err(|e| e.to_string())?;
        let rt = Arc::new(HlsRuntime::new(
            spec.out_dir.clone(),
            plan.cuts.clone(),
            plan.duration_ms,
            spec,
            state.ffmpeg.get().ffmpeg.clone(),
            hw,
            state.transcoder.clone(),
        ));
        let label = rt.backend();
        return Ok((Exec::Local(rt), label));
    }
    let live = state.nodes.live(node_id);
    let status = live.status.ok_or("nodo mai raggiunto")?;
    if !status.ffmpeg_available {
        return Err("FFmpeg non disponibile sul nodo".into());
    }
    let hw = profile_for(&plan.spec, status.current.clone());
    if first {
        cap_for_software(&mut plan.spec, &hw);
    }
    let transcode = matches!(plan.spec.video_action, VideoAction::Transcode { .. });
    // Il file in una cartella del nodo, se la copia è verificata; altrimenti lo legge dal server.
    let mut source = None;
    if let Some(root) = state.nodes.node_path(node_id, plan.library_path_id) {
        let path = super::join_node_path(&root, &plan.rel);
        if super::verify_copy(state, &row, plan.file_id, &path, plan.file_size).await {
            source = Some(Source::Path(path));
        }
    }
    let source = match source {
        Some(s) => s,
        None if !transcode => return Err("remux su un nodo senza il file in locale".into()),
        None => {
            let base = super::server_base_for(state, node_id).ok_or("indirizzo del server per i nodi sconosciuto (Impostazioni › Nodi)")?;
            let token = crate::util::random_hex(24);
            state.nodes.sources.lock().unwrap().insert(token.clone(), super::SourceGrant { file_id: plan.file_id, session_id: plan.session_id.clone() });
            Source::Url(format!("{base}/api/v1/node-source/{token}"))
        }
    };
    let s = state.settings.get();
    let remote_id = dir_name;
    let req = CreateHls {
        id: remote_id.clone(),
        spec: plan.spec.clone(),
        hw,
        cuts: plan.cuts.clone(),
        duration_ms: plan.duration_ms,
        source,
        ahead_segments: plan.ahead,
        keep_behind: plan.keep.max(s.playback.keep_segments_behind as usize),
    };
    let client = row.client(&state.nodes.http);
    let reply = client.create_hls(&req).await.map_err(|e| e.to_string())?;
    Ok((Exec::Remote { node_id, client, remote_id }, reply.backend))
}

fn is_failover_error(e: &NodeCallError) -> bool {
    !matches!(e.status(), Some(404))
}

impl HlsHandle {
    pub fn is_fmp4(&self) -> bool {
        self.fmp4
    }

    pub fn playlist(&self, segment_url: impl Fn(usize) -> String) -> String {
        crate::transcode::playlist(&self.cuts, self.duration_ms, self.fmp4, segment_url)
    }

    pub fn executor_id(&self) -> i64 {
        self.exec_id.load(Ordering::Relaxed)
    }

    /// Nome del nodo che esegue, vuoto se è questo server.
    pub fn executor_name(&self) -> String {
        self.exec_name.lock().unwrap().clone()
    }

    /// Id della sessione sul nodo `node`, se è lui a eseguirla (per l'heartbeat).
    pub fn remote_id_on(&self, node: i64) -> Option<String> {
        self.remote.lock().unwrap().as_ref().filter(|(n, _)| *n == node).map(|(_, id)| id.clone())
    }

    async fn current(&self) -> (Exec, u32) {
        let g = self.exec.lock().await;
        (g.clone(), self.gen.load(Ordering::SeqCst))
    }

    /// Segmento `n` pronto da inviare.
    pub async fn segment(&self, state: &Shared, n: usize) -> Result<Vec<u8>, HlsError> {
        if n >= self.cuts.len() {
            return Err(HlsError::OutOfRange);
        }
        self.fetch(state, Part::Segment(n)).await
    }

    /// Segmento d'inizializzazione fMP4.
    pub async fn init(&self, state: &Shared) -> Result<Vec<u8>, HlsError> {
        self.fetch(state, Part::Init).await
    }

    async fn fetch(&self, state: &Shared, part: Part) -> Result<Vec<u8>, HlsError> {
        for _ in 0..4 {
            if self.stopped.load(Ordering::SeqCst) {
                return Err(HlsError::Stopped);
            }
            let (exec, gen) = self.current().await;
            match &exec {
                Exec::Local(rt) => {
                    return match part {
                        Part::Segment(n) => rt.segment_bytes(n).await,
                        Part::Init => rt.init_bytes().await,
                    }
                }
                Exec::Remote { node_id, client, remote_id } => {
                    let p = match part {
                        Part::Segment(n) => format!("seg/{n}"),
                        Part::Init => "init".to_string(),
                    };
                    match client.hls_bytes(remote_id, &p).await {
                        Ok((bytes, backend)) => {
                            if let Some(b) = backend {
                                *self.remote_backend.lock().unwrap() = b;
                            }
                            return Ok(bytes);
                        }
                        Err(e) if !is_failover_error(&e) => return Err(HlsError::OutOfRange),
                        Err(e) => {
                            tracing::warn!(session = %self.session_id, node = node_id, error = %e, "esecutore della sessione in errore: failover");
                            self.failover(state, gen, *node_id, &e.to_string()).await?;
                        }
                    }
                }
            }
        }
        Err(HlsError::Failed("nessun esecutore riesce a produrre il segmento".into()))
    }

    /// Passa al prossimo esecutore disponibile. Se un'altra richiesta l'ha già fatto, non fa nulla.
    async fn failover(&self, state: &Shared, seen_gen: u32, failed: i64, why: &str) -> Result<(), HlsError> {
        let mut g = self.exec.lock().await;
        if self.gen.load(Ordering::SeqCst) != seen_gen {
            return Ok(());
        }
        state.nodes.record_failure(failed, why);
        let old = g.clone();
        if self.fmp4 && self.transcode {
            // L'init cambierebbe con l'encoder: il player riapre la sessione (e sceglie un esecutore sano).
            stop_exec(&old);
            self.stopped.store(true, Ordering::SeqCst);
            return Err(HlsError::Stopped);
        }
        let exclude = {
            let mut t = self.tried.lock().unwrap();
            if !t.contains(&failed) {
                t.push(failed);
            }
            t.clone()
        };
        let mut plan = self.plan.lock().unwrap().clone();
        let order = executors(state, &plan, &exclude).unwrap_or_default();
        for id in order {
            let gen = seen_gen + 1;
            match launch(state, &mut plan, id, gen, false).await {
                Ok((exec, backend)) => {
                    let name = exec_name(state, &exec);
                    tracing::warn!(session = %self.session_id, from = failed, to = %if name.is_empty() { "server".to_string() } else { name.clone() }, "sessione spostata su un altro esecutore");
                    self.exec_id.store(exec_node(state, &exec), Ordering::Relaxed);
                    *self.exec_name.lock().unwrap() = name;
                    *self.remote.lock().unwrap() = remote_of(&exec);
                    *self.remote_backend.lock().unwrap() = backend;
                    *g = exec;
                    self.gen.store(gen, Ordering::SeqCst);
                    stop_exec(&old);
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!(session = %self.session_id, node = id, error = %e, "esecutore di riserva non disponibile");
                    self.tried.lock().unwrap().push(id);
                }
            }
        }
        stop_exec(&old);
        self.stopped.store(true, Ordering::SeqCst);
        Err(HlsError::Stopped)
    }

    /// Rallentamento e pulizia: per i nodi li fa il nodo stesso.
    pub async fn maintain(&self, ahead: usize, keep: usize) {
        let (exec, _) = self.current().await;
        if let Exec::Local(rt) = exec {
            rt.maintain(ahead, keep).await;
        }
    }

    pub async fn buffered_until_ms(&self, state: &Shared) -> Option<i64> {
        match self.current().await.0 {
            Exec::Local(rt) => rt.buffered_until_ms().await,
            Exec::Remote { remote_id, .. } => state.nodes.remote_status.lock().unwrap().get(&remote_id).and_then(|s| s.buffered_until_ms),
        }
    }

    pub async fn is_paused(&self, state: &Shared) -> bool {
        match self.current().await.0 {
            Exec::Local(rt) => rt.is_paused().await,
            Exec::Remote { remote_id, .. } => state.nodes.remote_status.lock().unwrap().get(&remote_id).is_some_and(|s| s.paused),
        }
    }

    /// Chi codifica adesso: "copia", il backend hardware, "Software"…
    pub async fn backend(&self, state: &Shared) -> String {
        match self.current().await.0 {
            Exec::Local(rt) => rt.backend(),
            Exec::Remote { remote_id, .. } => state
                .nodes
                .remote_status
                .lock()
                .unwrap()
                .get(&remote_id)
                .map(|s| s.backend.clone())
                .unwrap_or_else(|| self.remote_backend.lock().unwrap().clone()),
        }
    }

    pub async fn stop(&self, state: &Shared) {
        self.stopped.store(true, Ordering::SeqCst);
        let (exec, _) = self.current().await;
        if let Exec::Remote { remote_id, .. } = &exec {
            state.nodes.remote_status.lock().unwrap().remove(remote_id);
        }
        match exec {
            Exec::Local(rt) => rt.stop().await,
            other => stop_exec(&other),
        }
        let sid = self.session_id.clone();
        state.nodes.sources.lock().unwrap().retain(|_, g| g.session_id != sid);
    }
}

#[derive(Clone, Copy)]
enum Part {
    Segment(usize),
    Init,
}

/// Ferma un esecutore senza aspettare (il nodo può essere già sparito).
fn stop_exec(e: &Exec) {
    match e.clone() {
        Exec::Local(rt) => {
            tokio::spawn(async move { rt.stop().await });
        }
        Exec::Remote { client, remote_id, .. } => {
            tokio::spawn(async move { client.delete_hls(&remote_id).await });
        }
    }
}
