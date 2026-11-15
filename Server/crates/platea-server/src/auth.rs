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
    /// Scadenza dell'account (ms). Dopo questa data l'utente sfoglia ma non riproduce né scarica.
    pub expires_at: Option<i64>,
    /// Età massima dei contenuti visibili (dalla classificazione); `None` = nessun limite.
    pub max_rating_age: Option<u8>,
    /// Scaricare il file originale.
    pub allow_download: bool,
    /// Nascondere i titoli senza classificazione (per profili di bambini).
    pub block_unrated: bool,
    /// Fasce orarie in cui si può riprodurre; vuoto = sempre.
    pub schedule: Vec<ScheduleSlot>,
    /// Generi nascosti (identificativi): i titoli con almeno uno di questi generi non si vedono.
    pub blocked_genre_ids: Vec<i64>,
    /// Generi consentiti (identificativi); vuoto = tutti. Si vedono solo i titoli con almeno uno di questi generi.
    pub allowed_genre_ids: Vec<i64>,
}

/// Una fascia oraria di accesso: giorni (0 = domenica) e intervallo in minuti dalla mezzanotte, ora locale.
/// Una fascia che supera la mezzanotte (es. 22:00–02:00) vale per il giorno in cui inizia.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ScheduleSlot {
    pub days: Vec<u8>,
    pub start: u32,
    pub end: u32,
}

/// Il momento (giorno, minuto) cade in almeno una fascia? Nessuna fascia = sempre consentito.
pub fn schedule_allows(slots: &[ScheduleSlot], day: u8, minute: u32) -> bool {
    if slots.is_empty() {
        return true;
    }
    slots.iter().any(|s| {
        if s.start <= s.end {
            s.days.contains(&day) && minute >= s.start && minute < s.end
        } else {
            // Oltre la mezzanotte: la sera del giorno indicato e la notte del giorno dopo.
            (s.days.contains(&day) && minute >= s.start) || (s.days.contains(&((day + 6) % 7)) && minute < s.end)
        }
    })
}

