//! Metadati da file NFO locali (formato Kodi, scritto anche da Jellyfin, Emby, Radarr, Sonarr, tinyMediaManager).
//! Il file si legge una sola volta, quando l'elemento è nuovo, e mai si scrive: le cartelle dei media restano intatte.
//! Con una chiave TMDB l'NFO serve solo a dare l'identificativo esatto; senza, i suoi campi diventano i metadati.

use super::{locked, set_image_url, tmdb, year_of};
use crate::state::Shared;
use crate::util::now_ms;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Dimensione massima di un NFO: oltre, non è un NFO.
const MAX_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Person {
    pub name: String,
    pub role: Option<String>,
    pub order: Option<i64>,
    pub thumb: Option<String>,
    pub tmdb_id: Option<i64>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Nfo {
    pub kind: String,
    pub title: Option<String>,
    pub original_title: Option<String>,
    pub plot: Option<String>,
    pub tagline: Option<String>,
    pub runtime_min: Option<i64>,
    pub rating: Option<f64>,
    pub mpaa: Option<String>,
    pub premiered: Option<String>,
    pub year: Option<i64>,
    pub status: Option<String>,
    pub tmdb_id: Option<i64>,
    pub imdb_id: Option<String>,
    pub tvdb_id: Option<String>,
    pub genres: Vec<String>,
    pub studios: Vec<String>,
    pub directors: Vec<Person>,
    pub writers: Vec<Person>,
    pub actors: Vec<Person>,
    pub poster: Option<String>,
    pub backdrop: Option<String>,
    pub logo: Option<String>,
    pub set_name: Option<String>,
    /// Solo negli NFO degli episodi.
    pub season: Option<i64>,
    pub episode: Option<i64>,
}

impl Nfo {
    /// Un NFO con almeno un titolo o un identificativo: abbastanza per usarlo.
    pub fn useful(&self) -> bool {
        self.tmdb_id.is_some() || self.imdb_id.is_some() || self.title.is_some()
    }
    /// Abbastanza per fare da metadati completi senza TMDB.
    pub fn complete(&self) -> bool {
        self.title.is_some() && (self.plot.is_some() || self.poster.is_some())
    }
}

fn text(n: roxmltree::Node) -> Option<String> {
    let t: String = n.children().filter(|c| c.is_text()).filter_map(|c| c.text()).collect();
    let t = t.trim();
    (!t.is_empty()).then(|| t.to_string())
}

fn child<'a>(n: roxmltree::Node<'a, 'a>, name: &str) -> Option<roxmltree::Node<'a, 'a>> {
    n.children().find(|c| c.has_tag_name(name))
}

fn child_text(n: roxmltree::Node, name: &str) -> Option<String> {
    child(n, name).and_then(text)
}

fn http(u: Option<String>) -> Option<String> {
    u.filter(|u| u.starts_with("http://") || u.starts_with("https://"))
}

/// Identificativi in un NFO "corto", che contiene solo un indirizzo IMDb o TMDB.
fn ids_from_urls(s: &str, out: &mut Nfo) {
    static IMDB: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| regex::Regex::new(r"\b(tt\d{6,10})\b").unwrap());
    static TMDB: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| regex::Regex::new(r"themoviedb\.org/(?:movie|tv)/(\d+)").unwrap());
    if out.imdb_id.is_none() {
        out.imdb_id = IMDB.captures(s).map(|c| c[1].to_string());
    }
    if out.tmdb_id.is_none() {
        out.tmdb_id = TMDB.captures(s).and_then(|c| c[1].parse().ok());
    }
}

