//! Statistiche delle librerie per la pagina Amministrazione › Librerie › Statistiche: spazio, durata, qualità
//! (risoluzione, codec, HDR, audio), lingue, generi, crescita nel tempo, titoli più pesanti, riproduzioni.
//!
//! Su una libreria grande (centinaia di migliaia di file, milioni di flussi) il calcolo richiede alcuni secondi:
//! si fa una passata sola sui file (in streaming, aggregando in memoria) e poche GROUP BY, e il risultato resta in
//! memoria per qualche minuto. Le richieste contemporanee aspettano lo stesso calcolo invece di ripeterlo.

use super::{Access, Api};
use crate::auth::AdminUser;
use crate::error::{ApiError, ApiResult};
use crate::state::Shared;
use crate::util::now_ms;
use axum::extract::{Query, State};
use axum::Json;
use futures_util::TryStreamExt;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::Mutex;

/// Dopo quanto il risultato in memoria si ricalcola.
const TTL_MS: i64 = 10 * 60 * 1000;
/// Oltre questo tempo il calcolo si abbandona (database occupato, disco lento).
const TIMEOUT: Duration = Duration::from_secs(180);

pub fn routes(api: Api) -> Api {
    api.get(
        "/api/v1/libraries/stats",
        "Librerie",
        Access::Admin,
        "Statistiche delle librerie: spazio, durata, qualità, lingue, generi, crescita (calcolate al più ogni 10 minuti)",
        stats,
    )
}

type Cached = Option<(i64, Arc<Value>)>;

fn cache() -> &'static Mutex<Cached> {
    static C: OnceLock<Mutex<Cached>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(None))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct StatsQuery {
    /// `true`: ricalcola subito invece di usare il risultato in memoria.
    refresh: bool,
}

async fn stats(State(state): State<Shared>, _admin: AdminUser, Query(q): Query<StatsQuery>) -> ApiResult<Json<Value>> {
    let mut guard = cache().lock().await;
    if let Some((at, v)) = guard.as_ref() {
        if !q.refresh && now_ms() - at < TTL_MS {
            return Ok(Json((**v).clone()));
        }
    }
    let started = now_ms();
    let mut v = tokio::time::timeout(TIMEOUT, compute(&state))
        .await
        .map_err(|_| ApiError::Internal(anyhow::anyhow!("Calcolo delle statistiche troppo lento: riprova più tardi")))??;
    v["generated_at"] = json!(started);
    v["took_ms"] = json!(now_ms() - started);
    let v = Arc::new(v);
    *guard = Some((started, v.clone()));
    Ok(Json((*v).clone()))
}

/// Somme di un gruppo: file (o titoli), byte, durata.
#[derive(Default, Clone, Copy)]
struct Agg {
    files: i64,
    size: i64,
    duration_ms: i64,
}

impl Agg {
    fn add(&mut self, size: i64, duration_ms: i64) {
        self.files += 1;
        self.size += size;
        self.duration_ms += duration_ms;
    }
}

/// Totali di una libreria.
#[derive(Default)]
struct LibTotals {
    all: Agg,
    video: Agg,
    missing: i64,
    probe_failed: i64,
    probe_pending: i64,
    strm: i64,
    extras: i64,
    extras_size: i64,
    bitrate_sum: i64,
    bitrate_n: i64,
}

#[derive(sqlx::FromRow)]
struct FileRow {
    lib: i64,
    size: i64,
    duration_ms: i64,
    bitrate: i64,
    container: String,
    mtime: i64,
    missing: bool,
    probe_state: String,
    strm: bool,
    extra: bool,
    year: Option<i64>,
    /// Primo flusso video (copertine escluse): `codec|larghezza|altezza|hdr`.
    v: Option<String>,
    /// Primo flusso audio: `codec|canali`.
    a: Option<String>,
}

