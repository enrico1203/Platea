//! Dashboard admin: panoramica, sessioni, lavori, impostazioni, metadati da verificare,
//! registro delle attività, transcodifica, backup.

use super::{clamp_limit, Access, Api};
use crate::auth::AdminUser;
use crate::error::{ApiError, ApiResult};
use crate::jobs::JobView;
use crate::settings::HwBackend;
use crate::state::Shared;
use crate::transcode::command::Backend;
use crate::util::now_ms;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::Response;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

pub fn routes(api: Api) -> Api {
    api.get("/api/v1/admin/overview", "Amministrazione", Access::Admin, "Riproduzioni attive, risorse, lavori, avvisi", overview)
        .get("/api/v1/admin/sessions", "Amministrazione", Access::Admin, "Riproduzioni attive con metodo e motivo", sessions)
        .delete("/api/v1/admin/sessions/{id}", "Amministrazione", Access::Admin, "Interrompe una riproduzione", kill_session)
        .get("/api/v1/admin/jobs", "Amministrazione", Access::Admin, "Coda e cronologia dei lavori", jobs)
        .post("/api/v1/admin/jobs/{id}/retry", "Amministrazione", Access::Admin, "Nuovo tentativo di un lavoro non riuscito", retry_job)
        .delete("/api/v1/admin/jobs/failed", "Amministrazione", Access::Admin, "Svuota l'elenco dei lavori non riusciti", clear_failed)
        .get("/api/v1/admin/settings", "Impostazioni", Access::Admin, "Schema e valori di tutte le impostazioni", settings_get)
        .put("/api/v1/admin/settings/{group}", "Impostazioni", Access::Admin, "Aggiorna un gruppo di impostazioni (applicato a caldo)", settings_put)
        .get("/api/v1/admin/settings/export", "Impostazioni", Access::Admin, "Esporta la configurazione completa", settings_export)
        .post("/api/v1/admin/settings/import", "Impostazioni", Access::Admin, "Importa una configurazione esportata", settings_import)
        .get("/api/v1/admin/review", "Metadati", Access::Admin, "Elementi da verificare con i candidati", review)
        .post("/api/v1/admin/metadata/test-key", "Metadati", Access::Admin, "Verifica una chiave TMDB", test_key)
        .post("/api/v1/admin/metadata/match-pending", "Metadati", Access::Admin, "Associa tutti gli elementi in attesa", match_pending)
        .get("/api/v1/admin/activity", "Amministrazione", Access::Admin, "Registro delle attività", activity)
        .get("/api/v1/admin/history", "Amministrazione", Access::Admin, "Cronologia delle riproduzioni con metodo e motivo", history)
        .get("/api/v1/admin/transcode", "Transcodifica", Access::Admin, "FFmpeg, backend e matrice delle capacità rilevate", transcode_info)
        .post("/api/v1/admin/transcode/test", "Transcodifica", Access::Admin, "Prova ora: rileva di nuovo le GPU", transcode_test)
        .get("/api/v1/admin/system", "Sistema", Access::Admin, "Versione, percorsi, database", system)
        .post("/api/v1/admin/backup", "Sistema", Access::Admin, "Copia di sicurezza del database da scaricare", backup)
        .get("/api/v1/admin/api-keys", "Sistema", Access::Admin, "Chiavi API per le integrazioni", api_keys)
        .post("/api/v1/admin/api-keys", "Sistema", Access::Admin, "Crea una chiave API (mostrata una sola volta)", api_key_create)
        .delete("/api/v1/admin/api-keys/{id}", "Sistema", Access::Admin, "Revoca una chiave API", api_key_delete)
        .get("/api/v1/admin/logs", "Sistema", Access::Admin, "Ultime righe del log (le nuove arrivano sul WebSocket come log.line)", logs)
}

