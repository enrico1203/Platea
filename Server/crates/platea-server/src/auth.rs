//! Autenticazione: password Argon2id, token casuali da 256 bit salvati come hash,
//! permessi per utente, limite ai tentativi di login.

use crate::error::{ApiError, ApiResult};
use crate::state::Shared;
use crate::util::{now_ms, random_hex, sha256_hex};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use serde::Serialize;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Hash della password su un thread dedicato: Argon2 è lento per scelta e non deve bloccare il runtime.
pub async fn hash_password(password: String) -> anyhow::Result<String> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
        Argon2::default().hash_password(password.as_bytes(), &salt).map(|h| h.to_string()).map_err(|e| anyhow::anyhow!("hash della password: {e}"))
    })
    .await?
}

pub async fn verify_password(password: String, hash: String) -> bool {
    tokio::task::spawn_blocking(move || PasswordHash::new(&hash).map(|h| Argon2::default().verify_password(password.as_bytes(), &h).is_ok()).unwrap_or(false))
        .await
        .unwrap_or(false)
}

#[derive(Debug, Clone, Serialize)]
pub struct Permissions {
    pub all_libraries: bool,
    pub library_ids: Vec<i64>,
    pub allow_remux: bool,
    pub allow_transcode: bool,
    pub max_bitrate: Option<i64>,
    pub max_streams: Option<i64>,
    pub allow_remote: bool,
    pub manage_devices: bool,
}

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: i64,
    pub token_id: i64,
    pub username: String,
    pub is_admin: bool,
    pub perms: Permissions,
    #[allow(dead_code)]
    pub device_id: String,
    pub device_name: String,
    pub client: String,
    pub ip: Option<IpAddr>,
}

impl AuthUser {
    pub fn can_see_library(&self, library_id: i64) -> bool {
        self.perms.all_libraries || self.perms.library_ids.contains(&library_id)
    }

    /// Frammento SQL che limita le librerie visibili. Solo interi: nessun testo dell'utente.
    pub fn library_filter(&self, column: &str) -> String {
        if self.perms.all_libraries {
            String::new()
        } else if self.perms.library_ids.is_empty() {
            " AND 0".into()
        } else {
            let ids: Vec<String> = self.perms.library_ids.iter().map(|i| i.to_string()).collect();
            format!(" AND {column} IN ({})", ids.join(","))
        }
    }

    pub fn require_admin(&self) -> ApiResult<()> {
        if self.is_admin {
            Ok(())
        } else {
            Err(ApiError::Forbidden("Serve un account amministratore".into()))
        }
    }
}

/// Estrattore che richiede un amministratore.
pub struct AdminUser(pub AuthUser);

#[derive(Clone)]
struct CachedAuth {
    user: AuthUser,
    at: Instant,
    last_touch_ms: i64,
}

/// Cache dei token validi: la maggior parte delle richieste non tocca il database.
#[derive(Default)]
pub struct TokenCache {
    map: Mutex<HashMap<String, CachedAuth>>,
}

const TOKEN_CACHE_TTL: Duration = Duration::from_secs(60);

impl TokenCache {
    pub fn clear(&self) {
        self.map.lock().unwrap().clear();
    }
    pub fn remove_user(&self, user_id: i64) {
        self.map.lock().unwrap().retain(|_, v| v.user.user_id != user_id);
    }
}

pub fn extract_token(parts: &Parts) -> Option<String> {
    let auth = parts.headers.get(axum::http::header::AUTHORIZATION).or_else(|| parts.headers.get("x-emby-authorization"));
    if let Some(v) = auth.and_then(|v| v.to_str().ok()) {
        let v = v.trim();
        if let Some(t) = v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")) {
            return Some(t.trim().to_string());
        }
        // Stile Jellyfin/Emby: MediaBrowser Token="..."
        if let Some(pos) = v.find("Token=\"") {
            let rest = &v[pos + 7..];
            if let Some(end) = rest.find('"') {
                return Some(rest[..end].to_string());
            }
        }
    }
    // X-Emby-Authorization senza token ma con Authorization separata.
    if let Some(v) = parts.headers.get("x-emby-authorization").and_then(|v| v.to_str().ok()) {
        if let Some(pos) = v.find("Token=\"") {
            let rest = &v[pos + 7..];
            if let Some(end) = rest.find('"') {
                return Some(rest[..end].to_string());
            }
        }
    }
    for h in ["x-platea-token", "x-emby-token", "x-mediabrowser-token"] {
        if let Some(v) = parts.headers.get(h).and_then(|v| v.to_str().ok()) {
            return Some(v.trim().to_string());
        }
    }
    None
}

