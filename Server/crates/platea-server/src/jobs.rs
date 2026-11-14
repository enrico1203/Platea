//! Coda dei lavori persistente: tabella SQLite e worker Tokio.
//! Priorità fisse, limiti di concorrenza per tipo e per libreria, nuovi tentativi con attesa crescente.
//! Nessun broker esterno.

use crate::state::Shared;
use crate::util::now_ms;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{Notify, Semaphore};

/// Priorità: più basso = prima. Riproduzione, poi interattive, poi metadati, poi analisi, poi manutenzione.
pub mod priority {
    #[allow(dead_code)]
    pub const PLAYBACK: i64 = 0;
    pub const INTERACTIVE: i64 = 10;
    pub const SCAN: i64 = 20;
    pub const METADATA: i64 = 30;
    pub const IMAGES: i64 = 35;
    pub const PROBE: i64 = 40;
    pub const MAINTENANCE: i64 = 50;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobKind {
    ScanLibrary {
        library_id: i64,
        #[serde(default)]
        subpath: Option<String>,
    },
    ProbeFile {
        file_id: i64,
    },
    ExtractKeyframes {
        file_id: i64,
    },
    MatchItem {
        item_id: i64,
    },
    RefreshMetadata {
        item_id: i64,
    },
    DownloadImage {
        image_id: i64,
    },
    CleanupMissing,
}

impl JobKind {
    pub fn name(&self) -> &'static str {
        match self {
            JobKind::ScanLibrary { .. } => "scan_library",
            JobKind::ProbeFile { .. } => "probe_file",
            JobKind::ExtractKeyframes { .. } => "extract_keyframes",
            JobKind::MatchItem { .. } => "match_item",
            JobKind::RefreshMetadata { .. } => "refresh_metadata",
            JobKind::DownloadImage { .. } => "download_image",
            JobKind::CleanupMissing => "cleanup_missing",
        }
    }

    fn dedupe_key(&self) -> Option<String> {
        Some(match self {
            JobKind::ScanLibrary { library_id, subpath } => format!("scan:{library_id}:{}", subpath.clone().unwrap_or_default()),
            JobKind::ProbeFile { file_id } => format!("probe:{file_id}"),
            JobKind::ExtractKeyframes { file_id } => format!("kf:{file_id}"),
            JobKind::MatchItem { item_id } => format!("match:{item_id}"),
            JobKind::RefreshMetadata { item_id } => format!("refresh:{item_id}"),
            JobKind::DownloadImage { image_id } => format!("img:{image_id}"),
            JobKind::CleanupMissing => "cleanup".into(),
        })
    }

    /// Gruppo di concorrenza: ogni gruppo ha il proprio semaforo.
    fn lane(&self) -> &'static str {
        match self {
            JobKind::ScanLibrary { .. } => "scan",
            JobKind::ProbeFile { .. } | JobKind::ExtractKeyframes { .. } => "probe",
            JobKind::MatchItem { .. } | JobKind::RefreshMetadata { .. } => "metadata",
            JobKind::DownloadImage { .. } => "images",
            JobKind::CleanupMissing => "maintenance",
        }
    }
}

#[derive(Clone)]
pub struct JobQueue {
    notify: Arc<Notify>,
    lanes: Arc<HashMap<&'static str, Arc<Semaphore>>>,
    /// Semafori per libreria per l'analisi dei file (concorrenza configurata per libreria).
    probe_lanes: Arc<Mutex<HashMap<i64, (usize, Arc<Semaphore>)>>>,
    /// Ultimi avvii di analisi per libreria, per il tetto al minuto.
    probe_rate: Arc<Mutex<HashMap<i64, Vec<i64>>>>,
}

impl JobQueue {
    pub fn new() -> Self {
        let mut lanes = HashMap::new();
        lanes.insert("scan", Arc::new(Semaphore::new(2)));
        lanes.insert("probe", Arc::new(Semaphore::new(8)));
        lanes.insert("metadata", Arc::new(Semaphore::new(4)));
        lanes.insert("images", Arc::new(Semaphore::new(4)));
        lanes.insert("maintenance", Arc::new(Semaphore::new(1)));
        JobQueue { notify: Arc::new(Notify::new()), lanes: Arc::new(lanes), probe_lanes: Default::default(), probe_rate: Default::default() }
    }

