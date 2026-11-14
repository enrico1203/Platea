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
        let url = format!("sqlite://{}", path.display());
        let base = SqliteConnectOptions::from_str(&url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(10))
            .pragma("temp_store", "memory")
            .pragma("cache_size", "-32000")
            .pragma("mmap_size", "268435456");

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
            let applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
                .fetch_one(&write)
                .await
                .unwrap_or(0);
            if applied > 0 {
                let backup = boot.data_dir.join(format!("platea.db.bak-{}", crate::util::now_ms()));
                sqlx::query(&format!("VACUUM INTO '{}'", backup.display().to_string().replace('\'', "''")))
                    .execute(&write)
                    .await
                    .context("copia di sicurezza prima della migrazione")?;
                tracing::info!(file = %backup.display(), "copia di sicurezza del database prima della migrazione");
            }
        }
        MIGRATOR.run(&write).await.context("migrazioni del database")?;

        let readers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(4, 16) as u32;
        let read = SqlitePoolOptions::new()
            .max_connections(readers)
            .min_connections(1)
            .acquire_timeout(Duration::from_secs(10))
            .connect_with(base.read_only(true))
            .await
            .context("apertura del database in lettura")?;

        Ok(Db { read, write })
    }

    async fn pending_migrations(pool: &SqlitePool) -> anyhow::Result<usize> {
        let exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='_sqlx_migrations'",
        )
        .fetch_one(pool)
        .await?;
        if exists == 0 {
            return Ok(MIGRATOR.iter().count());
        }
        let applied: Vec<i64> = sqlx::query_scalar("SELECT version FROM _sqlx_migrations").fetch_all(pool).await?;
        Ok(MIGRATOR.iter().filter(|m| !applied.contains(&m.version)).count())
    }

    pub async fn state_get(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(sqlx::query_scalar("SELECT value FROM server_state WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.read)
            .await?)
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
