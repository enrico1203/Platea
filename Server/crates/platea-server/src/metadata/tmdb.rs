//! Client TMDB: limitatore di frequenza, attesa crescente sulle risposte 429, cache nel database.

use crate::db::Db;
use crate::util::now_ms;
use anyhow::{bail, Context};
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Semaphore};

const BASE: &str = "https://api.themoviedb.org/3";
pub const IMAGE_BASE: &str = "https://image.tmdb.org/t/p";

#[derive(Clone)]
pub struct Tmdb {
    http: reqwest::Client,
    db: Db,
    next_slot: Arc<Mutex<Instant>>,
    connections: Arc<Semaphore>,
    base: String,
}

impl Tmdb {
    pub fn new(http: reqwest::Client, db: Db) -> Self {
        let base = std::env::var("PLATEA_TMDB_BASE").unwrap_or_else(|_| BASE.to_string());
        Tmdb { http, db, next_slot: Arc::new(Mutex::new(Instant::now())), connections: Arc::new(Semaphore::new(8)), base }
    }

    async fn wait_slot(&self, rps: u32) {
        let interval = Duration::from_secs_f64(1.0 / rps.clamp(1, 40) as f64);
        let wait = {
            let mut next = self.next_slot.lock().await;
            let now = Instant::now();
            let slot = if *next > now { *next } else { now };
            *next = slot + interval;
            slot.saturating_duration_since(now)
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }

    /// GET con cache. `max_age_ms` = quanto vale una risposta in cache.
    pub async fn get(&self, key: &str, rps: u32, path: &str, params: &[(&str, String)], language: &str, max_age_ms: i64) -> anyhow::Result<Value> {
        if key.trim().is_empty() {
            bail!("chiave TMDB non impostata");
        }
        let mut sorted: Vec<(&str, String)> = params.to_vec();
        sorted.sort();
        let cache_key = format!("{path}?{}", sorted.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&"));
        if let Some((resp, at)) =
            sqlx::query_as::<_, (String, i64)>("SELECT response, fetched_at FROM metadata_cache WHERE provider = 'tmdb' AND key = ? AND language = ?")
                .bind(&cache_key)
                .bind(language)
                .fetch_optional(&self.db.read)
                .await?
        {
            if now_ms() - at < max_age_ms {
                if let Ok(v) = serde_json::from_str(&resp) {
                    return Ok(v);
                }
            }
        }

        let _permit = self.connections.acquire().await?;
        let mut attempt = 0;
        loop {
            self.wait_slot(rps).await;
            let mut req = self.http.get(format!("{}{path}", self.base)).timeout(Duration::from_secs(20));
            let mut q: Vec<(&str, String)> = sorted.clone();
            if !language.is_empty() {
                q.push(("language", language.to_string()));
            }
            let key = key.trim();
            if key.len() > 40 {
                req = req.bearer_auth(key);
            } else {
                q.push(("api_key", key.to_string()));
            }
            let resp = req.query(&q).send().await.context("richiesta a TMDB")?;
            let status = resp.status();
            if status.as_u16() == 429 && attempt < 5 {
                let retry = resp.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok()).unwrap_or(1 << attempt);
                attempt += 1;
                tracing::warn!(retry, "TMDB: troppe richieste, attesa");
                tokio::time::sleep(Duration::from_secs(retry.min(60))).await;
                continue;
            }
            if status.as_u16() == 401 {
                bail!("TMDB ha rifiutato la chiave API (401)");
            }
            if status.as_u16() == 404 {
                bail!("TMDB: risorsa non trovata ({path})");
            }
            if !status.is_success() {
                if status.is_server_error() && attempt < 3 {
                    attempt += 1;
                    tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
                    continue;
                }
                bail!("TMDB: risposta {status}");
            }
            let v: Value = resp.json().await.context("risposta TMDB non leggibile")?;
            sqlx::query(
                "INSERT INTO metadata_cache (provider, key, language, response, fetched_at) VALUES ('tmdb', ?, ?, ?, ?)
                 ON CONFLICT(provider, key, language) DO UPDATE SET response = excluded.response, fetched_at = excluded.fetched_at",
            )
            .bind(&cache_key)
            .bind(language)
            .bind(v.to_string())
            .bind(now_ms())
            .execute(&self.db.write)
            .await?;
            return Ok(v);
        }
    }

    /// Verifica la chiave: usata dalla procedura guidata e dalla dashboard.
    pub async fn check_key(&self, key: &str) -> anyhow::Result<()> {
        let mut req = self.http.get(format!("{}/configuration", self.base)).timeout(Duration::from_secs(15));
        if key.trim().len() > 40 {
            req = req.bearer_auth(key.trim());
        } else {
            req = req.query(&[("api_key", key.trim())]);
        }
        let r = req.send().await.context("TMDB non raggiungibile")?;
        if r.status().is_success() {
            Ok(())
        } else {
            bail!("TMDB ha risposto {}", r.status())
        }
    }
}
