//! API del nodo (`/node/v1`): la chiama solo il server, con la chiave del nodo.

use super::{NodeSession, NodeShared};
use crate::error::{ApiError, ApiResult};
use crate::nodes::proto::{CreateHls, CreateHlsReply, Heartbeat, PushConfig, RemoteSessionStatus, Source, StatEntry, StatReq};
use crate::transcode::{HlsError, HlsRuntime};
use axum::body::Body;
use axum::extract::{ConnectInfo, Path, Query, Request, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub fn router(state: NodeShared) -> Router {
    let protected = Router::new()
        .route("/node/v1/heartbeat", post(heartbeat))
        .route("/node/v1/status", get(status))
        .route("/node/v1/config", put(config))
        .route("/node/v1/hw-test", post(hw_test))
        .route("/node/v1/logs", get(logs))
        .route("/node/v1/restart", post(restart))
        .route("/node/v1/hls", post(create_hls))
        .route("/node/v1/hls/{id}", get(hls_status).delete(delete_hls))
        .route("/node/v1/hls/{id}/init", get(hls_init))
        .route("/node/v1/hls/{id}/seg/{n}", get(hls_segment))
        .route("/node/v1/file", get(file).head(file))
        .route("/node/v1/fs/stat", post(stat))
        .route("/node/v1/fs/browse", get(browse))
        .layer(middleware::from_fn_with_state(state.clone(), auth));
    Router::new()
        .route("/node/v1/health", get(|| async { Json(json!({ "ok": true, "protocol": crate::nodes::proto::PROTOCOL })) }))
        .route("/node/v1/d/{exp}/{sig}/{name}", get(direct).head(direct))
        .merge(protected)
        .fallback(|| async { ApiError::not_found("Percorso") })
        .layer(tower_http::limit::RequestBodyLimitLayer::new(4 * 1024 * 1024))
        .with_state(state)
}

/// Chiave e, se impostate, reti ammesse. Confronto a tempo costante.
async fn auth(State(state): State<NodeShared>, ConnectInfo(peer): ConnectInfo<SocketAddr>, req: Request, next: Next) -> Response {
    if !state.allow.is_empty() && !crate::net::in_any(&state.allow, peer.ip()) {
        tracing::warn!(peer = %peer.ip(), "richiesta da una rete non ammessa");
        return ApiError::Forbidden("Rete non ammessa".into()).into_response();
    }
    let given = req.headers().get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).unwrap_or("");
    if !crate::util::constant_time_eq(given.trim().as_bytes(), state.key.as_bytes()) {
        // Una chiave sbagliata costa tempo: niente prove a raffica.
        tokio::time::sleep(Duration::from_millis(300)).await;
        tracing::warn!(peer = %peer.ip(), "chiave del nodo sbagliata");
        return ApiError::Unauthorized.into_response();
    }
    next.run(req).await
}

async fn heartbeat(State(state): State<NodeShared>, ConnectInfo(peer): ConnectInfo<SocketAddr>, Json(hb): Json<Heartbeat>) -> Json<Value> {
    let now = crate::util::now_ms();
    state.last_heartbeat.store(now, Ordering::Relaxed);
    // Sessioni che il server non conosce più (riavvio del server, stop perso): via. Le appena create si lasciano
    // stare, l'heartbeat potrebbe essere partito prima della loro apertura.
    let stale: Vec<String> =
        state.sessions.lock().unwrap().iter().filter(|(id, s)| !hb.sessions.contains(id) && now - s.created > 15_000).map(|(id, _)| id.clone()).collect();
    for id in stale {
        super::stop_session(&state, &id).await;
    }
    Json(serde_json::to_value(super::status(&state, Some(peer.ip())).await).unwrap_or_default())
}

async fn status(State(state): State<NodeShared>, ConnectInfo(peer): ConnectInfo<SocketAddr>) -> Json<Value> {
    Json(serde_json::to_value(super::status(&state, Some(peer.ip())).await).unwrap_or_default())
}

async fn config(State(state): State<NodeShared>, Json(c): Json<PushConfig>) -> Json<Value> {
    if super::apply_config(&state, c.rev, c.config) {
        tracing::info!("configurazione della GPU cambiata: nuova prova");
        let st = state.clone();
        tokio::spawn(async move { super::detect(&st).await });
    }
    Json(json!({ "ok": true, "rev": c.rev }))
}

