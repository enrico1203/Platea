//! Metadati da The Movie Database.
//! Associazione automatica in due passi: identificativo esplicito nel nome se c'è, altrimenti
//! ricerca per titolo e anno con un punteggio. Sotto la soglia l'elemento va "da verificare".
//! Un campo modificato a mano (bloccato) non viene sovrascritto.

pub mod collections;
pub mod images;
pub mod nfo;
pub mod plexmatch;
pub mod tmdb;

use crate::jobs::{self, priority, JobKind};
use crate::state::Shared;
use crate::util::now_ms;
use anyhow::bail;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;

const DAY: i64 = 86_400_000;

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub provider_id: String,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i64>,
    pub score: f64,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub popularity: f64,
    /// Somiglianza del titolo (0–1, il migliore tra titolo, originale e traduzioni) e anno identico.
    #[serde(skip)]
    pub title_sim: f64,
    #[serde(skip)]
    pub year_exact: bool,
}

#[derive(sqlx::FromRow)]
struct ItemRow {
    #[allow(dead_code)]
    id: i64,
    library_id: i64,
    kind: String,
    title: String,
    year: Option<i64>,
    locked_fields: String,
    match_state: String,
}

async fn library_language(state: &Shared, library_id: i64) -> (String, String) {
    sqlx::query_as::<_, (String, String)>("SELECT language, country FROM libraries WHERE id = ?")
        .bind(library_id)
        .fetch_optional(&state.db.read)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| ("it-IT".into(), "IT".into()))
}

fn year_of(date: Option<&str>) -> Option<i64> {
    date.filter(|d| d.len() >= 4).and_then(|d| d[0..4].parse().ok())
}

/// Somiglianza tra il nome del file e un titolo. Le uscite italiane aggiungono spesso un sottotitolo
/// ("Pay the Ghost - Il male cammina tra noi", "Dirty Dancing - Balli proibiti"): se un titolo è l'inizio
/// dell'altro, parola per parola, conta quasi come uguale.
pub fn title_match(query: &str, candidate: &str) -> f64 {
    let sim = platea_naming::title_similarity(query, candidate);
    let q = platea_naming::normalize_title(query);
    let c = platea_naming::normalize_title(candidate);
    let (short, long) = if q.len() <= c.len() { (&q, &c) } else { (&c, &q) };
    let words = short.split(' ').filter(|w| !w.is_empty()).count();
    let enough = short.chars().filter(|c| c.is_alphanumeric()).count() >= 4 && (words >= 2 || short.len() >= 5);
    let prefix = enough && long.starts_with(&format!("{short} "));
    // "Asino per sempre Jackass Forever": titolo italiano e poi quello originale.
    let suffix = enough && words >= 2 && long.ends_with(&format!(" {short}"));
    if prefix || suffix {
        sim.max(0.9)
    } else {
        sim
    }
}

/// Punteggio di un candidato rispetto al nome letto dal file (usato nei test e come riferimento).
#[cfg_attr(not(test), allow(dead_code))]
pub fn score_candidate(query_title: &str, query_year: Option<i64>, c_title: &str, c_original: Option<&str>, c_year: Option<i64>, popularity: f64) -> f64 {
    let mut sim = title_match(query_title, c_title);
    if let Some(o) = c_original {
        sim = sim.max(title_match(query_title, o));
    }
    score_from(sim, query_year, c_year, popularity)
}

fn score_from(sim: f64, query_year: Option<i64>, c_year: Option<i64>, popularity: f64) -> f64 {
    let year_part = match (query_year, c_year) {
        (Some(q), Some(c)) if q == c => 0.2,
        (Some(q), Some(c)) if (q - c).abs() == 1 => 0.12,
        (Some(_), Some(_)) => -0.25,
        (None, _) => 0.08,
        (Some(_), None) => 0.0,
    };
    // Popolarità come spareggio: al massimo 0,05.
    let pop = (popularity.max(0.0).ln_1p() / 10.0).min(1.0) * 0.05;
    (sim * 0.75 + year_part + pop).clamp(0.0, 1.0)
}

/// Ricerche di ripiego per un nome lungo: prima l'inizio (togliendo parole dalla fine), poi la fine.
/// Al massimo 6, mai sotto le 2 parole.
pub fn partial_queries(title: &str) -> Vec<String> {
    let words: Vec<&str> = title.split_whitespace().collect();
    let n = words.len();
    if n < 3 {
        return vec![];
    }
    let mut out = Vec::new();
    for k in (2..n).rev().take(4) {
        out.push(words[..k].join(" "));
    }
    // La fine: titoli originali brevi ("Jackass Forever"), 2 o 3 parole.
    for k in 2..=3.min(n - 1) {
        out.push(words[n - k..].join(" "));
    }
    out.dedup();
    out
}

/// Quando il candidato migliore si può accettare senza chiedere.
/// - punteggio sopra la soglia e staccato dal secondo (la regola di sempre);
/// - titolo identico (anche tradotto o originale) e anno identico: due voci TMDB con stesso titolo e anno
///   sono quasi sempre duplicati, e vince la più popolare;
/// - unico risultato di TMDB, anno identico e titolo abbastanza simile.
pub fn auto_accept(cands: &[Candidate], threshold: f64) -> bool {
    let Some(b) = cands.first() else { return false };
    let second = cands.get(1).map(|c| c.score).unwrap_or(0.0);
    (b.score >= threshold && (b.score - second >= 0.05 || second < threshold))
        || (b.title_sim >= 0.99 && b.year_exact)
        || (cands.len() == 1 && b.year_exact && b.title_sim >= 0.5)
}

