//! Librerie: percorsi, tipo, profilo Locale o Remoto, pianificazione, limiti dell'analisi.

use super::{Access, Api};
use crate::auth::{AdminUser, AuthUser, ScanAccess};
use crate::error::{ApiError, ApiResult};
use crate::jobs::{self, priority, JobKind};
use crate::state::Shared;
use crate::util::now_ms;
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;

pub fn routes(api: Api) -> Api {
    api.get("/api/v1/libraries", "Librerie", Access::User, "Librerie visibili all'utente", list)
        .post("/api/v1/libraries", "Librerie", Access::Admin, "Crea una libreria", create)
        .get("/api/v1/libraries/{id}", "Librerie", Access::User, "Dettaglio di una libreria", get_one)
        .patch("/api/v1/libraries/{id}", "Librerie", Access::Admin, "Modifica una libreria", update)
        .delete("/api/v1/libraries/{id}", "Librerie", Access::Admin, "Elimina una libreria (i file non vengono toccati)", remove)
        .post("/api/v1/libraries/{id}/scan", "Librerie", Access::Admin, "Avvia la scansione di una libreria o di una sua cartella", scan)
        .post(
            "/api/v1/libraries/{id}/confirm-missing",
            "Librerie",
            Access::Admin,
            "Conferma che i file spariti tutti insieme sono stati cancellati davvero e avvia la scansione che li toglie",
            confirm_missing,
        )
        .post(
            "/api/v1/libraries/{id}/refresh-metadata",
            "Librerie",
            Access::Admin,
            "Rilegge i metadati di tutta la libreria (TMDB, NFO, .plexmatch); i campi bloccati restano",
            refresh_metadata,
        )
        .post("/api/v1/libraries/scan-all", "Librerie", Access::Admin, "Avvia la scansione di tutte le librerie", scan_all)
        .post("/api/v1/scan/path", "Librerie", Access::Admin, "Scansione mirata della cartella cambiata (per strumenti esterni)", scan_path)
        .get("/api/v1/fs/browse", "Librerie", Access::Admin, "Sfoglia le cartelle visibili al server", browse)
}

#[derive(sqlx::FromRow)]
struct LibRow {
    id: i64,
    name: String,
    kind: String,
    scan_profile: String,
    language: String,
    country: String,
    scan_interval_minutes: Option<i64>,
    probe_concurrency: i64,
    probe_per_minute: Option<i64>,
    probe_window_start: Option<i64>,
    probe_window_end: Option<i64>,
    probe_pause_on_play: i64,
    extract_keyframes: i64,
    trickplay: i64,
    created_at: i64,
    last_scan_at: Option<i64>,
    last_scan_status: Option<String>,
    last_scan_message: Option<String>,
    missing_held: i64,
}

const COLS: &str = "id, name, kind, scan_profile, language, country, scan_interval_minutes, probe_concurrency, probe_per_minute, probe_window_start, probe_window_end, probe_pause_on_play, extract_keyframes, trickplay, created_at, last_scan_at, last_scan_status, last_scan_message, missing_held";