/// Legge un NFO. Tollera testo dopo il documento (riga con l'indirizzo, usata da Kodi) e NFO fatti di soli indirizzi.
pub fn parse(raw: &str) -> Option<Nfo> {
    let raw = raw.trim_start_matches('\u{feff}');
    let mut out = Nfo::default();
    let roots = ["movie", "tvshow", "episodedetails"];
    let span = roots.iter().find_map(|r| {
        let start = raw.find(&format!("<{r}"))?;
        let end_tag = format!("</{r}>");
        let end = raw.rfind(&end_tag)? + end_tag.len();
        (end > start).then_some((start, end))
    });
    let Some((start, end)) = span else {
        ids_from_urls(raw, &mut out);
        out.kind = "movie".into();
        return out.useful().then_some(out);
    };
    // Entità HTML non dichiarate (&nbsp; ecc.) farebbero fallire il parser XML.
    let cleaned = raw[start..end].replace("&nbsp;", " ");
    let doc = roxmltree::Document::parse_with_options(&cleaned, roxmltree::ParsingOptions { allow_dtd: true, nodes_limit: 200_000 }).ok()?;
    let root = doc.root_element();
    out.kind = root.tag_name().name().to_string();
    out.title = child_text(root, "title");
    out.original_title = child_text(root, "originaltitle");
    out.plot = child_text(root, "plot").or_else(|| child_text(root, "outline"));
    out.tagline = child_text(root, "tagline");
    out.season = child_text(root, "season").and_then(|v| v.parse().ok());
    out.episode = child_text(root, "episode").and_then(|v| v.parse().ok());
    out.runtime_min = child_text(root, "runtime").and_then(|r| r.split_whitespace().next()?.parse::<f64>().ok()).map(|m| m.round() as i64).filter(|m| *m > 0);
    out.status = child_text(root, "status");
    out.premiered = child_text(root, "premiered").or_else(|| child_text(root, "aired")).or_else(|| child_text(root, "releasedate"));
    out.year = child_text(root, "year").and_then(|y| y.parse().ok()).filter(|y| (1870..2200).contains(y)).or_else(|| year_of(out.premiered.as_deref()));
    out.mpaa = child_text(root, "mpaa").map(|m| {
        let m = m.trim_start_matches("Rated ").to_string();
        // "IT:T / US:PG" → la prima voce, senza paese.
        let first = m.split('/').next().unwrap_or(&m).trim().to_string();
        first.split_once(':').map(|(_, v)| v.trim().to_string()).unwrap_or(first)
    });
    // Valutazione: quella predefinita in <ratings>, altrimenti <rating>.
    let default_rating = child(root, "ratings").and_then(|rs| {
        let list: Vec<_> = rs.children().filter(|c| c.has_tag_name("rating")).collect();
        let pick = list.iter().find(|r| r.attribute("default") == Some("true")).or(list.first())?;
        let max: f64 = pick.attribute("max").and_then(|m| m.parse().ok()).unwrap_or(10.0);
        child_text(*pick, "value")?.parse::<f64>().ok().map(|v| if max > 0.0 { v * 10.0 / max } else { v })
    });
    out.rating = default_rating.or_else(|| child_text(root, "rating").and_then(|r| r.replace(',', ".").parse().ok())).filter(|r| *r > 0.0);

    for u in root.children().filter(|c| c.has_tag_name("uniqueid")) {
        let Some(v) = text(u) else { continue };
        match u.attribute("type").unwrap_or("").to_ascii_lowercase().as_str() {
            "tmdb" => out.tmdb_id = out.tmdb_id.or(v.parse().ok()),
            "imdb" => out.imdb_id = out.imdb_id.clone().or(Some(v)),
            "tvdb" => out.tvdb_id = out.tvdb_id.clone().or(Some(v)),
            _ => {}
        }
    }
    for (tag, slot) in [("tmdbid", 0), ("imdbid", 1), ("imdb_id", 1), ("tvdbid", 2)] {
        if let Some(v) = child_text(root, tag) {
            match slot {
                0 => out.tmdb_id = out.tmdb_id.or(v.parse().ok()),
                1 => out.imdb_id = out.imdb_id.clone().or(Some(v)),
                _ => out.tvdb_id = out.tvdb_id.clone().or(Some(v)),
            }
        }
    }
    // <id> è IMDb (tt…) nei NFO vecchi, TMDB nei film recenti di Kodi, TVDB nelle serie.
    if let Some(id) = child_text(root, "id") {
        if id.starts_with("tt") {
            out.imdb_id = out.imdb_id.clone().or(Some(id));
        } else if out.kind == "movie" {
            out.tmdb_id = out.tmdb_id.or(id.parse().ok());
        } else if out.kind == "tvshow" {
            out.tvdb_id = out.tvdb_id.clone().or(Some(id));
        }
    }
    ids_from_urls(&raw[end..], &mut out);
    if out.imdb_id.as_deref().is_some_and(|i| !i.starts_with("tt")) {
        out.imdb_id = None;
    }

    let mut seen_g = std::collections::HashSet::new();
    for g in root.children().filter(|c| c.has_tag_name("genre")).filter_map(text) {
        for part in g.split(" / ") {
            let p = part.trim().to_string();
            if !p.is_empty() && seen_g.insert(p.to_lowercase()) {
                out.genres.push(p);
            }
        }
    }
    out.studios = root.children().filter(|c| c.has_tag_name("studio")).filter_map(text).take(5).collect();
    let person = |n: roxmltree::Node| -> Option<Person> {
        Some(Person { name: text(n)?, tmdb_id: n.attribute("tmdbid").and_then(|v| v.parse().ok()), ..Default::default() })
    };
    out.directors = root.children().filter(|c| c.has_tag_name("director")).filter_map(person).collect();
    out.writers = root.children().filter(|c| c.has_tag_name("credits") || c.has_tag_name("writer")).filter_map(person).collect();
    for a in root.children().filter(|c| c.has_tag_name("actor")) {
        let Some(name) = child_text(a, "name") else { continue };
        out.actors.push(Person {
            name,
            role: child_text(a, "role"),
            order: child_text(a, "order").and_then(|o| o.parse().ok()),
            thumb: http(child_text(a, "thumb")),
            tmdb_id: child_text(a, "tmdbid").and_then(|v| v.parse().ok()),
        });
    }
    out.actors.sort_by_key(|a| a.order.unwrap_or(i64::MAX));

    for t in root.children().filter(|c| c.has_tag_name("thumb")) {
        let url = http(text(t)).or_else(|| http(t.attribute("preview").map(String::from)));
        match t.attribute("aspect").unwrap_or("poster") {
            "poster" if out.poster.is_none() => out.poster = url,
            "clearlogo" | "logo" if out.logo.is_none() => out.logo = url,
            "fanart" | "backdrop" if out.backdrop.is_none() => out.backdrop = url,
            _ => {}
        }
    }
    if out.backdrop.is_none() {
        out.backdrop = child(root, "fanart").and_then(|f| f.children().find(|c| c.has_tag_name("thumb"))).and_then(|t| http(text(t)));
    }
    out.set_name = child(root, "set").and_then(|s| child_text(s, "name").or_else(|| text(s)));
    Some(out)
}

