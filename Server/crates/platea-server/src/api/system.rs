//! Informazioni pubbliche sul server, per il collegamento iniziale dei client.

use super::{Access, Api};
use crate::state::{Shared, VERSION};
use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

pub fn routes(api: Api) -> Api {
    api.get("/api/v1/system/info", "Sistema", Access::Public, "Nome, identificativo e versione del server; stato della configurazione iniziale", info)
        .get("/api/v1/health", "Sistema", Access::Public, "Stato di salute", health)
        .get("/api/v1/system/custom.css", "Sistema", Access::Public, "CSS personalizzato della web UI (Impostazioni › Generali)", custom_css)
}

async fn custom_css(State(state): State<Shared>) -> axum::response::Response {
    let css = state.settings.get().general.custom_css.clone();
    axum::response::Response::builder()
        .header(axum::http::header::CONTENT_TYPE, "text/css; charset=utf-8")
        .header(axum::http::header::CACHE_CONTROL, "public, max-age=86400")
        .header("X-Content-Type-Options", "nosniff")
        .body(axum::body::Body::from(css))
        .unwrap()
}

async fn info(State(state): State<Shared>) -> Json<Value> {
    let s = state.settings.get();
    let setup_required = state.setup_code.lock().unwrap().is_some();
    Json(json!({
        "name": s.general.server_name,
        "id": state.server_id,
        "version": VERSION,
        "product": "Platea",
        "api": "v1",
        "setup_required": setup_required,
        "quick_connect": s.general.quick_connect,
        "login_message": Some(s.general.login_message.trim()).filter(|m| !m.is_empty()),
        // Versione del CSS personalizzato (per la cache del browser); null = nessuno.
        "custom_css": Some(&s.general.custom_css).filter(|c| !c.trim().is_empty()).map(|c| crate::util::sha256_hex(c)[..12].to_string()),
    }))
}

async fn health(State(state): State<Shared>) -> Json<Value> {
    let db_ok = sqlx::query_scalar::<_, i64>("SELECT 1").fetch_one(&state.db.read).await.is_ok();
    Json(json!({ "status": if db_ok { "ok" } else { "degraded" }, "database": db_ok, "ffmpeg": state.ffmpeg.get().available }))
}
