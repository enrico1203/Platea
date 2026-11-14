//! Login, logout, profilo e dispositivi dell'utente.

use super::setup::validate_password;
use super::{Access, Api};
use crate::auth::{hash_password, issue_token, verify_password, AuthUser};
use crate::error::{ApiError, ApiResult};
use crate::state::Shared;
use axum::extract::{ConnectInfo, Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::SocketAddr;

pub fn routes(api: Api) -> Api {
    api.post("/api/v1/auth/login", "Accesso", Access::Public, "Login con utente e password: restituisce un token", login)
        .post("/api/v1/auth/logout", "Accesso", Access::User, "Revoca il token corrente", logout)
        .get("/api/v1/me", "Accesso", Access::User, "Utente corrente con permessi", me)
        .post("/api/v1/me/password", "Accesso", Access::User, "Cambia la propria password", change_password)
        .get("/api/v1/me/devices", "Accesso", Access::User, "Dispositivi collegati al proprio account", my_devices)
        .delete("/api/v1/me/devices/{id}", "Accesso", Access::User, "Scollega un proprio dispositivo", revoke_my_device)
        .get("/api/v1/me/preferences", "Accesso", Access::User, "Preferenze dell'utente: lingue, sottotitoli, player, home, tema", get_prefs)
        .put("/api/v1/me/preferences", "Accesso", Access::User, "Salva le preferenze (i campi mancanti tornano ai valori predefiniti)", put_prefs)
        .post("/api/v1/me/avatar", "Accesso", Access::User, "Carica l'immagine del profilo (JPEG, PNG o WebP; ritagliata a 256×256)", put_avatar)
        .delete("/api/v1/me/avatar", "Accesso", Access::User, "Toglie l'immagine del profilo", delete_avatar)
        .get("/api/v1/users/{id}/avatar", "Accesso", Access::Public, "Immagine del profilo di un utente", get_avatar)
}

#[derive(Deserialize)]
pub struct LoginReq {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub device_name: Option<String>,
    #[serde(default)]
    pub client: Option<String>,
    #[serde(default)]
    pub client_version: Option<String>,
}

pub struct LoginOk {
    pub user_id: i64,
    pub username: String,
    pub is_admin: bool,
    pub token: String,
}

/// Login condiviso tra API nativa e strato compatibile.
pub async fn do_login(state: &Shared, req: &LoginReq, ip: Option<std::net::IpAddr>) -> ApiResult<LoginOk> {
    let uname = req.username.trim().to_lowercase();
    let keys = vec![format!("ip:{}", ip.map(|i| i.to_string()).unwrap_or_default()), format!("user:{uname}")];
    if let Some(wait) = state.login_limiter.check(&keys) {
        return Err(ApiError::TooManyRequests(format!("Troppi tentativi di accesso: riprova tra {wait} s")));
    }
    let row: Option<(i64, String, String, i64, i64)> =
        sqlx::query_as("SELECT id, username, password_hash, is_admin, disabled FROM users WHERE username_lower = ?")
            .bind(&uname)
            .fetch_optional(&state.db.read)
            .await?;
    // Si verifica sempre un hash, anche per utenti inesistenti: stessi tempi di risposta.
    let (id, username, hash, is_admin, disabled) =
        row.unwrap_or((0, String::new(), "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$YmFkaGFzaGJhZGhhc2hiYWRoYXNoYmFkaGFzaA".into(), 0, 1));
    let ok = verify_password(req.password.clone(), hash).await;
    if !ok || id == 0 || disabled != 0 {
        state.login_limiter.fail(&keys);
        state.db.activity(None, "login_failed", &format!("Accesso non riuscito per {}", req.username.trim()), ip.map(|i| i.to_string()).as_deref()).await;
        return Err(ApiError::coded(axum::http::StatusCode::UNAUTHORIZED, "InvalidCredentials", "Nome utente o password non corretti"));
    }
    state.login_limiter.success(&keys);
    let device_id = req.device_id.clone().filter(|d| !d.is_empty()).unwrap_or_else(|| crate::util::random_hex(8));
    let t = issue_token(
        state,
        id,
        &device_id,
        req.device_name.as_deref().unwrap_or("Dispositivo"),
        req.client.as_deref().unwrap_or("Sconosciuto"),
        req.client_version.as_deref().unwrap_or(""),
        ip,
    )
    .await?;
    state
        .db
        .activity(
            Some(id),
            "login",
            &format!("{username} ha effettuato l'accesso da {}", req.device_name.as_deref().unwrap_or("un dispositivo")),
            ip.map(|i| i.to_string()).as_deref(),
        )
        .await;
    Ok(LoginOk { user_id: id, username, is_admin: is_admin != 0, token: t.token })
}

async fn login(
    State(state): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<LoginReq>,
) -> ApiResult<Json<Value>> {
    let ip = crate::net::client_ip(Some(peer), &headers, &state.settings.get().network.trusted_proxies);
    let ok = do_login(&state, &req, ip).await?;
    Ok(Json(json!({
        "token": ok.token,
        "user": { "id": state.ids.encode(ok.user_id), "username": ok.username, "is_admin": ok.is_admin }
    })))
}

async fn logout(State(state): State<Shared>, user: AuthUser) -> ApiResult<Json<Value>> {
    sqlx::query("DELETE FROM tokens WHERE id = ?").bind(user.token_id).execute(&state.db.write).await?;
    state.token_cache.clear();
    Ok(Json(json!({ "ok": true })))
}

async fn me(State(state): State<Shared>, user: AuthUser) -> ApiResult<Json<Value>> {
    let libs: Vec<String> = user.perms.library_ids.iter().map(|i| state.ids.encode(*i)).collect();
    Ok(Json(json!({
        "id": state.ids.encode(user.user_id),
        "username": user.username,
        "is_admin": user.is_admin,
        "device_name": user.device_name,
        "permissions": {
            "all_libraries": user.perms.all_libraries,
            "library_ids": libs,
            "allow_remux": user.perms.allow_remux,
            "allow_transcode": user.perms.allow_transcode,
            "max_bitrate": user.perms.max_bitrate,
            "max_streams": user.perms.max_streams,
            "allow_remote": user.perms.allow_remote,
            "manage_devices": user.perms.manage_devices,
            "expires_at": user.perms.expires_at,
            "expired": !user.is_admin && user.perms.expired(),
            "allow_download": user.is_admin || user.perms.allow_download,
            "block_unrated": user.perms.block_unrated,
            "access_schedule": user.perms.schedule,
            "within_schedule": user.is_admin || user.within_schedule(),
        },
        "avatar": avatar_url(&state, user.user_id).await,
        "preferences": crate::prefs::load(&state, user.user_id).await,
    })))
}

pub async fn avatar_url(state: &Shared, user_id: i64) -> Option<String> {
    let v: Option<i64> = sqlx::query_scalar("SELECT avatar_version FROM users WHERE id = ?").bind(user_id).fetch_optional(&state.db.read).await.ok().flatten();
    v.map(|v| format!("/api/v1/users/{}/avatar?v={v}", state.ids.encode(user_id)))
}

async fn get_prefs(State(state): State<Shared>, user: AuthUser) -> ApiResult<Json<Value>> {
    Ok(Json(json!(crate::prefs::load(&state, user.user_id).await)))
}

async fn put_prefs(State(state): State<Shared>, user: AuthUser, Json(p): Json<crate::prefs::UserPrefs>) -> ApiResult<Json<Value>> {
    let p = p.sanitize();
    sqlx::query("UPDATE users SET preferences = ? WHERE id = ?")
        .bind(serde_json::to_string(&p).map_err(|e| ApiError::Internal(e.into()))?)
        .bind(user.user_id)
        .execute(&state.db.write)
        .await?;
    Ok(Json(json!(p)))
}

fn avatar_path(state: &Shared, user_id: i64) -> std::path::PathBuf {
    state.boot.data_dir.join("avatars").join(format!("{user_id}.png"))
}

/// L'immagine si ritaglia al centro e si riduce a 256×256: poche decine di KB, mai l'originale.
async fn put_avatar(State(state): State<Shared>, user: AuthUser, body: axum::body::Bytes) -> ApiResult<Json<Value>> {
    if body.is_empty() || body.len() > 15 * 1024 * 1024 {
        return Err(ApiError::BadRequest("Immagine vuota o troppo grande (massimo 15 MB)".into()));
    }
    let path = avatar_path(&state, user.user_id);
    let p = path.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let img = image::load_from_memory(&body)?;
        let small = img.resize_to_fill(256, 256, image::imageops::FilterType::Lanczos3);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = p.with_extension("tmp");
        small.save_with_format(&tmp, image::ImageFormat::Png)?;
        std::fs::rename(&tmp, &p)?;
        Ok(())
    })
    .await
    .map_err(|e| ApiError::Internal(e.into()))?
    .map_err(|e| ApiError::BadRequest(format!("Immagine non leggibile: {e}")))?;
    sqlx::query("UPDATE users SET avatar_version = ? WHERE id = ?").bind(crate::util::now_ms()).bind(user.user_id).execute(&state.db.write).await?;
    Ok(Json(json!({ "avatar": avatar_url(&state, user.user_id).await })))
}