async fn overview(State(state): State<Shared>, _a: AdminUser) -> ApiResult<Json<Value>> {
    let mut sess = Vec::new();
    let mut all = state.sessions.all();
    all.sort_by_key(|s| crate::sessions::method_rank(s.decision.method));
    let mut bandwidth: i64 = 0;
    for s in &all {
        bandwidth += s.bitrate.unwrap_or(0);
        sess.push(crate::sessions::session_view(&state, s).await);
    }
    let (queued, running, failed): (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM jobs WHERE state = 'queued'), (SELECT COUNT(*) FROM jobs WHERE state = 'running'), (SELECT COUNT(*) FROM jobs WHERE state = 'failed')",
    )
    .fetch_one(&state.db.read)
    .await?;
    let (movies, series, episodes, files, review): (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM items WHERE kind='movie'), (SELECT COUNT(*) FROM items WHERE kind='series'), (SELECT COUNT(*) FROM items WHERE kind='episode'),
                (SELECT COUNT(*) FROM media_files WHERE state = 'present'), (SELECT COUNT(*) FROM items WHERE match_state IN ('review','unmatched'))",
    )
    .fetch_one(&state.db.read)
    .await?;
    let mut alerts: Vec<Value> = Vec::new();
    let mount_missing: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT name, last_scan_message FROM libraries WHERE last_scan_status = 'mount_missing'").fetch_all(&state.db.read).await?;
    for (name, msg) in mount_missing {
        alerts.push(json!({ "level": "danger", "code": "MountMissing", "message": format!("Libreria {name}: {}", msg.unwrap_or_default()) }));
    }
    if !state.ffmpeg.get().available {
        alerts.push(json!({ "level": "danger", "code": "FfmpegMissing", "message": "FFmpeg non trovato: analisi e transcodifica non disponibili" }));
    }
    if state.settings.get().metadata.tmdb_api_key.is_empty() {
        alerts.push(json!({ "level": "warning", "code": "TmdbKeyMissing", "message": "Chiave TMDB non impostata: i metadati non vengono scaricati" }));
    }
    if review > 0 {
        alerts.push(json!({ "level": "warning", "code": "MetadataReview", "message": format!("{review} elementi da verificare") }));
    }
    let (hw_used, hw_max, sw_used, sw_max) = state.transcoder.active_counts();
    let mem = read_memory();
    let load = std::fs::read_to_string("/proc/loadavg").ok().and_then(|l| l.split_whitespace().next().and_then(|x| x.parse::<f64>().ok()));
    Ok(Json(json!({
        "sessions": sess,
        "bandwidth_bps": bandwidth,
        "counts": {
            "direct_play": all.iter().filter(|s| s.decision.method == platea_playback::Method::DirectPlay).count(),
            "remux": all.iter().filter(|s| s.decision.method == platea_playback::Method::Remux).count(),
            "transcode": all.iter().filter(|s| s.decision.method == platea_playback::Method::Transcode).count(),
        },
        "transcode_slots": { "hw_used": hw_used, "hw_max": hw_max, "sw_used": sw_used, "sw_max": sw_max },
        "jobs": { "queued": queued, "running": running, "failed": failed },
        "library": { "movies": movies, "series": series, "episodes": episodes, "files": files, "review": review },
        "system": { "memory_rss_bytes": mem, "load_1m": load, "cpus": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1), "uptime_ms": now_ms() - state.started_at },
        "alerts": alerts,
    })))
}

