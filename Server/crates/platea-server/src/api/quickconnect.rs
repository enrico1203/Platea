//! Quick Connect nell'API nativa: il dispositivo chiede un codice, l'utente lo conferma da un dispositivo già collegato.

use super::{Access, Api};
use crate::auth::{issue_token, AuthUser};
use crate::error::{ApiError, ApiResult};
use crate::quickconnect::Pending;
use crate::state::Shared;
use axum::extract::{ConnectInfo, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::{IpAddr, SocketAddr};

pub fn routes(api: Api) -> Api {
    api.post("/api/v1/quickconnect/initiate", "Accesso", Access::Public, "Quick Connect: il dispositivo chiede un codice di 6 cifre e un segreto", initiate)
        .get("/api/v1/quickconnect/status/{secret}", "Accesso", Access::Public, "Quick Connect: stato della richiesta (pending o authorized)", status)
        .post("/api/v1/quickconnect/redeem", "Accesso", Access::Public, "Quick Connect: scambia il segreto confermato con un token (una volta sola)", redeem)
        .post("/api/v1/quickconnect/authorize", "Accesso", Access::User, "Quick Connect: conferma il codice mostrato da un altro dispositivo", authorize)
}

pub fn enabled(state: &Shared) -> ApiResult<()> {
    if state.settings.get().general.quick_connect {
        Ok(())
    } else {
        Err(ApiError::coded(StatusCode::FORBIDDEN, "QuickConnectDisabled", "Quick Connect è disattivato su questo server"))
    }
}

#[derive(Deserialize, Default)]
pub struct InitiateReq {
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub device_name: Option<String>,
    #[serde(default)]
    pub client: Option<String>,
    #[serde(default)]
    pub client_version: Option<String>,
}

/// Richiesta nuova, condivisa con lo strato compatibile Jellyfin: (codice, segreto, scadenza, id del dispositivo).
pub fn start(state: &Shared, req: &InitiateReq, ip: Option<IpAddr>) -> ApiResult<(String, String, i64, String)> {
    enabled(state)?;
    let device_id = req.device_id.clone().filter(|d| !d.is_empty()).unwrap_or_else(|| crate::util::random_hex(8));
    let ip = ip.map(|i| i.to_string()).unwrap_or_default();
    let (code, secret, expires) = state
        .quick_connect
        .initiate(
            &device_id,
            req.device_name.as_deref().unwrap_or("Dispositivo"),
            req.client.as_deref().unwrap_or("Sconosciuto"),
            req.client_version.as_deref().unwrap_or(""),
            &ip,
        )
        .ok_or_else(|| ApiError::TooManyRequests("Troppe richieste di Quick Connect in attesa: riprova tra qualche minuto".into()))?;
    Ok((code, secret, expires, device_id))
}

async fn initiate(
    State(state): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Option<Json<InitiateReq>>,
) -> ApiResult<Json<Value>> {
    let ip = crate::net::client_ip(Some(peer), &headers, &state.settings.get().network.trusted_proxies);
    let req = body.map(|b| b.0).unwrap_or_default();
    let (code, secret, expires_at, _) = start(&state, &req, ip)?;
    Ok(Json(json!({ "code": code, "secret": secret, "expires_at": expires_at })))
}

async fn status(State(state): State<Shared>, Path(secret): Path<String>) -> ApiResult<Json<Value>> {
    enabled(&state)?;
    let p = state.quick_connect.status(&secret).ok_or_else(|| ApiError::not_found("Richiesta di Quick Connect"))?;
    Ok(Json(json!({ "state": if p.user_id.is_some() { "authorized" } else { "pending" }, "expires_at": p.created_at + crate::quickconnect::TTL_MS })))
}

/// Token per una richiesta confermata, condiviso con lo strato compatibile: (utente, nome, admin, token).
pub async fn finish(state: &Shared, secret: &str, ip: Option<IpAddr>) -> ApiResult<(i64, String, bool, String, Pending)> {
    enabled(state)?;
    let p = state.quick_connect.redeem(secret).ok_or(ApiError::Unauthorized)?;
    let user_id = p.user_id.ok_or(ApiError::Unauthorized)?;
    let row: Option<(String, i64, i64)> =
        sqlx::query_as("SELECT username, is_admin, disabled FROM users WHERE id = ?").bind(user_id).fetch_optional(&state.db.read).await?;
    let Some((username, is_admin, 0)) = row else { return Err(ApiError::Unauthorized) };
    let t = issue_token(state, user_id, &p.device_id, &p.device_name, &p.client, &p.client_version, ip).await?;
    state
        .db
        .activity(
            Some(user_id),
            "login",
            &format!("{username} ha effettuato l'accesso con Quick Connect da {}", p.device_name),
            ip.map(|i| i.to_string()).as_deref(),
        )
        .await;
    Ok((user_id, username, is_admin != 0, t.token, p))
}

#[derive(Deserialize)]
struct RedeemReq {
    secret: String,
}

async fn redeem(
    State(state): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<RedeemReq>,
) -> ApiResult<Json<Value>> {
    let ip = crate::net::client_ip(Some(peer), &headers, &state.settings.get().network.trusted_proxies);
    let (user_id, username, is_admin, token, _) = finish(&state, &req.secret, ip).await?;
    Ok(Json(json!({ "token": token, "user": { "id": state.ids.encode(user_id), "username": username, "is_admin": is_admin } })))
}

/// Conferma condivisa con lo strato compatibile. Codici sbagliati ripetuti rallentano l'utente come i login sbagliati.
pub async fn confirm(state: &Shared, user: &AuthUser, code: &str) -> ApiResult<Pending> {
    enabled(state)?;
    let keys = vec![format!("qc:{}", user.user_id)];
    if let Some(wait) = state.login_limiter.check(&keys) {
        return Err(ApiError::TooManyRequests(format!("Troppi codici sbagliati: riprova tra {wait} s")));
    }
    match state.quick_connect.authorize(code, user.user_id) {
        Some(p) => {
            state.login_limiter.success(&keys);
            Ok(p)
        }
        None => {
            state.login_limiter.fail(&keys);
            Err(ApiError::coded(StatusCode::NOT_FOUND, "QuickConnectCodeInvalid", "Codice non valido o scaduto"))
        }
    }
}

#[derive(Deserialize)]
struct AuthorizeReq {
    code: String,
}

async fn authorize(State(state): State<Shared>, user: AuthUser, Json(req): Json<AuthorizeReq>) -> ApiResult<Json<Value>> {
    let p = confirm(&state, &user, &req.code).await?;
    Ok(Json(json!({ "ok": true, "device_name": p.device_name, "client": p.client })))
}
