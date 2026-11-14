//! Bus interno degli eventi: libreria aggiornata, sessioni, lavori, comandi remoti.
//! I client li ricevono via WebSocket: nessun client interroga il server a intervalli.

use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize)]
pub struct Event {
    #[serde(rename = "type")]
    pub kind: String,
    /// `None` = per tutti; altrimenti solo per l'utente indicato.
    #[serde(skip)]
    pub user_id: Option<i64>,
    /// Solo per gli amministratori.
    #[serde(skip)]
    pub admin_only: bool,
    pub data: Value,
}

#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<Event>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1024);
        EventBus { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    pub fn publish(&self, kind: &str, data: Value) {
        let _ = self.tx.send(Event { kind: kind.into(), user_id: None, admin_only: false, data });
    }

    pub fn admin(&self, kind: &str, data: Value) {
        let _ = self.tx.send(Event { kind: kind.into(), user_id: None, admin_only: true, data });
    }

    pub fn to_user(&self, user_id: i64, kind: &str, data: Value) {
        let _ = self.tx.send(Event { kind: kind.into(), user_id: Some(user_id), admin_only: false, data });
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