async fn view(state: &Shared, l: LibRow, admin: bool) -> ApiResult<Value> {
    let paths: Vec<String> =
        sqlx::query_scalar("SELECT path FROM library_paths WHERE library_id = ? ORDER BY path").bind(l.id).fetch_all(&state.db.read).await?;
    let (items, files): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM items WHERE library_id = ?1 AND kind IN ('movie','series')),
                (SELECT COUNT(*) FROM media_files f JOIN library_paths p ON p.id = f.library_path_id WHERE p.library_id = ?1)",
    )
    .bind(l.id)
    .fetch_one(&state.db.read)
    .await?;
    let mut v = json!({
        "id": state.ids.encode(l.id),
        "name": l.name,
        "kind": l.kind,
        "item_count": items,
    });
    if admin {
        let pending: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM media_files f JOIN library_paths p ON p.id = f.library_path_id WHERE p.library_id = ? AND f.probe_state = 'pending'",
        )
        .bind(l.id)
        .fetch_one(&state.db.read)
        .await?;
        let o = v.as_object_mut().unwrap();
        o.insert("paths".into(), json!(paths));
        o.insert("scan_profile".into(), json!(l.scan_profile));
        o.insert("language".into(), json!(l.language));
        o.insert("country".into(), json!(l.country));
        o.insert("scan_interval_minutes".into(), json!(l.scan_interval_minutes));
        o.insert("probe_concurrency".into(), json!(l.probe_concurrency));
        o.insert("probe_per_minute".into(), json!(l.probe_per_minute));
        o.insert("probe_window_start".into(), json!(l.probe_window_start));
        o.insert("probe_window_end".into(), json!(l.probe_window_end));
        o.insert("probe_pause_on_play".into(), json!(l.probe_pause_on_play != 0));
        o.insert("extract_keyframes".into(), json!(l.extract_keyframes != 0));
        o.insert("trickplay".into(), json!(l.trickplay != 0));
        if l.trickplay != 0 {
            let done: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM trickplay t JOIN media_files f ON f.id = t.file_id JOIN library_paths p ON p.id = f.library_path_id WHERE p.library_id = ?",
            )
            .bind(l.id)
            .fetch_one(&state.db.read)
            .await?;
            o.insert("trickplay_done".into(), json!(done));
        }
        o.insert("created_at".into(), json!(l.created_at));
        o.insert("last_scan_at".into(), json!(l.last_scan_at));
        o.insert("last_scan_status".into(), json!(l.last_scan_status));
        o.insert("last_scan_message".into(), json!(l.last_scan_message));
        o.insert("missing_held".into(), json!(l.missing_held));
        o.insert("file_count".into(), json!(files));
        o.insert("probe_pending".into(), json!(pending));
    }
    Ok(v)
}

async fn list(State(state): State<Shared>, user: AuthUser) -> ApiResult<Json<Value>> {
    let rows: Vec<LibRow> =
        sqlx::query_as(&format!("SELECT {COLS} FROM libraries WHERE 1 {} ORDER BY name", user.library_filter("id"))).fetch_all(&state.db.read).await?;
    let mut out = Vec::new();
    for r in rows {
        out.push(view(&state, r, user.is_admin).await?);
    }
    Ok(Json(json!({ "items": out })))
}

fn decode(state: &Shared, id: &str) -> ApiResult<i64> {
    state.ids.decode(id).ok_or_else(|| ApiError::not_found("Libreria"))
}

async fn get_one(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let lid = decode(&state, &id)?;
    if !user.can_see_library(lid) {
        return Err(ApiError::not_found("Libreria"));
    }
    let r: LibRow = sqlx::query_as(&format!("SELECT {COLS} FROM libraries WHERE id = ?"))
        .bind(lid)
        .fetch_optional(&state.db.read)
        .await?
        .ok_or_else(|| ApiError::not_found("Libreria"))?;
    Ok(Json(view(&state, r, user.is_admin).await?))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct LibPatch {
    name: Option<String>,
    kind: Option<String>,
    paths: Option<Vec<String>>,
    scan_profile: Option<String>,
    language: Option<String>,
    country: Option<String>,
    #[serde(deserialize_with = "crate::util::double_option")]
    scan_interval_minutes: Option<Option<i64>>,
    probe_concurrency: Option<i64>,
    #[serde(deserialize_with = "crate::util::double_option")]
    probe_per_minute: Option<Option<i64>>,
    #[serde(deserialize_with = "crate::util::double_option")]
    probe_window_start: Option<Option<i64>>,
    #[serde(deserialize_with = "crate::util::double_option")]
    probe_window_end: Option<Option<i64>>,
    probe_pause_on_play: Option<bool>,
    extract_keyframes: Option<bool>,
    /// Miniature sulla barra di avanzamento; `true` mette in coda i file che non le hanno ancora.
    trickplay: Option<bool>,
    scan_now: Option<bool>,
}

/// Una cartella di libreria deve essere assoluta, esistente e una directory.
async fn validate_path(p: &str) -> ApiResult<String> {
    let path = PathBuf::from(p.trim());
    if !path.is_absolute() {
        return Err(ApiError::BadRequest(format!("Il percorso deve essere assoluto: {p}")));
    }
    if path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err(ApiError::BadRequest("Il percorso non può contenere ..".into()));
    }
    let check = path.clone();
    let is_dir = tokio::time::timeout(std::time::Duration::from_secs(10), tokio::task::spawn_blocking(move || check.is_dir()))
        .await
        .map_err(|_| ApiError::BadRequest(format!("{p} non risponde: mount bloccato?")))?
        .unwrap_or(false);
    if !is_dir {
        return Err(ApiError::BadRequest(format!("{p} non esiste o non è una cartella visibile al server")));
    }
    Ok(path.to_string_lossy().trim_end_matches('/').to_string().chars().collect::<String>().replace("//", "/"))
}

