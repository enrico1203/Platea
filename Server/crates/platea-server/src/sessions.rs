//! Sessioni di riproduzione.
//! Ogni sessione ha un token breve valido solo per lei: i player che non possono inviare intestazioni
//! lo ricevono nell'URL, e il token principale non finisce mai nei log.
//! Le posizioni di riproduzione si accumulano in memoria e partono insieme in una transazione.

use crate::nodes::hls::HlsHandle;
use crate::state::Shared;
use crate::util::{now_ms, random_hex};
use platea_playback::{Decision, Method};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

pub struct PlaySession {
    pub id: String,
    pub token: String,
    pub user_id: i64,
    /// Token del dispositivo: una nuova riproduzione dallo stesso dispositivo chiude la precedente.
    pub token_id: i64,
    /// Scadenza dell'account del proprietario: alla scadenza la sessione si chiude.
    pub expires_at: Option<i64>,
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
    /// Quando il client ha riferito l'ultima posizione: il pannello la fa avanzare da lì.
    pub position_at: AtomicI64,
    /// Inizio della pausa in corso (0 = in riproduzione).
    pub paused_since: AtomicI64,
    /// Indirizzo del client e se è fuori dalle reti locali.
    pub ip: Option<String>,
    pub remote: bool,
    /// Contenuto extra (trailer, dietro le quinte): l'avanzamento non tocca lo stato di visione del titolo.
    pub extra: bool,
    /// Sorgente e uscita (file, tracce, cosa fa il server), calcolate all'avvio.
    pub details: serde_json::Value,
    /// Byte inviati al client (file o segmenti), per la banda.
    pub bytes_sent: Arc<AtomicU64>,
    rate: Mutex<RateSample>,
    pub history_id: i64,
    pub hls: Option<Arc<HlsHandle>>,
    /// Copia da cui si legge il file in direct play (id del nodo, 0 = non ancora scelta) e nome del nodo.
    pub copy_node: AtomicI64,
    pub copy_name: Mutex<Option<String>>,
}

#[derive(Default)]
struct RateSample {
    at: i64,
    bytes: u64,
    bps: u64,
}

/// Campi che dipendono dal client, uguali per le sessioni web e Jellyfin.
pub struct SessionExtra {
    pub ip: Option<String>,
    pub remote: bool,
    pub details: serde_json::Value,
    pub position_ms: i64,
}

impl PlaySession {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        token: String,
        user: &crate::auth::AuthUser,
        item_id: i64,
        file_id: i64,
        title: String,
        subtitle: Option<String>,
        poster_image: Option<i64>,
        decision: Decision,
        chain: String,
        bitrate: Option<i64>,
        duration_ms: i64,
        history_id: i64,
        hls: Option<Arc<HlsHandle>>,
        extra: SessionExtra,
    ) -> Self {
        let now = now_ms();
        PlaySession {
            extra: false,
            id,
            token,
            user_id: user.user_id,
            token_id: user.token_id,
            expires_at: if user.is_admin { None } else { user.perms.expires_at },
            username: user.username.clone(),
            item_id,
            file_id,
            title,
            subtitle,
            poster_image,
            device_name: user.device_name.clone(),
            client: user.client.clone(),
            decision,
            chain,
            bitrate,
            duration_ms,
            started_at: now,
            last_seen: AtomicI64::new(now),
            position_ms: AtomicI64::new(extra.position_ms),
            paused: AtomicBool::new(false),
            position_at: AtomicI64::new(now),
            paused_since: AtomicI64::new(0),
            ip: extra.ip,
            remote: extra.remote,
            details: extra.details,
            bytes_sent: Arc::new(AtomicU64::new(0)),
            rate: Mutex::new(RateSample { at: now, ..Default::default() }),
            history_id,
            hls,
            copy_node: AtomicI64::new(0),
            copy_name: Mutex::new(None),
        }
    }

    pub fn touch(&self) {
        self.last_seen.store(now_ms(), Ordering::Relaxed);
    }

    /// Rapporto del client: posizione (se nota) e pausa, con l'istante in cui la pausa è iniziata.
    pub fn report(&self, position_ms: Option<i64>, paused: bool) {
        let now = now_ms();
        if let Some(p) = position_ms {
            self.position_ms.store(p, Ordering::Relaxed);
            self.position_at.store(now, Ordering::Relaxed);
        }
        self.paused.store(paused, Ordering::Relaxed);
        if paused {
            let _ = self.paused_since.compare_exchange(0, now, Ordering::Relaxed, Ordering::Relaxed);
        } else {
            self.paused_since.store(0, Ordering::Relaxed);
        }
        self.touch();
    }

    /// Banda media dall'ultima lettura (almeno 2 s), in bit/s.
    fn bandwidth(&self) -> u64 {
        let now = now_ms();
        let bytes = self.bytes_sent.load(Ordering::Relaxed);
        let mut r = self.rate.lock().unwrap();
        let dt = now - r.at;
        if dt >= 2000 {
            r.bps = bytes.saturating_sub(r.bytes) * 8 * 1000 / dt as u64;
            r.at = now;
            r.bytes = bytes;
        }
        r.bps
    }
}