/// Un indirizzo di immagine TMDB alla dimensione richiesta, dalla base configurata; gli altri restano com'erano.
pub fn image_url(url: &str, size: &str) -> String {
    for prefix in ["https://image.tmdb.org/t/p/", "http://image.tmdb.org/t/p/"] {
        if let Some(rest) = url.strip_prefix(prefix) {
            if let Some((_, path)) = rest.split_once('/') {
                return format!("{}/{size}/{path}", tmdb::IMAGE_BASE.as_str());
            }
        }
    }
    url.to_string()
}

/// Percorso TMDB di una foto ("/abc.jpg"), se l'indirizzo viene da TMDB.
fn tmdb_path(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://image.tmdb.org/t/p/").or_else(|| url.strip_prefix("http://image.tmdb.org/t/p/"))?;
    rest.split_once('/').map(|(_, p)| format!("/{p}"))
}

/// NFO di un episodio: uno o più `<episodedetails>` (un file con più episodi ne ha uno per episodio, uno dopo l'altro).
pub fn parse_episodes(raw: &str) -> Vec<Nfo> {
    let mut out = Vec::new();
    let mut rest = raw;
    while let Some(start) = rest.find("<episodedetails") {
        let block = &rest[start..];
        let Some(end) = block.find("</episodedetails>") else { break };
        let end = end + "</episodedetails>".len();
        if let Some(n) = parse(&block[..end]) {
            out.push(n);
        }
        rest = &block[end..];
    }
    out
}

