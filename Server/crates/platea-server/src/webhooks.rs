//! Webhook: gli eventi scelti in Impostazioni › Notifiche partono con una POST verso gli indirizzi configurati.
//! Corpo JSON generico (`event`, `text`, `server`, `time`, `data`); a Discord va un messaggio già pronto (`content`),
//! a ntfy il solo testo. Ogni invio ha un tempo limite e non blocca mai chi lo genera; un errore finisce nel log.

use crate::state::Shared;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Playback,
    ItemAdded,
    Login,
    Problem,
}

/// Invii in corso: oltre questo numero gli eventi nuovi si scartano (un endpoint lento non accumula task).
static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
const MAX_IN_FLIGHT: usize = 32;

/// Corpo della richiesta per un indirizzo: Discord e ntfy hanno formati propri.
pub fn body_for(url: &str, event: &str, text: &str, server: &str, data: &Value) -> (String, &'static str) {
    if url.contains("discord.com/api/webhooks") || url.contains("discordapp.com/api/webhooks") {
        let content: String = format!("**{server}** · {text}").chars().take(1900).collect();
        return (json!({ "content": content, "username": "Platea" }).to_string(), "application/json");
    }
    if url.contains("ntfy.") {
        return (format!("{server}: {text}"), "text/plain; charset=utf-8");
    }
    (json!({ "event": event, "text": text, "server": server, "time": crate::util::iso(crate::util::now_ms()), "data": data }).to_string(), "application/json")
}

/// Invia un evento a tutti i webhook, se il suo tipo è attivo. Non attende: l'invio parte in un task.
pub fn emit(state: &Shared, kind: Kind, event: &str, text: impl Into<String>, data: Value) {
    let s = state.settings.get();
    let n = &s.notifications;
    let on = match kind {
        Kind::Playback => n.on_playback,
        Kind::ItemAdded => n.on_item_added,
        Kind::Login => n.on_login,
        Kind::Problem => n.on_problem,
    };
    if !on || n.webhook_urls.is_empty() {
        return;
    }
    if IN_FLIGHT.load(Ordering::Relaxed) >= MAX_IN_FLIGHT {
        tracing::warn!(event, "webhook scartato: troppi invii in corso");
        return;
    }
    let urls = n.webhook_urls.clone();
    let server = s.general.server_name.clone();
    let (event, text) = (event.to_string(), text.into());
    let http = state.http.clone();
    IN_FLIGHT.fetch_add(1, Ordering::Relaxed);
    tokio::spawn(async move {
        for url in urls {
            let (body, ctype) = body_for(&url, &event, &text, &server, &data);
            let r = http.post(&url).header("content-type", ctype).body(body).timeout(Duration::from_secs(10)).send().await;
            match r {
                Ok(resp) if resp.status().is_success() => {}
                Ok(resp) => tracing::warn!(event = %event, status = %resp.status(), "webhook rifiutato"),
                Err(e) => tracing::warn!(event = %event, error = %e, "webhook non raggiungibile"),
            }
        }
        IN_FLIGHT.fetch_sub(1, Ordering::Relaxed);
    });
}

/// Ultimo avviso di inizio per (utente, elemento): un ripiego del player apre una sessione nuova, non una visione nuova.
static LAST_START: once_cell::sync::Lazy<std::sync::Mutex<std::collections::HashMap<(i64, i64), i64>>> = once_cell::sync::Lazy::new(Default::default);

fn who_what(s: &crate::sessions::PlaySession) -> String {
    match &s.subtitle {
        Some(sub) => format!("{} · {sub}", s.title),
        None => s.title.clone(),
    }
}

