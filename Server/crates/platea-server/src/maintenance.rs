//! Manutenzione programmata del database.
//!
//! Gli indici SQLite si aggiornano da soli a ogni scrittura; quello che invecchia sono le statistiche con cui
//! SQLite sceglie l'indice (senza, con 150.000 episodi può scegliere male), il file WAL che cresce durante le
//! scansioni e lo spazio lasciato dalle righe cancellate.
//!
//! - Ogni ora: `PRAGMA optimize` (ricalcola solo le statistiche che servono, con un limite di righe lette) e
//!   un checkpoint del WAL che non blocca nessuno.
//! - Ogni notte all'ora scelta: cronologia dei lavori ridotta, `ANALYZE` completo, WAL riportato a zero e,
//!   solo se più di un quinto delle pagine è vuoto e nessuno sta guardando, `VACUUM`.

use crate::state::Shared;
use crate::util::now_ms;
use serde::{Deserialize, Serialize};
use std::time::Instant;

const STATE_KEY: &str = "maintenance_last";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Report {
    pub at: i64,
    pub ms: u64,
    pub full: bool,
    pub jobs_removed: u64,
    pub analyzed: bool,
    pub vacuumed: bool,
    pub size_before: u64,
    pub size_after: u64,
    pub wal_before: u64,
    pub wal_after: u64,
    pub free_pages_percent: f64,
    pub error: Option<String>,
}

fn file_size(p: &std::path::Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

/// Leggera, ogni ora.
pub async fn hourly(state: &Shared) {
    let w = &state.db.write;
    // analysis_limit: ANALYZE legge al massimo ~1000 righe per indice, millisecondi anche su tabelle grandi.
    let _ = sqlx::query("PRAGMA analysis_limit = 1000").execute(w).await;
    if let Err(e) = sqlx::query("PRAGMA optimize").execute(w).await {
        tracing::warn!(error = %e, "PRAGMA optimize non riuscito");
    }
    let _ = sqlx::query("PRAGMA wal_checkpoint(PASSIVE)").execute(w).await;
}

/// Completa, una volta al giorno (o su richiesta dell'amministratore).
pub async fn full(state: &Shared) -> Report {
    let t0 = Instant::now();
    let db_path = state.boot.db_path();
    let wal_path = std::path::PathBuf::from(format!("{}-wal", db_path.display()));
    let mut r = Report { at: now_ms(), full: true, size_before: file_size(&db_path), wal_before: file_size(&wal_path), ..Default::default() };
    let w = &state.db.write;
    let res: anyhow::Result<()> = async {
        // Lavori: i completati servono solo per la cronologia recente; i non riusciti restano di più.
        let now = now_ms();
        r.jobs_removed = sqlx::query("DELETE FROM jobs WHERE (state = 'done' AND finished_at < ?) OR (state = 'failed' AND finished_at < ?)")
            .bind(now - 86_400_000)
            .bind(now - 7 * 86_400_000)
            .execute(w)
            .await?
            .rows_affected();
        sqlx::query("PRAGMA analysis_limit = 1000").execute(w).await?;
        sqlx::query("ANALYZE").execute(w).await?;
        r.analyzed = true;
        let pages: i64 = sqlx::query_scalar("PRAGMA page_count").fetch_one(w).await?;
        let free: i64 = sqlx::query_scalar("PRAGMA freelist_count").fetch_one(w).await?;
        r.free_pages_percent = if pages > 0 { free as f64 * 100.0 / pages as f64 } else { 0.0 };
        // VACUUM riscrive tutto il file: solo se ne vale la pena e nessuno sta guardando.
        let page_size: i64 = sqlx::query_scalar("PRAGMA page_size").fetch_one(w).await?;
        let free_bytes = free * page_size;
        if r.free_pages_percent >= 20.0 && free_bytes > 64 * 1024 * 1024 && state.sessions.active_count() == 0 {
            tracing::info!(free_mb = free_bytes / 1_048_576, "VACUUM del database");
            sqlx::query("VACUUM").execute(w).await?;
            r.vacuumed = true;
        }
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)").execute(w).await?;
        Ok(())
    }
    .await;
    if let Err(e) = res {
        tracing::warn!(error = %format!("{e:#}"), "manutenzione del database non completa");
        r.error = Some(format!("{e:#}"));
    }
    r.ms = t0.elapsed().as_millis() as u64;
    r.size_after = file_size(&db_path);
    r.wal_after = file_size(&wal_path);
    tracing::info!(
        ms = r.ms,
        jobs_removed = r.jobs_removed,
        vacuum = r.vacuumed,
        db_mb = r.size_after / 1_048_576,
        wal_mb_before = r.wal_before / 1_048_576,
        wal_mb_after = r.wal_after / 1_048_576,
        "manutenzione del database completata"
    );
    if let Ok(s) = serde_json::to_string(&r) {
        let _ = state.db.state_set(STATE_KEY, &s).await;
    }
    r
}

pub async fn last(state: &Shared) -> Option<Report> {
    state.db.state_get(STATE_KEY).await.ok().flatten().and_then(|s| serde_json::from_str(&s).ok())
}

/// Chiamata ogni minuto dal pianificatore: la completa all'ora scelta, una volta al giorno.
pub async fn tick(state: &Shared, minute_of_day: i64, minutes_since_start: u64) {
    // Anche subito dopo l'avvio: un database aperto per la prima volta non ha statistiche.
    if minutes_since_start % 60 == 30 || minutes_since_start == 1 {
        hourly(state).await;
    }
    let hour = state.settings.get().general.maintenance_hour as i64;
    if minute_of_day == hour * 60 + 15 {
        let done_today = last(state).await.is_some_and(|r| now_ms() - r.at < 20 * 3_600_000);
        if !done_today {
            full(state).await;
        }
    }
}