/// Applica gli NFO degli episodi di una serie (solo i file con un NFO visto durante la scansione).
/// L'NFO è la scelta locale dell'utente: vince sui metadati di TMDB, ma non sui campi bloccati.
pub async fn apply_episodes(state: &Shared, series_id: i64) -> anyhow::Result<usize> {
    if !state.settings.get().metadata.read_nfo {
        return Ok(0);
    }
    let rows: Vec<(i64, i64, Option<i64>, Option<i64>, String)> = sqlx::query_as(
        "SELECT e.id, f.id, e.parent_index_number, e.index_number, e.locked_fields FROM items e JOIN media_files f ON f.item_id = e.id
         WHERE e.series_id = ? AND e.kind = 'episode' AND f.has_nfo = 1 AND f.state = 'present'",
    )
    .bind(series_id)
    .fetch_all(&state.db.read)
    .await?;
    let timeout = fs_timeout(state);
    let mut applied = 0;
    for (ep_id, file_id, season, number, lf) in rows {
        let Some((abs, ..)) = crate::probe::file_abs_path(state, file_id).await? else { continue };
        let Some(raw) = read_small(abs.with_extension("nfo"), timeout).await else { continue };
        let blocks = parse_episodes(&raw);
        // Il blocco del proprio episodio; altrimenti il primo (NFO senza numeri).
        let Some(n) = blocks.iter().find(|n| n.episode == number && n.season.is_none_or(|s| Some(s) == season)).or(blocks.first()) else { continue };
        if n.title.is_none() && n.plot.is_none() && n.premiered.is_none() {
            continue;
        }
        sqlx::query(
            "UPDATE items SET
               title = CASE WHEN ?2 OR ?3 IS NULL THEN title ELSE ?3 END,
               sort_title = CASE WHEN ?2 OR ?3 IS NULL THEN sort_title ELSE ?4 END,
               overview = CASE WHEN ?5 OR ?6 IS NULL THEN overview ELSE ?6 END,
               premiere_date = COALESCE(?7, premiere_date), year = COALESCE(?8, year),
               community_rating = COALESCE(?9, community_rating), runtime_ms = COALESCE(?10, runtime_ms),
               match_state = 'matched', updated_at = ?11
             WHERE id = ?1",
        )
        .bind(ep_id)
        .bind(locked(&lf, "title"))
        .bind(&n.title)
        .bind(n.title.as_deref().map(platea_naming::sort_title))
        .bind(locked(&lf, "overview"))
        .bind(&n.plot)
        .bind(&n.premiered)
        .bind(year_of(n.premiered.as_deref()))
        .bind(n.rating)
        .bind(n.runtime_min.map(|m| m * 60_000))
        .bind(now_ms())
        .execute(&state.db.write)
        .await?;
        applied += 1;
    }
    if applied > 0 {
        tracing::debug!(series = series_id, episodes = applied, "NFO degli episodi applicati");
    }
    Ok(applied)
}

pub(crate) async fn read_small(path: PathBuf, timeout: Duration) -> Option<String> {
    let fut = async {
        let meta = tokio::fs::metadata(&path).await.ok()?;
        if !meta.is_file() || meta.len() > MAX_BYTES {
            return None;
        }
        let bytes = tokio::fs::read(&path).await.ok()?;
        Some(String::from_utf8_lossy(&bytes).into_owned())
    };
    tokio::time::timeout(timeout, fut).await.ok().flatten()
}

