//! Gestione utenti (amministratori). L'ultimo amministratore non si può eliminare né declassare.

use super::setup::{validate_password, validate_username};
use super::{Access, Api};
use crate::auth::{hash_password, AdminUser};
use crate::error::{ApiError, ApiResult};
use crate::state::Shared;
use crate::util::now_ms;
use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

pub fn routes(api: Api) -> Api {
    api.get("/api/v1/users", "Utenti", Access::Admin, "Elenco degli utenti", list)
        .post("/api/v1/users", "Utenti", Access::Admin, "Crea un utente", create)
        .get("/api/v1/users/{id}", "Utenti", Access::Admin, "Dettaglio di un utente", get_one)
        .patch("/api/v1/users/{id}", "Utenti", Access::Admin, "Modifica permessi, ruolo, stato o password", update)
        .delete("/api/v1/users/{id}", "Utenti", Access::Admin, "Elimina un utente", remove)
        .get("/api/v1/users/{id}/devices", "Utenti", Access::Admin, "Dispositivi e sessioni di un utente", devices)
        .delete("/api/v1/users/{id}/devices", "Utenti", Access::Admin, "Scollega tutti i dispositivi di un utente", revoke_all_devices)
        .delete("/api/v1/users/{id}/devices/{device}", "Utenti", Access::Admin, "Revoca un token di un utente", revoke_device)
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: i64,
    username: String,
    is_admin: i64,
    disabled: i64,
    all_libraries: i64,
    allow_remux: i64,
    allow_transcode: i64,
    max_bitrate: Option<i64>,
    max_streams: Option<i64>,
    max_rating: Option<String>,
    allow_remote: i64,
    manage_devices: i64,
    created_at: i64,
    last_login_at: Option<i64>,
    expires_at: Option<i64>,
    allow_download: i64,
    block_unrated: i64,
    access_schedule: Option<String>,
    avatar_version: Option<i64>,
}

const USER_COLS: &str = "id, username, is_admin, disabled, all_libraries, allow_remux, allow_transcode, max_bitrate, max_streams, max_rating, allow_remote, manage_devices, created_at, last_login_at, expires_at, allow_download, block_unrated, access_schedule, avatar_version";

async fn view(state: &Shared, u: UserRow) -> ApiResult<Value> {
    let libs: Vec<i64> = sqlx::query_scalar("SELECT library_id FROM user_library_access WHERE user_id = ?").bind(u.id).fetch_all(&state.db.read).await?;
    let devices: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tokens WHERE user_id = ?").bind(u.id).fetch_one(&state.db.read).await?;
    Ok(json!({
        "id": state.ids.encode(u.id),
        "username": u.username,
        "is_admin": u.is_admin != 0,
        "disabled": u.disabled != 0,
        "all_libraries": u.all_libraries != 0,
        "library_ids": libs.iter().map(|l| state.ids.encode(*l)).collect::<Vec<_>>(),
        "allow_remux": u.allow_remux != 0,
        "allow_transcode": u.allow_transcode != 0,
        "max_bitrate": u.max_bitrate,
        "max_streams": u.max_streams,
        "max_rating": u.max_rating,
        "allow_remote": u.allow_remote != 0,
        "manage_devices": u.manage_devices != 0,
        "created_at": u.created_at,
        "last_login_at": u.last_login_at,
        "expires_at": u.expires_at,
        "expired": u.is_admin == 0 && u.expires_at.is_some_and(|e| now_ms() >= e),
        "allow_download": u.allow_download != 0,
        "block_unrated": u.block_unrated != 0,
        "access_schedule": u.access_schedule.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok()).unwrap_or(json!([])),
        "avatar": u.avatar_version.map(|v| format!("/api/v1/users/{}/avatar?v={v}", state.ids.encode(u.id))),
        "devices": devices,
        "active_streams": state.sessions.user_count(u.id),
    }))
}

async fn list(State(state): State<Shared>, _a: AdminUser) -> ApiResult<Json<Value>> {
    let rows: Vec<UserRow> = sqlx::query_as(&format!("SELECT {USER_COLS} FROM users ORDER BY username_lower")).fetch_all(&state.db.read).await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(view(&state, r).await?);
    }
    Ok(Json(json!({ "items": out })))
}