const FILES_SQL: &str = "
SELECT p.library_id AS lib, f.size AS size, COALESCE(f.duration_ms, 0) AS duration_ms, COALESCE(f.bitrate, 0) AS bitrate,
       COALESCE(f.container, '') AS container, f.mtime AS mtime, f.state = 'missing' AS missing, f.probe_state AS probe_state,
       f.stream_url IS NOT NULL AS strm, f.extra_type IS NOT NULL AS extra, COALESCE(s.year, i.year) AS year,
       (SELECT v.codec || '|' || IFNULL(v.width, 0) || '|' || IFNULL(v.height, 0) || '|' || IFNULL(v.hdr, '')
          FROM media_streams v WHERE v.file_id = f.id AND v.kind = 'video'
           AND v.codec NOT IN ('mjpeg', 'png', 'bmp', 'gif', 'webp') ORDER BY v.stream_index LIMIT 1) AS v,
       (SELECT a.codec || '|' || IFNULL(a.channels, 0)
          FROM media_streams a WHERE a.file_id = f.id AND a.kind = 'audio' ORDER BY a.stream_index LIMIT 1) AS a
  FROM media_files f
  JOIN library_paths p ON p.id = f.library_path_id
  LEFT JOIN items i ON i.id = f.item_id
  LEFT JOIN items s ON s.id = i.series_id";

fn resolution(w: i64, h: i64) -> &'static str {
    // La larghezza decide (un 1920×800 è un 1080p in formato cinema), l'altezza copre i formati verticali.
    if w >= 3200 || h >= 2000 {
        "2160p"
    } else if w >= 1700 || h >= 1000 {
        "1080p"
    } else if w >= 1200 || h >= 700 {
        "720p"
    } else {
        "sd"
    }
}

fn channels(n: i64) -> String {
    match n {
        1 => "mono".into(),
        2 => "stereo".into(),
        3..=5 => format!("{n}ch"),
        6 => "5.1".into(),
        7 => "6.1".into(),
        8 => "7.1".into(),
        0 => "unknown".into(),
        _ => format!("{n}ch"),
    }
}

fn bitrate_bucket(mbps: f64) -> &'static str {
    match mbps {
        x if x < 2.0 => "0-2",
        x if x < 5.0 => "2-5",
        x if x < 10.0 => "5-10",
        x if x < 20.0 => "10-20",
        x if x < 40.0 => "20-40",
        _ => "40+",
    }
}

/// `AAAA-MM` di un istante in secondi (calendario gregoriano, UTC).
fn month(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    // Algoritmo di Howard Hinnant (days_from_civil inverso).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}")
}