async fn create(State(state): State<Shared>, AdminUser(admin): AdminUser, Json(p): Json<LibPatch>) -> ApiResult<Json<Value>> {
    let name = p.name.clone().map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).ok_or_else(|| ApiError::BadRequest("Serve un nome".into()))?;
    let kind = p.kind.clone().unwrap_or_else(|| "movies".into());
    if kind != "movies" && kind != "shows" {
        return Err(ApiError::BadRequest("Tipo di libreria: movies oppure shows".into()));
    }
    let paths = p.paths.clone().unwrap_or_default();
    if paths.is_empty() {
        return Err(ApiError::BadRequest("Serve almeno una cartella".into()));
    }
    let mut valid = Vec::new();
    for x in &paths {
        valid.push(validate_path(x).await?);
    }
    let profile = p.scan_profile.clone().unwrap_or_else(|| "local".into());
    let remote = profile == "remote";
    let s = state.settings.get();
    let id = sqlx::query(
        "INSERT INTO libraries (name, kind, scan_profile, language, country, scan_interval_minutes, probe_concurrency, extract_keyframes, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&name)
    .bind(&kind)
    .bind(if remote { "remote" } else { "local" })
    .bind(p.language.clone().unwrap_or_else(|| s.metadata.language.clone()))
    .bind(p.country.clone().unwrap_or_else(|| s.metadata.country.clone()))
    // Profili predefiniti: Locale scansiona ogni ora e analizza 4 file alla volta; Remoto ogni 6 ore, 1 alla volta.
    .bind(if remote { 360 } else { 60 })
    .bind(if remote { 1 } else { 4 })
    .bind(if remote { 0 } else { 1 })
    .bind(now_ms())
    .execute(&state.db.write)
    .await?
    .last_insert_rowid();
    for path in valid {
        sqlx::query("INSERT OR IGNORE INTO library_paths (library_id, path) VALUES (?, ?)").bind(id).bind(path).execute(&state.db.write).await?;
    }
    // Remota: le opzioni che leggono i file per intero partono sempre spente; si accendono dopo, modificando la libreria.
    let p = if remote { LibPatch { extract_keyframes: Some(false), trickplay: Some(false), ..p } } else { p };
    apply_patch(&state, id, LibPatch { paths: None, name: None, kind: None, ..p }).await?;
    state.db.activity(Some(admin.user_id), "library_created", &format!("{} ha creato la libreria {name}", admin.username), None).await;
    jobs::enqueue(&state, JobKind::ScanLibrary { library_id: id, subpath: None }, priority::INTERACTIVE, Some(id)).await?;
    let r: LibRow = sqlx::query_as(&format!("SELECT {COLS} FROM libraries WHERE id = ?")).bind(id).fetch_one(&state.db.read).await?;
    Ok(Json(view(&state, r, true).await?))
}