fn decode(state: &Shared, id: &str) -> ApiResult<i64> {
    state.ids.decode(id).ok_or_else(|| ApiError::not_found("Utente"))
}

async fn get_one(State(state): State<Shared>, _a: AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let id = decode(&state, &id)?;
    let r: UserRow = sqlx::query_as(&format!("SELECT {USER_COLS} FROM users WHERE id = ?"))
        .bind(id)
        .fetch_optional(&state.db.read)
        .await?
        .ok_or_else(|| ApiError::not_found("Utente"))?;
    Ok(Json(view(&state, r).await?))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct UserPatch {
    username: Option<String>,
    password: Option<String>,
    is_admin: Option<bool>,
    disabled: Option<bool>,
    all_libraries: Option<bool>,
    library_ids: Option<Vec<String>>,
    allow_remux: Option<bool>,
    allow_transcode: Option<bool>,
    #[serde(deserialize_with = "crate::util::double_option")]
    max_bitrate: Option<Option<i64>>,
    #[serde(deserialize_with = "crate::util::double_option")]
    max_streams: Option<Option<i64>>,
    #[serde(deserialize_with = "crate::util::double_option")]
    max_rating: Option<Option<String>>,
    allow_remote: Option<bool>,
    manage_devices: Option<bool>,
    /// Scadenza in ms; `null` = nessuna scadenza.
    #[serde(deserialize_with = "crate::util::double_option")]
    expires_at: Option<Option<i64>>,
    allow_download: Option<bool>,
    block_unrated: Option<bool>,
    /// Fasce orarie di accesso; elenco vuoto o `null` = sempre.
    #[serde(deserialize_with = "crate::util::double_option")]
    access_schedule: Option<Option<Vec<crate::auth::ScheduleSlot>>>,
}

async fn create(State(state): State<Shared>, AdminUser(admin): AdminUser, Json(p): Json<UserPatch>) -> ApiResult<Json<Value>> {
    let username = validate_username(p.username.as_deref().unwrap_or(""))?;
    let password = p.password.clone().unwrap_or_default();
    validate_password(&state, &password)?;
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT id FROM users WHERE username_lower = ?").bind(username.to_lowercase()).fetch_optional(&state.db.read).await?;
    if exists.is_some() {
        return Err(ApiError::Conflict(format!("Esiste già un utente {username}")));
    }
    let hash = hash_password(password).await?;
    let id = sqlx::query("INSERT INTO users (username, username_lower, password_hash, is_admin, created_at) VALUES (?, ?, ?, ?, ?)")
        .bind(&username)
        .bind(username.to_lowercase())
        .bind(hash)
        .bind(p.is_admin.unwrap_or(false) as i64)
        .bind(now_ms())
        .execute(&state.db.write)
        .await?
        .last_insert_rowid();
    apply_patch(&state, id, UserPatch { username: None, password: None, ..p }).await?;
    state.db.activity(Some(admin.user_id), "user_created", &format!("{} ha creato l'utente {username}", admin.username), None).await;
    get_one(State(state.clone()), AdminUser(admin), Path(state.ids.encode(id))).await
}

async fn admin_count(state: &Shared) -> ApiResult<i64> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_admin = 1 AND disabled = 0").fetch_one(&state.db.read).await?)
}