async fn hw_test(State(state): State<NodeShared>, ConnectInfo(peer): ConnectInfo<SocketAddr>) -> Json<Value> {
    super::detect(&state).await;
    Json(serde_json::to_value(super::status(&state, Some(peer.ip())).await).unwrap_or_default())
}

#[derive(Deserialize)]
struct LogQuery {
    limit: Option<usize>,
    level: Option<String>,
}

async fn logs(Query(q): Query<LogQuery>) -> Json<Value> {
    let min = crate::logs::level_rank(&q.level.unwrap_or_else(|| "INFO".into()).to_uppercase());
    Json(json!({ "lines": crate::logs::LOGS.recent(min, q.limit.unwrap_or(300).min(2000)) }))
}

async fn restart() -> Json<Value> {
    tracing::warn!("riavvio chiesto dal server");
    tokio::spawn(async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        // Docker (restart: unless-stopped) lo fa ripartire.
        std::process::exit(0);
    });
    Json(json!({ "ok": true }))
}

fn hls_error(e: HlsError) -> Response {
    let (status, code) = match &e {
        HlsError::OutOfRange => (StatusCode::NOT_FOUND, "NotFound"),
        HlsError::Busy => (StatusCode::SERVICE_UNAVAILABLE, "TranscodeBusy"),
        HlsError::Stopped => (StatusCode::GONE, "SessionEnded"),
        HlsError::Failed(_) => (StatusCode::INTERNAL_SERVER_ERROR, "TranscodeFailed"),
    };
    ApiError::coded(status, code, e.to_string()).into_response()
}

async fn create_hls(State(state): State<NodeShared>, Json(req): Json<CreateHls>) -> ApiResult<Json<CreateHlsReply>> {
    let f = state.ffmpeg.get();
    if !f.available {
        return Err(ApiError::coded(StatusCode::SERVICE_UNAVAILABLE, "FfmpegMissing", "FFmpeg non disponibile sul nodo"));
    }
    if req.id.is_empty() || !req.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') || req.id.len() > 64 {
        return Err(ApiError::BadRequest("Identificativo di sessione non valido".into()));
    }
    if req.cuts.is_empty() {
        return Err(ApiError::BadRequest("Piano dei segmenti vuoto".into()));
    }
    let input = match &req.source {
        Source::Path(p) => check(&state, p).await?.0,
        Source::Url(u) if u.starts_with("http://") || u.starts_with("https://") => std::path::PathBuf::from(u),
        Source::Url(_) => return Err(ApiError::BadRequest("Sorgente non valida".into())),
    };
    super::stop_session(&state, &req.id).await;
    let cfg = state.config.read().unwrap().1.clone();
    let mut spec = req.spec;
    spec.input = input;
    spec.out_dir = state.transcode_dir.join(&req.id);
    // Scelte della macchina: le decide il nodo con la sua configurazione.
    spec.threads = cfg.threads;
    spec.sw_preset = cfg.sw_preset.clone();
    spec.sw_crf = cfg.sw_crf;
    spec.tonemapx = f.is_jellyfin;
    tokio::fs::create_dir_all(&spec.out_dir).await.map_err(|e| ApiError::Internal(e.into()))?;
    let rt = Arc::new(HlsRuntime::new(spec.out_dir.clone(), req.cuts, req.duration_ms, spec, f.ffmpeg.clone(), req.hw, state.transcoder.clone()));
    let backend = rt.backend();
    let now = crate::util::now_ms();
    state.sessions.lock().unwrap().insert(
        req.id.clone(),
        Arc::new(NodeSession { rt, ahead: req.ahead_segments.max(5), keep: req.keep_behind, created: now, last_used: AtomicI64::new(now) }),
    );
    tracing::info!(session = %req.id, backend = %backend, source = ?req.source, "sessione aperta");
    Ok(Json(CreateHlsReply { id: req.id, backend }))
}

fn session(state: &NodeShared, id: &str) -> ApiResult<Arc<NodeSession>> {
    let s = state.sessions.lock().unwrap().get(id).cloned();
    let s = s.ok_or_else(|| ApiError::coded(StatusCode::GONE, "SessionEnded", "Sessione non presente sul nodo"))?;
    s.last_used.store(crate::util::now_ms(), Ordering::Relaxed);
    Ok(s)
}

