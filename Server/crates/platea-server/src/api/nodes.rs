//! Nodi: elenco con stato dal vivo, aggiunta, modifica, azioni (prova, GPU, log, riavvio, svuotamento),
//! cartelle in copia. Più la sorgente dei file per i nodi che transcodificano senza averli in locale.

use super::{Access, Api};
use crate::auth::AdminUser;
use crate::error::{ApiError, ApiResult};
use crate::nodes::client::normalize_url;
use crate::nodes::proto::{NodeConfig, NodeStatus, Slots, PROTOCOL};
use crate::nodes::route::Rules;
use crate::nodes::{Health, NodeRow};
use crate::state::Shared;
use crate::util::now_ms;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::Response;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::atomic::Ordering;

pub fn routes(api: Api) -> Api {
    api.get("/api/v1/admin/nodes", "Nodi", Access::Admin, "Nodi con stato dal vivo, cartelle e librerie", list)
        .post("/api/v1/admin/nodes", "Nodi", Access::Admin, "Aggiunge un nodo (indirizzo e chiave)", create)
        .patch("/api/v1/admin/nodes/{id}", "Nodi", Access::Admin, "Modifica un nodo (ruoli, bilanciamento, regole, GPU)", update)
        .delete("/api/v1/admin/nodes/{id}", "Nodi", Access::Admin, "Elimina un nodo", remove)
        .post("/api/v1/admin/nodes/{id}/check", "Nodi", Access::Admin, "Prova subito il collegamento", check)
        .post("/api/v1/admin/nodes/{id}/speed-test", "Nodi", Access::Admin, "Prova di velocità tra server e nodo (circa 15 secondi)", speed_test)
        .post("/api/v1/admin/nodes/{id}/hw-test", "Nodi", Access::Admin, "Prova le GPU del nodo", hw_test)
        .get("/api/v1/admin/nodes/{id}/logs", "Nodi", Access::Admin, "Ultime righe del log del nodo", logs)
        .post("/api/v1/admin/nodes/{id}/restart", "Nodi", Access::Admin, "Riavvia il nodo", restart)
        .post("/api/v1/admin/nodes/{id}/stop-sessions", "Nodi", Access::Admin, "Ferma le riproduzioni servite dal nodo", stop_sessions)
        .get("/api/v1/admin/nodes/{id}/browse", "Nodi", Access::Admin, "Sfoglia le cartelle del nodo", browse)
        .put("/api/v1/admin/nodes/{id}/paths", "Nodi", Access::Admin, "Cartelle delle librerie in copia sul nodo (con verifica)", set_paths)
        .get("/api/v1/node-speed/{token}", "Nodi", Access::Public, "Dati casuali per la prova di velocità di un nodo (token monouso)", speed_source)
        .get("/api/v1/node-source/{token}", "Nodi", Access::Public, "File letto da un nodo che transcodifica (token della sessione)", source)
}

fn local_status(state: &Shared) -> NodeStatus {
    let f = state.ffmpeg.get();
    let (hw_used, hw_max, sw_used, sw_max) = state.transcoder.active_counts();
    let (memory_total, memory_available) = crate::sysinfo::meminfo();
    NodeStatus {
        version: crate::state::VERSION.into(),
        protocol: PROTOCOL,
        name: state.settings.get().general.server_name.clone(),
        ffmpeg_version: f.version.clone(),
        ffmpeg_available: f.available,
        ffmpeg_jellyfin: f.is_jellyfin,
        caps: state.transcoder.caps.read().unwrap().clone(),
        current: Some(state.transcoder.current_profile()),
        encoders: super::playback::allowed_encoders(state),
        slots: Slots { hw_used, hw_max, sw_used, sw_max },
        cpus: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
        load_1m: crate::sysinfo::load_1m(),
        memory_total,
        memory_available,
        rss: crate::sysinfo::rss(),
        transcode_free_bytes: crate::sysinfo::free_bytes(&state.boot.transcode_dir),
        source_cache_bytes: 0,
        source_cache_files: 0,
        sessions: vec![],
        config_rev: 0,
        config: None,
        peer_ip: None,
        media_roots: vec![],
        uptime_ms: now_ms() - state.started_at,
    }
}