fn lang(l: &Option<String>) -> Option<String> {
    l.as_ref().filter(|l| !l.is_empty() && l.as_str() != "und").cloned()
}

/// Sorgente e uscita di una riproduzione, per il pannello dell'amministratore.
pub fn details(info: &crate::api::playback::FileInfo, d: &Decision, fmp4: Option<bool>) -> serde_json::Value {
    use platea_playback::{AudioAction, SubtitleDelivery, VideoAction};
    let m = &info.media;
    let audio = d.audio_index.and_then(|i| m.audio.iter().find(|a| a.index == i)).or_else(|| m.audio.iter().find(|a| a.default)).or(m.audio.first());
    let sub_index = match d.subtitle {
        SubtitleDelivery::None => None,
        SubtitleDelivery::External { index } | SubtitleDelivery::Embedded { index } | SubtitleDelivery::BurnIn { index } => Some(index),
    };
    let sub = sub_index.and_then(|i| m.subtitles.iter().find(|s| s.index == i));
    let video = m.video.as_ref().map(|v| {
        serde_json::json!({
            "codec": v.codec, "profile": v.profile, "width": v.width, "height": v.height,
            "bit_depth": v.bit_depth, "hdr": v.hdr, "bitrate": v.bitrate,
        })
    });
    serde_json::json!({
        "file": {
            "name": info.abs.file_name().map(|n| n.to_string_lossy().to_string()),
            "size": info.size,
            "container": m.container,
            "bitrate": m.bitrate,
        },
        "video": video,
        "audio": audio.map(|a| serde_json::json!({ "index": a.index, "codec": a.codec, "channels": a.channels, "language": lang(&a.language), "bitrate": a.bitrate })),
        "audio_tracks": m.audio.len(),
        "subtitle": sub.map(|s| serde_json::json!({ "index": s.index, "codec": s.codec, "language": lang(&s.language), "forced": s.forced, "external": s.external })),
        "subtitle_tracks": m.subtitles.len(),
        "out": {
            "video": match &d.video {
                VideoAction::Copy => serde_json::json!({ "action": "copy" }),
                VideoAction::Transcode { codec, max_width, max_height, bitrate, tonemap, burn_subtitle } => serde_json::json!({
                    "action": "transcode", "codec": codec, "max_width": max_width, "max_height": max_height,
                    "bitrate": bitrate, "tonemap": tonemap, "burn_subtitle": burn_subtitle.is_some(),
                }),
            },
            "audio": match &d.audio {
                AudioAction::Copy => serde_json::json!({ "action": "copy" }),
                AudioAction::Transcode { codec, channels, bitrate } => serde_json::json!({ "action": "transcode", "codec": codec, "channels": channels, "bitrate": bitrate }),
                AudioAction::None => serde_json::json!({ "action": "none" }),
            },
            "subtitle": match d.subtitle {
                SubtitleDelivery::None => "none",
                SubtitleDelivery::External { .. } => "external",
                SubtitleDelivery::Embedded { .. } => "embedded",
                SubtitleDelivery::BurnIn { .. } => "burn_in",
            },
            "container": match fmp4 { None => "file", Some(true) => "hls_fmp4", Some(false) => "hls_ts" },
        },
    })
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
    /// Ora del server: il pannello fa avanzare la posizione da `position_at` senza dipendere dall'orologio del browser.
    pub server_now: i64,
    pub position_at: i64,
    pub paused_since: Option<i64>,
    pub last_seen: i64,
    pub ip: Option<String>,
    pub remote: bool,
    pub details: serde_json::Value,
    /// Fin dove è pronto il video sul server (HLS), in ms dall'inizio.
    pub buffered_until_ms: Option<i64>,
    /// Chi codifica davvero adesso (dopo un eventuale ripiego): "copia", "NVENC", "Software"…
    pub backend: Option<String>,
    /// Nodo che esegue la transcodifica o il remux (assente = questo server).
    pub executor: Option<String>,
    /// Nodo da cui si legge il file in direct play (assente = questo server).
    pub source: Option<String>,
    pub bandwidth_bps: u64,
    pub bytes_sent: u64,
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

    pub fn by_device(&self, token_id: i64) -> Vec<String> {
        self.by_id.read().unwrap().values().filter(|s| s.token_id == token_id).map(|s| s.id.clone()).collect()
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
        // Un audiolibro è finito all'ultimo minuto, non al 90%: si riprende anche dopo dieci ore su undici.
        let book =
            sqlx::query_scalar::<_, String>("SELECT kind FROM items WHERE id = ?").bind(item).fetch_optional(&mut *tx).await?.as_deref() == Some("audiobook");
        let played = if book { dur > 0 && pos >= dur - 60_000 } else { dur > 0 && pos * 100 >= dur * threshold };
        let pos_saved = if played || (!book && pos < resume_min) { 0 } else { pos };
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
        h.stop(state).await;
    }
    let pos = s.position_ms.load(Ordering::Relaxed);
    if pos > 0 && !s.extra {
        state.sessions.record_progress(s.user_id, s.item_id, pos, s.duration_ms);
    }
    let watched = (now_ms() - s.started_at).max(0);
    crate::webhooks::playback_stopped(state, &s, watched);
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
            if sess.expires_at.is_some_and(|e| now >= e) {
                tracing::info!(session = %sess.id, user = %sess.username, "sessione chiusa: account scaduto");
                stop_session(&state, &sess.id).await;
                continue;
            }
            if now - sess.last_seen.load(Ordering::Relaxed) > idle_ms {
                tracing::info!(session = %sess.id, user = %sess.username, "sessione chiusa per inattività");
                stop_session(&state, &sess.id).await;
                continue;
            }
            if let Some(h) = &sess.hls {
                h.maintain(ahead, keep).await;
            }
        }
        if n.is_multiple_of(3) {
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
            Some(h) => h.is_paused(state).await,
            None => false,
        },
        started_at: s.started_at,
        server_now: now_ms(),
        position_at: s.position_at.load(Ordering::Relaxed),
        paused_since: Some(s.paused_since.load(Ordering::Relaxed)).filter(|t| *t > 0),
        last_seen: s.last_seen.load(Ordering::Relaxed),
        ip: s.ip.clone(),
        remote: s.remote,
        details: s.details.clone(),
        buffered_until_ms: match &s.hls {
            Some(h) => h.buffered_until_ms(state).await,
            None => None,
        },
        backend: match &s.hls {
            Some(h) => Some(h.backend(state).await),
            None => None,
        },
        executor: s.hls.as_ref().map(|h| h.executor_name()).filter(|n| !n.is_empty()),
        source: s.copy_name.lock().unwrap().clone(),
        bandwidth_bps: s.bandwidth(),
        bytes_sent: s.bytes_sent.load(Ordering::Relaxed),
    }
}

pub fn method_rank(m: Method) -> u8 {
    match m {
        Method::Transcode => 0,
        Method::Remux => 1,
        Method::DirectPlay => 2,
    }
}