impl Permissions {
    pub fn expired(&self) -> bool {
        self.expires_at.is_some_and(|e| now_ms() >= e)
    }
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
        let libs = if self.perms.all_libraries {
            String::new()
        } else if self.perms.library_ids.is_empty() {
            " AND 0".into()
        } else {
            let ids: Vec<String> = self.perms.library_ids.iter().map(|i| i.to_string()).collect();
            format!(" AND {column} IN ({})", ids.join(","))
        };
        libs + &self.rating_filter(column) + &self.genre_filter(column)
    }

    /// Generi bloccati e consentiti (`alias.library_id` → `alias`). Stagioni ed episodi usano i generi della serie;
    /// le collezioni restano visibili (i loro film sono filtrati uno per uno). Solo identificativi interi nella query.
    fn genre_filter(&self, column: &str) -> String {
        let Some(a) = column.strip_suffix(".library_id") else { return String::new() };
        let mut out = String::new();
        let ids = |v: &[i64]| v.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
        if !self.perms.blocked_genre_ids.is_empty() {
            out.push_str(&format!(
                " AND ({a}.kind = 'collection' OR NOT EXISTS (SELECT 1 FROM item_genres xg WHERE xg.item_id = COALESCE({a}.series_id, {a}.id) AND xg.genre_id IN ({})))",
                ids(&self.perms.blocked_genre_ids)
            ));
        }
        if !self.perms.allowed_genre_ids.is_empty() {
            out.push_str(&format!(
                " AND ({a}.kind = 'collection' OR EXISTS (SELECT 1 FROM item_genres xg WHERE xg.item_id = COALESCE({a}.series_id, {a}.id) AND xg.genre_id IN ({})))",
                ids(&self.perms.allowed_genre_ids)
            ));
        }
        out
    }

    /// Limite di classificazione sugli elementi (`alias.library_id` → `alias`). Episodi e stagioni usano quella della serie;
    /// i titoli senza classificazione restano visibili. Solo interi: nessun testo dell'utente.
    fn rating_filter(&self, column: &str) -> String {
        let Some(a) = column.strip_suffix(".library_id") else { return String::new() };
        // Senza classificazione: visibile (0), oppure nascosto per i profili che bloccano i non classificati
        // (le collezioni non hanno classificazione propria e restano visibili).
        let unrated = if self.perms.block_unrated { format!("CASE WHEN {a}.kind = 'collection' THEN 0 ELSE 99 END") } else { "0".into() };
        match self.effective_max_age() {
            Some(max) => format!(
                " AND COALESCE(NULLIF({a}.rating_age, -1), (SELECT NULLIF(s.rating_age, -1) FROM items s WHERE s.id = {a}.series_id), {unrated}) <= {max}"
            ),
            None => String::new(),
        }
    }

    fn effective_max_age(&self) -> Option<u8> {
        self.perms.max_rating_age.or(if self.perms.block_unrated { Some(21) } else { None })
    }

    /// Un elemento è visibile: libreria consentita e classificazione entro il limite.
    pub async fn can_see_item(&self, state: &Shared, item_id: i64, library_id: i64) -> ApiResult<bool> {
        if !self.can_see_library(library_id) {
            return Ok(false);
        }
        let genres = self.genre_filter("i.library_id");
        if !genres.is_empty() {
            let ok: Option<i64> =
                sqlx::query_scalar(&format!("SELECT 1 FROM items i WHERE i.id = ? {genres}")).bind(item_id).fetch_optional(&state.db.read).await?;
            if ok.is_none() {
                return Ok(false);
            }
        }
        let Some(max) = self.effective_max_age() else { return Ok(true) };
        let unrated = if self.perms.block_unrated { "CASE WHEN i.kind = 'collection' THEN 0 ELSE 99 END" } else { "0" };
        let age: Option<i64> = sqlx::query_scalar(&format!(
            "SELECT COALESCE(NULLIF(i.rating_age, -1), (SELECT NULLIF(s.rating_age, -1) FROM items s WHERE s.id = i.series_id), {unrated}) FROM items i WHERE i.id = ?",
        ))
        .bind(item_id)
        .fetch_optional(&state.db.read)
        .await?;
        Ok(age.unwrap_or(0) <= max as i64)
    }

    /// Riproduzione e download: negati agli account scaduti. Gli amministratori non scadono.
    pub fn require_playback(&self) -> ApiResult<()> {
        if !self.is_admin && self.perms.expired() {
            return Err(ApiError::coded(
                axum::http::StatusCode::FORBIDDEN,
                "AccountExpired",
                "Account scaduto: puoi sfogliare la libreria ma non riprodurre. Chiedi all'amministratore di rinnovarlo.",
            ));
        }
        if !self.is_admin && !self.within_schedule() {
            return Err(ApiError::coded(
                axum::http::StatusCode::FORBIDDEN,
                "OutsideSchedule",
                "In questo orario la riproduzione non è consentita per il tuo account.",
            ));
        }
        Ok(())
    }

    /// Fasce orarie di accesso rispettate adesso (ora locale del server).
    pub fn within_schedule(&self) -> bool {
        let (day, minute) = crate::util::local_now();
        schedule_allows(&self.perms.schedule, day, minute)
    }

    /// Download del file originale: permesso, scadenza e fasce orarie.
    pub fn require_download(&self) -> ApiResult<()> {
        self.require_playback()?;
        if !self.is_admin && !self.perms.allow_download {
            return Err(ApiError::coded(axum::http::StatusCode::FORBIDDEN, "DownloadNotAllowed", "Il tuo account non può scaricare i file."));
        }
        Ok(())
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

/// Campi dell'intestazione `MediaBrowser Client=…, Device=…, DeviceId=…, Version=…, Token=…` (anche `Emby …`).
/// I valori possono essere tra virgolette o no (Swiftfin 1.7 li manda senza), codificati in URL, in qualsiasi ordine.
pub fn mediabrowser_fields(raw: &str) -> std::collections::HashMap<String, String> {
    let body = raw.trim();
    let body = body.strip_prefix("MediaBrowser").or_else(|| body.strip_prefix("Emby")).unwrap_or(body).trim();
    let mut out = std::collections::HashMap::new();
    for part in body.split(',') {
        if let Some((k, v)) = part.trim().split_once('=') {
            let v = v.trim().trim_matches('"');
            let v = urlencoding::decode(v).map(|x| x.into_owned()).unwrap_or_else(|_| v.to_string());
            out.insert(k.trim().to_ascii_lowercase(), v);
        }
    }
    out
}

pub fn extract_token(parts: &Parts) -> Option<String> {
    for h in [axum::http::header::AUTHORIZATION.as_str(), "x-emby-authorization"] {
        let Some(v) = parts.headers.get(h).and_then(|v| v.to_str().ok()) else { continue };
        let v = v.trim();
        if let Some(t) = v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")) {
            return Some(t.trim().to_string());
        }
        // Stile Jellyfin/Emby: MediaBrowser …, Token=… (con o senza virgolette).
        if let Some(t) = mediabrowser_fields(v).remove("token").filter(|t| !t.is_empty()) {
            return Some(t);
        }
    }
    for h in ["x-platea-token", "x-emby-token", "x-mediabrowser-token"] {
        if let Some(v) = parts.headers.get(h).and_then(|v| v.to_str().ok()) {
            return Some(v.trim().to_string());
        }
    }
    None
}

