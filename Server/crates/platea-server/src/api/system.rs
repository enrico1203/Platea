//! Informazioni pubbliche sul server, per il collegamento iniziale dei client.

use super::{Access, Api};
use crate::state::{Shared, VERSION};
use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

pub fn routes(api: Api) -> Api {
    api.get("/api/v1/system/info", "Sistema", Access::Public, "Nome, identificativo e versione del server; stato della configurazione iniziale", info)
        .get("/api/v1/health", "Sistema", Access::Public, "Stato di salute", health)
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
    }))
}

async fn health(State(state): State<Shared>) -> Json<Value> {
    let db_ok = sqlx::query_scalar::<_, i64>("SELECT 1").fetch_one(&state.db.read).await.is_ok();
    Json(json!({ "status": if db_ok { "ok" } else { "degraded" }, "database": db_ok, "ffmpeg": state.ffmpeg.get().available }))
}