fn candidates_from(results: &Value, movie: bool, title: &str, year: Option<i64>, rejected: &HashSet<String>) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = results["results"]
        .as_array()
        .map(|a| a.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter_map(|r| {
            let id = r["id"].as_i64()?.to_string();
            if rejected.contains(&id) {
                return None;
            }
            let (t, o, d) = if movie {
                (r["title"].as_str()?, r["original_title"].as_str(), r["release_date"].as_str())
            } else {
                (r["name"].as_str()?, r["original_name"].as_str(), r["first_air_date"].as_str())
            };
            let cy = year_of(d);
            let pop = r["popularity"].as_f64().unwrap_or(0.0);
            let sim = o.map(|o| title_match(title, o)).unwrap_or(0.0).max(title_match(title, t));
            Some(Candidate {
                provider_id: id,
                title: t.to_string(),
                original_title: o.map(String::from),
                year: cy,
                score: score_from(sim, year, cy, pop),
                title_sim: sim,
                year_exact: year.is_some() && year == cy,
                overview: r["overview"].as_str().map(String::from),
                poster_path: r["poster_path"].as_str().map(String::from),
                popularity: pop,
            })
        })
        .collect();
    sort_candidates(&mut out);
    out
}

fn sort_candidates(c: &mut [Candidate]) {
    c.sort_by(|a, b| {
        b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal).then(b.popularity.partial_cmp(&a.popularity).unwrap_or(std::cmp::Ordering::Equal))
    });
}

/// Stessi risultati in un'altra lingua: per ogni candidato il titolo tradotto che somiglia di più al nome.
fn merge_translations(base: &mut [Candidate], other: &[Candidate], year: Option<i64>) {
    for c in base.iter_mut() {
        if let Some(o) = other.iter().find(|o| o.provider_id == c.provider_id) {
            if o.title_sim > c.title_sim {
                c.title_sim = o.title_sim;
                c.score = score_from(o.title_sim, year, c.year, c.popularity);
            }
        }
    }
}

/// Cerca i candidati su TMDB per titolo e anno.
pub async fn search(state: &Shared, kind: &str, title: &str, year: Option<i64>, language: &str, rejected: &HashSet<String>) -> anyhow::Result<Vec<Candidate>> {
    let s = state.settings.get();
    let key = s.metadata.tmdb_api_key.clone();
    let rps = s.metadata.tmdb_requests_per_second;
    let movie = kind == "movie";
    let path = if movie { "/search/movie" } else { "/search/tv" };
    let mut params = vec![("query", title.to_string()), ("include_adult", "false".to_string())];
    if let Some(y) = year {
        params.push((if movie { "year" } else { "first_air_date_year" }, y.to_string()));
    }
    let r = state.tmdb.get(&key, rps, path, &params, language, 7 * DAY).await?;
    let mut c = candidates_from(&r, movie, title, year, rejected);
    if c.is_empty() && year.is_some() {
        // L'anno nel nome può essere sbagliato: nuova ricerca senza anno.
        let r = state.tmdb.get(&key, rps, path, &params[..2], language, 7 * DAY).await?;
        c = candidates_from(&r, movie, title, year, rejected);
    }
    if c.is_empty() {
        // Nomi come "Now You See Me 2 I Maghi Del Crimine": titolo originale e sottotitolo italiano senza
        // separatore. TMDB non trova la frase intera: si cerca con l'inizio o la fine del nome, e i
        // candidati si confrontano sempre con il nome completo.
        for q in partial_queries(title) {
            let mut p = params.clone();
            p[0].1 = q;
            let r = state.tmdb.get(&key, rps, path, &p, language, 7 * DAY).await?;
            c = candidates_from(&r, movie, title, year, rejected);
            if !c.is_empty() {
                break;
            }
        }
    }
    // Il nome del file è spesso in un'altra lingua rispetto alla libreria (cartelle in italiano, metadati
    // in inglese): se nessun candidato è convincente si confrontano anche i titoli tradotti.
    if !c.is_empty() && !auto_accept(&c, s.metadata.auto_match_threshold) {
        let mut langs = vec![s.metadata.language.clone(), "en-US".to_string()];
        langs.retain(|l| !l.eq_ignore_ascii_case(language));
        langs.dedup();
        for l in langs {
            let p = if year.is_some() && c.iter().any(|x| x.year_exact) { &params[..] } else { &params[..2] };
            let r = state.tmdb.get(&key, rps, path, p, &l, 7 * DAY).await?;
            merge_translations(&mut c, &candidates_from(&r, movie, title, year, rejected), year);
            sort_candidates(&mut c);
            if auto_accept(&c, s.metadata.auto_match_threshold) {
                break;
            }
        }
    }
    c.truncate(8);
    Ok(c)
}