fn bytes_response(bytes: Vec<u8>, ct: &'static str, backend: String) -> Response {
    let mut r = Response::builder().header(header::CONTENT_TYPE, ct).header(header::CONTENT_LENGTH, bytes.len()).body(Body::from(bytes)).unwrap();
    if let Ok(v) = axum::http::HeaderValue::from_str(&backend) {
        r.headers_mut().insert("x-platea-backend", v);
    }
    r
}

async fn hls_segment(State(state): State<NodeShared>, Path((id, n)): Path<(String, usize)>) -> Response {
    let s = match session(&state, &id) {
        Ok(s) => s,
        Err(e) => return e.into_response(),
    };
    match s.rt.segment_bytes(n).await {
        Ok(b) => bytes_response(b, if s.rt.is_fmp4() { "video/mp4" } else { "video/mp2t" }, s.rt.backend()),
        Err(e) => hls_error(e),
    }
}

async fn hls_init(State(state): State<NodeShared>, Path(id): Path<String>) -> Response {
    let s = match session(&state, &id) {
        Ok(s) => s,
        Err(e) => return e.into_response(),
    };
    match s.rt.init_bytes().await {
        Ok(b) => bytes_response(b, "video/mp4", s.rt.backend()),
        Err(e) => hls_error(e),
    }
}

async fn hls_status(State(state): State<NodeShared>, Path(id): Path<String>) -> ApiResult<Json<RemoteSessionStatus>> {
    let s = session(&state, &id)?;
    Ok(Json(RemoteSessionStatus { id, paused: s.rt.is_paused().await, buffered_until_ms: s.rt.buffered_until_ms().await, backend: s.rt.backend() }))
}

async fn delete_hls(State(state): State<NodeShared>, Path(id): Path<String>) -> Json<Value> {
    Json(json!({ "stopped": super::stop_session(&state, &id).await }))
}

async fn check(state: &NodeShared, p: &str) -> ApiResult<(std::path::PathBuf, std::path::PathBuf)> {
    super::check_path(state, p).await.map_err(|e| ApiError::coded(StatusCode::NOT_FOUND, "FileUnavailable", format!("{p}: {e}")))
}

#[derive(Deserialize)]
struct FileQuery {
    path: String,
}

/// Chiave dei descrittori condivisi (`stream::open_library_file`): sul nodo non ci sono identificativi del database.
fn path_key(p: &std::path::Path) -> i64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    p.hash(&mut h);
    (h.finish() >> 1) as i64
}

/// File di una cartella in copia, con le richieste Range (direct play, download, sorgente di FFmpeg altrove).
async fn file(State(state): State<NodeShared>, Query(q): Query<FileQuery>, method: Method, headers: HeaderMap) -> ApiResult<Response> {
    let (canon, root) = check(&state, &q.path).await?;
    let c2 = canon.clone();
    let meta = tokio::time::timeout(Duration::from_secs(20), tokio::task::spawn_blocking(move || std::fs::metadata(&c2)))
        .await
        .map_err(|_| ApiError::coded(StatusCode::GATEWAY_TIMEOUT, "StorageTimeout", "Lo storage del nodo non risponde"))?
        .map_err(|e| ApiError::Internal(e.into()))?
        .map_err(|e| ApiError::coded(StatusCode::NOT_FOUND, "FileUnavailable", e.to_string()))?;
    if !meta.is_file() {
        return Err(ApiError::coded(StatusCode::NOT_FOUND, "FileUnavailable", "Non è un file"));
    }
    let mtime = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
    let f = crate::stream::open_library_file(path_key(&canon), &canon, &root, Duration::from_secs(20)).await?;
    let ct = crate::stream::content_type_for("", &canon);
    crate::stream::serve_file(f, meta.len(), mtime, ct, &method, &headers, None)
}

#[derive(Deserialize)]
struct DirectQuery {
    path: String,
    #[serde(default)]
    dl: Option<u8>,
}