pub fn query_param(parts: &Parts, name: &str) -> Option<String> {
    let q = parts.uri.query()?;
    for pair in q.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        if k.eq_ignore_ascii_case(name) {
            return urlencoding::decode(v).ok().map(|s| s.into_owned());
        }
    }
    None
}

pub fn peer_ip(state: &Shared, parts: &Parts) -> Option<IpAddr> {
    let peer = parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|c| c.0);
    crate::net::client_ip(peer, &parts.headers, &state.settings.get().network.trusted_proxies)
}

pub async fn load_permissions(state: &Shared, user_id: i64) -> anyhow::Result<Option<(String, bool, Permissions)>> {
    let row: Option<(String, i64, i64, i64, i64, Option<i64>, Option<i64>, i64, i64, i64)> = sqlx::query_as(
        "SELECT username, is_admin, all_libraries, allow_remux, allow_transcode, max_bitrate, max_streams, allow_remote, manage_devices, disabled
         FROM users WHERE id = ?",
    )
    .bind(user_id)
    .fetch_optional(&state.db.read)
    .await?;
    let Some((username, is_admin, all_libraries, allow_remux, allow_transcode, max_bitrate, max_streams, allow_remote, manage_devices, disabled)) = row else {
        return Ok(None);
    };
    if disabled != 0 {
        return Ok(None);
    }
    let library_ids: Vec<i64> = if all_libraries != 0 {
        vec![]
    } else {
        sqlx::query_scalar("SELECT library_id FROM user_library_access WHERE user_id = ?").bind(user_id).fetch_all(&state.db.read).await?
    };
    Ok(Some((
        username,
        is_admin != 0,
        Permissions {
            all_libraries: all_libraries != 0,
            library_ids,
            allow_remux: allow_remux != 0,
            allow_transcode: allow_transcode != 0,
            max_bitrate,
            max_streams,
            allow_remote: allow_remote != 0,
            manage_devices: manage_devices != 0,
        },
    )))
}

/// Risolve un token in un utente. Usa la cache; aggiorna "ultimo utilizzo" al massimo una volta al minuto.
pub async fn resolve_token(state: &Shared, token: &str, ip: Option<IpAddr>) -> ApiResult<AuthUser> {
    if token.len() < 16 || token.len() > 200 {
        return Err(ApiError::Unauthorized);
    }
    let hash = sha256_hex(token);
    let now = now_ms();
    {
        let mut map = state.token_cache.map.lock().unwrap();
        if let Some(c) = map.get_mut(&hash) {
            if c.at.elapsed() < TOKEN_CACHE_TTL {
                let mut user = c.user.clone();
                user.ip = ip;
                if now - c.last_touch_ms > 60_000 {
                    c.last_touch_ms = now;
                    touch_token(state, c.user.token_id, ip);
                }
                return Ok(user);
            }
        }
    }
    let row: Option<(i64, i64, String, String, String)> = sqlx::query_as("SELECT id, user_id, device_id, device_name, client FROM tokens WHERE token_hash = ?")
        .bind(&hash)
        .fetch_optional(&state.db.read)
        .await?;
    let Some((token_id, user_id, device_id, device_name, client)) = row else {
        return Err(ApiError::Unauthorized);
    };
    let Some((username, is_admin, perms)) = load_permissions(state, user_id).await? else {
        return Err(ApiError::Unauthorized);
    };
    if !perms.allow_remote {
        if let Some(ip) = ip {
            if !crate::net::in_any(&state.settings.get().network.local_networks, ip) {
                return Err(ApiError::Forbidden("Questo account può accedere solo dalla rete locale".into()));
            }
        }
    }
    let user = AuthUser { user_id, token_id, username, is_admin, perms, device_id, device_name, client, ip };
    touch_token(state, token_id, ip);
    state.token_cache.map.lock().unwrap().insert(hash, CachedAuth { user: user.clone(), at: Instant::now(), last_touch_ms: now });
    Ok(user)
}