fn read_memory() -> Option<u64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = s.lines().find(|l| l.starts_with("VmRSS:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}

async fn sessions(State(state): State<Shared>, _a: AdminUser) -> ApiResult<Json<Value>> {
    let mut all = state.sessions.all();
    all.sort_by_key(|s| crate::sessions::method_rank(s.decision.method));
    let mut out = Vec::new();
    for s in &all {
        out.push(crate::sessions::session_view(&state, s).await);
    }
    Ok(Json(json!({ "items": out })))
}

async fn kill_session(State(state): State<Shared>, AdminUser(admin): AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let s = crate::sessions::stop_session(&state, &id).await.ok_or_else(|| ApiError::not_found("Sessione"))?;
    state.events.to_user(s.user_id, "playback.stopped", json!({ "session_id": id, "by": "admin" }));
    state.db.activity(Some(admin.user_id), "session_killed", &format!("{} ha interrotto la riproduzione di {}", admin.username, s.username), None).await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct JobQuery {
    state: Option<String>,
    limit: Option<i64>,
    before: Option<i64>,
}

async fn jobs(State(state): State<Shared>, _a: AdminUser, Query(q): Query<JobQuery>) -> ApiResult<Json<Value>> {
    let limit = clamp_limit(q.limit, 100);
    let st = q.state.unwrap_or_else(|| "all".into());
    let filter = match st.as_str() {
        "queued" | "running" | "done" | "failed" => format!("WHERE state = '{st}'"),
        _ => "WHERE 1".into(),
    };
    let before = q.before.map(|b| format!(" AND id < {b}")).unwrap_or_default();
    let order = if st == "queued" { "priority, run_after, id" } else { "id DESC" };
    let rows: Vec<(i64, String, String, i64, String, i64, i64, i64, Option<i64>, Option<i64>, Option<String>)> = sqlx::query_as(&format!(
        "SELECT id, kind, payload, priority, state, attempts, run_after, created_at, started_at, finished_at, error FROM jobs {filter}{before} ORDER BY {order} LIMIT {limit}"
    ))
    .fetch_all(&state.db.read)
    .await?;
    let items: Vec<JobView> = rows
        .into_iter()
        .map(|(id, kind, payload, priority, st, attempts, run_after, created_at, started_at, finished_at, error)| JobView {
            id,
            kind,
            payload: serde_json::from_str(&payload).unwrap_or(Value::Null),
            priority,
            state: st,
            attempts,
            run_after,
            created_at,
            started_at,
            finished_at,
            error,
        })
        .collect();
    let counts: Vec<(String, i64)> = sqlx::query_as("SELECT state, COUNT(*) FROM jobs GROUP BY state").fetch_all(&state.db.read).await?;
    Ok(Json(json!({ "items": items, "counts": counts.into_iter().collect::<std::collections::HashMap<_, _>>() })))
}

async fn retry_job(State(state): State<Shared>, _a: AdminUser, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    sqlx::query("UPDATE jobs SET state = 'queued', attempts = 0, run_after = ?, error = NULL, finished_at = NULL WHERE id = ? AND state = 'failed'")
        .bind(now_ms())
        .bind(id)
        .execute(&state.db.write)
        .await?;
    state.jobs.wake();
    Ok(Json(json!({ "ok": true })))
}

async fn clear_failed(State(state): State<Shared>, _a: AdminUser) -> ApiResult<Json<Value>> {
    let n = sqlx::query("DELETE FROM jobs WHERE state = 'failed'").execute(&state.db.write).await?.rows_affected();
    Ok(Json(json!({ "deleted": n })))
}

async fn settings_get(State(state): State<Shared>, _a: AdminUser) -> ApiResult<Json<Value>> {
    Ok(Json(json!({ "schema": crate::settings::Settings::schema(), "values": state.settings.values_public(), "groups": crate::settings::GROUPS })))
}

async fn settings_put(State(state): State<Shared>, AdminUser(admin): AdminUser, Path(group): Path<String>, Json(v): Json<Value>) -> ApiResult<Json<Value>> {
    let before_key = state.settings.get().metadata.tmdb_api_key.clone();
    let (prev, next) = state.settings.update(&group, v).await.map_err(ApiError::BadRequest)?;
    state.db.activity(Some(admin.user_id), "settings", &format!("{} ha modificato le impostazioni {group}: {prev} → {next}", admin.username), None).await;
    after_settings_change(&state, &group, &before_key).await;
    Ok(Json(json!({ "values": state.settings.values_public() })))
}

/// Effetti a caldo dei cambi di impostazioni.
async fn after_settings_change(state: &Shared, group: &str, before_key: &str) {
    let s = state.settings.get();
    match group {
        "transcode" => {
            state.transcoder.apply_limits(s.transcode.max_hw_transcodes, s.transcode.max_sw_transcodes);
            let f = state.ffmpeg.clone();
            let (a, b) = (s.transcode.ffmpeg_path.clone(), s.transcode.ffprobe_path.clone());
            let _ = tokio::task::spawn_blocking(move || f.resolve(&a, &b)).await;
            let st = state.clone();
            tokio::spawn(async move { run_hw_detection(&st).await });
        }
        "metadata" if s.metadata.tmdb_api_key != before_key && !s.metadata.tmdb_api_key.is_empty() => {
            if let Ok(n) = crate::metadata::enqueue_pending(state).await {
                tracing::info!(items = n, "chiave TMDB impostata: associazione degli elementi in attesa");
            }
        }
        _ => {}
    }
}

async fn settings_export(State(state): State<Shared>, _a: AdminUser) -> ApiResult<Response> {
    let body = serde_json::to_vec_pretty(&json!({ "platea": crate::state::VERSION, "exported_at": now_ms(), "settings": state.settings.export() }))
        .unwrap_or_default();
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CONTENT_DISPOSITION, "attachment; filename=\"platea-config.json\"")
        .body(Body::from(body))
        .unwrap())
}

async fn settings_import(State(state): State<Shared>, AdminUser(admin): AdminUser, Json(v): Json<Value>) -> ApiResult<Json<Value>> {
    let settings = v.get("settings").cloned().unwrap_or(v);
    let Value::Object(groups) = settings else { return Err(ApiError::BadRequest("File di configurazione non valido".into())) };
    let before_key = state.settings.get().metadata.tmdb_api_key.clone();
    let mut applied = Vec::new();
    for (g, val) in groups {
        if crate::settings::GROUPS.contains(&g.as_str()) {
            state.settings.update(&g, val).await.map_err(|e| ApiError::BadRequest(format!("{g}: {e}")))?;
            after_settings_change(&state, &g, &before_key).await;
            applied.push(g);
        }
    }
    state
        .db
        .activity(Some(admin.user_id), "settings_import", &format!("{} ha importato la configurazione ({})", admin.username, applied.join(", ")), None)
        .await;
    Ok(Json(json!({ "applied": applied })))
}

async fn review(State(state): State<Shared>, _a: AdminUser) -> ApiResult<Json<Value>> {
    let rows: Vec<(i64, String, String, Option<i64>, String, Option<f64>, String)> = sqlx::query_as(
        "SELECT i.id, i.kind, i.title, i.year, i.match_state, i.match_score, l.name FROM items i JOIN libraries l ON l.id = i.library_id
         WHERE i.match_state IN ('review','unmatched') AND i.kind IN ('movie','series') ORDER BY i.added_at DESC LIMIT 500",
    )
    .fetch_all(&state.db.read)
    .await?;
    let mut out = Vec::new();
    for (id, kind, title, year, ms, score, lib) in rows {
        let file: Option<String> =
            sqlx::query_scalar("SELECT f.relative_path FROM media_files f JOIN items e ON e.id = f.item_id WHERE e.id = ?1 OR e.series_id = ?1 LIMIT 1")
                .bind(id)
                .fetch_optional(&state.db.read)
                .await?;
        let cands: Vec<(String, String, Option<i64>, f64, Option<String>)> =
            sqlx::query_as("SELECT provider_id, title, year, score, poster_path FROM match_candidates WHERE item_id = ? ORDER BY score DESC LIMIT 5")
                .bind(id)
                .fetch_all(&state.db.read)
                .await?;
        out.push(json!({
            "id": state.ids.encode(id),
            "kind": kind,
            "title": title,
            "year": year,
            "match_state": ms,
            "match_score": score,
            "library": lib,
            "path": file,
            "candidates": cands.into_iter().map(|(p, t, y, s, pp)| json!({
                "provider_id": p, "title": t, "year": y, "score": s,
                "poster": pp.map(|x| format!("{}/w185{x}", crate::metadata::tmdb::IMAGE_BASE.as_str())),
            })).collect::<Vec<_>>(),
        }));
    }
    Ok(Json(json!({ "items": out })))
}

#[derive(Deserialize)]
struct KeyReq {
    key: Option<String>,
}

async fn test_key(State(state): State<Shared>, _a: AdminUser, Json(req): Json<KeyReq>) -> ApiResult<Json<Value>> {
    let key = req.key.filter(|k| !k.is_empty()).unwrap_or_else(|| state.settings.get().metadata.tmdb_api_key.clone());
    if key.is_empty() {
        return Err(ApiError::BadRequest("Nessuna chiave da verificare".into()));
    }
    match state.tmdb.check_key(&key).await {
        Ok(()) => Ok(Json(json!({ "ok": true }))),
        Err(e) => Ok(Json(json!({ "ok": false, "message": format!("{e:#}") }))),
    }
}

async fn match_pending(State(state): State<Shared>, _a: AdminUser) -> ApiResult<Json<Value>> {
    let n = crate::metadata::enqueue_pending(&state).await?;
    Ok(Json(json!({ "queued": n })))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct PageQuery {
    limit: Option<i64>,
    before: Option<i64>,
}

async fn activity(State(state): State<Shared>, _a: AdminUser, Query(q): Query<PageQuery>) -> ApiResult<Json<Value>> {
    let limit = clamp_limit(q.limit, 100);
    let rows: Vec<(i64, i64, Option<i64>, String, String, Option<String>, Option<String>)> = sqlx::query_as(&format!(
        "SELECT a.id, a.at, a.user_id, a.kind, a.message, a.ip, u.username FROM activity_log a LEFT JOIN users u ON u.id = a.user_id
         WHERE a.id < ? ORDER BY a.id DESC LIMIT {limit}"
    ))
    .bind(q.before.unwrap_or(i64::MAX))
    .fetch_all(&state.db.read)
    .await?;
    Ok(Json(
        json!({ "items": rows.into_iter().map(|(id, at, _uid, kind, msg, ip, user)| json!({ "id": id, "at": at, "kind": kind, "message": msg, "ip": ip, "user": user })).collect::<Vec<_>>() }),
    ))
}

async fn history(State(state): State<Shared>, _a: AdminUser, Query(q): Query<PageQuery>) -> ApiResult<Json<Value>> {
    let limit = clamp_limit(q.limit, 100);
    let rows: Vec<(i64, Option<String>, Option<String>, Option<String>, Option<String>, String, Option<String>, i64, Option<i64>, i64)> =
        sqlx::query_as(&format!(
            "SELECT h.id, u.username, i.title, h.client, h.device_name, h.method, h.reason, h.started_at, h.ended_at, h.watched_ms
         FROM playback_history h LEFT JOIN users u ON u.id = h.user_id LEFT JOIN items i ON i.id = h.item_id
         WHERE h.id < ? ORDER BY h.id DESC LIMIT {limit}"
        ))
        .bind(q.before.unwrap_or(i64::MAX))
        .fetch_all(&state.db.read)
        .await?;
    let stats: Vec<(String, i64)> = sqlx::query_as("SELECT method, COUNT(*) FROM playback_history WHERE started_at > ? GROUP BY method")
        .bind(now_ms() - 30 * 86_400_000)
        .fetch_all(&state.db.read)
        .await?;
    Ok(Json(json!({
        "items": rows.into_iter().map(|(id, u, t, c, d, m, r, s, e, w)| json!({ "id": id, "user": u, "title": t, "client": c, "device_name": d, "method": m, "reasons": r.map(|x| x.split(',').map(String::from).collect::<Vec<_>>()).unwrap_or_default(), "started_at": s, "ended_at": e, "watched_ms": w })).collect::<Vec<_>>(),
        "last_30_days": stats.into_iter().collect::<std::collections::HashMap<_, _>>(),
    })))
}

/// Prova reale delle GPU: all'avvio e su richiesta dalla dashboard.
pub async fn run_hw_detection(state: &Shared) {
    let f = state.ffmpeg.get();
    if !f.available {
        return;
    }
    let s = state.settings.get();
    let forced = match s.transcode.hw_backend {
        HwBackend::Auto => None,
        HwBackend::None => Some(Backend::Software),
        HwBackend::Qsv => Some(Backend::Qsv),
        HwBackend::Vaapi => Some(Backend::Vaapi),
        HwBackend::Nvenc => Some(Backend::Nvenc),
        HwBackend::Amf => Some(Backend::Amf),
    };
    let mut caps = crate::transcode::hw::detect(&f.ffmpeg, &s.transcode.vaapi_device, forced, &state.boot.cache_dir).await;
    caps.ffmpeg_version = f.version.clone();
    if let Ok(j) = serde_json::to_string(&caps) {
        let _ = state.db.state_set("hw_capabilities", &j).await;
    }
    *state.transcoder.caps.write().unwrap() = caps;
    state.transcoder.reset_failures();
    state.events.admin("transcode.capabilities", json!({}));
}

async fn transcode_info(State(state): State<Shared>, _a: AdminUser) -> ApiResult<Json<Value>> {
    let caps = state.transcoder.caps.read().unwrap().clone();
    let (hw_used, hw_max, sw_used, sw_max) = state.transcoder.active_counts();
    Ok(Json(json!({
        "ffmpeg": *state.ffmpeg.get(),
        "capabilities": caps,
        "current": state.transcoder.current_profile(),
        "slots": { "hw_used": hw_used, "hw_max": hw_max, "sw_used": sw_used, "sw_max": sw_max },
    })))
}

async fn transcode_test(State(state): State<Shared>, _a: AdminUser) -> ApiResult<Json<Value>> {
    run_hw_detection(&state).await;
    transcode_info(State(state), _a).await
}

async fn system(State(state): State<Shared>, _a: AdminUser) -> ApiResult<Json<Value>> {
    let db_size = std::fs::metadata(state.boot.db_path()).map(|m| m.len()).unwrap_or(0);
    Ok(Json(json!({
        "version": crate::state::VERSION,
        "server_id": state.server_id,
        "data_dir": state.boot.data_dir,
        "cache_dir": state.boot.cache_dir,
        "transcode_dir": state.boot.transcode_dir,
        "database_bytes": db_size,
        "started_at": state.started_at,
        "ffmpeg": *state.ffmpeg.get(),
    })))
}

async fn api_keys(State(state): State<Shared>, _a: AdminUser) -> ApiResult<Json<Value>> {
    let rows: Vec<(i64, String, String, i64, Option<i64>)> =
        sqlx::query_as("SELECT id, name, scopes, created_at, last_used_at FROM api_keys ORDER BY created_at DESC").fetch_all(&state.db.read).await?;
    Ok(Json(json!({
        "items": rows.into_iter().map(|(id, name, scopes, c, l)| json!({ "id": state.ids.encode(id), "name": name, "scopes": scopes.split(',').collect::<Vec<_>>(), "created_at": c, "last_used_at": l })).collect::<Vec<_>>(),
        "available_scopes": crate::auth::API_SCOPES,
    })))
}

#[derive(Deserialize)]
struct NewKey {
    name: String,
    scopes: Vec<String>,
}

async fn api_key_create(State(state): State<Shared>, AdminUser(admin): AdminUser, Json(req): Json<NewKey>) -> ApiResult<Json<Value>> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(ApiError::BadRequest("Serve un nome".into()));
    }
    let scopes: Vec<&str> = req.scopes.iter().map(|s| s.as_str()).filter(|s| crate::auth::API_SCOPES.contains(s)).collect();
    if scopes.is_empty() {
        return Err(ApiError::BadRequest("Serve almeno un ambito valido".into()));
    }
    let key = crate::util::random_hex(24);
    let id = sqlx::query("INSERT INTO api_keys (name, key_hash, scopes, created_at) VALUES (?, ?, ?, ?)")
        .bind(name)
        .bind(crate::util::sha256_hex(&key))
        .bind(scopes.join(","))
        .bind(now_ms())
        .execute(&state.db.write)
        .await?
        .last_insert_rowid();
    state.db.activity(Some(admin.user_id), "api_key_created", &format!("{} ha creato la chiave API {name}", admin.username), None).await;
    Ok(Json(json!({ "id": state.ids.encode(id), "key": key })))
}