    pub fn wake(&self) {
        self.notify.notify_one();
    }
}

impl Default for JobQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Mette in coda un lavoro. Un lavoro identico già in coda non viene duplicato
/// (ma la sua priorità sale se quella nuova è più alta).
pub async fn enqueue(state: &Shared, job: JobKind, priority: i64, library_id: Option<i64>) -> anyhow::Result<()> {
    enqueue_with(&state.db.write, &state.jobs, job, priority, library_id).await
}

pub async fn enqueue_with(pool: &sqlx::SqlitePool, queue: &JobQueue, job: JobKind, priority: i64, library_id: Option<i64>) -> anyhow::Result<()> {
    let now = now_ms();
    let payload = serde_json::to_string(&job)?;
    let dedupe = job.dedupe_key();
    let res = sqlx::query(
        "INSERT INTO jobs (kind, payload, library_id, priority, state, run_after, dedupe_key, created_at)
         VALUES (?, ?, ?, ?, 'queued', ?, ?, ?)
         ON CONFLICT (dedupe_key) WHERE state IN ('queued', 'running') AND dedupe_key IS NOT NULL
         DO UPDATE SET priority = MIN(priority, excluded.priority), run_after = MIN(run_after, excluded.run_after)",
    )
    .bind(job.name())
    .bind(&payload)
    .bind(library_id)
    .bind(priority)
    .bind(now)
    .bind(&dedupe)
    .bind(now)
    .execute(pool)
    .await;
    res?;
    queue.wake();
    Ok(())
}

/// Accoda molti lavori in una sola transazione (scansioni con migliaia di file nuovi).
pub async fn enqueue_batch(state: &Shared, jobs: Vec<(JobKind, i64, Option<i64>)>) -> anyhow::Result<()> {
    if jobs.is_empty() {
        return Ok(());
    }
    let now = now_ms();
    for chunk in jobs.chunks(2000) {
        let mut tx = state.db.write.begin().await?;
        for (job, priority, library_id) in chunk {
            sqlx::query(
                "INSERT INTO jobs (kind, payload, library_id, priority, state, run_after, dedupe_key, created_at)
                 VALUES (?, ?, ?, ?, 'queued', ?, ?, ?)
                 ON CONFLICT (dedupe_key) WHERE state IN ('queued', 'running') AND dedupe_key IS NOT NULL
                 DO UPDATE SET priority = MIN(priority, excluded.priority)",
            )
            .bind(job.name())
            .bind(serde_json::to_string(job)?)
            .bind(library_id)
            .bind(priority)
            .bind(now)
            .bind(job.dedupe_key())
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
    }
    state.jobs.wake();
    Ok(())
}

#[derive(sqlx::FromRow)]
struct JobRow {
    id: i64,
    payload: String,
    library_id: Option<i64>,
    attempts: i64,
    max_attempts: i64,
}

/// All'avvio i lavori rimasti "in esecuzione" da un arresto brusco tornano in coda.
pub async fn recover(state: &Shared) -> anyhow::Result<()> {
    let n = sqlx::query("UPDATE jobs SET state = 'queued', started_at = NULL WHERE state = 'running'").execute(&state.db.write).await?.rows_affected();
    if n > 0 {
        tracing::info!(jobs = n, "lavori interrotti rimessi in coda");
    }
    Ok(())
}

/// Ciclo principale: preleva i lavori pronti, li avvia rispettando i semafori.
pub async fn run_dispatcher(state: Shared) {
    let queue = state.jobs.clone();
    loop {
        let dispatched = match dispatch_some(&state).await {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!(error = %e, "coda lavori: errore nel prelievo");
                0
            }
        };
        if dispatched == 0 {
            // Niente da fare: attende un nuovo lavoro o al massimo 5 secondi (lavori pianificati).
            let _ = tokio::time::timeout(Duration::from_secs(5), queue.notify.notified()).await;
        } else {
            tokio::task::yield_now().await;
        }
    }
}

/// Tipi di lavoro di ogni corsia, nell'ordine in cui si servono.
const LANES: &[(&str, &[&str])] = &[
    ("scan", &["scan_library"]),
    ("metadata", &["match_item", "refresh_metadata"]),
    ("images", &["download_image"]),
    ("probe", &["probe_file", "extract_keyframes"]),
    ("maintenance", &["cleanup_missing"]),
];

