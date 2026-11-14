//! Sessioni di riproduzione.
//! Ogni sessione ha un token breve valido solo per lei: i player che non possono inviare intestazioni
//! lo ricevono nell'URL, e il token principale non finisce mai nei log.
//! Le posizioni di riproduzione si accumulano in memoria e partono insieme in una transazione.

use crate::state::Shared;
use crate::transcode::HlsRuntime;
use crate::util::{now_ms, random_hex};
use platea_playback::{Decision, Method};
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

pub struct PlaySession {
    pub id: String,
    pub token: String,
    pub user_id: i64,
    pub username: String,
    pub item_id: i64,
    pub file_id: i64,
    pub title: String,
    pub subtitle: Option<String>,
    pub poster_image: Option<i64>,
    pub device_name: String,
    pub client: String,
    pub decision: Decision,
    pub chain: String,
    pub bitrate: Option<i64>,
    pub duration_ms: i64,
    pub started_at: i64,
    pub last_seen: AtomicI64,
    pub position_ms: AtomicI64,
    pub paused: AtomicBool,
    pub history_id: i64,
    pub hls: Option<Arc<HlsRuntime>>,
    /// Descrittore aperto per la durata della sessione: su un mount cloud riaprire costa secondi.
    pub file: Mutex<Option<Arc<File>>>,
}

impl PlaySession {
    pub fn touch(&self) {
        self.last_seen.store(now_ms(), Ordering::Relaxed);
    }
}

#[derive(Serialize)]
pub struct SessionView {
    pub id: String,
    pub user: String,
    pub item_id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub poster_url: Option<String>,
    pub device_name: String,
    pub client: String,
    pub method: String,
    pub reasons: Vec<String>,
    pub chain: String,
    pub bitrate: Option<i64>,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub paused: bool,
    pub throttled: bool,
    pub started_at: i64,
}

#[derive(Default)]
pub struct SessionManager {
    by_id: RwLock<HashMap<String, Arc<PlaySession>>>,
    by_token: RwLock<HashMap<String, String>>,
    /// (user_id, item_id) → (posizione, durata). Svuotata ogni pochi secondi.
    pending_progress: Mutex<HashMap<(i64, i64), (i64, i64)>>,
}

impl SessionManager {
    pub fn new_ids() -> (String, String) {
        (random_hex(12), random_hex(24))
    }

    pub fn insert(&self, s: Arc<PlaySession>) {
        self.by_token.write().unwrap().insert(s.token.clone(), s.id.clone());
        self.by_id.write().unwrap().insert(s.id.clone(), s);
    }

    pub fn get(&self, id: &str) -> Option<Arc<PlaySession>> {
        self.by_id.read().unwrap().get(id).cloned()
    }

    pub fn by_token(&self, token: &str) -> Option<Arc<PlaySession>> {
        let id = self.by_token.read().unwrap().get(token).cloned()?;
        self.get(&id)
    }

    pub fn active_count(&self) -> usize {
        self.by_id.read().unwrap().len()
    }

    pub fn user_count(&self, user_id: i64) -> usize {
        self.by_id.read().unwrap().values().filter(|s| s.user_id == user_id).count()
    }

    pub fn all(&self) -> Vec<Arc<PlaySession>> {
        self.by_id.read().unwrap().values().cloned().collect()
    }

    pub fn record_progress(&self, user_id: i64, item_id: i64, position_ms: i64, duration_ms: i64) {
        self.pending_progress.lock().unwrap().insert((user_id, item_id), (position_ms, duration_ms));
    }

    fn take_progress(&self) -> HashMap<(i64, i64), (i64, i64)> {
        std::mem::take(&mut *self.pending_progress.lock().unwrap())
    }

    fn remove(&self, id: &str) -> Option<Arc<PlaySession>> {
        let s = self.by_id.write().unwrap().remove(id)?;
        self.by_token.write().unwrap().remove(&s.token);
        Some(s)
    }
}