/// Consegna diretta: link firmato dal server (direct play o download), senza chiave nell'intestazione.
async fn direct(
    State(state): State<NodeShared>,
    Path((exp, sig, name)): Path<(i64, String, String)>,
    Query(q): Query<DirectQuery>,
    method: Method,
    headers: HeaderMap,
) -> ApiResult<Response> {
    if exp < crate::util::now_ms() / 1000 {
        return Err(ApiError::coded(StatusCode::FORBIDDEN, "LinkExpired", "Link scaduto"));
    }
    let want = crate::nodes::cdn::sign(&state.key, exp, &q.path);
    if !crate::util::constant_time_eq(want.as_bytes(), sig.as_bytes()) {
        tokio::time::sleep(Duration::from_millis(300)).await;
        return Err(ApiError::Unauthorized);
    }
    let mut r = file(State(state), Query(FileQuery { path: q.path }), method, headers).await?;
    let h = r.headers_mut();
    h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, axum::http::HeaderValue::from_static("*"));
    h.insert(header::ACCESS_CONTROL_EXPOSE_HEADERS, axum::http::HeaderValue::from_static("content-range, content-length, accept-ranges, etag"));
    if q.dl == Some(1) {
        let disp = format!("attachment; filename=\"{}\"; filename*=UTF-8''{}", name.replace(['"', '\\'], "_"), urlencoding::encode(&name));
        if let Ok(v) = axum::http::HeaderValue::from_str(&disp) {
            h.insert(header::CONTENT_DISPOSITION, v);
        }
    }
    Ok(r)
}

async fn stat(State(state): State<NodeShared>, Json(req): Json<StatReq>) -> ApiResult<Json<Vec<StatEntry>>> {
    if req.paths.len() > 64 {
        return Err(ApiError::BadRequest("Al massimo 64 percorsi".into()));
    }
    let mut out = Vec::with_capacity(req.paths.len());
    for p in req.paths {
        let entry = match super::check_path(&state, &p).await {
            Ok((canon, _)) => match tokio::time::timeout(Duration::from_secs(20), tokio::task::spawn_blocking(move || std::fs::metadata(&canon))).await {
                Ok(Ok(Ok(m))) => StatEntry { path: p, exists: true, size: m.is_file().then_some(m.len()), is_dir: m.is_dir(), error: None },
                Ok(Ok(Err(e))) => StatEntry { path: p, exists: false, size: None, is_dir: false, error: Some(e.to_string()) },
                _ => StatEntry { path: p, exists: false, size: None, is_dir: false, error: Some("Lo storage non risponde".into()) },
            },
            Err(e) => StatEntry { path: p, exists: false, size: None, is_dir: false, error: Some(e) },
        };
        out.push(entry);
    }
    Ok(Json(out))
}

#[derive(Deserialize)]
struct BrowseQuery {
    path: Option<String>,
}

/// Cartelle del nodo, per scegliere la copia di una libreria. Senza percorso: le radici dei media.
async fn browse(State(state): State<NodeShared>, Query(q): Query<BrowseQuery>) -> ApiResult<Json<Value>> {
    let Some(path) = q.path.filter(|p| !p.is_empty()) else {
        return Ok(Json(json!({
            "path": null,
            "parent": null,
            "directories": state.roots.iter().map(|r| json!({ "name": r.to_string_lossy(), "path": r.to_string_lossy() })).collect::<Vec<_>>(),
        })));
    };
    let (canon, root) = check(&state, &path).await?;
    let c2 = canon.clone();
    let listing = tokio::time::timeout(
        Duration::from_secs(15),
        tokio::task::spawn_blocking(move || -> std::io::Result<Vec<String>> {
            let mut dirs: Vec<String> = std::fs::read_dir(&c2)?
                .flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| !n.starts_with('.'))
                .collect();
            dirs.sort_by_key(|a| a.to_lowercase());
            dirs.truncate(2000);
            Ok(dirs)
        }),
    )
    .await
    .map_err(|_| ApiError::BadRequest("La cartella non risponde: mount bloccato?".into()))?
    .map_err(|e| ApiError::Internal(e.into()))?
    .map_err(|e| ApiError::BadRequest(format!("{path}: {e}")))?;
    let parent = if canon == root { None } else { canon.parent().map(|x| x.to_string_lossy().to_string()) };
    Ok(Json(json!({
        "path": canon.to_string_lossy(),
        "parent": parent,
        "directories": listing.iter().map(|d| json!({ "name": d, "path": canon.join(d).to_string_lossy() })).collect::<Vec<_>>(),
    })))
}