async fn delete_avatar(State(state): State<Shared>, user: AuthUser) -> ApiResult<Json<Value>> {
    let _ = tokio::fs::remove_file(avatar_path(&state, user.user_id)).await;
    sqlx::query("UPDATE users SET avatar_version = NULL WHERE id = ?").bind(user.user_id).execute(&state.db.write).await?;
    Ok(Json(json!({ "avatar": null })))
}

async fn get_avatar(State(state): State<Shared>, Path(id): Path<String>) -> ApiResult<axum::response::Response> {
    let uid = state.ids.decode(&id).ok_or_else(|| ApiError::not_found("Immagine"))?;
    let bytes = tokio::fs::read(avatar_path(&state, uid)).await.map_err(|_| ApiError::not_found("Immagine"))?;
    Ok(axum::response::Response::builder()
        .header(axum::http::header::CONTENT_TYPE, "image/png")
        // L'indirizzo cambia a ogni nuova immagine (?v=…): si può tenere in cache a lungo.
        .header(axum::http::header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(axum::body::Body::from(bytes))
        .unwrap())
}

#[derive(Deserialize)]
struct PasswordReq {
    current: String,
    new: String,
}

async fn change_password(State(state): State<Shared>, user: AuthUser, Json(req): Json<PasswordReq>) -> ApiResult<Json<Value>> {
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?").bind(user.user_id).fetch_one(&state.db.read).await?;
    if !verify_password(req.current, hash).await {
        return Err(ApiError::coded(axum::http::StatusCode::FORBIDDEN, "InvalidCredentials", "La password attuale non è corretta"));
    }
    validate_password(&state, &req.new)?;
    let h = hash_password(req.new).await?;
    sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?").bind(h).bind(user.user_id).execute(&state.db.write).await?;
    // Gli altri dispositivi vengono scollegati.
    sqlx::query("DELETE FROM tokens WHERE user_id = ? AND id <> ?").bind(user.user_id).bind(user.token_id).execute(&state.db.write).await?;
    state.token_cache.clear();
    state.db.activity(Some(user.user_id), "password", &format!("{} ha cambiato la password", user.username), None).await;
    Ok(Json(json!({ "ok": true })))
}

pub async fn devices_of(state: &Shared, user_id: i64, current_token: Option<i64>) -> ApiResult<Vec<Value>> {
    let rows: Vec<(i64, String, String, String, String, i64, i64, Option<String>)> = sqlx::query_as(
        "SELECT id, device_id, device_name, client, client_version, created_at, last_used_at, last_ip FROM tokens WHERE user_id = ? ORDER BY last_used_at DESC",
    )
    .bind(user_id)
    .fetch_all(&state.db.read)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, device_id, name, client, ver, created, last, ip)| {
            json!({
                "id": state.ids.encode(id),
                "device_id": device_id,
                "device_name": name,
                "client": client,
                "client_version": ver,
                "created_at": created,
                "last_used_at": last,
                "last_ip": ip,
                "current": Some(id) == current_token,
            })
        })
        .collect())
}

async fn my_devices(State(state): State<Shared>, user: AuthUser) -> ApiResult<Json<Value>> {
    Ok(Json(json!({ "items": devices_of(&state, user.user_id, Some(user.token_id)).await? })))
}

async fn revoke_my_device(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    if !user.perms.manage_devices && !user.is_admin {
        return Err(ApiError::Forbidden("L'amministratore non consente di gestire i dispositivi".into()));
    }
    let id = state.ids.decode(&id).ok_or_else(|| ApiError::not_found("Dispositivo"))?;
    let n = sqlx::query("DELETE FROM tokens WHERE id = ? AND user_id = ?").bind(id).bind(user.user_id).execute(&state.db.write).await?.rows_affected();
    if n == 0 {
        return Err(ApiError::not_found("Dispositivo"));
    }
    state.token_cache.clear();
    Ok(Json(json!({ "ok": true })))
}