/// Lavori pronti, presi corsia per corsia (e libreria per libreria per le analisi): migliaia di
/// associazioni in coda non fermano locandine e analisi, e una libreria remota lenta non ferma le altre.
async fn ready_jobs(state: &Shared, now: i64) -> anyhow::Result<Vec<JobRow>> {
    let mut rows = Vec::new();
    for (lane, kinds) in LANES {
        let free = state.jobs.lanes.get(lane).map(|s| s.available_permits()).unwrap_or(0);
        if free == 0 {
            continue;
        }
        let list = kinds.iter().map(|k| format!("'{k}'")).collect::<Vec<_>>().join(",");
        if *lane == "probe" {
            let libs: Vec<Option<i64>> =
                sqlx::query_scalar(&format!("SELECT DISTINCT library_id FROM jobs WHERE state = 'queued' AND kind IN ({list}) AND run_after <= ?"))
                    .bind(now)
                    .fetch_all(&state.db.read)
                    .await?;
            for lib in libs {
                let mut part: Vec<JobRow> = sqlx::query_as(&format!(
                    "SELECT id, payload, library_id, attempts, max_attempts FROM jobs
                     WHERE state = 'queued' AND kind IN ({list}) AND library_id IS ? AND run_after <= ?
                     ORDER BY priority, run_after, id LIMIT ?"
                ))
                .bind(lib)
                .bind(now)
                .bind(free as i64)
                .fetch_all(&state.db.read)
                .await?;
                rows.append(&mut part);
            }
        } else {
            let mut part: Vec<JobRow> = sqlx::query_as(&format!(
                "SELECT id, payload, library_id, attempts, max_attempts FROM jobs
                 WHERE state = 'queued' AND kind IN ({list}) AND run_after <= ?
                 ORDER BY priority, run_after, id LIMIT ?"
            ))
            .bind(now)
            .bind((free * 2) as i64)
            .fetch_all(&state.db.read)
            .await?;
            rows.append(&mut part);
        }
    }
    Ok(rows)
}

async fn dispatch_some(state: &Shared) -> anyhow::Result<usize> {
    let now = now_ms();
    let rows = ready_jobs(state, now).await?;
    let mut started = 0;
    for row in rows {
        let job: JobKind = match serde_json::from_str(&row.payload) {
            Ok(j) => j,
            Err(e) => {
                finish(state, row.id, Err(format!("lavoro non leggibile: {e}")), row.attempts, 0).await;
                continue;
            }
        };
        let lane = state.jobs.lanes.get(job.lane()).cloned().expect("lane");
        let Ok(lane_permit) = lane.try_acquire_owned() else { continue };
        // Analisi: limiti per libreria (concorrenza, tetto al minuto, fasce orarie, pausa durante le riproduzioni).
        let mut lib_permit = None;
        if job.lane() == "probe" {
            if let Some(lib) = row.library_id {
                match probe_gate(state, lib).await {
                    ProbeGate::Go(p) => lib_permit = Some(p),
                    ProbeGate::Later(ms) => {
                        let _ = sqlx::query("UPDATE jobs SET run_after = ? WHERE id = ? AND state = 'queued'")
                            .bind(now + ms)
                            .bind(row.id)
                            .execute(&state.db.write)
                            .await;
                        continue;
                    }
                    ProbeGate::Busy => continue,
                }
            }
        }
        // Prenotazione atomica: se un altro ciclo l'ha già preso, si salta.
        let claimed = sqlx::query("UPDATE jobs SET state = 'running', started_at = ?, attempts = attempts + 1 WHERE id = ? AND state = 'queued'")
            .bind(now_ms())
            .bind(row.id)
            .execute(&state.db.write)
            .await?
            .rows_affected();
        if claimed == 0 {
            continue;
        }
        started += 1;
        let st = state.clone();
        let attempts = row.attempts + 1;
        let max_attempts = row.max_attempts;
        tokio::spawn(async move {
            let _lane = lane_permit;
            let _lib = lib_permit;
            let name = job.name();
            let t0 = std::time::Instant::now();
            let result = execute(&st, job).await.map_err(|e| format!("{e:#}"));
            if let Err(e) = &result {
                tracing::warn!(job = name, id = row.id, error = %e, "lavoro non riuscito");
            } else {
                tracing::debug!(job = name, id = row.id, ms = t0.elapsed().as_millis() as u64, "lavoro completato");
            }
            finish(&st, row.id, result, attempts, max_attempts).await;
            st.jobs.wake();
        });
    }
    Ok(started)
}