async fn view(state: &Shared, r: &NodeRow) -> ApiResult<Value> {
    let (tc, cdn) = crate::nodes::active_counts(state);
    let live = state.nodes.live(r.id);
    let (health, status) = if r.is_local { (Health::Online, Some(local_status(state))) } else { (live.health, live.status.clone()) };
    let paths: Vec<(i64, String, Option<i64>, Option<String>)> =
        sqlx::query_as("SELECT library_path_id, node_path, verified_at, verify_error FROM node_paths WHERE node_id = ? ORDER BY library_path_id")
            .bind(r.id)
            .fetch_all(&state.db.read)
            .await?;
    let server_base = if r.is_local { None } else { crate::nodes::server_base_for(state, r.id) };
    Ok(json!({
        "id": r.id,
        "name": r.name,
        "is_local": r.is_local,
        "url": r.url,
        "key_set": !r.key.is_empty(),
        "enabled": r.enabled,
        "draining": r.draining,
        "role_transcode": r.role_transcode,
        "role_cdn": r.role_cdn,
        "tc_priority": r.tc_priority,
        "tc_weight": r.tc_weight,
        "tc_max_sessions": r.tc_max_sessions,
        "rules": r.rules,
        "cdn_priority": r.cdn_priority,
        "cdn_weight": r.cdn_weight,
        "cdn_max_streams": r.cdn_max_streams,
        "config": r.config,
        "public_url": r.public_url,
        "health": if r.enabled { health } else { Health::Unknown },
        "last_error": live.last_error,
        "last_ok": if live.last_ok > 0 { Some(live.last_ok) } else { None },
        "latency_ms": live.latency_ms,
        "breaker_until": if live.breaker_until > now_ms() { Some(live.breaker_until) } else { None },
        "config_synced": r.is_local || status.as_ref().is_some_and(|s| s.config_rev == r.updated_at),
        "status": status,
        "active_transcodes": tc.get(&r.id).copied().unwrap_or(0),
        "active_streams": cdn.get(&r.id).copied().unwrap_or(0),
        "server_url": server_base,
        "server_reach": if r.is_local { None } else { live.reach.clone() },
        "speed": live.speed,
        "paths": paths.into_iter().map(|(lp, p, at, err)| json!({ "library_path_id": lp, "node_path": p, "verified_at": at, "verify_error": err })).collect::<Vec<_>>(),
    }))
}