/// Dove cercare i file accanto a un elemento: il primo file video e le cartelle candidate
/// (quella del film; per una serie quella dell'episodio e fino a due livelli sopra, dentro la libreria).
pub struct Located {
    pub file: PathBuf,
    pub dirs: Vec<PathBuf>,
}

pub async fn locate(state: &Shared, item_id: i64, kind: &str) -> Option<Located> {
    let file_id: Option<i64> = if kind == "series" {
        sqlx::query_scalar("SELECT f.id FROM media_files f JOIN items e ON e.id = f.item_id WHERE e.series_id = ? AND f.state = 'present' LIMIT 1")
            .bind(item_id)
            .fetch_optional(&state.db.read)
            .await
            .ok()
            .flatten()
    } else {
        sqlx::query_scalar("SELECT id FROM media_files WHERE item_id = ? AND state = 'present' ORDER BY id LIMIT 1")
            .bind(item_id)
            .fetch_optional(&state.db.read)
            .await
            .ok()
            .flatten()
    };
    let (abs, rel, ..) = crate::probe::file_abs_path(state, file_id?).await.ok()??;
    let dir = abs.parent()?.to_path_buf();
    let mut dirs = Vec::new();
    if kind == "series" {
        let depth = rel.matches('/').count();
        let mut d: &Path = &dir;
        for _ in 0..depth.min(3) {
            dirs.push(d.to_path_buf());
            match d.parent() {
                Some(p) => d = p,
                None => break,
            }
        }
    } else {
        dirs.push(dir);
    }
    Some(Located { file: abs, dirs })
}

pub fn fs_timeout(state: &Shared) -> Duration {
    Duration::from_secs(state.settings.get().scanner.fs_timeout_seconds.max(2) as u64)
}

/// Cerca e legge l'NFO di un elemento: `<film>.nfo` o `movie.nfo` accanto al file; `tvshow.nfo` nella cartella della serie.
pub async fn find(state: &Shared, loc: &Located, kind: &str) -> Option<Nfo> {
    let candidates: Vec<PathBuf> = if kind == "series" {
        loc.dirs.iter().map(|d| d.join("tvshow.nfo")).collect()
    } else {
        vec![loc.file.with_extension("nfo"), loc.dirs[0].join("movie.nfo")]
    };
    let timeout = fs_timeout(state);
    for c in candidates {
        if let Some(raw) = read_small(c.clone(), timeout).await {
            if let Some(n) = parse(&raw) {
                if n.useful() {
                    tracing::debug!(file = %c.display(), "NFO letto");
                    return Some(n);
                }
            }
        }
    }
    None
}

/// Salva gli identificativi dell'NFO, così l'associazione con TMDB è esatta.
pub async fn store_ids(state: &Shared, item_id: i64, n: &Nfo) -> anyhow::Result<()> {
    for (p, v) in [("tmdb", n.tmdb_id.map(|v| v.to_string())), ("imdb", n.imdb_id.clone()), ("tvdb", n.tvdb_id.clone())] {
        if let Some(v) = v {
            sqlx::query("INSERT OR REPLACE INTO external_ids (item_id, provider, value) VALUES (?, ?, ?)")
                .bind(item_id)
                .bind(p)
                .bind(v)
                .execute(&state.db.write)
                .await?;
        }
    }
    Ok(())
}