enum ProbeGate {
    Go(tokio::sync::OwnedSemaphorePermit),
    Later(i64),
    Busy,
}

async fn probe_gate(state: &Shared, library_id: i64) -> ProbeGate {
    let lib: Option<(i64, Option<i64>, Option<i64>, Option<i64>, i64)> =
        sqlx::query_as("SELECT probe_concurrency, probe_per_minute, probe_window_start, probe_window_end, probe_pause_on_play FROM libraries WHERE id = ?")
            .bind(library_id)
            .fetch_optional(&state.db.read)
            .await
            .ok()
            .flatten();
    let Some((concurrency, per_minute, win_start, win_end, pause_on_play)) = lib else {
        return ProbeGate::Busy;
    };
    // Fascia oraria consentita (ore locali del server, 0-23).
    if let (Some(s), Some(e)) = (win_start, win_end) {
        let hour = current_hour();
        let inside = if s <= e { hour >= s && hour < e } else { hour >= s || hour < e };
        if !inside {
            return ProbeGate::Later(10 * 60_000);
        }
    }
    if pause_on_play != 0 && state.sessions.active_count() > 0 {
        return ProbeGate::Later(60_000);
    }
    if let Some(limit) = per_minute.filter(|l| *l > 0) {
        let now = now_ms();
        let mut rate = state.jobs.probe_rate.lock().unwrap();
        let v = rate.entry(library_id).or_default();
        v.retain(|t| now - *t < 60_000);
        if v.len() as i64 >= limit {
            let wait = 60_000 - (now - v[0]);
            return ProbeGate::Later(wait.max(1000));
        }
        v.push(now);
    }
    let concurrency = concurrency.clamp(1, 64) as usize;
    let sem = {
        let mut lanes = state.jobs.probe_lanes.lock().unwrap();
        let entry = lanes.entry(library_id).or_insert_with(|| (concurrency, Arc::new(Semaphore::new(concurrency))));
        if entry.0 != concurrency {
            *entry = (concurrency, Arc::new(Semaphore::new(concurrency)));
        }
        entry.1.clone()
    };
    match sem.try_acquire_owned() {
        Ok(p) => ProbeGate::Go(p),
        Err(_) => ProbeGate::Busy,
    }
}

fn current_hour() -> i64 {
    // Ora locale del server (TZ del container, ora legale compresa).
    (crate::util::local_now().1 / 60) as i64
}

async fn finish(state: &Shared, id: i64, result: Result<(), String>, attempts: i64, max_attempts: i64) {
    let now = now_ms();
    let r = match result {
        Ok(()) => sqlx::query("UPDATE jobs SET state = 'done', finished_at = ?, error = NULL WHERE id = ?").bind(now).bind(id).execute(&state.db.write).await,
        Err(e) if attempts < max_attempts => {
            // Nuovo tentativo con attesa crescente: 30 s, 2 min, 8 min...
            let delay = 30_000i64 * 4i64.pow((attempts - 1).clamp(0, 6) as u32);
            sqlx::query("UPDATE jobs SET state = 'queued', run_after = ?, error = ? WHERE id = ?")
                .bind(now + delay)
                .bind(&e)
                .bind(id)
                .execute(&state.db.write)
                .await
        }
        Err(e) => {
            sqlx::query("UPDATE jobs SET state = 'failed', finished_at = ?, error = ? WHERE id = ?").bind(now).bind(&e).bind(id).execute(&state.db.write).await
        }
    };
    if let Err(e) = r {
        tracing::warn!(error = %e, "aggiornamento dello stato del lavoro non riuscito");
    }
    state.events.admin("jobs.changed", serde_json::json!({ "id": id }));
}