async fn compute(state: &Shared) -> ApiResult<Value> {
    let db = &state.db.read;
    let libs: Vec<(i64, String, String)> = sqlx::query_as("SELECT id, name, kind FROM libraries ORDER BY name").fetch_all(db).await?;

    // Negli audiolibri un flusso video è la copertina (anche in H.264): non conta come video.
    let audiobook_libs: Vec<i64> = libs.iter().filter(|l| l.2 == "audiobooks").map(|l| l.0).collect();
    // Una passata sui file: ogni dimensione è (libreria, dimensione, chiave) → somme.
    let mut totals: HashMap<i64, LibTotals> = HashMap::new();
    let mut dims: BTreeMap<(&'static str, i64, String), Agg> = BTreeMap::new();
    {
        let mut rows = sqlx::query_as::<_, FileRow>(FILES_SQL).fetch(db);
        while let Some(r) = rows.try_next().await? {
            let t = totals.entry(r.lib).or_default();
            let size = r.size.max(0);
            let dur = r.duration_ms.max(0);
            t.all.add(size, dur);
            if r.missing {
                t.missing += 1;
            }
            match r.probe_state.as_str() {
                "failed" => t.probe_failed += 1,
                "pending" => t.probe_pending += 1,
                _ => {}
            }
            if r.strm {
                t.strm += 1;
            }
            if r.extra {
                t.extras += 1;
                t.extras_size += size;
            }
            let mut put = |dim: &'static str, key: String| dims.entry((dim, r.lib, key)).or_default().add(size, dur);
            put("container", if r.container.is_empty() { "unknown".into() } else { r.container.to_lowercase() });
            put("growth", month(r.mtime));
            put("decade", r.year.filter(|y| *y > 1800).map(|y| format!("{}", y / 10 * 10)).unwrap_or_else(|| "unknown".into()));
            if let Some(v) = r.v.as_ref().filter(|_| !audiobook_libs.contains(&r.lib)) {
                let mut p = v.split('|');
                let codec = p.next().unwrap_or("").to_string();
                let w: i64 = p.next().and_then(|x| x.parse().ok()).unwrap_or(0);
                let h: i64 = p.next().and_then(|x| x.parse().ok()).unwrap_or(0);
                let hdr = p.next().unwrap_or("");
                t.video.add(size, dur);
                put("resolution", if w == 0 && h == 0 { "unknown".into() } else { resolution(w, h).into() });
                put("video_codec", codec);
                put("hdr", if hdr.is_empty() { "sdr".into() } else { hdr.to_string() });
                // Bitrate complessivo del file: quello dichiarato, altrimenti dimensione/durata.
                let bps = if r.bitrate > 0 {
                    r.bitrate
                } else if dur > 0 {
                    size.saturating_mul(8000) / dur
                } else {
                    0
                };
                if bps > 0 {
                    t.bitrate_sum += bps;
                    t.bitrate_n += 1;
                    put("bitrate", bitrate_bucket(bps as f64 / 1e6).into());
                }
            } else if r.probe_state == "done" {
                put("resolution", if r.a.is_some() { "audio".into() } else { "unknown".into() });
            } else {
                put("resolution", "unknown".into());
            }
            if let Some(a) = &r.a {
                let mut p = a.split('|');
                put("audio_codec", p.next().unwrap_or("").to_string());
                put("channels", channels(p.next().and_then(|x| x.parse().ok()).unwrap_or(0)));
            }
        }
    }

    // Titoli per tipo.
    let kinds: Vec<(i64, String, i64)> = sqlx::query_as("SELECT library_id, kind, COUNT(*) FROM items GROUP BY 1, 2").fetch_all(db).await?;
    // Stato dell'associazione ai metadati, voto, generi, solo per i titoli principali.
    let matches: Vec<(i64, String, i64)> =
        sqlx::query_as("SELECT library_id, match_state, COUNT(*) FROM items WHERE kind IN ('movie', 'series', 'audiobook') GROUP BY 1, 2")
            .fetch_all(db)
            .await?;
    let ratings: Vec<(i64, i64, i64)> = sqlx::query_as(
        "SELECT library_id, CAST(community_rating AS INTEGER), COUNT(*) FROM items
          WHERE kind IN ('movie', 'series') AND community_rating > 0 GROUP BY 1, 2",
    )
    .fetch_all(db)
    .await?;
    let genres: Vec<(i64, String, i64)> = sqlx::query_as(
        "SELECT i.library_id, g.name, COUNT(*) FROM item_genres ig JOIN items i ON i.id = ig.item_id JOIN genres g ON g.id = ig.genre_id
          WHERE i.kind IN ('movie', 'series', 'audiobook') GROUP BY 1, 2",
    )
    .fetch_all(db)
    .await?;
    // Lingue: in quanti file compare almeno una traccia in quella lingua.
    let langs: Vec<(i64, String, String, i64)> = sqlx::query_as(
        "SELECT p.library_id, s.kind, LOWER(COALESCE(NULLIF(s.language, ''), 'und')), COUNT(DISTINCT s.file_id)
           FROM media_streams s JOIN media_files f ON f.id = s.file_id JOIN library_paths p ON p.id = f.library_path_id
          WHERE s.kind IN ('audio', 'subtitle') GROUP BY 1, 2, 3",
    )
    .fetch_all(db)
    .await?;
    // Titoli con più versioni: spazio oltre la versione più grande.
    let versions: Vec<(i64, i64, i64)> = sqlx::query_as(
        "SELECT lib, COUNT(*), SUM(total - biggest) FROM (
           SELECT p.library_id AS lib, f.item_id, COUNT(*) AS n, SUM(f.size) AS total, MAX(f.size) AS biggest
             FROM media_files f JOIN library_paths p ON p.id = f.library_path_id
            WHERE f.item_id IS NOT NULL AND f.extra_type IS NULL AND f.state = 'present'
            GROUP BY p.library_id, f.item_id HAVING n > 1)
          GROUP BY lib",
    )
    .fetch_all(db)
    .await?;
    let plays: Vec<(i64, i64, i64)> =
        sqlx::query_as("SELECT i.library_id, COUNT(*), COALESCE(SUM(h.watched_ms), 0) FROM playback_history h JOIN items i ON i.id = h.item_id GROUP BY 1")
            .fetch_all(db)
            .await?;
    // Titoli più pesanti: un film, o una serie con tutti i suoi episodi.
    let heavy: Vec<(i64, i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT COALESCE(i.series_id, i.id) AS top, p.library_id, COUNT(*), SUM(f.size), COALESCE(SUM(f.duration_ms), 0)
           FROM media_files f JOIN library_paths p ON p.id = f.library_path_id JOIN items i ON i.id = f.item_id
          WHERE f.extra_type IS NULL GROUP BY 1 ORDER BY 4 DESC LIMIT 20",
    )
    .fetch_all(db)
    .await?;
    let mut heavy_out = Vec::new();
    for (top, lib, files, size, dur) in heavy {
        let t: Option<(String, Option<i64>, String)> = sqlx::query_as("SELECT title, year, kind FROM items WHERE id = ?").bind(top).fetch_optional(db).await?;
        if let Some((title, year, kind)) = t {
            heavy_out.push(json!({
                "id": state.ids.encode(top), "title": title, "year": year, "kind": kind,
                "library": state.ids.encode(lib), "files": files, "size": size, "duration_ms": dur,
            }));
        }
    }

    let enc = |id: i64| state.ids.encode(id);
    let lib_out: Vec<Value> = libs
        .iter()
        .map(|(id, name, kind)| {
            let t = totals.remove(id).unwrap_or_default();
            let count = |k: &str| kinds.iter().find(|(l, kk, _)| l == id && kk == k).map(|x| x.2).unwrap_or(0);
            let (mv_items, mv_extra) = versions.iter().find(|x| x.0 == *id).map(|x| (x.1, x.2)).unwrap_or((0, 0));
            let (n_plays, watched) = plays.iter().find(|x| x.0 == *id).map(|x| (x.1, x.2)).unwrap_or((0, 0));
            json!({
                "id": enc(*id), "name": name, "kind": kind,
                "files": t.all.files, "size": t.all.size, "duration_ms": t.all.duration_ms,
                "video_files": t.video.files, "video_size": t.video.size, "video_duration_ms": t.video.duration_ms,
                "movies": count("movie"), "series": count("series"), "seasons": count("season"), "episodes": count("episode"),
                "audiobooks": count("audiobook"), "collections": count("collection"),
                "missing": t.missing, "probe_failed": t.probe_failed, "probe_pending": t.probe_pending,
                "strm": t.strm, "extras": t.extras, "extras_size": t.extras_size,
                "avg_bitrate": if t.bitrate_n > 0 { t.bitrate_sum / t.bitrate_n } else { 0 },
                "multi_version_items": mv_items, "multi_version_extra_size": mv_extra,
                "plays": n_plays, "watched_ms": watched,
            })
        })
        .collect();

    let mut breakdowns: BTreeMap<&'static str, Vec<Value>> = BTreeMap::new();
    for ((dim, lib, key), a) in dims {
        breakdowns.entry(dim).or_default().push(json!({ "lib": enc(lib), "key": key, "files": a.files, "size": a.size, "duration_ms": a.duration_ms }));
    }
    let items_rows = |rows: Vec<(i64, String, i64)>| -> Vec<Value> {
        rows.into_iter().map(|(lib, key, n)| json!({ "lib": enc(lib), "key": key, "files": n, "size": 0, "duration_ms": 0 })).collect()
    };
    breakdowns.insert("match", items_rows(matches));
    breakdowns.insert("genre", items_rows(genres));
    breakdowns.insert(
        "rating",
        ratings.into_iter().map(|(lib, r, n)| json!({ "lib": enc(lib), "key": r.clamp(0, 10).to_string(), "files": n, "size": 0, "duration_ms": 0 })).collect(),
    );
    let mut audio_lang = Vec::new();
    let mut sub_lang = Vec::new();
    for (lib, kind, lang, n) in langs {
        // Codici che vogliono dire "non indicata": una voce sola.
        let lang = if matches!(lang.as_str(), "unknown" | "unk" | "und" | "zxx" | "mis") { "und".to_string() } else { lang };
        let row = json!({ "lib": enc(lib), "key": lang, "files": n, "size": 0, "duration_ms": 0 });
        if kind == "audio" {
            audio_lang.push(row);
        } else {
            sub_lang.push(row);
        }
    }
    breakdowns.insert("audio_language", audio_lang);
    breakdowns.insert("subtitle_language", sub_lang);

    Ok(json!({ "libraries": lib_out, "breakdowns": breakdowns, "heaviest": heavy_out }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn months() {
        assert_eq!(super::month(0), "1970-01");
        assert_eq!(super::month(1_712_576_254), "2024-04");
        assert_eq!(super::month(951_782_400), "2000-02"); // 29/02/2000
        assert_eq!(super::month(951_868_800), "2000-03");
    }

    #[test]
    fn resolutions() {
        assert_eq!(super::resolution(1920, 800), "1080p");
        assert_eq!(super::resolution(3840, 1600), "2160p");
        assert_eq!(super::resolution(1280, 534), "720p");
        assert_eq!(super::resolution(720, 576), "sd");
    }
}
