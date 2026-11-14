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
}

const USER_COLS: &str = "id, username, is_admin, disabled, all_libraries, allow_remux, allow_transcode, max_bitrate, max_streams, max_rating, allow_remote, manage_devices, created_at, last_login_at";

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
    let r: UserRow = sqlx::query_as(&format!("SELECT {USER_COLS} FROM users WHERE id = ?")).bind(id).fetch_optional(&state.db.read).await?.ok_or_else(|| ApiError::not_found("Utente"))?;
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
    max_bitrate: Option<Option<i64>>,
    max_streams: Option<Option<i64>>,
    max_rating: Option<Option<String>>,
    allow_remote: Option<bool>,
    manage_devices: Option<bool>,
}

async fn create(State(state): State<Shared>, AdminUser(admin): AdminUser, Json(p): Json<UserPatch>) -> ApiResult<Json<Value>> {
    let username = validate_username(p.username.as_deref().unwrap_or(""))?;
    let password = p.password.clone().unwrap_or_default();
    validate_password(&state, &password)?;
    let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM users WHERE username_lower = ?").bind(username.to_lowercase()).fetch_optional(&state.db.read).await?;
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
    let is_admin: i64 = sqlx::query_scalar("SELECT is_admin FROM users WHERE id = ?").bind(id).fetch_optional(&state.db.read).await?.ok_or_else(|| ApiError::not_found("Utente"))?;
    let demoting = is_admin != 0 && (p.is_admin == Some(false) || p.disabled == Some(true));
    if demoting && admin_count(state).await? <= 1 {
        return Err(ApiError::Conflict("È l'ultimo amministratore: non si può declassare né disattivare".into()));
    }
    let mut tx = state.db.write.begin().await?;
    if let Some(u) = &p.username {
        let u = validate_username(u)?;
        let clash: Option<i64> = sqlx::query_scalar("SELECT id FROM users WHERE username_lower = ? AND id <> ?").bind(u.to_lowercase()).bind(id).fetch_optional(&mut *tx).await?;
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
    if let Some(v) = p.max_bitrate {
        sqlx::query("UPDATE users SET max_bitrate = ? WHERE id = ?").bind(v.filter(|b| *b > 0)).bind(id).execute(&mut *tx).await?;
    }
    if let Some(v) = p.max_streams {
        sqlx::query("UPDATE users SET max_streams = ? WHERE id = ?").bind(v.filter(|b| *b > 0)).bind(id).execute(&mut *tx).await?;
    }
    if let Some(v) = p.max_rating {
        sqlx::query("UPDATE users SET max_rating = ? WHERE id = ?").bind(v.filter(|r| !r.is_empty())).bind(id).execute(&mut *tx).await?;
    }
    if let Some(libs) = p.library_ids {
        sqlx::query("DELETE FROM user_library_access WHERE user_id = ?").bind(id).execute(&mut *tx).await?;
        for l in libs {
            if let Some(lid) = state.ids.decode(&l) {
                sqlx::query("INSERT OR IGNORE INTO user_library_access (user_id, library_id) SELECT ?, id FROM libraries WHERE id = ?").bind(id).bind(lid).execute(&mut *tx).await?;
            }
        }
    }
    if p.disabled == Some(true) {
        sqlx::query("DELETE FROM tokens WHERE user_id = ?").bind(id).execute(&mut *tx).await?;
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

async fn devices(State(state): State<Shared>, _a: AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let uid = decode(&state, &id)?;
    Ok(Json(json!({ "items": super::auth::devices_of(&state, uid, None).await? })))
}

async fn revoke_device(State(state): State<Shared>, AdminUser(admin): AdminUser, Path((id, device)): Path<(String, String)>) -> ApiResult<Json<Value>> {
    let uid = decode(&state, &id)?;
    let tid = state.ids.decode(&device).ok_or_else(|| ApiError::not_found("Dispositivo"))?;
    sqlx::query("DELETE FROM tokens WHERE id = ? AND user_id = ?").bind(tid).bind(uid).execute(&state.db.write).await?;
    state.token_cache.clear();
    state.db.activity(Some(admin.user_id), "token_revoked", &format!("{} ha revocato un dispositivo", admin.username), None).await;
    Ok(Json(json!({ "ok": true })))
}