async fn api_key_delete(State(state): State<Shared>, AdminUser(admin): AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let kid = state.ids.decode(&id).ok_or_else(|| ApiError::not_found("Chiave"))?;
    sqlx::query("DELETE FROM api_keys WHERE id = ?").bind(kid).execute(&state.db.write).await?;
    state.db.activity(Some(admin.user_id), "api_key_deleted", &format!("{} ha revocato una chiave API", admin.username), None).await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct LogQuery {
    level: Option<String>,
    limit: Option<usize>,
}

async fn logs(_a: AdminUser, Query(q): Query<LogQuery>) -> ApiResult<Json<Value>> {
    let min = crate::logs::level_rank(&q.level.unwrap_or_else(|| "INFO".into()).to_uppercase());
    Ok(Json(json!({ "items": crate::logs::LOGS.recent(min, q.limit.unwrap_or(500).min(2000)) })))
}

/// Copia coerente del database (VACUUM INTO) restituita come file da scaricare.
async fn backup(State(state): State<Shared>, AdminUser(admin): AdminUser) -> ApiResult<Response> {
    let path = state.boot.data_dir.join(format!("backup-{}.db", now_ms()));
    sqlx::query(&format!("VACUUM INTO '{}'", path.display().to_string().replace('\'', "''"))).execute(&state.db.write).await?;
    let bytes = tokio::fs::read(&path).await.map_err(|e| ApiError::Internal(e.into()))?;
    let _ = tokio::fs::remove_file(&path).await;
    state.db.activity(Some(admin.user_id), "backup", &format!("{} ha scaricato una copia del database", admin.username), None).await;
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "application/vnd.sqlite3")
        .header(header::CONTENT_DISPOSITION, "attachment; filename=\"platea-backup.db\"")
        .body(Body::from(bytes))
        .unwrap())
}