/// Maschera i valori dopo `Token=` (per i log).
fn mask_tokens(v: &str) -> String {
    let mut out = String::new();
    let mut rest = v;
    while let Some(i) = rest.to_ascii_lowercase().find("token=") {
        out.push_str(&rest[..i + 6]);
        let tail = &rest[i + 6..];
        let end = tail.find(',').unwrap_or(tail.len());
        out.push_str(&format!("<{} caratteri>", tail[..end].len()));
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
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
    let row: Option<(String, i64, i64, i64, i64, Option<i64>, Option<i64>, i64, i64, i64, Option<i64>, Option<String>)> = sqlx::query_as(
        "SELECT username, is_admin, all_libraries, allow_remux, allow_transcode, max_bitrate, max_streams, allow_remote, manage_devices, disabled, expires_at, max_rating
         FROM users WHERE id = ?",
    )
    .bind(user_id)
    .fetch_optional(&state.db.read)
    .await?;
    let Some((
        username,
        is_admin,
        all_libraries,
        allow_remux,
        allow_transcode,
        max_bitrate,
        max_streams,
        allow_remote,
        manage_devices,
        disabled,
        expires_at,
        max_rating,
    )) = row
    else {
        return Ok(None);
    };
    if disabled != 0 {
        return Ok(None);
    }
    let (allow_download, block_unrated, schedule, blocked, allowed): (i64, i64, Option<String>, Option<String>, Option<String>) =
        sqlx::query_as("SELECT allow_download, block_unrated, access_schedule, blocked_genres, allowed_genres FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_one(&state.db.read)
            .await?;
    // Nomi dei generi → identificativi (un genere che non esiste più non conta).
    let genre_ids = |raw: Option<String>| async move {
        let names: Vec<String> = raw.as_deref().and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default();
        let mut ids = Vec::new();
        for n in names {
            if let Ok(Some(id)) = sqlx::query_scalar::<_, i64>("SELECT id FROM genres WHERE name = ?").bind(&n).fetch_optional(&state.db.read).await {
                ids.push(id);
            }
        }
        ids
    };
    let blocked_genre_ids = genre_ids(blocked).await;
    let allowed_genre_ids = genre_ids(allowed).await;
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
            expires_at,
            max_rating_age: max_rating.as_deref().and_then(crate::ratings::age),
            allow_download: allow_download != 0,
            block_unrated: block_unrated != 0,
            schedule: schedule.as_deref().and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default(),
            blocked_genre_ids,
            allowed_genre_ids,
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
        tracing::info!(len = token.len(), "token sconosciuto");
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
        let token =
            extract_token(parts).or_else(|| query_param(parts, "api_key")).or_else(|| query_param(parts, "apikey")).or_else(|| query_param(parts, "token"));
        let Some(token) = token else {
            // Diagnosi delle app: quali intestazioni arrivano (i valori dei token sono mascherati).
            if !parts.uri.path().starts_with("/api/v1/") {
                let names: Vec<&str> = parts.headers.keys().map(|k| k.as_str()).collect();
                let auth: Vec<String> = ["authorization", "x-emby-authorization"]
                    .iter()
                    .filter_map(|h| parts.headers.get(*h).and_then(|v| v.to_str().ok()).map(|v| format!("{h}: {}", mask_tokens(v))))
                    .collect();
                tracing::info!(path = %parts.uri.path(), headers = ?names, auth = ?auth, "richiesta Jellyfin senza token riconosciuto");
            }
            return Err(ApiError::Unauthorized);
        };
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

/// Accesso per le scansioni: un amministratore oppure una chiave API con ambito `scan`.
/// Chiave nell'intestazione `X-Platea-Key` o nel parametro `key`.
pub struct ScanAccess;

pub const API_SCOPES: &[&str] = &["scan"];

impl FromRequestParts<Shared> for ScanAccess {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &Shared) -> Result<Self, Self::Rejection> {
        let key = parts.headers.get("x-platea-key").and_then(|v| v.to_str().ok()).map(String::from).or_else(|| query_param(parts, "key"));
        if let Some(k) = key {
            let row: Option<(i64, String)> =
                sqlx::query_as("SELECT id, scopes FROM api_keys WHERE key_hash = ?").bind(sha256_hex(k.trim())).fetch_optional(&state.db.read).await?;
            return match row {
                Some((id, scopes)) if scopes.split(',').any(|s| s == "scan") => {
                    let db = state.db.clone();
                    tokio::spawn(async move {
                        let _ = sqlx::query("UPDATE api_keys SET last_used_at = ? WHERE id = ?").bind(now_ms()).bind(id).execute(&db.write).await;
                    });
                    Ok(ScanAccess)
                }
                Some(_) => Err(ApiError::Forbidden("Questa chiave API non ha l'ambito scan".into())),
                None => Err(ApiError::Unauthorized),
            };
        }
        AdminUser::from_request_parts(parts, state).await.map(|_| ScanAccess)
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

#[cfg(test)]
mod schedule_tests {
    use super::*;

    #[test]
    fn schedule_slots() {
        // Lunedì-venerdì 16:00-21:00, sabato 10:00-23:00.
        let s = vec![ScheduleSlot { days: vec![1, 2, 3, 4, 5], start: 960, end: 1260 }, ScheduleSlot { days: vec![6], start: 600, end: 1380 }];
        assert!(schedule_allows(&s, 1, 17 * 60));
        assert!(!schedule_allows(&s, 1, 22 * 60));
        assert!(!schedule_allows(&s, 0, 12 * 60));
        assert!(schedule_allows(&s, 6, 12 * 60));
        assert!(schedule_allows(&[], 3, 3));
        // Oltre la mezzanotte: venerdì 22:00-02:00 vale anche sabato all'1:00, non sabato alle 3:00.
        let n = vec![ScheduleSlot { days: vec![5], start: 1320, end: 120 }];
        assert!(schedule_allows(&n, 5, 23 * 60));
        assert!(schedule_allows(&n, 6, 60));
        assert!(!schedule_allows(&n, 6, 180));
        assert!(!schedule_allows(&n, 4, 23 * 60));
    }
}

#[cfg(test)]
mod mediabrowser_tests {
    use super::*;

    #[test]
    fn quoted_and_unquoted() {
        let a = mediabrowser_fields(r#"MediaBrowser Client="Infuse-Direct", Device="iPhone", DeviceId="X1", Version="8.0", Token="abc""#);
        assert_eq!((a["client"].as_str(), a["token"].as_str()), ("Infuse-Direct", "abc"));
        // Swiftfin 1.7: senza virgolette, ordine qualsiasi, spazi codificati.
        let b = mediabrowser_fields("MediaBrowser Token=def123, Client=Swiftfin%20iOS, Version=1.7, Device=iPhone, DeviceId=iOS_2A72");
        assert_eq!((b["client"].as_str(), b["token"].as_str(), b["deviceid"].as_str()), ("Swiftfin iOS", "def123", "iOS_2A72"));
        assert!(!mediabrowser_fields("Bearer xyz").contains_key("token"));
    }
}