/// Inizio di una riproduzione (web, app, Infuse): al massimo un avviso ogni 10 minuti per utente e titolo.
pub fn playback_started(state: &Shared, s: &crate::sessions::PlaySession) {
    let now = crate::util::now_ms();
    {
        let mut m = LAST_START.lock().unwrap();
        m.retain(|_, t| now - *t < 600_000);
        if m.contains_key(&(s.user_id, s.item_id)) {
            return;
        }
        m.insert((s.user_id, s.item_id), now);
    }
    emit(
        state,
        Kind::Playback,
        "playback.start",
        format!("▶ {} guarda {} su {}", s.username, who_what(s), s.device_name),
        json!({ "user": s.username, "title": s.title, "subtitle": s.subtitle, "device": s.device_name, "client": s.client, "method": s.decision.method.as_str(), "remote": s.remote, "item_id": state.ids.encode(s.item_id) }),
    );
}

/// Fine di una riproduzione durata almeno un minuto (le sessioni sostituite da un ripiego non contano).
pub fn playback_stopped(state: &Shared, s: &crate::sessions::PlaySession, watched_ms: i64) {
    if watched_ms < 60_000 {
        return;
    }
    let pos = s.position_ms.load(Ordering::Relaxed);
    let pct = if s.duration_ms > 0 { (pos * 100 / s.duration_ms).clamp(0, 100) } else { 0 };
    emit(
        state,
        Kind::Playback,
        "playback.stop",
        format!("⏹ {} ha smesso di guardare {} ({pct}%)", s.username, who_what(s)),
        json!({ "user": s.username, "title": s.title, "subtitle": s.subtitle, "device": s.device_name, "watched_ms": watched_ms, "position_ms": pos, "percent": pct, "item_id": state.ids.encode(s.item_id) }),
    );
}

/// Film o serie appena associato per la prima volta. Durante la prima importazione di una libreria (elementi nati
/// entro 6 ore dalla libreria) non parte nulla: sarebbero migliaia di avvisi. Nemmeno per elementi vecchi.
pub async fn item_added(state: &Shared, item_id: i64) {
    let row: Option<(String, String, Option<i64>, i64, i64, String)> =
        sqlx::query_as("SELECT i.kind, i.title, i.year, i.added_at, l.created_at, l.name FROM items i JOIN libraries l ON l.id = i.library_id WHERE i.id = ?")
            .bind(item_id)
            .fetch_optional(&state.db.read)
            .await
            .ok()
            .flatten();
    let Some((kind, title, year, added, lib_created, lib)) = row else { return };
    // Solo elementi arrivati nelle ultime 24 ore: un rinnovo dei metadati rimette "in attesa" anche titoli vecchi.
    let recent = crate::util::now_ms() - added < 24 * 3_600_000;
    if !matches!(kind.as_str(), "movie" | "series") || added - lib_created < 6 * 3_600_000 || !recent {
        return;
    }
    let what = if kind == "movie" { "Film nuovo" } else { "Serie nuova" };
    let y = year.map(|y| format!(" ({y})")).unwrap_or_default();
    emit(
        state,
        Kind::ItemAdded,
        "item.added",
        format!("🎬 {what} in {lib}: {title}{y}"),
        json!({ "kind": kind, "title": title, "year": year, "library": lib, "item_id": state.ids.encode(item_id) }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        let d = json!({ "title": "Dune" });
        let (b, t) = body_for("https://discord.com/api/webhooks/1/abc", "playback.start", "mario guarda Dune", "Casa", &d);
        assert_eq!(t, "application/json");
        assert!(b.contains("\"content\":\"**Casa** · mario guarda Dune\""));
        let (b, t) = body_for("https://ntfy.sh/platea", "playback.start", "mario guarda Dune", "Casa", &d);
        assert_eq!((b.as_str(), t), ("Casa: mario guarda Dune", "text/plain; charset=utf-8"));
        let (b, _) = body_for("https://example.org/hook", "playback.start", "mario guarda Dune", "Casa", &d);
        let v: Value = serde_json::from_str(&b).unwrap();
        assert_eq!(v["event"], "playback.start");
        assert_eq!(v["data"]["title"], "Dune");
    }
}