/// Applica l'NFO come fonte dei metadati (nessuna chiave TMDB). Rispetta i campi bloccati.
pub async fn apply(state: &Shared, item_id: i64, library_id: i64, current_title: &str, current_year: Option<i64>, lf: &str, n: &Nfo) -> anyhow::Result<()> {
    let title = n.title.clone().unwrap_or_else(|| current_title.to_string());
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
           status = ?16,
           match_state = 'matched',
           match_score = 1.0,
           metadata_refreshed_at = ?17,
           updated_at = ?17
         WHERE id = ?1",
    )
    .bind(item_id)
    .bind(locked(lf, "title"))
    .bind(&title)
    .bind(platea_naming::sort_title(&title))
    .bind(&n.original_title)
    .bind(locked(lf, "year"))
    .bind(n.year.or(current_year))
    .bind(locked(lf, "overview"))
    .bind(&n.plot)
    .bind(&n.tagline)
    .bind(n.runtime_min.map(|m| m * 60_000))
    .bind(n.rating)
    .bind(locked(lf, "official_rating"))
    .bind(&n.mpaa)
    .bind(&n.premiered)
    .bind(&n.status)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM match_candidates WHERE item_id = ?").bind(item_id).execute(&mut *tx).await?;

    if !locked(lf, "genres") && !n.genres.is_empty() {
        sqlx::query("DELETE FROM item_genres WHERE item_id = ?").bind(item_id).execute(&mut *tx).await?;
        for g in &n.genres {
            sqlx::query("INSERT OR IGNORE INTO genres (name) VALUES (?)").bind(g).execute(&mut *tx).await?;
            sqlx::query("INSERT OR IGNORE INTO item_genres (item_id, genre_id) SELECT ?, id FROM genres WHERE name = ?")
                .bind(item_id)
                .bind(g)
                .execute(&mut *tx)
                .await?;
        }
    }
    if !n.studios.is_empty() {
        sqlx::query("DELETE FROM item_studios WHERE item_id = ?").bind(item_id).execute(&mut *tx).await?;
        for st in &n.studios {
            sqlx::query("INSERT OR IGNORE INTO studios (name) VALUES (?)").bind(st).execute(&mut *tx).await?;
            sqlx::query("INSERT OR IGNORE INTO item_studios (item_id, studio_id) SELECT ?, id FROM studios WHERE name = ?")
                .bind(item_id)
                .bind(st)
                .execute(&mut *tx)
                .await?;
        }
    }
    if !n.actors.is_empty() || !n.directors.is_empty() {
        sqlx::query("DELETE FROM item_people WHERE item_id = ?").bind(item_id).execute(&mut *tx).await?;
        let mut order = 0i64;
        for a in n.actors.iter().take(20) {
            add_person(&mut tx, item_id, a, "actor", order).await?;
            order += 1;
        }
        for d in &n.directors {
            add_person(&mut tx, item_id, d, "director", order).await?;
            order += 1;
        }
        for w in n.writers.iter().take(6) {
            add_person(&mut tx, item_id, w, "writer", order).await?;
            order += 1;
        }
    }
    if let Some(set) = &n.set_name {
        super::collections::link(&mut tx, library_id, item_id, set, None).await?;
    }
    let mut new_images = Vec::new();
    for (kind, url, size) in [("poster", &n.poster, "w780"), ("backdrop", &n.backdrop, "w1280"), ("logo", &n.logo, "w500")] {
        if locked(lf, kind) {
            continue;
        }
        if let Some(u) = url {
            if let Some(id) = set_image_url(&mut tx, item_id, kind, &image_url(u, size)).await? {
                new_images.push(id);
            }
        }
    }
    tx.commit().await?;
    for id in new_images {
        crate::jobs::enqueue(state, crate::jobs::JobKind::DownloadImage { image_id: id }, crate::jobs::priority::IMAGES, Some(library_id)).await?;
    }
    state.events.publish("item.updated", serde_json::json!({ "item_id": state.ids.encode(item_id) }));
    Ok(())
}

