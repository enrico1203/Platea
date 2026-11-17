//! Copie di sicurezza del database e ripristino.
//!
//! - Copie coerenti con `VACUUM INTO` in `data/backups/`: automatiche (ogni N ore, le ultime K) e manuali.
//! - Ripristino in due tempi: la copia scelta viene verificata e preparata come `data/restore-pending.db`,
//!   poi il server si riavvia; all'avvio, prima di aprire il database, la sostituzione avviene a file chiusi
//!   e il database precedente resta accanto come `platea.db.before-restore-<ms>` (con il suo WAL).

use crate::state::Shared;
use crate::util::now_ms;
use anyhow::{bail, Context};
use serde::Serialize;
use std::path::{Path, PathBuf};

pub const PENDING: &str = "restore-pending.db";

#[derive(Debug, Serialize)]
pub struct BackupEntry {
    pub name: String,
    pub size: u64,
    pub created_at: i64,
    pub automatic: bool,
}

pub fn dir(state: &Shared) -> PathBuf {
    state.boot.data_dir.join("backups")
}

/// Nomi ammessi: solo quelli creati dal server (niente percorsi).
pub fn valid_name(name: &str) -> bool {
    name.len() < 100 && name.ends_with(".db") && name.starts_with("platea-") && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
}

/// Crea una copia: `platea-auto-<ms>.db` o `platea-manual-<ms>.db`.
pub async fn create(state: &Shared, automatic: bool) -> anyhow::Result<BackupEntry> {
    let d = dir(state);
    tokio::fs::create_dir_all(&d).await?;
    let ts = now_ms();
    let name = format!("platea-{}-{ts}.db", if automatic { "auto" } else { "manual" });
    let tmp = d.join(format!("{name}.part"));
    let _ = tokio::fs::remove_file(&tmp).await;
    sqlx::query(&format!("VACUUM INTO '{}'", tmp.display().to_string().replace('\'', "''"))).execute(&state.db.write).await.context("copia del database")?;
    tokio::fs::rename(&tmp, d.join(&name)).await?;
    let size = tokio::fs::metadata(d.join(&name)).await.map(|m| m.len()).unwrap_or(0);
    tracing::info!(file = %name, bytes = size, "copia di sicurezza del database creata");
    Ok(BackupEntry { name, size, created_at: ts, automatic })
}

pub async fn list(state: &Shared) -> Vec<BackupEntry> {
    let d = dir(state);
    let Ok(mut rd) = tokio::fs::read_dir(&d).await else { return vec![] };
    let mut out = Vec::new();
    while let Ok(Some(e)) = rd.next_entry().await {
        let name = e.file_name().to_string_lossy().to_string();
        if !valid_name(&name) {
            continue;
        }
        let ts = name.trim_end_matches(".db").rsplit('-').next().and_then(|t| t.parse().ok()).unwrap_or(0);
        let size = e.metadata().await.map(|m| m.len()).unwrap_or(0);
        out.push(BackupEntry { automatic: name.starts_with("platea-auto-"), name, size, created_at: ts });
    }
    out.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    out
}

/// Tiene le ultime `keep` copie automatiche; quelle manuali le cancella solo l'amministratore.
pub async fn prune(state: &Shared, keep: usize) {
    for b in list(state).await.into_iter().filter(|b| b.automatic).skip(keep) {
        let _ = tokio::fs::remove_file(dir(state).join(&b.name)).await;
    }
}

/// Copia automatica se l'ultima è più vecchia dell'intervallo. Chiamata ogni ora dal pianificatore.
pub async fn maybe_auto(state: &Shared) {
    let g = state.settings.get().general.clone();
    if g.backup_interval_hours == 0 {
        return;
    }
    let last = list(state).await.into_iter().find(|b| b.automatic).map(|b| b.created_at).unwrap_or(0);
    if now_ms() - last < g.backup_interval_hours as i64 * 3_600_000 {
        return;
    }
    match create(state, true).await {
        Ok(_) => prune(state, g.backup_keep.max(1) as usize).await,
        Err(e) => tracing::warn!(error = %format!("{e:#}"), "copia di sicurezza automatica non riuscita"),
    }
}

