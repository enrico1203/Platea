//! Download del file originale, con il permesso per utente.
//! Il client chiede un link (POST): il server verifica permesso, scadenza, fasce orarie e visibilità
//! dell'elemento e restituisce un indirizzo firmato valido 6 ore, che un browser o un download manager
//! possono aprire senza intestazioni e riprendere con richieste Range. Nessuna copia: si serve il file com'è.

use super::{Access, Api};
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::Shared;
use crate::util::now_ms;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, Method};
use axum::response::Response;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;

const VALID_MS: i64 = 6 * 3_600_000;

struct Grant {
    user_id: i64,
    file_id: i64,
    expires: i64,
}

static GRANTS: once_cell::sync::Lazy<Mutex<HashMap<String, Grant>>> = once_cell::sync::Lazy::new(Default::default);

pub fn routes(api: Api) -> Api {
    api.post("/api/v1/items/{id}/download", "Riproduzione", Access::User, "Link di download del file originale (permesso dell'utente, valido 6 ore)", create)
        .get("/api/v1/download/{token}/{name}", "Riproduzione", Access::Public, "Scarica il file originale (link firmato, riprendibile)", serve)
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct DownloadReq {
    file_id: Option<String>,
}

/// Nome del file proposto al browser: quello originale, senza percorso.
fn file_name(abs: &std::path::Path) -> String {
    abs.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "video".into())
}

pub async fn choose_file(state: &Shared, item_id: i64, wanted: Option<i64>) -> ApiResult<i64> {
    if let Some(f) = wanted {
        let ok: Option<i64> = sqlx::query_scalar("SELECT id FROM media_files WHERE id = ? AND item_id = ? AND state = 'present'")
            .bind(f)
            .bind(item_id)
            .fetch_optional(&state.db.read)
            .await?;
        return ok.ok_or_else(|| ApiError::not_found("File"));
    }
    sqlx::query_scalar("SELECT id FROM media_files WHERE item_id = ? AND state = 'present' ORDER BY size DESC LIMIT 1")
        .bind(item_id)
        .fetch_optional(&state.db.read)
        .await?
        .ok_or_else(|| ApiError::not_found("File"))
}

async fn create(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>, body: Option<Json<DownloadReq>>) -> ApiResult<Json<Value>> {
    user.require_download()?;
    let iid = state.ids.decode(&id).ok_or_else(|| ApiError::not_found("Elemento"))?;
    let item = super::items::load_item(&state, &user, iid).await?;
    if item.kind != "movie" && item.kind != "episode" {
        return Err(ApiError::BadRequest("Si scaricano film ed episodi".into()));
    }
    let wanted = body.and_then(|b| b.0.file_id).and_then(|f| state.ids.decode(&f));
    let fid = choose_file(&state, iid, wanted).await?;
    let info = super::playback::load_file(&state, fid).await?;
    let token = crate::util::random_hex(24);
    let now = now_ms();
    {
        let mut g = GRANTS.lock().unwrap();
        g.retain(|_, v| v.expires > now);
        g.insert(token.clone(), Grant { user_id: user.user_id, file_id: fid, expires: now + VALID_MS });
    }
    let name = file_name(&info.abs);
    state.db.activity(Some(user.user_id), "download", &format!("{} ha scaricato {}", user.username, item.title), None).await;
    Ok(Json(json!({
        "url": format!("/api/v1/download/{token}/{}", urlencoding::encode(&name)),
        "name": name,
        "size": info.size,
        "expires_at": now + VALID_MS,
    })))
}

async fn serve(State(state): State<Shared>, Path((token, _name)): Path<(String, String)>, method: Method, headers: HeaderMap) -> ApiResult<Response> {
    let (user_id, file_id) = {
        let g = GRANTS.lock().unwrap();
        let grant = g.get(&token).filter(|v| v.expires > now_ms()).ok_or(ApiError::Unauthorized)?;
        (grant.user_id, grant.file_id)
    };
    // A ogni richiesta (anche le riprese) i permessi attuali: un account disattivato o scaduto si ferma subito.
    let (_, is_admin, perms) = crate::auth::load_permissions(&state, user_id).await.map_err(ApiError::Internal)?.ok_or(ApiError::Unauthorized)?;
    if !is_admin
        && (perms.expired() || !perms.allow_download || !crate::auth::schedule_allows(&perms.schedule, crate::util::local_now().0, crate::util::local_now().1))
    {
        return Err(ApiError::coded(axum::http::StatusCode::FORBIDDEN, "DownloadNotAllowed", "Download non consentito"));
    }
    let info = super::playback::load_file(&state, file_id).await?;
    let timeout = std::time::Duration::from_secs(state.settings.get().scanner.fs_timeout_seconds as u64);
    let file = crate::stream::open_library_file(file_id, &info.abs, &info.root, timeout).await?;
    let ct = crate::stream::content_type_for(&info.media.container, &info.abs);
    let mut resp = crate::stream::serve_file(file, info.size as u64, info.mtime, ct, &method, &headers, None)?;
    let name = file_name(&info.abs);
    let disp = format!("attachment; filename=\"{}\"; filename*=UTF-8''{}", name.replace(['"', '\\'], "_"), urlencoding::encode(&name));
    if let Ok(v) = HeaderValue::from_str(&disp) {
        resp.headers_mut().insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(resp)
}

/// Download diretto per lo strato compatibile (Infuse: `/Items/{id}/Download`), con lo stesso permesso.
pub async fn serve_for_user(state: &Shared, user: &AuthUser, item_id: i64, method: &Method, headers: &HeaderMap) -> ApiResult<Response> {
    user.require_download()?;
    super::items::load_item(state, user, item_id).await?;
    let fid = choose_file(state, item_id, None).await?;
    let info = super::playback::load_file(state, fid).await?;
    let timeout = std::time::Duration::from_secs(state.settings.get().scanner.fs_timeout_seconds as u64);
    let file = crate::stream::open_library_file(fid, &info.abs, &info.root, timeout).await?;
    let ct = crate::stream::content_type_for(&info.media.container, &info.abs);
    let mut resp = crate::stream::serve_file(file, info.size as u64, info.mtime, ct, method, headers, None)?;
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename*=UTF-8''{}", urlencoding::encode(&file_name(&info.abs)))) {
        resp.headers_mut().insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(resp)
}
