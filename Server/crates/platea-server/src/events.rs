//! Bus interno degli eventi: libreria aggiornata, sessioni, lavori, comandi remoti.
//! I client li ricevono via WebSocket: nessun client interroga il server a intervalli.

use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

/// Eventi che arrivano a raffica (un'analisi di 150.000 file, migliaia di immagini): al massimo uno ogni
/// `NOISY_EVERY` per tipo, più uno finale dopo l'ultimo soppresso. I client ricaricano comunque per gruppo,
/// quindi i dati del singolo evento non servono.
const NOISY: &[&str] = &["item.probed", "item.updated", "jobs.changed"];
const NOISY_EVERY: Duration = Duration::from_secs(3);

#[derive(Default)]
struct Throttle {
    last: Option<Instant>,
    trailing: bool,
}

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
    throttle: Arc<Mutex<HashMap<String, Throttle>>>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1024);
        EventBus { tx, throttle: Arc::default() }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    pub fn publish(&self, kind: &str, data: Value) {
        self.send(Event { kind: kind.into(), user_id: None, admin_only: false, data });
    }

    pub fn admin(&self, kind: &str, data: Value) {
        self.send(Event { kind: kind.into(), user_id: None, admin_only: true, data });
    }

    fn send(&self, ev: Event) {
        if !NOISY.contains(&ev.kind.as_str()) {
            let _ = self.tx.send(ev);
            return;
        }
        let now = Instant::now();
        let mut map = self.throttle.lock().unwrap();
        let t = map.entry(ev.kind.clone()).or_default();
        if t.last.is_none_or(|l| now.duration_since(l) >= NOISY_EVERY) {
            t.last = Some(now);
            drop(map);
            let _ = self.tx.send(ev);
            return;
        }
        if t.trailing {
            return;
        }
        // Uno finale, così i client vedono anche l'ultimo cambiamento della raffica.
        t.trailing = true;
        let wait = NOISY_EVERY.saturating_sub(now.duration_since(t.last.unwrap_or(now)));
        drop(map);
        let me = self.clone();
        let Ok(rt) = tokio::runtime::Handle::try_current() else { return };
        rt.spawn(async move {
            tokio::time::sleep(wait).await;
            {
                let mut map = me.throttle.lock().unwrap();
                if let Some(t) = map.get_mut(&ev.kind) {
                    t.trailing = false;
                    t.last = Some(Instant::now());
                }
            }
            let _ = me.tx.send(Event { data: Value::Null, ..ev });
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn noisy_events_are_coalesced() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();
        for _ in 0..1000 {
            bus.publish("item.probed", Value::Null);
        }
        bus.publish("library.updated", Value::Null);
        // Il primo subito, gli altri tipi passano sempre.
        assert_eq!(rx.recv().await.unwrap().kind, "item.probed");
        assert_eq!(rx.recv().await.unwrap().kind, "library.updated");
        // Uno finale dopo la finestra, non 999.
        tokio::time::sleep(NOISY_EVERY + Duration::from_millis(10)).await;
        assert_eq!(rx.recv().await.unwrap().kind, "item.probed");
        assert!(rx.try_recv().is_err());
    }
}