/// Verifica che il file sia un database di Platea: intestazione SQLite e tabella delle migrazioni.
pub async fn validate(path: &Path) -> anyhow::Result<()> {
    let mut head = [0u8; 16];
    {
        use tokio::io::AsyncReadExt;
        let mut f = tokio::fs::File::open(path).await.context("apertura del file")?;
        f.read_exact(&mut head).await.context("file troppo corto")?;
    }
    if &head != b"SQLite format 3\0" {
        bail!("Il file non è un database SQLite");
    }
    let opts = sqlx::sqlite::SqliteConnectOptions::new().filename(path).read_only(true);
    use sqlx::ConnectOptions;
    let mut conn = opts.connect().await.context("apertura del database")?;
    let ok: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('_sqlx_migrations', 'items', 'users')")
        .fetch_one(&mut conn)
        .await
        .context("lettura del database")?;
    if ok < 3 {
        bail!("Il file non è una copia valida del database");
    }
    let integrity: String = sqlx::query_scalar("PRAGMA quick_check").fetch_one(&mut conn).await?;
    if integrity != "ok" {
        bail!("Il database è danneggiato: {integrity}");
    }
    Ok(())
}

/// Prepara il ripristino: la copia diventa `restore-pending.db`; verrà applicata al prossimo avvio.
pub async fn stage(state: &Shared, source: &Path) -> anyhow::Result<()> {
    validate(source).await?;
    let pending = state.boot.data_dir.join(PENDING);
    let tmp = state.boot.data_dir.join(format!("{PENDING}.part"));
    tokio::fs::copy(source, &tmp).await.context("copia del file da ripristinare")?;
    tokio::fs::rename(&tmp, &pending).await?;
    Ok(())
}

/// All'avvio, prima di aprire il database: applica un ripristino preparato.
pub fn apply_pending(data_dir: &Path, db_path: &Path) -> anyhow::Result<bool> {
    let pending = data_dir.join(PENDING);
    if !pending.exists() {
        return Ok(false);
    }
    let ts = now_ms();
    let keep = data_dir.join(format!("platea.db.before-restore-{ts}"));
    if db_path.exists() {
        std::fs::rename(db_path, &keep).context("database attuale messo da parte")?;
        for suffix in ["-wal", "-shm"] {
            let side = PathBuf::from(format!("{}{suffix}", db_path.display()));
            if side.exists() {
                std::fs::rename(&side, format!("{}{suffix}", keep.display()))?;
            }
        }
    }
    std::fs::rename(&pending, db_path).context("database ripristinato al suo posto")?;
    tracing::warn!(previous = %keep.display(), "database ripristinato da una copia di sicurezza");
    Ok(true)
}

/// Chiude il processo dopo la risposta: il container (restart: unless-stopped) riparte e applica il ripristino.
pub fn restart_soon() {
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        tracing::warn!("riavvio per applicare il ripristino del database");
        std::process::exit(0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(valid_name("platea-auto-1794660863606.db"));
        assert!(valid_name("platea-manual-1.db"));
        assert!(!valid_name("../platea.db"));
        assert!(!valid_name("platea-auto-1.db.part"));
        assert!(!valid_name("altro.db"));
        assert!(!valid_name("platea-x/../../y.db"));
    }

    #[test]
    fn apply_pending_swaps_files() {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("platea.db");
        std::fs::write(&db, b"vecchio").unwrap();
        std::fs::write(d.path().join("platea.db-wal"), b"wal").unwrap();
        assert!(!apply_pending(d.path(), &db).unwrap());
        std::fs::write(d.path().join(PENDING), b"nuovo").unwrap();
        assert!(apply_pending(d.path(), &db).unwrap());
        assert_eq!(std::fs::read(&db).unwrap(), b"nuovo");
        assert!(!d.path().join("platea.db-wal").exists());
        let names: Vec<String> = std::fs::read_dir(d.path()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        assert!(names.iter().any(|n| n.starts_with("platea.db.before-restore-") && !n.ends_with("-wal")));
        assert!(names.iter().any(|n| n.starts_with("platea.db.before-restore-") && n.ends_with("-wal")));
    }
}
