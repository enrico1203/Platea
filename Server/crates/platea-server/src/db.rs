//! Database: SQLite in modalità WAL.
//! Una sola connessione scrive (le scritture si mettono in fila), un pool separato legge
//! e non aspetta mai lo scrittore.

use crate::boot::BootConfig;
use anyhow::Context;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;
use std::str::FromStr;
use std::time::Duration;

#[derive(Clone)]
pub struct Db {
    pub read: SqlitePool,
    pub write: SqlitePool,
}

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

impl Db {
    pub async fn open(boot: &BootConfig) -> anyhow::Result<Db> {
        let path = boot.db_path();
        // Ripristino preparato dalla dashboard: si applica qui, a database chiuso.
        crate::backup::apply_pending(&boot.data_dir, &path)?;
        let url = format!("sqlite://{}", path.display());
        let base = SqliteConnectOptions::from_str(&url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(10))
            .pragma("temp_store", "memory")
            // 8 MB di cache per connessione, niente mmap: ogni connessione mapperebbe le stesse pagine
            // e la RSS mostrerebbe centinaia di MB che sono solo cache del sistema. Le letture passano dalla
            // cache di pagina del kernel, condivisa, e le misure di memoria restano oneste.
            .pragma("cache_size", "-8000")
            .pragma("mmap_size", "0");

        let write = SqlitePoolOptions::new()
            .max_connections(1)
            .min_connections(1)
            .acquire_timeout(Duration::from_secs(30))
            .connect_with(base.clone())
            .await
            .context("apertura del database in scrittura")?;

        // Copia di sicurezza prima di ogni migrazione da applicare.
        let pending = Self::pending_migrations(&write).await.unwrap_or(0);
        if pending > 0 && path.exists() && std::fs::metadata(&path).map(|m| m.len() > 0).unwrap_or(false) {
            let applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations").fetch_one(&write).await.unwrap_or(0);
            if applied > 0 {
                let backup = boot.data_dir.join(format!("platea.db.bak-{}", crate::util::now_ms()));
                sqlx::query(&format!("VACUUM INTO '{}'", backup.display().to_string().replace('\'', "''")))
                    .execute(&write)
                    .await
                    .context("copia di sicurezza prima della migrazione")?;
                tracing::info!(file = %backup.display(), "copia di sicurezza del database prima della migrazione");
                prune_backups(&boot.data_dir, 2);
            }
        }
        MIGRATOR.run(&write).await.context("migrazioni del database")?;
        prune_backups(&boot.data_dir, 2);
        // Alcune migrazioni riscrivono il testo dello schema (vincoli CHECK): la connessione che le ha applicate
        // tiene in memoria lo schema vecchio, quindi si riapre.
        let write = if pending > 0 {
            write.close().await;
            SqlitePoolOptions::new()
                .max_connections(1)
                .min_connections(1)
                .acquire_timeout(Duration::from_secs(30))
                .connect_with(base.clone())
                .await
                .context("riapertura del database in scrittura")?
        } else {
            write
        };

        let readers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8) as u32;
        let read = SqlitePoolOptions::new()
            .max_connections(readers)
            .min_connections(1)
            // I lettori inattivi si chiudono: la loro cache torna al sistema.
            .idle_timeout(Duration::from_secs(60))
            .acquire_timeout(Duration::from_secs(10))
            .connect_with(base.read_only(true))
            .await
            .context("apertura del database in lettura")?;