/// Persona dall'NFO: per identificativo TMDB se c'è, altrimenti per nome (e foto).
async fn add_person(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, item_id: i64, p: &Person, role: &str, order: i64) -> anyhow::Result<()> {
    let profile = p.thumb.as_deref().and_then(tmdb_path);
    let pid: i64 = if let Some(t) = p.tmdb_id {
        sqlx::query(
            "INSERT INTO people (name, tmdb_id, profile_path) VALUES (?, ?, ?)
             ON CONFLICT(tmdb_id) DO UPDATE SET name = excluded.name, profile_path = COALESCE(excluded.profile_path, people.profile_path)",
        )
        .bind(&p.name)
        .bind(t)
        .bind(&profile)
        .execute(&mut **tx)
        .await?;
        sqlx::query_scalar("SELECT id FROM people WHERE tmdb_id = ?").bind(t).fetch_one(&mut **tx).await?
    } else {
        let found: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM people WHERE name = ? AND (profile_path IS ? OR profile_path IS NULL OR ? IS NULL) ORDER BY tmdb_id IS NULL LIMIT 1",
        )
        .bind(&p.name)
        .bind(&profile)
        .bind(&profile)
        .fetch_optional(&mut **tx)
        .await?;
        match found {
            Some(id) => id,
            None => {
                sqlx::query("INSERT INTO people (name, profile_path) VALUES (?, ?)").bind(&p.name).bind(&profile).execute(&mut **tx).await?.last_insert_rowid()
            }
        }
    };
    sqlx::query("INSERT OR IGNORE INTO item_people (item_id, person_id, role, character, sort_order) VALUES (?, ?, ?, ?, ?)")
        .bind(item_id)
        .bind(pid)
        .bind(role)
        .bind(&p.role)
        .bind(order)
        .execute(&mut **tx)
        .await?;
    if let Some(thumb) = &p.thumb {
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT id FROM images WHERE person_id = ? AND kind = 'profile'").bind(pid).fetch_optional(&mut **tx).await?;
        if exists.is_none() {
            sqlx::query("INSERT INTO images (person_id, kind, source, source_url, state) VALUES (?, 'profile', 'nfo', ?, 'pending')")
                .bind(pid)
                .bind(image_url(thumb, "w185"))
                .execute(&mut **tx)
                .await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {

    #[test]
    fn episode_nfo_blocks() {
        let raw = "<?xml version=\"1.0\"?>\n<episodedetails><title>Il pilota</title><season>1</season><episode>1</episode><plot>Inizio.</plot><aired>2020-01-05</aired></episodedetails>\n<episodedetails><title>Seconda parte</title><season>1</season><episode>2</episode></episodedetails>\nhttps://example.org";
        let v = parse_episodes(raw);
        assert_eq!(v.len(), 2);
        assert_eq!((v[0].title.as_deref(), v[0].season, v[0].episode), (Some("Il pilota"), Some(1), Some(1)));
        assert_eq!(v[0].premiered.as_deref(), Some("2020-01-05"));
        assert_eq!((v[1].title.as_deref(), v[1].episode), (Some("Seconda parte"), Some(2)));
        assert!(parse_episodes("<movie><title>X</title></movie>").is_empty());
    }

    use super::*;

    const KODI: &str = r#"<?xml version="1.0" encoding="utf-8" standalone="yes"?>
<movie>
  <title>Agente 007 - Missione Goldfinger</title>
  <originaltitle>Goldfinger</originaltitle>
  <ratings>
    <rating name="imdb" max="10" default="true"><value>7.7</value><votes>208180</votes></rating>
    <rating name="themoviedb" max="10"><value>7.348</value></rating>
  </ratings>
  <rating>7.348</rating>
  <plot>Sospettato di contrabbandare oro &amp; altro.</plot>
  <tagline />
  <runtime>110</runtime>
  <thumb aspect="poster" preview="https://image.tmdb.org/t/p/original/yNW.jpg">https://image.tmdb.org/t/p/original/yNW.jpg</thumb>
  <fanart><thumb>https://image.tmdb.org/t/p/original/lh6.jpg</thumb></fanart>
  <mpaa>Rated T</mpaa>
  <id>658</id>
  <uniqueid type="tmdb" default="true">658</uniqueid>
  <uniqueid type="imdb">tt0058150</uniqueid>
  <genre>Adventure</genre>
  <genre>Action / Thriller</genre>
  <set><name>James Bond Collection</name><overview /></set>
  <credits>Richard Maibaum</credits>
  <director>Guy Hamilton</director>
  <premiered>1964-09-20</premiered>
  <year>1964</year>
  <studio>EON Productions</studio>
  <actor><name>Gert Fröbe</name><role>Auric Goldfinger</role><order>1</order></actor>
  <actor><name>Sean Connery</name><role>James Bond</role><order>0</order><thumb>https://image.tmdb.org/t/p/original/hbB.jpg</thumb></actor>
</movie>
https://www.imdb.com/title/tt0058150/"#;

    #[test]
    fn parses_kodi_movie() {
        let n = parse(KODI).unwrap();
        assert_eq!(n.kind, "movie");
        assert_eq!(n.title.as_deref(), Some("Agente 007 - Missione Goldfinger"));
        assert_eq!(n.original_title.as_deref(), Some("Goldfinger"));
        assert_eq!(n.tmdb_id, Some(658));
        assert_eq!(n.imdb_id.as_deref(), Some("tt0058150"));
        assert_eq!(n.year, Some(1964));
        assert_eq!(n.runtime_min, Some(110));
        assert_eq!(n.rating, Some(7.7));
        assert_eq!(n.mpaa.as_deref(), Some("T"));
        assert_eq!(n.plot.as_deref(), Some("Sospettato di contrabbandare oro & altro."));
        assert_eq!(n.tagline, None);
        assert_eq!(n.genres, vec!["Adventure", "Action", "Thriller"]);
        assert_eq!(n.actors[0].name, "Sean Connery");
        assert_eq!(n.actors[0].role.as_deref(), Some("James Bond"));
        assert_eq!(n.directors[0].name, "Guy Hamilton");
        assert_eq!(n.writers[0].name, "Richard Maibaum");
        assert_eq!(n.poster.as_deref(), Some("https://image.tmdb.org/t/p/original/yNW.jpg"));
        assert_eq!(n.backdrop.as_deref(), Some("https://image.tmdb.org/t/p/original/lh6.jpg"));
        assert_eq!(n.set_name.as_deref(), Some("James Bond Collection"));
        assert!(n.complete());
    }

    #[test]
    fn url_only_nfo() {
        let n = parse("https://www.themoviedb.org/movie/603-the-matrix\n").unwrap();
        assert_eq!(n.tmdb_id, Some(603));
        let n = parse("http://www.imdb.com/title/tt0133093/").unwrap();
        assert_eq!(n.imdb_id.as_deref(), Some("tt0133093"));
        assert!(!n.complete());
        assert!(parse("nient'altro").is_none());
        assert!(parse("<movie><title>Rotto</movie>").is_none());
    }

    #[test]
    fn tvshow_ids() {
        let n = parse("<tvshow><title>Dark</title><id>334824</id><uniqueid type=\"tmdb\">70523</uniqueid><mpaa>IT:VM14 / US:TV-MA</mpaa></tvshow>").unwrap();
        assert_eq!(n.kind, "tvshow");
        assert_eq!(n.tmdb_id, Some(70523));
        assert_eq!(n.tvdb_id.as_deref(), Some("334824"));
        assert_eq!(n.mpaa.as_deref(), Some("VM14"));
    }

    #[test]
    fn tmdb_image_sizes() {
        assert!(image_url("https://image.tmdb.org/t/p/original/abc.jpg", "w780").ends_with("/w780/abc.jpg"));
        assert_eq!(image_url("https://example.org/p.jpg", "w780"), "https://example.org/p.jpg");
        assert_eq!(tmdb_path("https://image.tmdb.org/t/p/original/abc.jpg").as_deref(), Some("/abc.jpg"));
    }
}