async fn execute(state: &Shared, job: JobKind) -> anyhow::Result<()> {
    match job {
        JobKind::ScanLibrary { library_id, subpath } => crate::scanner::scan_library(state, library_id, subpath).await,
        JobKind::ProbeFile { file_id } => crate::probe::probe_file_job(state, file_id).await,
        JobKind::ExtractKeyframes { file_id } => crate::probe::extract_keyframes_job(state, file_id).await.map(|_| ()),
        JobKind::MatchItem { item_id } => crate::metadata::match_item(state, item_id).await,
        JobKind::RefreshMetadata { item_id } => crate::metadata::refresh_item(state, item_id).await,
        JobKind::DownloadImage { image_id } => crate::metadata::images::download_image(state, image_id).await,
        JobKind::CleanupMissing => crate::scanner::cleanup_missing(state).await,
    }
}

/// Pianificazione: scansioni periodiche, pulizia dei file mancanti, pulizia della cronologia dei lavori.
pub async fn run_scheduler(state: Shared) {
    let mut tick = tokio::time::interval(Duration::from_secs(60));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut minutes: u64 = 0;
    loop {
        tick.tick().await;
        minutes += 1;
        // Età delle classificazioni nuove o cambiate, per il limite per utente.
        if let Err(e) = crate::ratings::sync(&state).await {
            tracing::warn!(error = %e, "classificazioni per età");
        }
        if let Err(e) = schedule_scans(&state).await {
            tracing::warn!(error = %e, "pianificazione delle scansioni");
        }
        if minutes % 60 == 1 {
            let _ = enqueue(&state, JobKind::CleanupMissing, priority::MAINTENANCE, None).await;
            // Cronologia dei lavori completati: si tengono 7 giorni.
            let _ = sqlx::query("DELETE FROM jobs WHERE state IN ('done', 'failed') AND finished_at < ?")
                .bind(now_ms() - 7 * 86_400_000)
                .execute(&state.db.write)
                .await;
        }
        if minutes % 60 == 5 {
            let st = state.clone();
            tokio::spawn(async move { crate::backup::maybe_auto(&st).await });
        }
        {
            // Manutenzione del database: leggera ogni ora, completa di notte. Fuori dal ciclo dei lavori.
            let st = state.clone();
            let minute_of_day = crate::util::local_now().1 as i64;
            let m = minutes;
            tokio::spawn(async move { crate::maintenance::tick(&st, minute_of_day, m).await });
        }
        if minutes % (24 * 60) == 2 {
            let _ = crate::metadata::schedule_refresh(&state).await;
        }
    }
}

async fn schedule_scans(state: &Shared) -> anyhow::Result<()> {
    let libs: Vec<(i64, Option<i64>, Option<i64>)> =
        sqlx::query_as("SELECT id, scan_interval_minutes, last_scan_at FROM libraries WHERE scan_interval_minutes > 0").fetch_all(&state.db.read).await?;
    let now = now_ms();
    for (id, interval, last) in libs {
        let interval_ms = interval.unwrap_or(0) * 60_000;
        if interval_ms > 0 && last.is_none_or(|l| now - l >= interval_ms) {
            enqueue(state, JobKind::ScanLibrary { library_id: id, subpath: None }, priority::SCAN, Some(id)).await?;
        }
    }
    Ok(())
}

#[derive(Serialize)]
pub struct JobView {
    pub id: i64,
    pub kind: String,
    pub payload: Value,
    pub priority: i64,
    pub state: String,
    pub attempts: i64,
    pub run_after: i64,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub error: Option<String>,
    /// Su cosa lavora: titolo leggibile (film, episodio con serie, persona, libreria) e dettaglio (file, immagine).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_is_served_by_its_lane() {
        let all = [
            JobKind::ScanLibrary { library_id: 1, subpath: None },
            JobKind::ProbeFile { file_id: 1 },
            JobKind::ExtractKeyframes { file_id: 1 },
            JobKind::MatchItem { item_id: 1 },
            JobKind::RefreshMetadata { item_id: 1 },
            JobKind::DownloadImage { image_id: 1 },
            JobKind::CleanupMissing,
        ];
        let q = JobQueue::new();
        for j in all {
            let (_, kinds) = LANES.iter().find(|(l, _)| *l == j.lane()).expect("corsia elencata");
            assert!(kinds.contains(&j.name()), "{} non servito dalla corsia {}", j.name(), j.lane());
            assert!(q.lanes.contains_key(j.lane()));
        }
    }
}