        Ok(Db { read, write })
    }

    async fn pending_migrations(pool: &SqlitePool) -> anyhow::Result<usize> {
        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='_sqlx_migrations'").fetch_one(pool).await?;
        if exists == 0 {
            return Ok(MIGRATOR.iter().count());
        }
        let applied: Vec<i64> = sqlx::query_scalar("SELECT version FROM _sqlx_migrations").fetch_all(pool).await?;
        Ok(MIGRATOR.iter().filter(|m| !applied.contains(&m.version)).count())
    }

    pub async fn state_get(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(sqlx::query_scalar("SELECT value FROM server_state WHERE key = ?").bind(key).fetch_optional(&self.read).await?)
    }

    pub async fn state_set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO server_state (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
            .bind(key)
            .bind(value)
            .execute(&self.write)
            .await?;
        Ok(())
    }

    pub async fn activity(&self, user_id: Option<i64>, kind: &str, message: &str, ip: Option<&str>) {
        let r = sqlx::query("INSERT INTO activity_log (at, user_id, kind, message, ip) VALUES (?, ?, ?, ?, ?)")
            .bind(crate::util::now_ms())
            .bind(user_id)
            .bind(kind)
            .bind(message)
            .bind(ip)
            .execute(&self.write)
            .await;
        if let Err(e) = r {
            tracing::warn!(error = %e, "scrittura del registro attività non riuscita");
        }
    }
}

/// Tiene solo le ultime `keep` copie di sicurezza delle migrazioni (`platea.db.bak-<ms>`).
fn prune_backups(dir: &std::path::Path, keep: usize) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut list: Vec<(i64, std::path::PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let ts = name.strip_prefix("platea.db.bak-")?.parse::<i64>().ok()?;
            Some((ts, e.path()))
        })
        .collect();
    list.sort_by_key(|e| std::cmp::Reverse(e.0));
    for (_, p) in list.into_iter().skip(keep) {
        if std::fs::remove_file(&p).is_ok() {
            tracing::info!(file = %p.display(), "copia di sicurezza vecchia rimossa");
        }
    }
}

#[cfg(test)]
mod backup_tests {
    #[test]
    fn keeps_latest_backups() {
        let d = tempfile::tempdir().unwrap();
        for ts in [100, 300, 200, 400] {
            std::fs::write(d.path().join(format!("platea.db.bak-{ts}")), b"x").unwrap();
        }
        std::fs::write(d.path().join("platea.db"), b"x").unwrap();
        super::prune_backups(d.path(), 2);
        let mut left: Vec<String> = std::fs::read_dir(d.path()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        left.sort();
        assert_eq!(left, vec!["platea.db", "platea.db.bak-300", "platea.db.bak-400"]);
    }
}

#[cfg(test)]
mod migration_tests {
    /// I vincoli CHECK allargati dalla migrazione degli audiolibri valgono subito, anche sulla connessione di scrittura,
    /// e quelli vecchi continuano a rifiutare i valori sconosciuti.
    #[tokio::test]
    async fn audiobook_kinds_are_accepted() {
        let d = tempfile::tempdir().unwrap();
        let boot = crate::boot::BootConfig {
            bind: String::new(),
            data_dir: d.path().join("data"),
            cache_dir: d.path().join("cache"),
            transcode_dir: d.path().join("tc"),
            log: String::new(),
            web_dir: None,
        };
        std::fs::create_dir_all(&boot.data_dir).unwrap();
        let db = super::Db::open(&boot).await.unwrap();
        sqlx::query("INSERT INTO libraries (name, kind, created_at) VALUES ('A', 'audiobooks', 0)").execute(&db.write).await.unwrap();
        sqlx::query("INSERT INTO items (library_id, kind, title, sort_title, added_at, updated_at) VALUES (1, 'audiobook', 'x', 'x', 0, 0)")
            .execute(&db.write)
            .await
            .unwrap();
        sqlx::query("INSERT INTO people (name) VALUES ('p')").execute(&db.write).await.unwrap();
        sqlx::query("INSERT INTO item_people (item_id, person_id, role) VALUES (1, 1, 'narrator')").execute(&db.write).await.unwrap();
        assert!(sqlx::query("INSERT INTO libraries (name, kind, created_at) VALUES ('B', 'music', 0)").execute(&db.write).await.is_err());
        let ok: String = sqlx::query_scalar("PRAGMA integrity_check").fetch_one(&db.read).await.unwrap();
        assert_eq!(ok, "ok");
    }
}