async fn apply_patch(state: &Shared, id: i64, p: LibPatch) -> ApiResult<()> {
    if let Some(n) = p.name.as_ref().map(|n| n.trim()).filter(|n| !n.is_empty()) {
        sqlx::query("UPDATE libraries SET name = ? WHERE id = ?").bind(n).bind(id).execute(&state.db.write).await?;
    }
    if let Some(k) = p.kind.as_deref() {
        change_kind(state, id, k).await?;
    }
    if let Some(sp) = &p.scan_profile {
        if sp != "local" && sp != "remote" {
            return Err(ApiError::BadRequest("Profilo: local oppure remote".into()));
        }
        sqlx::query("UPDATE libraries SET scan_profile = ? WHERE id = ?").bind(sp).bind(id).execute(&state.db.write).await?;
    }
    if let Some(v) = &p.language {
        sqlx::query("UPDATE libraries SET language = ? WHERE id = ?").bind(v).bind(id).execute(&state.db.write).await?;
    }
    if let Some(v) = &p.country {
        sqlx::query("UPDATE libraries SET country = ? WHERE id = ?").bind(v).bind(id).execute(&state.db.write).await?;
    }
    if let Some(v) = p.scan_interval_minutes {
        sqlx::query("UPDATE libraries SET scan_interval_minutes = ? WHERE id = ?").bind(v.filter(|m| *m > 0)).bind(id).execute(&state.db.write).await?;
    }
    if let Some(v) = p.probe_concurrency {
        sqlx::query("UPDATE libraries SET probe_concurrency = ? WHERE id = ?").bind(v.clamp(1, 32)).bind(id).execute(&state.db.write).await?;
    }
    if let Some(v) = p.probe_per_minute {
        sqlx::query("UPDATE libraries SET probe_per_minute = ? WHERE id = ?").bind(v.filter(|m| *m > 0)).bind(id).execute(&state.db.write).await?;
    }
    if let Some(v) = p.probe_window_start {
        sqlx::query("UPDATE libraries SET probe_window_start = ? WHERE id = ?").bind(v.map(|h| h.clamp(0, 23))).bind(id).execute(&state.db.write).await?;
    }
    if let Some(v) = p.probe_window_end {
        sqlx::query("UPDATE libraries SET probe_window_end = ? WHERE id = ?").bind(v.map(|h| h.clamp(0, 23))).bind(id).execute(&state.db.write).await?;
    }
    if let Some(v) = p.probe_pause_on_play {
        sqlx::query("UPDATE libraries SET probe_pause_on_play = ? WHERE id = ?").bind(v as i64).bind(id).execute(&state.db.write).await?;
    }
    if let Some(v) = p.extract_keyframes {
        sqlx::query("UPDATE libraries SET extract_keyframes = ? WHERE id = ?").bind(v as i64).bind(id).execute(&state.db.write).await?;
    }
    if let Some(v) = p.trickplay {
        sqlx::query("UPDATE libraries SET trickplay = ? WHERE id = ?").bind(v as i64).bind(id).execute(&state.db.write).await?;
        if v {
            crate::trickplay::enqueue_library(state, id).await.map_err(ApiError::Internal)?;
        } else {
            // Spente: i lavori in coda si tolgono, le miniature già fatte restano.
            sqlx::query("DELETE FROM jobs WHERE kind = 'generate_trickplay' AND state = 'queued' AND library_id = ?").bind(id).execute(&state.db.write).await?;
        }
    }
    if let Some(paths) = p.paths {
        let mut valid = Vec::new();
        for x in &paths {
            valid.push(validate_path(x).await?);
        }
        if valid.is_empty() {
            return Err(ApiError::BadRequest("Serve almeno una cartella".into()));
        }
        // Le cartelle rimosse portano con sé i loro file (gli elementi restano fino alla pulizia).
        let current: Vec<(i64, String)> = sqlx::query_as("SELECT id, path FROM library_paths WHERE library_id = ?").bind(id).fetch_all(&state.db.read).await?;
        for (pid, path) in current {
            if !valid.contains(&path) {
                sqlx::query("DELETE FROM library_paths WHERE id = ?").bind(pid).execute(&state.db.write).await?;
            }
        }
        for path in valid {
            sqlx::query("INSERT OR IGNORE INTO library_paths (library_id, path) VALUES (?, ?)").bind(id).bind(path).execute(&state.db.write).await?;
        }
    }
    if p.scan_now == Some(true) {
        jobs::enqueue(state, JobKind::ScanLibrary { library_id: id, subpath: None }, priority::INTERACTIVE, Some(id)).await?;
    }
    Ok(())
}