/// Scrive le posizioni accumulate: una transazione per tutte.
pub async fn flush_progress(state: &Shared) -> anyhow::Result<()> {
    let pending = state.sessions.take_progress();
    if pending.is_empty() {
        return Ok(());
    }
    let s = state.settings.get();
    let threshold = s.playback.played_threshold_percent as i64;
    let resume_min = s.playback.resume_min_seconds as i64 * 1000;
    let now = now_ms();
    let mut tx = state.db.write.begin().await?;
    for ((user, item), (pos, dur)) in pending {
        let played = dur > 0 && pos * 100 >= dur * threshold;
        let pos_saved = if played || pos < resume_min { 0 } else { pos };
        sqlx::query(
            "INSERT INTO user_item_data (user_id, item_id, position_ms, played, play_count, last_played_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(user_id, item_id) DO UPDATE SET
               position_ms = ?3,
               played = CASE WHEN ?4 = 1 THEN 1 ELSE played END,
               play_count = play_count + CASE WHEN ?4 = 1 AND played = 0 THEN 1 ELSE 0 END,
               last_played_at = ?6",
        )
        .bind(user)
        .bind(item)
        .bind(pos_saved)
        .bind(played as i64)
        .bind(played as i64)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        if played {
            mark_parents(&mut tx, user, item, now).await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

/// Un episodio visto fa avanzare "ultima visione" della serie (per "continua a guardare").
async fn mark_parents(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, user: i64, item: i64, now: i64) -> anyhow::Result<()> {
    let series: Option<Option<i64>> = sqlx::query_scalar("SELECT series_id FROM items WHERE id = ?").bind(item).fetch_optional(&mut **tx).await?;
    if let Some(Some(series_id)) = series {
        sqlx::query(
            "INSERT INTO user_item_data (user_id, item_id, last_played_at) VALUES (?, ?, ?)
             ON CONFLICT(user_id, item_id) DO UPDATE SET last_played_at = excluded.last_played_at",
        )
        .bind(user)
        .bind(series_id)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// Chiude una sessione: ferma FFmpeg, scrive la posizione e la cronologia.
pub async fn stop_session(state: &Shared, id: &str) -> Option<Arc<PlaySession>> {
    let s = state.sessions.remove(id)?;
    if let Some(h) = &s.hls {
        h.stop().await;
    }
    let pos = s.position_ms.load(Ordering::Relaxed);
    if pos > 0 {
        state.sessions.record_progress(s.user_id, s.item_id, pos, s.duration_ms);
    }
    let watched = (now_ms() - s.started_at).max(0);
    let _ = sqlx::query("UPDATE playback_history SET ended_at = ?, watched_ms = ? WHERE id = ?")
        .bind(now_ms())
        .bind(watched)
        .bind(s.history_id)
        .execute(&state.db.write)
        .await;
    let _ = flush_progress(state).await;
    state.events.admin("sessions.changed", serde_json::json!({ "stopped": id }));
    state.events.to_user(s.user_id, "user_data.changed", serde_json::json!({ "item_id": state.ids.encode(s.item_id) }));
    Some(s)
}

/// Ciclo di manutenzione: scrittura delle posizioni, rallentamento di FFmpeg, chiusura delle sessioni inattive.
pub async fn run_maintenance(state: Shared) {
    let mut tick = tokio::time::interval(Duration::from_secs(2));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut n: u64 = 0;
    loop {
        tick.tick().await;
        n += 1;
        let s = state.settings.get();
        let seg = s.playback.segment_seconds.max(2) as usize;
        let ahead = (s.playback.throttle_ahead_seconds as usize / seg).max(5);
        let keep = s.playback.keep_segments_behind as usize;
        let idle_ms = s.playback.session_idle_seconds as i64 * 1000;
        let now = now_ms();
        for sess in state.sessions.all() {
            if now - sess.last_seen.load(Ordering::Relaxed) > idle_ms {
                tracing::info!(session = %sess.id, user = %sess.username, "sessione chiusa per inattività");
                stop_session(&state, &sess.id).await;
                continue;
            }
            if let Some(h) = &sess.hls {
                h.maintain(ahead, keep).await;
            }
        }
        if n % 3 == 0 {
            if let Err(e) = flush_progress(&state).await {
                tracing::warn!(error = %e, "scrittura delle posizioni non riuscita");
            }
        }
    }
}

pub async fn session_view(state: &Shared, s: &PlaySession) -> SessionView {
    SessionView {
        id: s.id.clone(),
        user: s.username.clone(),
        item_id: state.ids.encode(s.item_id),
        title: s.title.clone(),
        subtitle: s.subtitle.clone(),
        poster_url: s.poster_image.map(|i| format!("/api/v1/images/{}?w=185", state.ids.encode(i))),
        device_name: s.device_name.clone(),
        client: s.client.clone(),
        method: s.decision.method.as_str().to_string(),
        reasons: s.decision.reasons.iter().map(|r| r.as_str().to_string()).collect(),
        chain: s.chain.clone(),
        bitrate: s.bitrate,
        position_ms: s.position_ms.load(Ordering::Relaxed),
        duration_ms: s.duration_ms,
        paused: s.paused.load(Ordering::Relaxed),
        throttled: match &s.hls {
            Some(h) => h.is_paused().await,
            None => false,
        },
        started_at: s.started_at,
    }
}

pub fn method_rank(m: Method) -> u8 {
    match m {
        Method::Transcode => 0,
        Method::Remux => 1,
        Method::DirectPlay => 2,
    }
}