async fn apply_patch(state: &Shared, id: i64, p: UserPatch) -> ApiResult<()> {
    let is_admin: i64 = sqlx::query_scalar("SELECT is_admin FROM users WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.db.read)
        .await?
        .ok_or_else(|| ApiError::not_found("Utente"))?;
    let demoting = is_admin != 0 && (p.is_admin == Some(false) || p.disabled == Some(true));
    if demoting && admin_count(state).await? <= 1 {
        return Err(ApiError::Conflict("È l'ultimo amministratore: non si può declassare né disattivare".into()));
    }
    let mut tx = state.db.write.begin().await?;
    if let Some(u) = &p.username {
        let u = validate_username(u)?;
        let clash: Option<i64> =
            sqlx::query_scalar("SELECT id FROM users WHERE username_lower = ? AND id <> ?").bind(u.to_lowercase()).bind(id).fetch_optional(&mut *tx).await?;
        if clash.is_some() {
            return Err(ApiError::Conflict(format!("Esiste già un utente {u}")));
        }
        sqlx::query("UPDATE users SET username = ?, username_lower = ? WHERE id = ?").bind(&u).bind(u.to_lowercase()).bind(id).execute(&mut *tx).await?;
    }
    if let Some(pw) = &p.password {
        validate_password(state, pw)?;
        let h = hash_password(pw.clone()).await?;
        sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?").bind(h).bind(id).execute(&mut *tx).await?;
    }
    macro_rules! set_bool {
        ($field:ident) => {
            if let Some(v) = p.$field {
                sqlx::query(concat!("UPDATE users SET ", stringify!($field), " = ? WHERE id = ?")).bind(v as i64).bind(id).execute(&mut *tx).await?;
            }
        };
    }
    set_bool!(is_admin);
    set_bool!(disabled);
    set_bool!(all_libraries);
    set_bool!(allow_remux);
    set_bool!(allow_transcode);
    set_bool!(allow_remote);
    set_bool!(manage_devices);
    set_bool!(allow_download);
    set_bool!(block_unrated);
    if let Some(v) = p.access_schedule {
        let v = v.filter(|s| !s.is_empty());
        if let Some(slots) = &v {
            if slots.len() > 50 || slots.iter().any(|s| s.start >= 1440 || s.end > 1440 || s.days.is_empty() || s.days.iter().any(|d| *d > 6)) {
                return Err(ApiError::BadRequest("Fasce orarie non valide".into()));
            }
        }
        sqlx::query("UPDATE users SET access_schedule = ? WHERE id = ?")
            .bind(v.map(|s| serde_json::to_string(&s).unwrap_or_default()))
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(v) = p.max_bitrate {
        sqlx::query("UPDATE users SET max_bitrate = ? WHERE id = ?").bind(v.filter(|b| *b > 0)).bind(id).execute(&mut *tx).await?;
    }
    if let Some(v) = p.max_streams {
        sqlx::query("UPDATE users SET max_streams = ? WHERE id = ?").bind(v.filter(|b| *b > 0)).bind(id).execute(&mut *tx).await?;
    }
    if let Some(v) = p.expires_at {
        sqlx::query("UPDATE users SET expires_at = ? WHERE id = ?").bind(v).bind(id).execute(&mut *tx).await?;
    }
    if let Some(v) = p.max_rating {
        sqlx::query("UPDATE users SET max_rating = ? WHERE id = ?").bind(v.filter(|r| !r.is_empty())).bind(id).execute(&mut *tx).await?;
    }
    if let Some(libs) = p.library_ids {
        sqlx::query("DELETE FROM user_library_access WHERE user_id = ?").bind(id).execute(&mut *tx).await?;
        for l in libs {
            if let Some(lid) = state.ids.decode(&l) {
                sqlx::query("INSERT OR IGNORE INTO user_library_access (user_id, library_id) SELECT ?, id FROM libraries WHERE id = ?")
                    .bind(id)
                    .bind(lid)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }
    if p.disabled == Some(true) {
        sqlx::query("DELETE FROM tokens WHERE user_id = ?").bind(id).execute(&mut *tx).await?;
    }
    // Riattivato: si riparte da zero tentativi falliti (altrimenti il primo errore lo bloccherebbe di nuovo).
    if p.disabled == Some(false) {
        sqlx::query("UPDATE users SET failed_logins = 0 WHERE id = ?").bind(id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    state.token_cache.remove_user(id);
    Ok(())
}

async fn update(State(state): State<Shared>, AdminUser(admin): AdminUser, Path(id): Path<String>, Json(p): Json<UserPatch>) -> ApiResult<Json<Value>> {
    let uid = decode(&state, &id)?;
    apply_patch(&state, uid, p).await?;
    state.db.activity(Some(admin.user_id), "user_updated", &format!("{} ha modificato un utente", admin.username), None).await;
    get_one(State(state), AdminUser(admin), Path(id)).await
}

async fn remove(State(state): State<Shared>, AdminUser(admin): AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let uid = decode(&state, &id)?;
    let row: Option<(String, i64)> = sqlx::query_as("SELECT username, is_admin FROM users WHERE id = ?").bind(uid).fetch_optional(&state.db.read).await?;
    let Some((name, is_admin)) = row else { return Err(ApiError::not_found("Utente")) };
    if is_admin != 0 && admin_count(&state).await? <= 1 {
        return Err(ApiError::Conflict("È l'ultimo amministratore: non si può eliminare".into()));
    }
    sqlx::query("DELETE FROM users WHERE id = ?").bind(uid).execute(&state.db.write).await?;
    state.token_cache.remove_user(uid);
    state.db.activity(Some(admin.user_id), "user_deleted", &format!("{} ha eliminato l'utente {name}", admin.username), None).await;
    Ok(Json(json!({ "ok": true })))
}

/// Dispositivi di un utente per l'amministratore: oltre ai dati del token, se è in rete locale, cosa sta
/// guardando adesso, l'ultima cosa vista e quante riproduzioni ha fatto da quel dispositivo.
async fn devices(State(state): State<Shared>, _a: AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let uid = decode(&state, &id)?;
    let mut items = super::auth::devices_of(&state, uid, None).await?;
    let local = state.settings.get().network.local_networks.clone();
    let sessions = state.sessions.all();
    for d in items.iter_mut() {
        let Some(o) = d.as_object_mut() else { continue };
        let tid = o.get("id").and_then(|v| v.as_str()).and_then(|s| state.ids.decode(s));
        let ip = o.get("last_ip").and_then(|v| v.as_str()).and_then(|s| s.parse::<std::net::IpAddr>().ok());
        o.insert("remote".into(), json!(ip.map(|i| !crate::net::in_any(&local, i))));
        if let Some(s) = tid.and_then(|t| sessions.iter().find(|s| s.token_id == t)) {
            o.insert(
                "playing".into(),
                json!({ "title": s.title, "subtitle": s.subtitle, "method": s.decision.method.as_str(), "paused": s.paused.load(std::sync::atomic::Ordering::Relaxed) }),
            );
        }
        let name = o.get("device_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let client = o.get("client").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let stats: (i64, Option<i64>, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT COUNT(*), MAX(h.started_at),
                    (SELECT COALESCE(s.title || ' · ', '') || i.title FROM playback_history h2 JOIN items i ON i.id = h2.item_id LEFT JOIN items s ON s.id = i.series_id
                      WHERE h2.user_id = ?1 AND h2.device_name = ?2 AND h2.client = ?3 ORDER BY h2.started_at DESC LIMIT 1),
                    (SELECT h3.method FROM playback_history h3 WHERE h3.user_id = ?1 AND h3.device_name = ?2 AND h3.client = ?3 ORDER BY h3.started_at DESC LIMIT 1)
             FROM playback_history h WHERE h.user_id = ?1 AND h.device_name = ?2 AND h.client = ?3",
        )
        .bind(uid)
        .bind(&name)
        .bind(&client)
        .fetch_one(&state.db.read)
        .await?;
        o.insert("plays".into(), json!(stats.0));
        o.insert("last_played_at".into(), json!(stats.1));
        o.insert("last_played_title".into(), json!(stats.2));
        o.insert("last_method".into(), json!(stats.3));
    }
    Ok(Json(json!({ "items": items })))
}

/// Scollega tutti i dispositivi di un utente (dovrà accedere di nuovo ovunque).
async fn revoke_all_devices(State(state): State<Shared>, AdminUser(admin): AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let uid = decode(&state, &id)?;
    let n = sqlx::query("DELETE FROM tokens WHERE user_id = ?").bind(uid).execute(&state.db.write).await?.rows_affected();
    state.token_cache.remove_user(uid);
    state.db.activity(Some(admin.user_id), "token_revoked", &format!("{} ha scollegato {n} dispositivi di un utente", admin.username), None).await;
    Ok(Json(json!({ "revoked": n })))
}

async fn revoke_device(State(state): State<Shared>, AdminUser(admin): AdminUser, Path((id, device)): Path<(String, String)>) -> ApiResult<Json<Value>> {
    let uid = decode(&state, &id)?;
    let tid = state.ids.decode(&device).ok_or_else(|| ApiError::not_found("Dispositivo"))?;
    sqlx::query("DELETE FROM tokens WHERE id = ? AND user_id = ?").bind(tid).bind(uid).execute(&state.db.write).await?;
    state.token_cache.clear();
    state.db.activity(Some(admin.user_id), "token_revoked", &format!("{} ha revocato un dispositivo", admin.username), None).await;
    Ok(Json(json!({ "ok": true })))
}