/// Cambio di tipo (film ↔ serie): i titoli riconosciuti col tipo sbagliato si cancellano e la scansione successiva
/// li ricrea dagli stessi file. Le righe dei file restano, con l'analisi già fatta: non si rilegge nulla dal disco.
async fn change_kind(state: &Shared, id: i64, kind: &str) -> ApiResult<()> {
    if kind != "movies" && kind != "shows" {
        return Err(ApiError::BadRequest("Tipo di libreria: movies oppure shows".into()));
    }
    let current: String = sqlx::query_scalar("SELECT kind FROM libraries WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.db.read)
        .await?
        .ok_or_else(|| ApiError::not_found("Libreria"))?;
    if current == kind {
        return Ok(());
    }
    let mut tx = state.db.write.begin().await?;
    sqlx::query("UPDATE libraries SET kind = ? WHERE id = ?").bind(kind).bind(id).execute(&mut *tx).await?;
    // Lavori in coda sui titoli che stanno per sparire.
    sqlx::query("DELETE FROM jobs WHERE state = 'queued' AND library_id = ? AND kind IN ('match_item', 'refresh_metadata', 'download_image')")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    // Le serie si portano dietro stagioni ed episodi; i file restano con item_id ed extra_of a NULL.
    sqlx::query("DELETE FROM items WHERE library_id = ? AND kind IN ('series', 'movie')").bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM items WHERE library_id = ? AND kind IN ('season', 'episode')").bind(id).execute(&mut *tx).await?;
    crate::metadata::collections::prune(&mut tx).await.map_err(ApiError::Internal)?;
    tx.commit().await?;
    tracing::info!(library = id, from = %current, to = %kind, "tipo di libreria cambiato: titoli da ricreare");
    jobs::enqueue(state, JobKind::ScanLibrary { library_id: id, subpath: None }, priority::INTERACTIVE, Some(id)).await?;
    state.events.publish("library.updated", serde_json::json!({}));
    Ok(())
}

async fn update(State(state): State<Shared>, AdminUser(admin): AdminUser, Path(id): Path<String>, Json(p): Json<LibPatch>) -> ApiResult<Json<Value>> {
    let lid = decode(&state, &id)?;
    apply_patch(&state, lid, p).await?;
    state.db.activity(Some(admin.user_id), "library_updated", &format!("{} ha modificato una libreria", admin.username), None).await;
    let r: LibRow = sqlx::query_as(&format!("SELECT {COLS} FROM libraries WHERE id = ?"))
        .bind(lid)
        .fetch_optional(&state.db.read)
        .await?
        .ok_or_else(|| ApiError::not_found("Libreria"))?;
    Ok(Json(view(&state, r, true).await?))
}

async fn remove(State(state): State<Shared>, AdminUser(admin): AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let lid = decode(&state, &id)?;
    let name: Option<String> = sqlx::query_scalar("SELECT name FROM libraries WHERE id = ?").bind(lid).fetch_optional(&state.db.read).await?;
    let Some(name) = name else { return Err(ApiError::not_found("Libreria")) };
    sqlx::query("DELETE FROM libraries WHERE id = ?").bind(lid).execute(&state.db.write).await?;
    sqlx::query("DELETE FROM jobs WHERE library_id = ? AND state = 'queued'").bind(lid).execute(&state.db.write).await?;
    state.token_cache.clear();
    state.db.activity(Some(admin.user_id), "library_deleted", &format!("{} ha eliminato la libreria {name}", admin.username), None).await;
    state.events.publish("library.updated", json!({ "library_id": id, "deleted": true }));
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ScanReq {
    path: Option<String>,
}

async fn scan(State(state): State<Shared>, _a: ScanAccess, Path(id): Path<String>, body: Option<Json<ScanReq>>) -> ApiResult<Json<Value>> {
    let lid = decode(&state, &id)?;
    let sub = body.and_then(|b| b.0.path).filter(|p| !p.trim().is_empty());
    jobs::enqueue(&state, JobKind::ScanLibrary { library_id: lid, subpath: sub }, priority::INTERACTIVE, Some(lid)).await?;
    Ok(Json(json!({ "queued": true })))
}

async fn confirm_missing(State(state): State<Shared>, _a: crate::auth::AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let lid = decode(&state, &id)?;
    sqlx::query("UPDATE libraries SET confirm_missing = 1 WHERE id = ?").bind(lid).execute(&state.db.write).await?;
    jobs::enqueue(&state, JobKind::ScanLibrary { library_id: lid, subpath: None }, priority::INTERACTIVE, Some(lid)).await?;
    tracing::info!(library = lid, "rimozione dei file spariti confermata dall'amministratore");
    Ok(Json(json!({ "queued": true })))
}

async fn refresh_metadata(State(state): State<Shared>, _a: crate::auth::AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let lid = decode(&state, &id)?;
    let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM items WHERE library_id = ? AND kind IN ('movie','series') AND match_state <> 'manual'")
        .bind(lid)
        .fetch_all(&state.db.read)
        .await?;
    let batch: Vec<_> = ids.iter().map(|i| (JobKind::RefreshMetadata { item_id: *i }, priority::MAINTENANCE, Some(lid))).collect();
    jobs::enqueue_batch(&state, batch).await?;
    Ok(Json(json!({ "queued": ids.len() })))
}

async fn scan_all(State(state): State<Shared>, _a: ScanAccess) -> ApiResult<Json<Value>> {
    let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM libraries").fetch_all(&state.db.read).await?;
    for lid in &ids {
        jobs::enqueue(&state, JobKind::ScanLibrary { library_id: *lid, subpath: None }, priority::INTERACTIVE, Some(*lid)).await?;
    }
    Ok(Json(json!({ "queued": ids.len() })))
}

/// Lo strumento che aggiunge un file avvisa il server: la scansione tocca una sola cartella.
async fn scan_path(State(state): State<Shared>, _a: ScanAccess, Json(req): Json<ScanReq>) -> ApiResult<Json<Value>> {
    let path = req.path.unwrap_or_default();
    if !path.starts_with('/') {
        return Err(ApiError::BadRequest("Serve un percorso assoluto".into()));
    }
    let rows: Vec<(i64, String)> = sqlx::query_as("SELECT library_id, path FROM library_paths").fetch_all(&state.db.read).await?;
    let mut queued = 0;
    for (lid, root) in rows {
        if path == root || path.starts_with(&format!("{}/", root.trim_end_matches('/'))) {
            jobs::enqueue(&state, JobKind::ScanLibrary { library_id: lid, subpath: Some(path.clone()) }, priority::INTERACTIVE, Some(lid)).await?;
            queued += 1;
        }
    }
    Ok(Json(json!({ "queued": queued })))
}

#[derive(Deserialize)]
struct BrowseQuery {
    path: Option<String>,
}

async fn browse(State(_state): State<Shared>, _a: AdminUser, Query(q): Query<BrowseQuery>) -> ApiResult<Json<Value>> {
    let default = if std::path::Path::new("/media").is_dir() { "/media" } else { "/" };
    let path = q.path.filter(|p| !p.is_empty()).unwrap_or_else(|| default.to_string());
    let p = PathBuf::from(&path);
    if !p.is_absolute() || p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err(ApiError::BadRequest("Percorso non valido".into()));
    }
    let pp = p.clone();
    let listing = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio::task::spawn_blocking(move || -> std::io::Result<Vec<String>> {
            let mut dirs: Vec<String> = std::fs::read_dir(&pp)?
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
    let parent = p.parent().map(|x| x.to_string_lossy().to_string());
    Ok(Json(json!({
        "path": p.to_string_lossy(),
        "parent": parent,
        "directories": listing.iter().map(|d| json!({ "name": d, "path": p.join(d).to_string_lossy() })).collect::<Vec<_>>(),
    })))
}