fn touch_token(state: &Shared, token_id: i64, ip: Option<IpAddr>) {
    let db = state.db.clone();
    tokio::spawn(async move {
        let _ = sqlx::query("UPDATE tokens SET last_used_at = ?, last_ip = ? WHERE id = ?")
            .bind(now_ms())
            .bind(ip.map(|i| i.to_string()))
            .bind(token_id)
            .execute(&db.write)
            .await;
    });
}

impl FromRequestParts<Shared> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &Shared) -> Result<Self, Self::Rejection> {
        let token = extract_token(parts)
            .or_else(|| query_param(parts, "api_key"))
            .or_else(|| query_param(parts, "apikey"))
            .or_else(|| query_param(parts, "token"))
            .ok_or(ApiError::Unauthorized)?;
        let ip = peer_ip(state, parts);
        resolve_token(state, &token, ip).await
    }
}

impl FromRequestParts<Shared> for AdminUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &Shared) -> Result<Self, Self::Rejection> {
        let user = AuthUser::from_request_parts(parts, state).await?;
        user.require_admin()?;
        Ok(AdminUser(user))
    }
}

pub struct NewToken {
    pub token: String,
}

pub async fn issue_token(
    state: &Shared,
    user_id: i64,
    device_id: &str,
    device_name: &str,
    client: &str,
    client_version: &str,
    ip: Option<IpAddr>,
) -> anyhow::Result<NewToken> {
    let token = random_hex(32);
    let now = now_ms();
    // Un dispositivo ha un solo token per utente: un nuovo login sostituisce il precedente.
    sqlx::query("DELETE FROM tokens WHERE user_id = ? AND device_id = ?").bind(user_id).bind(device_id).execute(&state.db.write).await?;
    let id = sqlx::query(
        "INSERT INTO tokens (token_hash, user_id, device_id, device_name, client, client_version, created_at, last_used_at, last_ip)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(sha256_hex(&token))
    .bind(user_id)
    .bind(device_id)
    .bind(device_name)
    .bind(client)
    .bind(client_version)
    .bind(now)
    .bind(now)
    .bind(ip.map(|i| i.to_string()))
    .execute(&state.db.write)
    .await?
    .last_insert_rowid();
    sqlx::query("UPDATE users SET last_login_at = ? WHERE id = ?").bind(now).bind(user_id).execute(&state.db.write).await?;
    state.token_cache.clear();
    let _ = id;
    Ok(NewToken { token })
}

/// Limite ai tentativi di login per indirizzo IP e per utente, con attesa crescente.
#[derive(Default)]
pub struct LoginLimiter {
    failures: Mutex<HashMap<String, (u32, Instant)>>,
}

impl LoginLimiter {
    fn wait_for(count: u32) -> Duration {
        if count < 5 {
            Duration::ZERO
        } else {
            Duration::from_secs(2u64.saturating_pow((count - 4).min(10)).min(900))
        }
    }

    /// Secondi da attendere prima di un nuovo tentativo, se bloccato.
    pub fn check(&self, keys: &[String]) -> Option<u64> {
        let map = self.failures.lock().unwrap();
        let mut worst = None;
        for k in keys {
            if let Some((count, last)) = map.get(k) {
                let wait = Self::wait_for(*count);
                let elapsed = last.elapsed();
                if elapsed < wait {
                    let remaining = (wait - elapsed).as_secs().max(1);
                    worst = Some(worst.map_or(remaining, |w: u64| w.max(remaining)));
                }
            }
        }
        worst
    }

    pub fn fail(&self, keys: &[String]) {
        let mut map = self.failures.lock().unwrap();
        for k in keys {
            let e = map.entry(k.clone()).or_insert((0, Instant::now()));
            // Dopo un'ora senza errori il conteggio riparte.
            if e.1.elapsed() > Duration::from_secs(3600) {
                e.0 = 0;
            }
            e.0 += 1;
            e.1 = Instant::now();
        }
        if map.len() > 10_000 {
            map.retain(|_, v| v.1.elapsed() < Duration::from_secs(3600));
        }
    }

    pub fn success(&self, keys: &[String]) {
        let mut map = self.failures.lock().unwrap();
        for k in keys {
            map.remove(k);
        }
    }
}
