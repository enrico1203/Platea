//! Canale WebSocket per gli eventi: libreria aggiornata, sessioni, lavori, comandi remoti.

use super::{Access, Api};
use crate::auth::{query_param, resolve_token};
use crate::state::Shared;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
use axum::http::Uri;
use axum::response::Response;
use std::net::SocketAddr;
use std::time::Duration;

pub fn routes(api: Api) -> Api {
    api.get("/api/v1/events", "Eventi", Access::User, "WebSocket degli eventi (token nella query: ?token=)", ws)
}

async fn ws(
    State(state): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    uri: Uri,
    upgrade: WebSocketUpgrade,
) -> Result<Response, crate::error::ApiError> {
    let parts = axum::http::Request::builder().uri(uri).body(()).unwrap().into_parts().0;
    let token = query_param(&parts, "token").ok_or(crate::error::ApiError::Unauthorized)?;
    let user = resolve_token(&state, &token, Some(peer.ip())).await?;
    Ok(upgrade.on_upgrade(move |socket| handle(state, socket, user.user_id, user.is_admin)))
}

async fn handle(state: Shared, mut socket: WebSocket, user_id: i64, is_admin: bool) {
    let mut rx = state.events.subscribe();
    // Gli amministratori ricevono anche le righe del log.
    let mut logs = crate::logs::LOGS.tx.subscribe();
    let mut ping = tokio::time::interval(Duration::from_secs(30));
    let _ = socket.send(Message::Text(serde_json::json!({ "type": "hello" }).to_string().into())).await;
    loop {
        tokio::select! {
            ev = rx.recv() => {
                match ev {
                    Ok(e) => {
                        if e.admin_only && !is_admin { continue; }
                        if let Some(u) = e.user_id { if u != user_id { continue; } }
                        let text = serde_json::to_string(&e).unwrap_or_default();
                        if socket.send(Message::Text(text.into())).await.is_err() { break; }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        // Il client è rimasto indietro: gli si dice di ricaricare.
                        let _ = socket.send(Message::Text(serde_json::json!({ "type": "resync" }).to_string().into())).await;
                    }
                    Err(_) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                }
            }
            line = logs.recv(), if is_admin => {
                if let Ok(l) = line {
                    let text = serde_json::json!({ "type": "log.line", "data": l }).to_string();
                    if socket.send(Message::Text(text.into())).await.is_err() { break; }
                }
            }
            _ = ping.tick() => {
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() { break; }
            }
        }
    }
}