pub async fn match_item(state: &Shared, item_id: i64) -> anyhow::Result<()> {
    let Some(item) = sqlx::query_as::<_, ItemRow>("SELECT id, library_id, kind, title, year, locked_fields, match_state FROM items WHERE id = ?")
        .bind(item_id)
        .fetch_optional(&state.db.read)
        .await?
    else {
        return Ok(());
    };
    if item.kind != "movie" && item.kind != "series" {
        return Ok(());
    }
    if item.match_state == "manual" || item.match_state == "matched" {
        return Ok(());
    }
    let s = state.settings.get();
    let has_key = !s.metadata.tmdb_api_key.trim().is_empty();
    // 0. NFO locale (Kodi, Jellyfin, Radarr…): identificativo esatto e, senza chiave TMDB, i metadati stessi.
    // .plexmatch: la scelta esplicita dell'utente, i suoi identificativi vincono su quelli dell'NFO.
    let mut item = item;
    if s.metadata.read_nfo || s.metadata.read_plexmatch {
        if let Some(loc) = nfo::locate(state, item_id, &item.kind).await {
            let pm = if s.metadata.read_plexmatch { plexmatch::find(state, &loc).await } else { None };
            let n = if s.metadata.read_nfo { nfo::find(state, &loc, &item.kind).await } else { None };
            if let Some(n) = &n {
                nfo::store_ids(state, item_id, n).await?;
            }
            if let Some(p) = &pm {
                plexmatch::store_ids(state, item_id, p).await?;
                if let Some(t) = &p.title {
                    item.title = t.clone();
                }
                item.year = p.year.or(item.year);
            }
            // Un NFO che contraddice il .plexmatch descrive un altro titolo: non si usa come metadati.
            let agrees = match (&pm, &n) {
                (Some(p), Some(n)) => p.tmdb_id.is_none_or(|t| n.tmdb_id == Some(t)) && p.imdb_id.as_ref().is_none_or(|i| n.imdb_id.as_ref() == Some(i)),
                _ => true,
            };
            if let Some(n) = n.filter(|n| !has_key && agrees && n.complete()) {
                return nfo::apply(state, item_id, item.library_id, &item.title, item.year, &item.locked_fields, &n).await;
            }
        }
    }
    if !has_key {
        // Senza chiave l'elemento resta "in attesa": si associa quando la chiave viene impostata.
        return Ok(());
    }
    let (language, _) = library_language(state, item.library_id).await;
    let movie = item.kind == "movie";
    let key = s.metadata.tmdb_api_key.clone();
    let rps = s.metadata.tmdb_requests_per_second;

    // 1. Identificativo esplicito.
    let ids: Vec<(String, String)> =
        sqlx::query_as("SELECT provider, value FROM external_ids WHERE item_id = ?").bind(item_id).fetch_all(&state.db.read).await?;
    // Un identificativo TMDB scritto nell'NFO può essere stato cancellato o unito su TMDB (404):
    // si prova con IMDb/TVDB e poi con la ricerca, invece di fallire.
    if let Some(id) = ids.iter().find(|(p, _)| p == "tmdb").and_then(|(_, v)| v.parse::<i64>().ok()) {
        match apply_tmdb(state, item_id, id, 1.0, "matched").await {
            Ok(()) => return Ok(()),
            Err(e) if tmdb::is_not_found(&e) => tracing::info!(item = item_id, tmdb = id, "identificativo TMDB non più valido: ricerca alternativa"),
            Err(e) => return Err(e),
        }
    }
    for (p, v) in &ids {
        let source = match p.as_str() {
            "imdb" => "imdb_id",
            "tvdb" => "tvdb_id",
            _ => continue,
        };
        let r = state.tmdb.get(&key, rps, &format!("/find/{v}"), &[("external_source", source.to_string())], "", 30 * DAY).await?;
        let list = if movie { &r["movie_results"] } else { &r["tv_results"] };
        if let Some(id) = list.as_array().and_then(|a| a.first()).and_then(|x| x["id"].as_i64()) {
            apply_tmdb(state, item_id, id, 1.0, "matched").await?;
            return Ok(());
        }
    }

    // 2. Ricerca con punteggio.
    let rejected: HashSet<String> =
        sqlx::query_scalar("SELECT provider_id FROM rejected_matches WHERE item_id = ?").bind(item_id).fetch_all(&state.db.read).await?.into_iter().collect();
    let cands = search(state, &item.kind, &item.title, item.year, &language, &rejected).await?;
    let threshold = s.metadata.auto_match_threshold;
    let best = cands.first();
    let _ = item.locked_fields;
    match best {
        Some(b) if auto_accept(&cands, threshold) => {
            let id: i64 = b.provider_id.parse()?;
            let score = b.score;
            apply_tmdb(state, item_id, id, score, "matched").await?;
        }
        _ => {
            // Da verificare: i candidati restano pronti da scegliere in dashboard.
            let mut tx = state.db.write.begin().await?;
            sqlx::query("DELETE FROM match_candidates WHERE item_id = ?").bind(item_id).execute(&mut *tx).await?;
            for c in cands.iter().take(6) {
                sqlx::query(
                    "INSERT OR REPLACE INTO match_candidates (item_id, provider_id, title, year, score, overview, poster_path) VALUES (?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(item_id)
                .bind(&c.provider_id)
                .bind(&c.title)
                .bind(c.year)
                .bind(c.score)
                .bind(&c.overview)
                .bind(&c.poster_path)
                .execute(&mut *tx)
                .await?;
            }
            let state_name = if cands.is_empty() { "unmatched" } else { "review" };
            sqlx::query("UPDATE items SET match_state = ?, match_score = ?, updated_at = ? WHERE id = ?")
                .bind(state_name)
                .bind(best.map(|b| b.score))
                .bind(now_ms())
                .bind(item_id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            state.events.admin("metadata.review", serde_json::json!({ "item_id": state.ids.encode(item_id) }));
        }
    }
    Ok(())
}

pub async fn refresh_item(state: &Shared, item_id: i64) -> anyhow::Result<()> {
    let id: Option<String> =
        sqlx::query_scalar("SELECT value FROM external_ids WHERE item_id = ? AND provider = 'tmdb'").bind(item_id).fetch_optional(&state.db.read).await?;
    let has_key = !state.settings.get().metadata.tmdb_api_key.trim().is_empty();
    match id.and_then(|v| v.parse::<i64>().ok()).filter(|_| has_key) {
        Some(tmdb) => {
            let st: String = sqlx::query_scalar("SELECT match_state FROM items WHERE id = ?").bind(item_id).fetch_one(&state.db.read).await?;
            let score: Option<f64> = sqlx::query_scalar("SELECT match_score FROM items WHERE id = ?").bind(item_id).fetch_one(&state.db.read).await?;
            apply_tmdb(state, item_id, tmdb, score.unwrap_or(1.0), if st == "manual" { "manual" } else { "matched" }).await
        }
        None => {
            sqlx::query("UPDATE items SET match_state = 'pending' WHERE id = ? AND match_state <> 'manual'").bind(item_id).execute(&state.db.write).await?;
            match_item(state, item_id).await
        }
    }
}

fn locked(fields: &str, name: &str) -> bool {
    fields.split(',').any(|f| f.trim() == name)
}

fn release_rating(details: &Value, country: &str, movie: bool) -> Option<String> {
    if movie {
        let list = details["release_dates"]["results"].as_array()?;
        for c in [country, "US"] {
            if let Some(r) = list.iter().find(|r| r["iso_3166_1"].as_str() == Some(c)) {
                if let Some(cert) = r["release_dates"].as_array()?.iter().filter_map(|d| d["certification"].as_str()).find(|s| !s.is_empty()) {
                    return Some(cert.to_string());
                }
            }
        }
        None
    } else {
        let list = details["content_ratings"]["results"].as_array()?;
        for c in [country, "US"] {
            if let Some(r) = list.iter().find(|r| r["iso_3166_1"].as_str() == Some(c)) {
                if let Some(s) = r["rating"].as_str().filter(|s| !s.is_empty()) {
                    return Some(s.to_string());
                }
            }
        }
        None
    }
}

/// Scarica i dettagli da TMDB e li applica all'elemento (e, per le serie, a stagioni ed episodi presenti).
pub async fn apply_tmdb(state: &Shared, item_id: i64, tmdb_id: i64, score: f64, match_state: &str) -> anyhow::Result<()> {
    let Some(item) = sqlx::query_as::<_, ItemRow>("SELECT id, library_id, kind, title, year, locked_fields, match_state FROM items WHERE id = ?")
        .bind(item_id)
        .fetch_optional(&state.db.read)
        .await?
    else {
        return Ok(());
    };
    let s = state.settings.get();
    let key = s.metadata.tmdb_api_key.clone();
    let rps = s.metadata.tmdb_requests_per_second;
    let (language, country) = library_language(state, item.library_id).await;
    let movie = item.kind == "movie";
    let lang_short = language.split('-').next().unwrap_or("en").to_string();
    // Una sola richiesta porta con sé cast, immagini, identificativi esterni e classificazioni.
    let append = if movie { "credits,images,external_ids,release_dates" } else { "aggregate_credits,credits,images,external_ids,content_ratings" };
    let path = if movie { format!("/movie/{tmdb_id}") } else { format!("/tv/{tmdb_id}") };
    let d = state
        .tmdb
        .get(&key, rps, &path, &[("append_to_response", append.to_string()), ("include_image_language", format!("{lang_short},en,null"))], &language, DAY)
        .await?;
    if d["id"].as_i64().is_none() {
        bail!("risposta TMDB senza identificativo");
    }
    let lf = item.locked_fields.clone();
    let title = if movie { d["title"].as_str() } else { d["name"].as_str() }.unwrap_or(&item.title).to_string();
    let original = if movie { d["original_title"].as_str() } else { d["original_name"].as_str() }.map(String::from);
    let date = if movie { d["release_date"].as_str() } else { d["first_air_date"].as_str() }.filter(|x| !x.is_empty()).map(String::from);
    let year = year_of(date.as_deref()).or(item.year);
    let runtime = if movie {
        d["runtime"].as_i64().filter(|r| *r > 0).map(|m| m * 60_000)
    } else {
        d["episode_run_time"].as_array().and_then(|a| a.first()).and_then(|v| v.as_i64()).map(|m| m * 60_000)
    };
    let rating = release_rating(&d, &country, movie);
    let now = now_ms();

    let mut tx = state.db.write.begin().await?;
    sqlx::query(
        "UPDATE items SET
           title = CASE WHEN ?2 THEN title ELSE ?3 END,
           sort_title = CASE WHEN ?2 THEN sort_title ELSE ?4 END,
           original_title = ?5,
           year = CASE WHEN ?6 THEN year ELSE ?7 END,
           overview = CASE WHEN ?8 THEN overview ELSE ?9 END,
           tagline = ?10,
           runtime_ms = CASE WHEN ?11 IS NULL THEN runtime_ms ELSE ?11 END,
           community_rating = ?12,
           official_rating = CASE WHEN ?13 THEN official_rating ELSE ?14 END,
           premiere_date = ?15,
           end_date = ?16,
           status = ?17,
           match_state = ?18,
           match_score = ?19,
           metadata_refreshed_at = ?20,
           updated_at = ?20
         WHERE id = ?1",
    )
    .bind(item_id)
    .bind(locked(&lf, "title"))
    .bind(&title)
    .bind(platea_naming::sort_title(&title))
    .bind(&original)
    .bind(locked(&lf, "year"))
    .bind(year)
    .bind(locked(&lf, "overview"))
    .bind(d["overview"].as_str().filter(|o| !o.is_empty()))
    .bind(d["tagline"].as_str().filter(|o| !o.is_empty()))
    .bind(runtime)
    .bind(d["vote_average"].as_f64())
    .bind(locked(&lf, "official_rating"))
    .bind(&rating)
    .bind(&date)
    .bind(if movie { None } else { d["last_air_date"].as_str() })
    .bind(d["status"].as_str())
    .bind(match_state)
    .bind(score)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM match_candidates WHERE item_id = ?").bind(item_id).execute(&mut *tx).await?;
    sqlx::query("INSERT OR REPLACE INTO external_ids (item_id, provider, value) VALUES (?, 'tmdb', ?)")
        .bind(item_id)
        .bind(tmdb_id.to_string())
        .execute(&mut *tx)
        .await?;
    if let Some(imdb) = d["external_ids"]["imdb_id"].as_str().or(d["imdb_id"].as_str()).filter(|s| !s.is_empty()) {
        sqlx::query("INSERT OR REPLACE INTO external_ids (item_id, provider, value) VALUES (?, 'imdb', ?)").bind(item_id).bind(imdb).execute(&mut *tx).await?;
    }
    if let Some(tvdb) = d["external_ids"]["tvdb_id"].as_i64() {
        sqlx::query("INSERT OR REPLACE INTO external_ids (item_id, provider, value) VALUES (?, 'tvdb', ?)")
            .bind(item_id)
            .bind(tvdb.to_string())
            .execute(&mut *tx)
            .await?;
    }

    // Generi
    if !locked(&lf, "genres") {
        sqlx::query("DELETE FROM item_genres WHERE item_id = ?").bind(item_id).execute(&mut *tx).await?;
        for g in d["genres"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
            if let Some(name) = g["name"].as_str() {
                sqlx::query("INSERT OR IGNORE INTO genres (name) VALUES (?)").bind(name).execute(&mut *tx).await?;
                sqlx::query("INSERT OR IGNORE INTO item_genres (item_id, genre_id) SELECT ?, id FROM genres WHERE name = ?")
                    .bind(item_id)
                    .bind(name)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }
    // Studi / reti
    sqlx::query("DELETE FROM item_studios WHERE item_id = ?").bind(item_id).execute(&mut *tx).await?;
    let studios = if movie { &d["production_companies"] } else { &d["networks"] };
    for st in studios.as_array().map(|a| a.as_slice()).unwrap_or(&[]).iter().take(5) {
        if let Some(name) = st["name"].as_str() {
            sqlx::query("INSERT OR IGNORE INTO studios (name) VALUES (?)").bind(name).execute(&mut *tx).await?;
            sqlx::query("INSERT OR IGNORE INTO item_studios (item_id, studio_id) SELECT ?, id FROM studios WHERE name = ?")
                .bind(item_id)
                .bind(name)
                .execute(&mut *tx)
                .await?;
        }
    }
    // Persone: cast principale e registi/sceneggiatori. Le foto si scaricano solo quando una pagina le mostra.
    sqlx::query("DELETE FROM item_people WHERE item_id = ?").bind(item_id).execute(&mut *tx).await?;
    let credits = if movie {
        &d["credits"]
    } else if d["aggregate_credits"].is_object() {
        &d["aggregate_credits"]
    } else {
        &d["credits"]
    };
    let mut order = 0i64;
    for c in credits["cast"].as_array().map(|a| a.as_slice()).unwrap_or(&[]).iter().take(20) {
        let character = c["character"]
            .as_str()
            .map(String::from)
            .or_else(|| c["roles"].as_array().and_then(|r| r.first()).and_then(|r| r["character"].as_str()).map(String::from));
        upsert_person(&mut tx, item_id, c, "actor", character.as_deref(), order).await?;
        order += 1;
    }
    for c in credits["crew"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        let job = c["job"].as_str().or_else(|| c["jobs"].as_array().and_then(|j| j.first()).and_then(|j| j["job"].as_str()));
        let role = match job {
            Some("Director") => "director",
            Some("Screenplay") | Some("Writer") => "writer",
            _ => continue,
        };
        upsert_person(&mut tx, item_id, c, role, None, order).await?;
        order += 1;
        if order > 40 {
            break;
        }
    }
    if !movie {
        for c in d["created_by"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
            upsert_person(&mut tx, item_id, c, "creator", None, order).await?;
            order += 1;
        }
    }

    // Collezione (saga) di TMDB, con le sue immagini.
    let mut new_images: Vec<i64> = Vec::new();
    if movie {
        let col = &d["belongs_to_collection"];
        if let (Some(cid), Some(name)) = (col["id"].as_i64(), col["name"].as_str()) {
            if let Some(c) = collections::link(&mut tx, item.library_id, item_id, name, Some(cid)).await? {
                for (kind, path, size) in [("poster", col["poster_path"].as_str(), "w780"), ("backdrop", col["backdrop_path"].as_str(), "w1280")] {
                    if let Some(id) = set_image(&mut tx, Some(c), None, kind, path, size).await? {
                        new_images.push(id);
                    }
                }
            }
        }
    }
    // Immagini: locandina, sfondo, logo.
    let poster = d["poster_path"].as_str();
    let backdrop = d["backdrop_path"].as_str();
    let logo = pick_logo(&d["images"]["logos"], &lang_short);
    for (kind, path, size) in [("poster", poster, "w780"), ("backdrop", backdrop, "w1280"), ("logo", logo.as_deref(), "w500")] {
        if locked(&lf, kind) {
            continue;
        }
        if let Some(id) = set_image(&mut tx, Some(item_id), None, kind, path, size).await? {
            new_images.push(id);
        }
    }
    tx.commit().await?;

    for id in new_images {
        jobs::enqueue(state, JobKind::DownloadImage { image_id: id }, priority::IMAGES, Some(item.library_id)).await?;
    }

    if !movie {
        apply_seasons(state, item_id, tmdb_id, &key, rps, &language).await?;
    }
    state.events.publish("item.updated", serde_json::json!({ "item_id": state.ids.encode(item_id) }));
    Ok(())
}

fn pick_logo(logos: &Value, lang: &str) -> Option<String> {
    let list = logos.as_array()?;
    let by = |l: Option<&str>| list.iter().find(|x| x["iso_639_1"].as_str() == l).and_then(|x| x["file_path"].as_str()).map(String::from);
    by(Some(lang)).or_else(|| by(Some("en"))).or_else(|| list.first().and_then(|x| x["file_path"].as_str()).map(String::from))
}

async fn upsert_person(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    item_id: i64,
    c: &Value,
    role: &str,
    character: Option<&str>,
    order: i64,
) -> anyhow::Result<()> {
    let (Some(tmdb), Some(name)) = (c["id"].as_i64(), c["name"].as_str()) else { return Ok(()) };
    let profile = c["profile_path"].as_str();
    sqlx::query(
        "INSERT INTO people (name, tmdb_id, profile_path) VALUES (?, ?, ?)
         ON CONFLICT(tmdb_id) DO UPDATE SET name = excluded.name, profile_path = COALESCE(excluded.profile_path, people.profile_path)",
    )
    .bind(name)
    .bind(tmdb)
    .bind(profile)
    .execute(&mut **tx)
    .await?;
    let pid: i64 = sqlx::query_scalar("SELECT id FROM people WHERE tmdb_id = ?").bind(tmdb).fetch_one(&mut **tx).await?;
    sqlx::query("INSERT OR IGNORE INTO item_people (item_id, person_id, role, character, sort_order) VALUES (?, ?, ?, ?, ?)")
        .bind(item_id)
        .bind(pid)
        .bind(role)
        .bind(character)
        .bind(order)
        .execute(&mut **tx)
        .await?;
    if let Some(p) = profile {
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT id FROM images WHERE person_id = ? AND kind = 'profile'").bind(pid).fetch_optional(&mut **tx).await?;
        if exists.is_none() {
            sqlx::query("INSERT INTO images (person_id, kind, source_url, state) VALUES (?, 'profile', ?, 'pending')")
                .bind(pid)
                .bind(format!("{}/w185{p}", tmdb::IMAGE_BASE.as_str()))
                .execute(&mut **tx)
                .await?;
        }
    }
    Ok(())
}

/// Imposta l'immagine di un tipo da un percorso TMDB. Restituisce l'id se va scaricata.
async fn set_image(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    item_id: Option<i64>,
    _person: Option<i64>,
    kind: &str,
    path: Option<&str>,
    size: &str,
) -> anyhow::Result<Option<i64>> {
    let (Some(path), Some(item_id)) = (path.filter(|p| !p.is_empty()), item_id) else { return Ok(None) };
    set_image_url(tx, item_id, kind, &format!("{}/{size}{path}", tmdb::IMAGE_BASE.as_str())).await
}

/// Imposta l'immagine di un tipo da un indirizzo completo. Restituisce l'id se va scaricata.
async fn set_image_url(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, item_id: i64, kind: &str, url: &str) -> anyhow::Result<Option<i64>> {
    let existing: Option<(i64, String)> =
        sqlx::query_as("SELECT id, source_url FROM images WHERE item_id = ? AND kind = ?").bind(item_id).bind(kind).fetch_optional(&mut **tx).await?;
    match existing {
        Some((_, u)) if u == url => Ok(None),
        Some((old, _)) => {
            // Immagine cambiata: riga nuova inserita PRIMA di togliere la vecchia, così l'identificativo (e l'indirizzo,
            // tenuto in cache dai browser come immutabile) non si ripete mai.
            let id = sqlx::query("INSERT INTO images (item_id, kind, source_url, state) VALUES (?, ?, ?, 'pending')")
                .bind(item_id)
                .bind(kind)
                .bind(url)
                .execute(&mut **tx)
                .await?
                .last_insert_rowid();
            sqlx::query("DELETE FROM images WHERE id = ?").bind(old).execute(&mut **tx).await?;
            Ok(Some(id))
        }
        None => {
            let id = sqlx::query("INSERT INTO images (item_id, kind, source_url, state) VALUES (?, ?, ?, 'pending')")
                .bind(item_id)
                .bind(kind)
                .bind(url)
                .execute(&mut **tx)
                .await?
                .last_insert_rowid();
            Ok(Some(id))
        }
    }
}

/// Serie: una richiesta per stagione, applicata a stagioni ed episodi già in libreria.
async fn apply_seasons(state: &Shared, series_id: i64, tmdb_id: i64, key: &str, rps: u32, language: &str) -> anyhow::Result<()> {
    let seasons: Vec<(i64, i64, String)> =
        sqlx::query_as("SELECT id, index_number, locked_fields FROM items WHERE parent_id = ? AND kind = 'season' AND index_number IS NOT NULL")
            .bind(series_id)
            .fetch_all(&state.db.read)
            .await?;
    let library_id: i64 = sqlx::query_scalar("SELECT library_id FROM items WHERE id = ?").bind(series_id).fetch_one(&state.db.read).await?;
    for (season_id, number, lf) in seasons {
        let d = match state.tmdb.get(key, rps, &format!("/tv/{tmdb_id}/season/{number}"), &[], language, DAY).await {
            Ok(d) => d,
            Err(e) => {
                tracing::debug!(season = number, error = %e, "stagione non trovata su TMDB");
                continue;
            }
        };
        let now = now_ms();
        let mut tx = state.db.write.begin().await?;
        let name = d["name"].as_str().filter(|n| !n.is_empty());
        sqlx::query(
            "UPDATE items SET title = CASE WHEN ?2 OR ?3 IS NULL THEN title ELSE ?3 END,
               sort_title = CASE WHEN ?2 OR ?3 IS NULL THEN sort_title ELSE ?4 END,
               overview = ?5, premiere_date = ?6, year = ?7, metadata_refreshed_at = ?8, updated_at = ?8, match_state = 'matched'
             WHERE id = ?1",
        )
        .bind(season_id)
        .bind(locked(&lf, "title"))
        .bind(name)
        .bind(name.map(platea_naming::sort_title))
        .bind(d["overview"].as_str().filter(|o| !o.is_empty()))
        .bind(d["air_date"].as_str())
        .bind(year_of(d["air_date"].as_str()))
        .bind(now)
        .execute(&mut *tx)
        .await?;
        let mut imgs = Vec::new();
        if let Some(id) = set_image(&mut tx, Some(season_id), None, "poster", d["poster_path"].as_str(), "w780").await? {
            imgs.push(id);
        }
        let episodes: Vec<(i64, i64, String)> =
            sqlx::query_as("SELECT id, index_number, locked_fields FROM items WHERE parent_id = ? AND kind = 'episode' AND index_number IS NOT NULL")
                .bind(season_id)
                .fetch_all(&mut *tx)
                .await?;
        let tmdb_eps = d["episodes"].as_array().cloned().unwrap_or_default();
        for (ep_id, n, elf) in episodes {
            let Some(e) = tmdb_eps.iter().find(|e| e["episode_number"].as_i64() == Some(n)) else { continue };
            let name = e["name"].as_str().filter(|n| !n.is_empty());
            sqlx::query(
                "UPDATE items SET title = CASE WHEN ?2 OR ?3 IS NULL THEN title ELSE ?3 END,
                   sort_title = CASE WHEN ?2 OR ?3 IS NULL THEN sort_title ELSE ?4 END,
                   overview = CASE WHEN ?5 THEN overview ELSE ?6 END,
                   premiere_date = ?7, year = ?8, community_rating = ?9,
                   runtime_ms = COALESCE(?10, runtime_ms), metadata_refreshed_at = ?11, updated_at = ?11, match_state = 'matched'
                 WHERE id = ?1",
            )
            .bind(ep_id)
            .bind(locked(&elf, "title"))
            .bind(name)
            .bind(name.map(platea_naming::sort_title))
            .bind(locked(&elf, "overview"))
            .bind(e["overview"].as_str().filter(|o| !o.is_empty()))
            .bind(e["air_date"].as_str())
            .bind(year_of(e["air_date"].as_str()))
            .bind(e["vote_average"].as_f64())
            .bind(e["runtime"].as_i64().filter(|r| *r > 0).map(|m| m * 60_000))
            .bind(now)
            .execute(&mut *tx)
            .await?;
            if let Some(id) = set_image(&mut tx, Some(ep_id), None, "still", e["still_path"].as_str(), "w780").await? {
                imgs.push(id);
            }
        }
        tx.commit().await?;
        for id in imgs {
            jobs::enqueue(state, JobKind::DownloadImage { image_id: id }, priority::IMAGES, Some(library_id)).await?;
        }
    }
    Ok(())
}

/// Aggiornamento periodico: elementi con metadati più vecchi di `refresh_days`; serie in corso ogni settimana.
pub async fn schedule_refresh(state: &Shared) -> anyhow::Result<()> {
    let days = state.settings.get().metadata.refresh_days as i64;
    if days == 0 || state.settings.get().metadata.tmdb_api_key.is_empty() {
        return Ok(());
    }
    let now = now_ms();
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT id, library_id FROM items WHERE kind IN ('movie','series') AND match_state IN ('matched','manual')
           AND (metadata_refreshed_at < ? OR (kind = 'series' AND status = 'Returning Series' AND metadata_refreshed_at < ?))
         ORDER BY metadata_refreshed_at LIMIT 500",
    )
    .bind(now - days * DAY)
    .bind(now - 7 * DAY)
    .fetch_all(&state.db.read)
    .await?;
    for (id, lib) in rows {
        jobs::enqueue(state, JobKind::RefreshMetadata { item_id: id }, priority::MAINTENANCE, Some(lib)).await?;
    }
    Ok(())
}

/// Accoda l'associazione di tutti gli elementi ancora in attesa (per esempio dopo aver impostato la chiave).
pub async fn enqueue_pending(state: &Shared) -> anyhow::Result<usize> {
    let rows: Vec<(i64, i64)> = sqlx::query_as("SELECT id, library_id FROM items WHERE kind IN ('movie','series') AND match_state IN ('pending','unmatched')")
        .fetch_all(&state.db.read)
        .await?;
    let n = rows.len();
    for (id, lib) in rows {
        jobs::enqueue(state, JobKind::MatchItem { item_id: id }, priority::METADATA, Some(lib)).await?;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scoring_prefers_exact_title_and_year() {
        let exact = score_candidate("The Matrix", Some(1999), "The Matrix", None, Some(1999), 50.0);
        let other = score_candidate("The Matrix", Some(1999), "The Matrix Resurrections", None, Some(2021), 80.0);
        assert!(exact > 0.9, "{exact}");
        assert!(other < 0.6, "{other}");
        let original = score_candidate("La vita è bella", Some(1997), "Life Is Beautiful", Some("La vita è bella"), Some(1997), 20.0);
        assert!(original > 0.9);
        let close_year = score_candidate("Dune", Some(2020), "Dune", None, Some(2021), 100.0);
        assert!(close_year > 0.8);
    }

    #[test]
    fn italian_subtitles_and_apostrophes() {
        // Titolo inglese + sottotitolo italiano nel nome della cartella.
        assert!(score_candidate("Pay The Ghost Il Male Cammina Tra Noi", Some(2015), "Pay the Ghost", None, Some(2015), 10.0) >= 0.8);
        assert!(score_candidate("Dirty Dancing Balli Proibiti", Some(1987), "Dirty Dancing", None, Some(1987), 30.0) >= 0.8);
        // "L Odissea" (apostrofo perso nel nome del file) = "L'Odissea".
        assert!(title_match("L Odissea", "L'Odissea") > 0.99);
        // Un sequel con un altro anno resta sotto la soglia.
        assert!(
            score_candidate("Scream", Some(1996), "Scream 2", None, Some(1997), 30.0) < score_candidate("Scream", Some(1996), "Scream", None, Some(1996), 30.0)
        );
        assert!(score_candidate("The Matrix", Some(1999), "The Matrix Resurrections", None, Some(2021), 80.0) < 0.6);
        // Titolo italiano seguito dall'originale.
        assert!(title_match("Asino Per Sempre Jackass Forever", "Jackass Forever") >= 0.9);
        assert_eq!(partial_queries("Now You See Me 2 I Maghi Del Crimine")[0], "Now You See Me 2 I Maghi Del");
        assert!(partial_queries("Now You See Me 2 I Maghi Del Crimine").contains(&"Del Crimine".to_string()));
        assert!(partial_queries("Due parole").is_empty());
        // Una parola sola e corta non basta come prefisso.
        assert!(title_match("Up", "Up in the Air") < 0.9);
    }

    fn cand(id: &str, score: f64, sim: f64, year_exact: bool) -> Candidate {
        Candidate {
            provider_id: id.into(),
            title: id.into(),
            original_title: None,
            year: None,
            score,
            overview: None,
            poster_path: None,
            popularity: 0.0,
            title_sim: sim,
            year_exact,
        }
    }

    #[test]
    fn accept_rules() {
        // Due voci con stesso titolo e anno (duplicati TMDB): si prende la prima.
        assert!(auto_accept(&[cand("a", 0.96, 1.0, true), cand("b", 0.95, 1.0, true)], 0.8));
        // Unico risultato con l'anno giusto e titolo simile (traduzione diversa).
        assert!(auto_accept(&[cand("a", 0.6, 0.55, true)], 0.8));
        // Unico risultato ma anno diverso: si chiede.
        assert!(!auto_accept(&[cand("a", 0.6, 0.55, false)], 0.8));
        // Due candidati vicini sotto il titolo identico: si chiede.
        assert!(!auto_accept(&[cand("a", 0.85, 0.9, true), cand("b", 0.84, 0.9, true)], 0.8));
    }
}