async fn list(State(state): State<Shared>, _a: AdminUser) -> ApiResult<Json<Value>> {
    let mut nodes = Vec::new();
    for r in state.nodes.rows().iter() {
        nodes.push(view(&state, r).await?);
    }
    let paths: Vec<(i64, i64, String, String)> =
        sqlx::query_as("SELECT p.id, l.id, l.name, p.path FROM library_paths p JOIN libraries l ON l.id = p.library_id ORDER BY l.name, p.path")
            .fetch_all(&state.db.read)
            .await?;
    let libraries: Vec<(i64, String)> = sqlx::query_as("SELECT id, name FROM libraries ORDER BY name").fetch_all(&state.db.read).await?;
    Ok(Json(json!({
        "nodes": nodes,
        "library_paths": paths.into_iter().map(|(id, lid, name, path)| json!({ "id": id, "library_id": lid, "library": name, "path": path })).collect::<Vec<_>>(),
        "libraries": libraries.into_iter().map(|(id, name)| json!({ "id": id, "name": name })).collect::<Vec<_>>(),
        "protocol": PROTOCOL,
    })))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct NodeReq {
    name: Option<String>,
    url: Option<String>,
    key: Option<String>,
    enabled: Option<bool>,
    draining: Option<bool>,
    role_transcode: Option<bool>,
    role_cdn: Option<bool>,
    tc_priority: Option<i64>,
    tc_weight: Option<u32>,
    tc_max_sessions: Option<u32>,
    rules: Option<Rules>,
    cdn_priority: Option<i64>,
    cdn_weight: Option<u32>,
    cdn_max_streams: Option<u32>,
    config: Option<NodeConfig>,
    public_url: Option<String>,
}

const PRESETS: &[&str] = &["ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow"];

fn validate(r: &NodeRow) -> ApiResult<()> {
    if !r.is_local {
        if r.name.trim().is_empty() || r.name.chars().count() > 64 {
            return Err(ApiError::BadRequest("Nome del nodo obbligatorio (al massimo 64 caratteri)".into()));
        }
        if r.key.trim().len() < 16 {
            return Err(ApiError::BadRequest("Chiave del nodo mancante o troppo corta (la trovi nel log del nodo o con: platea node key)".into()));
        }
    }
    if !(1..=99).contains(&r.tc_priority) || !(1..=99).contains(&r.cdn_priority) {
        return Err(ApiError::BadRequest("La priorità va da 1 (prima scelta) a 99".into()));
    }
    if r.tc_weight > 1000 || r.cdn_weight > 1000 {
        return Err(ApiError::BadRequest("Il peso va da 0 a 1000".into()));
    }
    if r.tc_max_sessions > 256 || r.cdn_max_streams > 4096 {
        return Err(ApiError::BadRequest("Limite di sessioni troppo alto".into()));
    }
    let c = &r.config;
    if !PRESETS.contains(&c.sw_preset.as_str()) {
        return Err(ApiError::BadRequest(format!("Preset non valido: {}", c.sw_preset)));
    }
    if !(16..=35).contains(&c.sw_crf) || c.threads > 64 || c.max_hw_transcodes > 64 || c.max_sw_transcodes > 64 {
        return Err(ApiError::BadRequest("Valori della transcodifica fuori dai limiti".into()));
    }
    if !(16..=65536).contains(&c.readahead_mb) || !(1..=16).contains(&c.source_parallel) || c.source_cache_gb > 100_000 {
        return Err(ApiError::BadRequest("Lettura dal server: anticipo 16–65536 MB, blocchi in parallelo 1–16".into()));
    }
    if !c.vaapi_device.starts_with("/dev/") {
        return Err(ApiError::BadRequest("Il dispositivo DRI deve stare sotto /dev/".into()));
    }
    let pu = r.public_url.trim();
    if !pu.is_empty() && !(pu.starts_with("http://") || pu.starts_with("https://")) {
        return Err(ApiError::BadRequest("Indirizzo pubblico non valido (serve http:// o https://)".into()));
    }
    if r.rules.min_height > 0 && r.rules.max_height > 0 && r.rules.min_height > r.rules.max_height {
        return Err(ApiError::BadRequest("Altezza minima maggiore della massima".into()));
    }
    Ok(())
}

fn apply(r: &mut NodeRow, q: NodeReq) -> ApiResult<()> {
    if let Some(v) = q.name {
        r.name = v.trim().to_string();
    }
    if !r.is_local {
        if let Some(v) = q.url {
            r.url = normalize_url(&v).map_err(ApiError::BadRequest)?;
        }
        if let Some(v) = q.key.filter(|k| !k.trim().is_empty()) {
            r.key = v.trim().to_string();
        }
    }
    macro_rules! set {
        ($($f:ident),*) => { $( if let Some(v) = q.$f { r.$f = v; } )* };
    }
    set!(
        enabled,
        draining,
        role_transcode,
        role_cdn,
        tc_priority,
        tc_weight,
        tc_max_sessions,
        rules,
        cdn_priority,
        cdn_weight,
        cdn_max_streams,
        config,
        public_url
    );
    if r.is_local {
        // Il server non si spegne: si toglie un ruolo o si mette in svuotamento.
        r.enabled = true;
    }
    r.public_url = r.public_url.trim().trim_end_matches('/').to_string();
    validate(r)
}

async fn save(state: &Shared, r: &NodeRow, insert: bool) -> ApiResult<i64> {
    let rules = serde_json::to_string(&r.rules).unwrap_or_else(|_| "{}".into());
    let config = serde_json::to_string(&r.config).unwrap_or_else(|_| "{}".into());
    let now = now_ms().max(r.updated_at + 1);
    let sql = if insert {
        "INSERT INTO nodes (name, url, key, enabled, draining, role_transcode, role_cdn, tc_priority, tc_weight, tc_max_sessions, rules, cdn_priority, cdn_weight, cdn_max_streams, config, public_url, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?17)"
    } else {
        "UPDATE nodes SET name = ?1, url = ?2, key = ?3, enabled = ?4, draining = ?5, role_transcode = ?6, role_cdn = ?7, tc_priority = ?8, tc_weight = ?9,
         tc_max_sessions = ?10, rules = ?11, cdn_priority = ?12, cdn_weight = ?13, cdn_max_streams = ?14, config = ?15, public_url = ?16, updated_at = ?17 WHERE id = ?18"
    };
    let res = sqlx::query(sql)
        .bind(&r.name)
        .bind(&r.url)
        .bind(&r.key)
        .bind(r.enabled as i64)
        .bind(r.draining as i64)
        .bind(r.role_transcode as i64)
        .bind(r.role_cdn as i64)
        .bind(r.tc_priority)
        .bind(r.tc_weight as i64)
        .bind(r.tc_max_sessions as i64)
        .bind(rules)
        .bind(r.cdn_priority)
        .bind(r.cdn_weight as i64)
        .bind(r.cdn_max_streams as i64)
        .bind(config)
        .bind(&r.public_url)
        .bind(now)
        .bind(r.id)
        .execute(&state.db.write)
        .await?;
    state.nodes.reload(&state.db).await.map_err(ApiError::Internal)?;
    Ok(if insert { res.last_insert_rowid() } else { r.id })
}

fn node(state: &Shared, id: i64) -> ApiResult<NodeRow> {
    state.nodes.row(id).ok_or_else(|| ApiError::not_found("Nodo"))
}

fn remote(state: &Shared, id: i64) -> ApiResult<NodeRow> {
    let r = node(state, id)?;
    if r.is_local {
        return Err(ApiError::BadRequest("Azione disponibile solo per i nodi".into()));
    }
    Ok(r)
}

async fn create(State(state): State<Shared>, AdminUser(admin): AdminUser, Json(q): Json<NodeReq>) -> ApiResult<Json<Value>> {
    let mut r = NodeRow {
        id: 0,
        name: String::new(),
        is_local: false,
        url: String::new(),
        key: String::new(),
        enabled: true,
        draining: false,
        role_transcode: true,
        role_cdn: false,
        tc_priority: 1,
        tc_weight: 100,
        tc_max_sessions: 0,
        rules: Rules::default(),
        cdn_priority: 1,
        cdn_weight: 100,
        cdn_max_streams: 0,
        config: NodeConfig::default(),
        public_url: String::new(),
        updated_at: 0,
    };
    if q.url.as_deref().is_none_or(|u| u.trim().is_empty()) {
        return Err(ApiError::BadRequest("Indirizzo del nodo obbligatorio (es. 100.64.0.2:8097)".into()));
    }
    apply(&mut r, q)?;
    if state.nodes.rows().iter().any(|x| !x.is_local && x.url == r.url) {
        return Err(ApiError::Conflict(format!("Un nodo con indirizzo {} esiste già", r.url)));
    }
    let id = save(&state, &r, true).await?;
    state.db.activity(Some(admin.user_id), "node_added", &format!("{} ha aggiunto il nodo {} ({})", admin.username, r.name, r.url), None).await;
    let r = node(&state, id)?;
    crate::nodes::check_node(&state, &r).await;
    Ok(Json(view(&state, &r).await?))
}

async fn update(State(state): State<Shared>, AdminUser(admin): AdminUser, Path(id): Path<i64>, Json(q): Json<NodeReq>) -> ApiResult<Json<Value>> {
    let mut r = node(&state, id)?;
    apply(&mut r, q)?;
    save(&state, &r, false).await?;
    let r = node(&state, id)?;
    state.db.activity(Some(admin.user_id), "node_updated", &format!("{} ha modificato il nodo {}", admin.username, display_name(&state, &r)), None).await;
    state.events.admin("nodes.changed", json!({ "id": id }));
    if !r.is_local && r.enabled && crate::nodes::check_node(&state, &r).await == crate::nodes::Health::Online {
        crate::nodes::probe_server(&state, &r, true).await;
    }
    Ok(Json(view(&state, &r).await?))
}

fn display_name(state: &Shared, r: &NodeRow) -> String {
    if r.is_local {
        state.settings.get().general.server_name.clone()
    } else {
        r.name.clone()
    }
}

async fn remove(State(state): State<Shared>, AdminUser(admin): AdminUser, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let r = remote(&state, id)?;
    sqlx::query("DELETE FROM nodes WHERE id = ? AND is_local = 0").bind(id).execute(&state.db.write).await?;
    state.nodes.reload(&state.db).await.map_err(ApiError::Internal)?;
    state.db.activity(Some(admin.user_id), "node_removed", &format!("{} ha eliminato il nodo {}", admin.username, r.name), None).await;
    state.events.admin("nodes.changed", json!({ "id": id }));
    // Le sessioni in corso sul nodo passano a un altro esecutore alla prossima richiesta di segmento.
    Ok(Json(json!({ "ok": true })))
}

async fn check(State(state): State<Shared>, _a: AdminUser, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let r = node(&state, id)?;
    if !r.is_local && crate::nodes::check_node(&state, &r).await == crate::nodes::Health::Online {
        crate::nodes::probe_server(&state, &r, true).await;
    }
    Ok(Json(view(&state, &r).await?))
}

async fn hw_test(State(state): State<Shared>, _a: AdminUser, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let r = node(&state, id)?;
    if r.is_local {
        super::admin::run_hw_detection(&state).await;
    } else {
        let st = r.client(&state.nodes.http).hw_test().await.map_err(|e| ApiError::coded(StatusCode::BAD_GATEWAY, "NodeUnavailable", e.to_string()))?;
        crate::nodes::store_status(&state, r.id, st);
    }
    Ok(Json(view(&state, &r).await?))
}

async fn speed_test(State(state): State<Shared>, _a: AdminUser, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let r = remote(&state, id)?;
    if !state.nodes.online(r.id) {
        return Err(ApiError::coded(StatusCode::CONFLICT, "NodeUnavailable", "Il nodo non è online"));
    }
    let res = crate::nodes::speed::run(&state, &r)
        .await
        .ok_or_else(|| ApiError::coded(StatusCode::CONFLICT, "SpeedTestRunning", "C'è già una prova di velocità in corso su questo nodo"))?;
    Ok(Json(json!({ "result": res, "node": view(&state, &r).await? })))
}

/// Dati casuali per la prova "il nodo legge dal server". Il token vale una volta e per poco.
async fn speed_source(Path(token): Path<String>) -> ApiResult<Response> {
    if !crate::nodes::speed::take_token(&token) {
        return Err(ApiError::Unauthorized);
    }
    Ok(Response::builder()
        .header(axum::http::header::CONTENT_TYPE, "application/octet-stream")
        .header(axum::http::header::CACHE_CONTROL, "no-store")
        .body(axum::body::Body::from_stream(crate::nodes::speed::timed(std::time::Duration::from_secs(crate::nodes::speed::SECONDS + 5))))
        .unwrap())
}

#[derive(Deserialize)]
struct LogQuery {
    limit: Option<usize>,
    level: Option<String>,
}

async fn logs(State(state): State<Shared>, _a: AdminUser, Path(id): Path<i64>, Query(q): Query<LogQuery>) -> ApiResult<Json<Value>> {
    let r = node(&state, id)?;
    let limit = q.limit.unwrap_or(300).min(2000);
    let level = q.level.unwrap_or_else(|| "INFO".into()).to_uppercase();
    if r.is_local {
        return Ok(Json(json!({ "lines": crate::logs::LOGS.recent(crate::logs::level_rank(&level), limit) })));
    }
    let v = r.client(&state.nodes.http).logs(limit, &level).await.map_err(|e| ApiError::coded(StatusCode::BAD_GATEWAY, "NodeUnavailable", e.to_string()))?;
    Ok(Json(v))
}

async fn restart(State(state): State<Shared>, AdminUser(admin): AdminUser, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let r = remote(&state, id)?;
    r.client(&state.nodes.http).restart().await.map_err(|e| ApiError::coded(StatusCode::BAD_GATEWAY, "NodeUnavailable", e.to_string()))?;
    state.db.activity(Some(admin.user_id), "node_restart", &format!("{} ha riavviato il nodo {}", admin.username, r.name), None).await;
    Ok(Json(json!({ "ok": true })))
}

async fn stop_sessions(State(state): State<Shared>, AdminUser(admin): AdminUser, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let r = node(&state, id)?;
    let mut n = 0;
    for s in state.sessions.all() {
        let on_node = s.hls.as_ref().is_some_and(|h| h.executor_id() == id) || s.copy_node.load(Ordering::Relaxed) == id;
        if on_node {
            state.events.to_user(s.user_id, "playback.stopped", json!({ "session_id": s.id, "by": "admin" }));
            crate::sessions::stop_session(&state, &s.id).await;
            n += 1;
        }
    }
    state
        .db
        .activity(
            Some(admin.user_id),
            "node_stop_sessions",
            &format!("{} ha fermato {n} riproduzioni del nodo {}", admin.username, display_name(&state, &r)),
            None,
        )
        .await;
    Ok(Json(json!({ "stopped": n })))
}

#[derive(Deserialize)]
struct BrowseQuery {
    path: Option<String>,
}

async fn browse(State(state): State<Shared>, _a: AdminUser, Path(id): Path<i64>, Query(q): Query<BrowseQuery>) -> ApiResult<Json<Value>> {
    let r = remote(&state, id)?;
    let v =
        r.client(&state.nodes.http).browse(q.path.as_deref()).await.map_err(|e| ApiError::coded(StatusCode::BAD_GATEWAY, "NodeUnavailable", e.to_string()))?;
    Ok(Json(v))
}

#[derive(Deserialize)]
struct PathEntry {
    library_path_id: i64,
    node_path: String,
}

#[derive(Deserialize)]
struct PathsReq {
    paths: Vec<PathEntry>,
}

/// Cartelle in copia: si sostituiscono tutte. Ognuna si verifica sul nodo: la cartella esiste e alcuni file della
/// libreria hanno la stessa dimensione che sul server.
async fn set_paths(State(state): State<Shared>, AdminUser(admin): AdminUser, Path(id): Path<i64>, Json(q): Json<PathsReq>) -> ApiResult<Json<Value>> {
    let r = remote(&state, id)?;
    let c = r.client(&state.nodes.http);
    let mut out = Vec::new();
    let mut rows = Vec::new();
    for p in q.paths {
        let np = p.node_path.trim().trim_end_matches('/').to_string();
        if np.is_empty() {
            continue;
        }
        if !np.starts_with('/') || np.split('/').any(|x| x == "..") {
            return Err(ApiError::BadRequest(format!("Percorso non valido: {np}")));
        }
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT id FROM library_paths WHERE id = ?").bind(p.library_path_id).fetch_optional(&state.db.read).await?;
        if exists.is_none() {
            return Err(ApiError::not_found("Cartella della libreria"));
        }
        let samples: Vec<(String, i64)> = sqlx::query_as(
            "SELECT relative_path, size FROM media_files WHERE library_path_id = ? AND state = 'present' AND stream_url IS NULL ORDER BY id DESC LIMIT 3",
        )
        .bind(p.library_path_id)
        .fetch_all(&state.db.read)
        .await?;
        let mut req = vec![np.clone()];
        req.extend(samples.iter().map(|(rel, _)| crate::nodes::join_node_path(&np, rel)));
        let err = match c.stat(req).await {
            Ok(entries) => {
                if !entries.first().is_some_and(|e| e.exists && e.is_dir) {
                    Some(entries.first().and_then(|e| e.error.clone()).unwrap_or_else(|| "cartella non trovata sul nodo".into()))
                } else {
                    let bad: Vec<String> = samples
                        .iter()
                        .zip(entries.iter().skip(1))
                        .filter(|((_, size), e)| !(e.exists && e.size == Some(*size as u64)))
                        .map(|((rel, _), _)| rel.clone())
                        .collect();
                    if bad.is_empty() {
                        None
                    } else {
                        Some(format!("{} file su {} assenti o diversi sul nodo (es. {})", bad.len(), samples.len(), bad[0]))
                    }
                }
            }
            Err(e) => Some(e.to_string()),
        };
        out.push(json!({ "library_path_id": p.library_path_id, "node_path": np, "ok": err.is_none(), "error": err }));
        rows.push((p.library_path_id, np, err));
    }
    let mut tx = state.db.write.begin().await?;
    sqlx::query("DELETE FROM node_paths WHERE node_id = ?").bind(id).execute(&mut *tx).await?;
    for (lp, np, err) in &rows {
        sqlx::query("INSERT INTO node_paths (node_id, library_path_id, node_path, verified_at, verify_error) VALUES (?, ?, ?, ?, ?)")
            .bind(id)
            .bind(lp)
            .bind(np)
            .bind(now_ms())
            .bind(err)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    state.nodes.reload(&state.db).await.map_err(ApiError::Internal)?;
    state
        .db
        .activity(Some(admin.user_id), "node_paths", &format!("{} ha impostato {} cartelle in copia sul nodo {}", admin.username, rows.len(), r.name), None)
        .await;
    Ok(Json(json!({ "results": out, "node": view(&state, &node(&state, id)?).await? })))
}

/// File per un nodo che transcodifica senza averlo in locale: valido finché la sessione vive.
async fn source(State(state): State<Shared>, Path(token): Path<String>, method: Method, headers: HeaderMap) -> ApiResult<Response> {
    let grant = state.nodes.sources.lock().unwrap().get(&token).cloned().ok_or(ApiError::Unauthorized)?;
    // Solo finché la sessione vive (senza rinnovarla: la tengono viva le richieste del player, non FFmpeg).
    state.sessions.get(&grant.session_id).ok_or(ApiError::Unauthorized)?;
    let info = super::playback::load_file(&state, grant.file_id).await?;
    let opts = crate::nodes::cdn::Options { sticky: state.nodes.sticky_get((0, grant.file_id)), allow_redirect: false, download: false };
    let served = crate::nodes::cdn::serve(&state, &info, &method, &headers, opts, None).await?;
    state.nodes.sticky_set((0, grant.file_id), served.node);
    Ok(served.response)
}
