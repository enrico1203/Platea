//! Procedura di primo avvio: crea il primo amministratore. Il codice stampato nei log impedisce
//! che qualcun altro sulla rete configuri il server prima del proprietario. Completata, l'endpoint
//! si disattiva in modo permanente.

use super::{Access, Api};
use crate::auth::{hash_password, issue_token};
use crate::error::{ApiError, ApiResult};
use crate::state::Shared;
use crate::util::{constant_time_eq, now_ms};
use axum::extract::{ConnectInfo, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::SocketAddr;

pub fn routes(api: Api) -> Api {
    api.post("/api/v1/setup/verify", "Primo avvio", Access::Public, "Verifica il codice di primo avvio", verify)
        .post("/api/v1/setup/admin", "Primo avvio", Access::Public, "Crea il primo amministratore con il codice di primo avvio", create_admin)
}

#[derive(Deserialize)]
struct VerifyReq {
    code: String,
}

fn code_matches(state: &Shared, code: &str) -> ApiResult<()> {
    let guard = state.setup_code.lock().unwrap();
    let Some(expected) = guard.as_ref() else {
        return Err(ApiError::Forbidden("La configurazione iniziale è già stata completata".into()));
    };
    let norm = |s: &str| s.chars().filter(|c| c.is_ascii_digit()).collect::<String>();
    if !constant_time_eq(norm(code).as_bytes(), norm(expected).as_bytes()) {
        return Err(ApiError::coded(axum::http::StatusCode::FORBIDDEN, "SetupCodeInvalid", "Il codice non corrisponde a quello nei log del server"));
    }
    Ok(())
}

async fn verify(State(state): State<Shared>, Json(req): Json<VerifyReq>) -> ApiResult<Json<Value>> {
    if let Some(wait) = state.login_limiter.check(&["setup".into()]) {
        return Err(ApiError::TooManyRequests(format!("Troppi tentativi: riprova tra {wait} s")));
    }
    if let Err(e) = code_matches(&state, &req.code) {
        state.login_limiter.fail(&["setup".into()]);
        return Err(e);
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct AdminReq {
    code: String,
    username: String,
    password: String,
    #[serde(default)]
    device_id: Option<String>,
    #[serde(default)]
    device_name: Option<String>,
    #[serde(default)]
    client: Option<String>,
    #[serde(default)]
    client_version: Option<String>,
}

pub fn validate_username(u: &str) -> ApiResult<String> {
    let u = u.trim();
    if u.is_empty() || u.chars().count() > 64 {
        return Err(ApiError::BadRequest("Il nome utente deve avere da 1 a 64 caratteri".into()));
    }
    if u.chars().any(|c| c.is_control()) {
        return Err(ApiError::BadRequest("Il nome utente contiene caratteri non ammessi".into()));
    }
    Ok(u.to_string())
}

pub fn validate_password(state: &Shared, p: &str) -> ApiResult<()> {
    let min = state.settings.get().general.min_password_length as usize;
    if p.chars().count() < min {
        return Err(ApiError::BadRequest(format!("La password deve avere almeno {min} caratteri")));
    }
    if p.len() > 1024 {
        return Err(ApiError::BadRequest("Password troppo lunga".into()));
    }
    Ok(())
}

async fn create_admin(
    State(state): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<AdminReq>,
) -> ApiResult<Json<Value>> {
    if let Some(wait) = state.login_limiter.check(&["setup".into()]) {
        return Err(ApiError::TooManyRequests(format!("Troppi tentativi: riprova tra {wait} s")));
    }
    if let Err(e) = code_matches(&state, &req.code) {
        state.login_limiter.fail(&["setup".into()]);
        return Err(e);
    }
    let username = validate_username(&req.username)?;
    validate_password(&state, &req.password)?;
    let hash = hash_password(req.password.clone()).await?;
    let now = now_ms();
    // Il codice si consuma prima della scrittura: due richieste parallele non creano due amministratori.
    {
        let mut guard = state.setup_code.lock().unwrap();
        if guard.is_none() {
            return Err(ApiError::Forbidden("La configurazione iniziale è già stata completata".into()));
        }
        *guard = None;
    }
    let res = sqlx::query(
        "INSERT INTO users (username, username_lower, password_hash, is_admin, created_at) VALUES (?, ?, ?, 1, ?)",
    )
    .bind(&username)
    .bind(username.to_lowercase())
    .bind(&hash)
    .bind(now)
    .execute(&state.db.write)
    .await;
    let user_id = match res {
        Ok(r) => r.last_insert_rowid(),
        Err(e) => {
            // Ripristina il codice: la procedura si può ripetere.
            *state.setup_code.lock().unwrap() = Some(crate::util::setup_code());
            return Err(ApiError::Internal(e.into()));
        }
    };
    state.db.state_set("setup_completed_at", &now.to_string()).await?;
    let ip = crate::net::client_ip(Some(peer), &headers, &state.settings.get().network.trusted_proxies);
    state.db.activity(Some(user_id), "setup", &format!("Primo amministratore creato: {username}"), ip.map(|i| i.to_string()).as_deref()).await;
    let t = issue_token(
        &state,
        user_id,
        req.device_id.as_deref().unwrap_or("setup"),
        req.device_name.as_deref().unwrap_or("Browser"),
        req.client.as_deref().unwrap_or("Platea Web"),
        req.client_version.as_deref().unwrap_or(""),
        ip,
    )
    .await?;
    tracing::info!(%username, "configurazione iniziale completata");
    Ok(Json(json!({ "token": t.token, "user": { "id": state.ids.encode(user_id), "username": username, "is_admin": true } })))
}
