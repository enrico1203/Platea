//! Metadati degli audiolibri. Fonti, dalla più precisa (come Jellyfin con i plugin Audible/Audnexus e Audiobookshelf):
//! 1. `metadata.json` (scheda Audible salvata dai programmi di download, o il formato di Audiobookshelf), `metadata.opf`;
//! 2. NFO (`album.nfo`, `book.nfo`, `<libro>.nfo`: scritti anche da Jellyfin), `desc.txt` e `reader.txt`;
//! 3. tag del file (album, artista = autore, compositore = lettore, genere, descrizione, serie, ASIN);
//! 4. nome della cartella o del file (`Autore - Titolo 2021 Letto da Lettore`);
//! 5. il catalogo pubblico di Audible: per ASIN se noto, altrimenti per titolo e autore, con un punteggio che tiene conto
//!    anche della durata. Sopra la soglia l'associazione è automatica; sotto, con candidati plausibili, "da verificare".
//!
//! Copertina: un'immagine nella cartella (`cover.jpg`, `folder.jpg`, `<libro>.jpg`…), poi quella di Audible, poi quella
//! incorporata nel file. Le cartelle dei media si leggono e basta: nulla viene scritto lì.

use super::{collections, locked, set_image_url};
use crate::jobs::{self, priority, JobKind};
use crate::state::Shared;
use crate::util::now_ms;
use platea_naming::audiobook as ab;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Gruppi di risposta del catalogo Audible che servono per la scheda.
const RESPONSE_GROUPS: &str = "contributors,product_desc,product_extended_attrs,product_attrs,media,rating,series,category_ladders";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BookMeta {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub authors: Vec<String>,
    pub narrators: Vec<String>,
    pub year: Option<i64>,
    pub date: Option<String>,
    pub overview: Option<String>,
    pub genres: Vec<String>,
    pub publisher: Option<String>,
    pub series: Option<String>,
    pub series_index: Option<String>,
    pub asin: Option<String>,
    pub isbn: Option<String>,
    /// Voto su 10.
    pub rating: Option<f64>,
    pub language: Option<String>,
    /// Durata dichiarata dalla scheda (minuti), per il confronto con i file.
    pub runtime_min: Option<i64>,
    pub cover_url: Option<String>,
}

impl BookMeta {
    /// Riempie i campi vuoti con quelli di `o` (fonte meno precisa).
    pub fn fill(&mut self, o: BookMeta) {
        macro_rules! opt {
            ($($f:ident),*) => { $( if self.$f.is_none() { self.$f = o.$f; } )* };
        }
        macro_rules! list {
            ($($f:ident),*) => { $( if self.$f.is_empty() { self.$f = o.$f; } )* };
        }
        opt!(title, subtitle, year, date, overview, publisher, asin, isbn, rating, language, runtime_min, cover_url);
        if self.series.is_none() {
            self.series = o.series;
            self.series_index = o.series_index;
        }
        list!(authors, narrators, genres);
    }
}

fn clean(s: Option<&str>) -> Option<String> {
    s.map(|x| x.trim().to_string()).filter(|x| !x.is_empty())
}

/// Elenco di nomi da un campo di testo: "A, B & C", "A; B", "A / B".
pub fn split_names(s: &str) -> Vec<String> {
    let s = s.replace(" & ", ",").replace(" e ", ",").replace(" and ", ",").replace(';', ",").replace(" / ", ",");
    let mut v: Vec<String> = s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty() && x.len() < 120).collect();
    v.dedup();
    v
}

fn split_list(s: &str) -> Vec<String> {
    let mut v: Vec<String> = s.split([',', ';', '/', '|']).map(|x| x.trim().to_string()).filter(|x| !x.is_empty() && x.len() < 80).collect();
    v.dedup();
    v
}

/// Testo da HTML semplice (le descrizioni di Audible): paragrafi e a capo diventano righe, il resto si toglie.
pub fn html_to_text(s: &str) -> String {
    let s = s.replace("<br />", "\n").replace("<br/>", "\n").replace("<br>", "\n").replace("</p>", "\n\n").replace("</li>", "\n");
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ");
    let lines: Vec<&str> = out.lines().map(str::trim).collect();
    let mut joined = lines.join("\n");
    while joined.contains("\n\n\n") {
        joined = joined.replace("\n\n\n", "\n\n");
    }
    joined.trim().to_string()
}

fn year_from(s: &str) -> Option<i64> {
    let y: String = s.chars().take(4).collect();
    y.parse().ok().filter(|y| (1000..3000).contains(y))
}

/// Scheda di un prodotto Audible (risposta dell'API o `metadata.json` salvato accanto al libro).
/// Audible mette tra gli autori anche traduttori, curatori e prefatori ("Nome - traduttore"): non sono autori.
fn is_contributor_role(name: &str) -> bool {
    let Some((_, role)) = name.rsplit_once(" - ") else { return false };
    let r = role.trim().to_lowercase();
    ["tradutt", "translat", "übersetz", "tradu", "prefaz", "introdu", "forew", "editor", "curat", "a cura", "illustr", "postfaz", "adapt", "contribut"]
        .iter()
        .any(|k| r.starts_with(k))
}

pub fn from_audible(p: &Value) -> BookMeta {
    let names = |k: &str| -> Vec<String> {
        p[k].as_array().map(|a| a.iter().filter_map(|x| clean(x["name"].as_str())).filter(|n| !is_contributor_role(n)).collect()).unwrap_or_default()
    };
    let mut genres: Vec<String> = Vec::new();
    for l in p["category_ladders"].as_array().into_iter().flatten() {
        if l["root"].as_str().is_some_and(|r| r != "Genres") {
            continue;
        }
        for g in l["ladder"].as_array().into_iter().flatten().take(2) {
            if let Some(n) = clean(g["name"].as_str()) {
                if !genres.contains(&n) {
                    genres.push(n);
                }
            }
        }
    }
    let series = p["series"].as_array().and_then(|a| a.first());
    let images = &p["product_images"];
    let cover = ["2400", "1024", "500"].iter().find_map(|k| clean(images[*k].as_str()));
    let rating = p["rating"]["overall_distribution"]["display_average_rating"]
        .as_str()
        .and_then(|r| r.parse::<f64>().ok())
        .or_else(|| p["rating"]["overall_distribution"]["average_rating"].as_f64())
        .filter(|r| *r > 0.0)
        .map(|r| (r * 2.0 * 10.0).round() / 10.0);
    let summary = clean(p["publisher_summary"].as_str()).or_else(|| clean(p["merchandising_summary"].as_str())).map(|s| html_to_text(&s));
    let date = clean(p["release_date"].as_str()).or_else(|| clean(p["publication_datetime"].as_str()).map(|d| d.chars().take(10).collect()));
    BookMeta {
        title: clean(p["title"].as_str()),
        subtitle: clean(p["subtitle"].as_str()),
        authors: names("authors"),
        narrators: names("narrators"),
        year: date.as_deref().and_then(year_from),
        date,
        overview: summary.filter(|s| !s.is_empty()),
        genres,
        publisher: clean(p["publisher_name"].as_str()),
        series: series.and_then(|s| clean(s["title"].as_str())),
        series_index: series.and_then(|s| clean(s["sequence"].as_str())),
        asin: clean(p["asin"].as_str()),
        isbn: clean(p["isbn"].as_str()),
        rating,
        language: clean(p["language"].as_str()),
        runtime_min: p["runtime_length_min"].as_i64(),
        cover_url: cover,
    }
}

/// `metadata.json` di Audiobookshelf (autori e lettori come testo, serie "Nome #1").
pub fn from_abs_json(p: &Value) -> BookMeta {
    let strs = |k: &str| -> Vec<String> {
        p[k].as_array().map(|a| a.iter().filter_map(|x| clean(x.as_str()).or_else(|| clean(x["name"].as_str()))).collect()).unwrap_or_default()
    };
    let series_raw =
        p["series"].as_array().and_then(|a| a.first()).and_then(|s| s.as_str().map(str::to_string).or_else(|| s["name"].as_str().map(str::to_string)));
    let (series, series_index) =
        match series_raw.as_deref().map(|s| s.rsplit_once(" #").map(|(a, b)| (a.to_string(), Some(b.to_string()))).unwrap_or((s.to_string(), None))) {
            Some((s, i)) => (Some(s), i),
            None => (None, None),
        };
    BookMeta {
        title: clean(p["title"].as_str()),
        subtitle: clean(p["subtitle"].as_str()),
        authors: strs("authors"),
        narrators: strs("narrators"),
        year: p["publishedYear"].as_str().and_then(year_from).or_else(|| p["publishedYear"].as_i64()),
        date: clean(p["publishedDate"].as_str()),
        overview: clean(p["description"].as_str()).map(|s| html_to_text(&s)),
        genres: strs("genres"),
        publisher: clean(p["publisher"].as_str()),
        series,
        series_index,
        asin: clean(p["asin"].as_str()),
        isbn: clean(p["isbn"].as_str()),
        language: clean(p["language"].as_str()),
        ..Default::default()
    }
}

/// Un `metadata.json`: scheda Audible (autori come oggetti) o Audiobookshelf (autori come testo).
pub fn from_json(raw: &str) -> Option<BookMeta> {
    let v: Value = serde_json::from_str(raw.trim_start_matches('\u{feff}')).ok()?;
    let v = if v["product"].is_object() { v["product"].clone() } else { v };
    let audible = v["authors"].as_array().and_then(|a| a.first()).is_some_and(|a| a.is_object()) || v["product_images"].is_object();
    let m = if audible { from_audible(&v) } else { from_abs_json(&v) };
    m.title.is_some().then_some(m)
}

/// NFO di album o libro (Jellyfin scrive `album.nfo` con `<album>`; altri `<audiobook>` o `<book>`).
pub fn from_nfo(raw: &str) -> Option<BookMeta> {
    let raw = raw.trim_start_matches('\u{feff}');
    let doc = roxmltree::Document::parse(raw).ok()?;
    let root = doc.root_element();
    let all = |name: &str| -> Vec<String> { root.children().filter(|n| n.has_tag_name(name)).filter_map(|n| clean(n.text())).collect::<Vec<_>>() };
    let one = |names: &[&str]| -> Option<String> { names.iter().find_map(|n| all(n).into_iter().next()) };
    let mut authors: Vec<String> = Vec::new();
    for a in all("author").into_iter().chain(all("albumartist")).chain(all("artist")).chain(all("writer")) {
        for n in split_names(&a) {
            if !authors.contains(&n) {
                authors.push(n);
            }
        }
    }
    let narrators: Vec<String> = all("narrator").iter().chain(all("reader").iter()).flat_map(|n| split_names(n)).collect();
    let mut genres: Vec<String> = Vec::new();
    for g in all("genre") {
        for x in split_list(&g) {
            if !genres.contains(&x) && !x.eq_ignore_ascii_case("audiobook") && !x.eq_ignore_ascii_case("audiolibro") {
                genres.push(x);
            }
        }
    }
    let mut asin = one(&["asin", "audibleid"]);
    let mut isbn = one(&["isbn"]);
    for u in root.children().filter(|n| n.has_tag_name("uniqueid")) {
        match u.attribute("type").map(|t| t.to_lowercase()).as_deref() {
            Some("asin" | "audible") if asin.is_none() => asin = clean(u.text()),
            Some("isbn") if isbn.is_none() => isbn = clean(u.text()),
            _ => {}
        }
    }
    let date = one(&["releasedate", "premiered", "aired"]);
    let m = BookMeta {
        title: one(&["title"]),
        subtitle: one(&["subtitle", "tagline"]),
        authors,
        narrators,
        year: one(&["year"]).and_then(|y| y.parse().ok()).or_else(|| date.as_deref().and_then(year_from)),
        date,
        overview: one(&["review", "plot", "outline", "description", "summary"]).map(|s| html_to_text(&s)),
        genres,
        publisher: one(&["publisher", "studio", "label"]),
        series: one(&["series", "set"]),
        series_index: one(&["seriesindex", "series_index", "sequence"]),
        asin,
        isbn,
        rating: one(&["rating"]).and_then(|r| r.parse().ok()).filter(|r: &f64| *r > 0.0),
        language: one(&["language"]),
        ..Default::default()
    };
    (m.title.is_some() || !m.authors.is_empty()).then_some(m)
}

/// `metadata.opf` (Calibre, Audiobookshelf): Dublin Core con i ruoli `aut` e `nrt`.
pub fn from_opf(raw: &str) -> Option<BookMeta> {
    let doc = roxmltree::Document::parse(raw.trim_start_matches('\u{feff}')).ok()?;
    let mut m = BookMeta::default();
    for n in doc.descendants().filter(|n| n.is_element()) {
        let name = n.tag_name().name();
        let text = clean(n.text());
        let role = n.attributes().find(|a| a.name() == "role").map(|a| a.value().to_string());
        match name {
            "title" if m.title.is_none() => m.title = text,
            "creator" => {
                if let Some(t) = text {
                    if role.as_deref() == Some("nrt") {
                        m.narrators.push(t);
                    } else {
                        m.authors.push(t);
                    }
                }
            }
            "contributor" if role.as_deref() == Some("nrt") => m.narrators.extend(text),
            "description" if m.overview.is_none() => m.overview = text.map(|s| html_to_text(&s)),
            "publisher" if m.publisher.is_none() => m.publisher = text,
            "date" if m.date.is_none() => {
                m.year = text.as_deref().and_then(year_from);
                m.date = text;
            }
            "subject" => m.genres.extend(text),
            "language" if m.language.is_none() => m.language = text,
            "identifier" => {
                let scheme = n.attributes().find(|a| a.name() == "scheme").map(|a| a.value().to_lowercase());
                match scheme.as_deref() {
                    Some("isbn") => m.isbn = text,
                    Some("asin" | "audible") => m.asin = text,
                    _ => {}
                }
            }
            "meta" => match (n.attribute("name"), n.attribute("content")) {
                (Some("calibre:series"), Some(c)) => m.series = clean(Some(c)),
                (Some("calibre:series_index"), Some(c)) => m.series_index = clean(Some(c)),
                _ => {}
            },
            _ => {}
        }
    }
    m.title.is_some().then_some(m)
}

/// Tag del file (chiavi in minuscolo, come le salva l'analisi).
pub fn from_tags(t: &HashMap<String, String>) -> BookMeta {
    let get = |keys: &[&str]| keys.iter().find_map(|k| clean(t.get(*k).map(String::as_str)));
    let authors = get(&["album_artist", "albumartist", "artist", "author", "writer"]).map(|a| split_names(&a)).unwrap_or_default();
    let narrators = get(&["narrator", "narratedby", "composer", "performer"]).map(|a| split_names(&a)).unwrap_or_default();
    let date = get(&["date", "year", "originaldate", "release_date", "tdor"]);
    // I commenti brevi ("Chapter 38") non sono descrizioni.
    let overview = get(&["description", "synopsis", "desc", "summary", "long_description"])
        .or_else(|| get(&["comment"]).filter(|c| c.chars().count() > 60))
        .map(|s| html_to_text(&s));
    let genres = get(&["genre"])
        .map(|g| {
            split_list(&g)
                .into_iter()
                .filter(|x| !x.eq_ignore_ascii_case("audiobook") && !x.eq_ignore_ascii_case("audiolibro") && !x.eq_ignore_ascii_case("speech"))
                .collect()
        })
        .unwrap_or_default();
    BookMeta {
        title: get(&["album", "title"]),
        subtitle: get(&["subtitle"]),
        authors,
        narrators,
        year: date.as_deref().and_then(year_from),
        date: date.filter(|d| d.len() >= 10).map(|d| d.chars().take(10).collect()),
        overview,
        genres,
        publisher: get(&["publisher", "label", "copyright"]).filter(|p| !p.starts_with('©') && !p.starts_with("(c)") && !p.starts_with("(C)")),
        series: get(&["series", "mvnm", "grouping"]),
        series_index: get(&["series-part", "series_part", "mvin", "seriespart"]),
        asin: get(&["asin", "audible_asin", "cdek"]).filter(|a| a.len() == 10),
        isbn: get(&["isbn"]),
        language: get(&["language"]),
        ..Default::default()
    }
}

/// Dal nome della cartella o del file.
pub fn from_name(name: &str) -> BookMeta {
    let n = ab::parse_book_name(name);
    BookMeta {
        title: Some(n.title),
        authors: n.author.map(|a| split_names(&a)).unwrap_or_default(),
        narrators: n.narrator.map(|a| split_names(&a)).unwrap_or_default(),
        year: n.year.map(i64::from),
        date: n.date,
        series: n.series,
        series_index: n.series_index,
        ..Default::default()
    }
}

/// Punteggio di un prodotto Audible per il libro: titolo, autore, lettore e durata (0–1).
pub fn score(q: &BookMeta, runtime_ms: Option<i64>, c: &BookMeta) -> (f64, f64) {
    let ct = c.title.as_deref().unwrap_or("");
    let full = match &c.subtitle {
        Some(s) => format!("{ct} {s}"),
        None => ct.to_string(),
    };
    let qt = q.title.as_deref().unwrap_or("");
    let title = platea_naming::title_similarity(qt, ct).max(platea_naming::title_similarity(qt, &full));
    let best = |a: &[String], b: &[String]| -> Option<f64> {
        if a.is_empty() || b.is_empty() {
            return None;
        }
        Some(a.iter().flat_map(|x| b.iter().map(move |y| platea_naming::title_similarity(x, y))).fold(0.0, f64::max))
    };
    let mut parts: Vec<(f64, f64)> = vec![(title, 0.55)];
    if let Some(a) = best(&q.authors, &c.authors) {
        parts.push((a, 0.2));
    }
    if let Some(n) = best(&q.narrators, &c.narrators) {
        parts.push((n, 0.1));
    }
    if let (Some(ms), Some(min)) = (runtime_ms.filter(|m| *m > 60_000), c.runtime_min.filter(|m| *m > 0)) {
        let a = ms as f64 / 60_000.0;
        let b = min as f64;
        let diff = (a - b).abs() / a.max(b);
        parts.push(((1.0 - diff * 5.0).clamp(0.0, 1.0), 0.15));
    }
    let w: f64 = parts.iter().map(|p| p.1).sum();
    (parts.iter().map(|p| p.0 * p.1).sum::<f64>() / w, title)
}

/// Dominio del catalogo Audible per il paese della libreria (o la regione scelta nelle impostazioni).
pub fn audible_host(region: &str, country: &str) -> &'static str {
    let r = if region.trim().is_empty() || region.eq_ignore_ascii_case("auto") { country } else { region };
    match r.trim().to_ascii_lowercase().as_str() {
        "it" => "api.audible.it",
        "uk" | "gb" => "api.audible.co.uk",
        "de" | "at" | "ch" => "api.audible.de",
        "fr" | "be" => "api.audible.fr",
        "es" => "api.audible.es",
        "ca" => "api.audible.ca",
        "au" | "nz" => "api.audible.com.au",
        "jp" => "api.audible.co.jp",
        "in" => "api.audible.in",
        "br" => "api.audible.com.br",
        _ => "api.audible.com",
    }
}

/// Lingua del catalogo Audible ("italian") per il codice della libreria ("it-IT").
fn audible_language(lang: &str) -> &'static str {
    match lang.split('-').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "it" => "italian",
        "en" => "english",
        "de" => "german",
        "fr" => "french",
        "es" => "spanish",
        "pt" => "portuguese",
        "ja" => "japanese",
        _ => "",
    }
}

/// Le richieste ad Audible sono distanziate (al massimo 4 al secondo per tutto il server).
async fn audible_get(state: &Shared, url: &str) -> anyhow::Result<Value> {
    static NEXT: once_cell::sync::Lazy<tokio::sync::Mutex<tokio::time::Instant>> =
        once_cell::sync::Lazy::new(|| tokio::sync::Mutex::new(tokio::time::Instant::now()));
    {
        let mut next = NEXT.lock().await;
        let now = tokio::time::Instant::now();
        if *next > now {
            tokio::time::sleep_until(*next).await;
        }
        *next = tokio::time::Instant::now() + Duration::from_millis(250);
    }
    let r = state.http.get(url).timeout(Duration::from_secs(20)).send().await?;
    if r.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Value::Null);
    }
    if !r.status().is_success() {
        anyhow::bail!("Audible: {}", r.status());
    }
    Ok(r.json().await?)
}

struct Lib {
    host: &'static str,
    language: &'static str,
}

async fn lib_settings(state: &Shared, library_id: i64) -> Lib {
    let (language, country) = super::library_language(state, library_id).await;
    let region = state.settings.get().metadata.audible_region.clone();
    Lib { host: audible_host(&region, &country), language: audible_language(&language) }
}

pub async fn audible_product(state: &Shared, library_id: i64, asin: &str) -> anyhow::Result<Option<BookMeta>> {
    let lib = lib_settings(state, library_id).await;
    let asin: String = asin.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let url = format!("https://{}/1.0/catalog/products/{asin}?response_groups={RESPONSE_GROUPS}&image_sizes=500,1024", lib.host);
    let v = audible_get(state, &url).await?;
    Ok(v.get("product").map(from_audible).filter(|m| m.title.is_some()))
}

/// Ricerca nel catalogo: titolo e autore, poi solo il titolo se non esce nulla. Candidati in ordine di punteggio.
pub async fn audible_search(state: &Shared, library_id: i64, q: &BookMeta, runtime_ms: Option<i64>) -> anyhow::Result<Vec<(BookMeta, f64, f64)>> {
    let lib = lib_settings(state, library_id).await;
    let Some(title) = q.title.as_deref().filter(|t| !t.trim().is_empty()) else { return Ok(vec![]) };
    let author = q.authors.first().filter(|a| !a.contains("AA.VV") && !a.eq_ignore_ascii_case("autori vari") && !a.eq_ignore_ascii_case("various"));
    let mut tries: Vec<Vec<(&str, String)>> = Vec::new();
    if let Some(a) = author {
        tries.push(vec![("title", title.to_string()), ("author", a.clone())]);
    }
    tries.push(vec![("title", title.to_string())]);
    for params in tries {
        let mut url = format!(
            "https://{}/1.0/catalog/products?num_results=10&products_sort_by=Relevance&response_groups={RESPONSE_GROUPS}&image_sizes=500,1024",
            lib.host
        );
        for (k, v) in &params {
            url.push_str(&format!("&{k}={}", urlencoding::encode(v)));
        }
        let v = audible_get(state, &url).await?;
        let list: Vec<BookMeta> = v["products"].as_array().map(|a| a.iter().map(from_audible).filter(|m| m.asin.is_some()).collect()).unwrap_or_default();
        if list.is_empty() {
            continue;
        }
        let mut scored: Vec<(BookMeta, f64, f64)> = list
            .into_iter()
            .map(|c| {
                let (mut s, t) = score(q, runtime_ms, &c);
                // Lingua diversa da quella della libreria: meno probabile.
                if !lib.language.is_empty() && c.language.as_deref().is_some_and(|l| !l.eq_ignore_ascii_case(lib.language)) {
                    s *= 0.85;
                }
                (c, s, t)
            })
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        return Ok(scored);
    }
    Ok(vec![])
}

/// Cosa si trova accanto al libro.
#[derive(Default)]
struct Found {
    dir: PathBuf,
    meta: BookMeta,
    cover: Option<PathBuf>,
    /// La cartella contiene anche altri libri: i file generici (`album.nfo`, `cover.jpg`) sono della raccolta.
    shared: bool,
    dir_name: String,
}

fn is_image(name: &str) -> bool {
    matches!(platea_naming::extension(name).as_deref(), Some("jpg" | "jpeg" | "png" | "webp"))
}

fn file_stem(name: &str) -> &str {
    name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name)
}

/// Elenco della cartella con un tempo limite (mount lenti): solo i nomi.
async fn list_names(dir: PathBuf, timeout: Duration) -> Vec<String> {
    let f = tokio::task::spawn_blocking(move || -> Vec<String> {
        let Ok(rd) = std::fs::read_dir(&dir) else { return vec![] };
        rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect()
    });
    match tokio::time::timeout(timeout, f).await {
        Ok(Ok(v)) => v,
        _ => vec![],
    }
}

async fn read_text(path: PathBuf, timeout: Duration) -> Option<String> {
    super::nfo::read_small(path, timeout).await
}

/// Raccoglie i metadati locali: file della cartella, tag del primo file, nome.
async fn local(state: &Shared, item_id: i64, key: &str, parts: &[crate::audiobook::Part]) -> anyhow::Result<Found> {
    let first = parts.iter().find(|p| p.present).or(parts.first()).ok_or_else(|| anyhow::anyhow!("nessun file"))?;
    let _ = crate::probe::ensure_probed(state, first.file_id).await;
    let Some((abs, rel, ..)) = crate::probe::file_abs_path(state, first.file_id).await? else { anyhow::bail!("file non trovato") };
    let bk = ab::book_key(&rel).ok_or_else(|| anyhow::anyhow!("non è un file audio"))?;
    // Cartella del libro (sopra le cartelle dei dischi), dalla radice della libreria.
    let root = abs.ancestors().nth(rel.matches('/').count() + 1).map(Path::to_path_buf).unwrap_or_default();
    let dir = if bk.dir.is_empty() { root } else { root.join(&bk.dir) };
    let timeout = super::nfo::fs_timeout(state);
    let names = list_names(dir.clone(), timeout).await;
    let shared = names.iter().filter(|n| ab::is_audio_file(n)).any(|n| {
        let r = if bk.dir.is_empty() { n.clone() } else { format!("{}/{n}", bk.dir) };
        ab::book_key(&r).is_some_and(|k| k.key != key)
    });
    let stem = file_stem(rel.rsplit('/').next().unwrap_or(&rel)).to_string();
    let base = ab::strip_copy_suffix(&stem).0.to_string();
    let own = |n: &str| {
        let s = file_stem(n);
        s == stem || s == base || s.strip_suffix(".metadata").is_some_and(|x| x == stem || x == base)
    };
    let lower = |n: &str| n.to_lowercase();
    let mut meta = BookMeta::default();

    // Libro a più file: se gli album dei file sono diversi (una raccolta di puntate), il titolo dell'album o dell'NFO
    // (Jellyfin vi scrive quello di una puntata) e il commento sono della singola puntata, non del libro.
    let name_title = ab::parse_book_name(&bk.name).title;
    let albums: Vec<String> = if parts.len() > 1 {
        sqlx::query_scalar::<_, Option<String>>("SELECT json_extract(tags, '$.album') FROM media_files WHERE item_id = ? AND tags IS NOT NULL")
            .bind(item_id)
            .fetch_all(&state.db.read)
            .await?
            .into_iter()
            .flatten()
            .collect()
    } else {
        vec![]
    };
    let book_title_ok = |t: &Option<String>| -> bool {
        if parts.len() <= 1 {
            return true;
        }
        let Some(t) = t else { return true };
        let same_album = albums.len() >= 2 && albums.iter().all(|a| a == &albums[0]);
        same_album || platea_naming::title_similarity(t, &name_title) >= 0.6
    };
    let episodic = |m: &mut BookMeta| {
        if !book_title_ok(&m.title) {
            m.title = None;
            m.overview = None;
        }
    };

    // 1. metadata.json / metadata.opf
    let mut jsons: Vec<&String> = names.iter().filter(|n| lower(n).ends_with(".json") && own(n)).collect();
    if !shared {
        jsons.extend(names.iter().filter(|n| lower(n) == "metadata.json"));
    }
    for j in jsons {
        if let Some(m) = read_text(dir.join(j), timeout).await.as_deref().and_then(from_json) {
            meta.fill(m);
            break;
        }
    }
    let mut opfs: Vec<&String> = names.iter().filter(|n| lower(n).ends_with(".opf") && own(n)).collect();
    if !shared {
        opfs.extend(names.iter().filter(|n| lower(n).ends_with(".opf") && !own(n)));
    }
    if let Some(o) = opfs.first() {
        if let Some(m) = read_text(dir.join(o), timeout).await.as_deref().and_then(from_opf) {
            meta.fill(m);
        }
    }
    // 2. NFO, desc.txt, reader.txt
    let mut nfos: Vec<&String> = names.iter().filter(|n| lower(n).ends_with(".nfo") && own(n)).collect();
    if !shared {
        for generic in ["book.nfo", "audiobook.nfo", "album.nfo"] {
            nfos.extend(names.iter().filter(|n| lower(n) == generic));
        }
    }
    for n in nfos {
        if let Some(mut m) = read_text(dir.join(n), timeout).await.as_deref().and_then(from_nfo) {
            episodic(&mut m);
            meta.fill(m);
            break;
        }
    }
    if !shared {
        if let Some(d) = names.iter().find(|n| matches!(lower(n).as_str(), "desc.txt" | "description.txt")) {
            if let Some(t) = read_text(dir.join(d), timeout).await {
                meta.fill(BookMeta { overview: clean(Some(&t)), ..Default::default() });
            }
        }
        if let Some(d) = names.iter().find(|n| matches!(lower(n).as_str(), "reader.txt" | "narrator.txt")) {
            if let Some(t) = read_text(dir.join(d), timeout).await {
                meta.fill(BookMeta { narrators: split_names(t.lines().next().unwrap_or("")), ..Default::default() });
            }
        }
    }
    // 3. tag del primo file
    let tags: Option<String> =
        sqlx::query_scalar("SELECT tags FROM media_files WHERE id = ?").bind(first.file_id).fetch_optional(&state.db.read).await?.flatten();
    if let Some(t) = tags.as_deref().and_then(|t| serde_json::from_str::<HashMap<String, String>>(t).ok()) {
        let mut m = from_tags(&t);
        // In un libro a più file il titolo del file è quello del capitolo: vale solo l'album.
        if parts.len() > 1 && !t.contains_key("album") {
            m.title = None;
        }
        episodic(&mut m);
        meta.fill(m);
    }
    // 4. nome
    meta.fill(from_name(&bk.name));

    // Copertina: prima quella del libro, poi (se la cartella è solo sua) quelle generiche o la prima immagine.
    let mut images: Vec<&String> = names.iter().filter(|n| is_image(n)).collect();
    images.sort_by(|a, b| ab::natural_cmp(a, b));
    let cover = images
        .iter()
        .find(|n| own(n))
        .or_else(|| {
            if shared {
                return None;
            }
            ab::COVER_NAMES.iter().find_map(|c| images.iter().find(|n| lower(file_stem(n)) == *c)).or(images.first())
        })
        .map(|n| dir.join(n));
    let dir_name = if bk.dir.is_empty() { String::new() } else { bk.dir.rsplit('/').next().unwrap_or("").to_string() };
    let _ = item_id;
    Ok(Found { dir, meta, cover, shared, dir_name })
}

/// Associazione di un audiolibro: metadati locali, poi Audible. `force`: anche se già associato (aggiornamento).
pub async fn match_book(state: &Shared, item_id: i64, force: bool) -> anyhow::Result<()> {
    let row: Option<(i64, String, Option<String>, String, Option<i64>)> =
        sqlx::query_as("SELECT library_id, match_state, match_key, locked_fields, runtime_ms FROM items WHERE id = ? AND kind = 'audiobook'")
            .bind(item_id)
            .fetch_optional(&state.db.read)
            .await?;
    let Some((library_id, match_state, key, lf, runtime)) = row else { return Ok(()) };
    if !force && match_state != "pending" {
        return Ok(());
    }
    let parts = crate::audiobook::parts(state, item_id).await?;
    if parts.is_empty() {
        return Ok(());
    }
    let found = local(state, item_id, key.as_deref().unwrap_or(""), &parts).await?;
    let mut meta = found.meta.clone();
    let runtime = runtime.or_else(|| Some(parts.iter().map(|p| p.duration_ms).sum::<i64>()).filter(|r| *r > 0));
    let s = state.settings.get();
    let use_audible = s.metadata.audible;
    let threshold = s.metadata.auto_match_threshold;
    drop(s);

    // Associazione manuale già fatta: si riprende quella scheda.
    let manual_asin: Option<String> = if match_state == "manual" {
        sqlx::query_scalar("SELECT value FROM external_ids WHERE item_id = ? AND provider = 'audible'").bind(item_id).fetch_optional(&state.db.read).await?
    } else {
        None
    };
    let mut audible: Option<(BookMeta, f64)> = None;
    let mut candidates: Vec<(BookMeta, f64)> = Vec::new();
    if use_audible {
        let asin = manual_asin.clone().or_else(|| meta.asin.clone());
        let mut by_asin = None;
        if let Some(a) = &asin {
            match audible_product(state, library_id, a).await {
                Ok(p) => by_asin = p,
                Err(e) => tracing::debug!(item = item_id, error = %e, "scheda Audible non disponibile"),
            }
        }
        if let Some(p) = by_asin {
            audible = Some((p, 1.0));
        } else {
            match audible_search(state, library_id, &meta, runtime).await {
                Ok(list) => {
                    if let Some((best, sc, title_sim)) = list.first() {
                        // Sicuro: punteggio sopra la soglia, titolo quasi uguale e un distacco dal secondo.
                        let gap = list.get(1).map(|x| sc - x.1).unwrap_or(1.0);
                        if *sc >= threshold && *title_sim >= 0.8 && (gap >= 0.05 || *sc >= 0.92) {
                            audible = Some((best.clone(), *sc));
                        }
                    }
                    candidates = list.into_iter().filter(|c| c.1 >= 0.5).take(6).map(|(m, s, _)| (m, s)).collect();
                }
                Err(e) => tracing::info!(item = item_id, error = %e, "ricerca su Audible non riuscita: restano i metadati locali"),
            }
        }
    }
    let state_name;
    let score;
    match &audible {
        Some((p, sc)) => {
            // La scheda Audible vince; i dati locali riempiono ciò che manca (il lettore dal nome, la serie della cartella).
            let mut m = p.clone();
            m.fill(meta.clone());
            meta = m;
            state_name = if manual_asin.is_some() { "manual" } else { "matched" };
            score = Some(*sc);
        }
        None if !candidates.is_empty() => {
            state_name = "review";
            score = candidates.first().map(|c| c.1);
        }
        None => {
            state_name = "matched";
            score = None;
        }
    }
    // Più libri nella stessa cartella e nessuna serie dichiarata: la cartella è la raccolta.
    if meta.series.is_none() && found.shared && !found.dir_name.is_empty() {
        let n = ab::parse_book_name(&found.dir_name);
        meta.series = n.series.or(Some(n.title)).filter(|s| !s.is_empty());
    }
    apply(state, item_id, library_id, &lf, &meta, state_name, score).await?;
    if state_name == "review" {
        let mut tx = state.db.write.begin().await?;
        sqlx::query("DELETE FROM match_candidates WHERE item_id = ?").bind(item_id).execute(&mut *tx).await?;
        for (c, sc) in &candidates {
            let overview = [c.authors.join(", "), c.narrators.first().map(|n| format!("Letto da {n}")).unwrap_or_default()]
                .into_iter()
                .filter(|x| !x.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            sqlx::query(
                "INSERT OR REPLACE INTO match_candidates (item_id, provider_id, title, year, score, overview, poster_path) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(item_id)
            .bind(c.asin.as_deref().unwrap_or(""))
            .bind(c.title.as_deref().unwrap_or(""))
            .bind(c.year)
            .bind(sc)
            .bind(overview)
            .bind(c.cover_url.as_deref().map(|u| u.replace("._SL1024_", "._SL300_").replace("._SL500_", "._SL300_")))
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        state.events.admin("metadata.review", serde_json::json!({ "item_id": state.ids.encode(item_id) }));
    }
    cover(state, item_id, library_id, &lf, &found, audible.as_ref().and_then(|a| a.0.cover_url.clone()), &parts).await?;
    if match_state == "pending" {
        crate::webhooks::item_added(state, item_id).await;
    }
    state.events.publish("item.updated", serde_json::json!({ "item_id": state.ids.encode(item_id) }));
    Ok(())
}

/// Identificazione manuale con un ASIN scelto dall'amministratore.
pub async fn identify(state: &Shared, item_id: i64, asin: &str) -> anyhow::Result<()> {
    let library_id: i64 = sqlx::query_scalar("SELECT library_id FROM items WHERE id = ?").bind(item_id).fetch_one(&state.db.read).await?;
    if audible_product(state, library_id, asin).await?.is_none() {
        anyhow::bail!("ASIN {asin} non trovato su Audible");
    }
    sqlx::query("INSERT OR REPLACE INTO external_ids (item_id, provider, value) VALUES (?, 'audible', ?)")
        .bind(item_id)
        .bind(asin)
        .execute(&state.db.write)
        .await?;
    sqlx::query("UPDATE items SET match_state = 'manual' WHERE id = ?").bind(item_id).execute(&state.db.write).await?;
    match_book(state, item_id, true).await
}

/// Candidati di Audible per la finestra "Identifica": ricerca libera o con i dati del libro.
pub async fn candidates(state: &Shared, item_id: i64, query: Option<&str>) -> anyhow::Result<Vec<Value>> {
    let (library_id, title, runtime): (i64, String, Option<i64>) =
        sqlx::query_as("SELECT library_id, title, runtime_ms FROM items WHERE id = ?").bind(item_id).fetch_one(&state.db.read).await?;
    let authors: Vec<String> =
        sqlx::query_scalar("SELECT p.name FROM item_people ip JOIN people p ON p.id = ip.person_id WHERE ip.item_id = ? AND ip.role = 'author'")
            .bind(item_id)
            .fetch_all(&state.db.read)
            .await?;
    let q = match query.map(str::trim).filter(|q| !q.is_empty()) {
        // Un ASIN scritto a mano.
        Some(a) if a.len() == 10 && a.chars().all(|c| c.is_ascii_alphanumeric()) && a.chars().any(|c| c.is_ascii_digit()) => {
            return Ok(audible_product(state, library_id, a).await?.into_iter().map(|m| candidate_json(&m, 1.0)).collect());
        }
        Some(t) => BookMeta { title: Some(t.to_string()), ..Default::default() },
        None => BookMeta { title: Some(title), authors, ..Default::default() },
    };
    let list = audible_search(state, library_id, &q, runtime).await?;
    Ok(list.into_iter().take(12).map(|(m, s, _)| candidate_json(&m, s)).collect())
}

fn candidate_json(m: &BookMeta, score: f64) -> Value {
    let overview = [m.authors.join(", "), m.narrators.first().map(|n| format!("Letto da {n}")).unwrap_or_default()]
        .into_iter()
        .filter(|x| !x.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    serde_json::json!({
        "provider_id": m.asin, "title": m.title, "year": m.year, "score": score, "overview": overview,
        "poster": m.cover_url.as_deref().map(|u| u.replace("._SL1024_", "._SL300_").replace("._SL500_", "._SL300_")),
    })
}

/// Scrive la scheda: testi, persone, generi, editore, serie, identificativi. Rispetta i campi bloccati.
async fn apply(state: &Shared, item_id: i64, library_id: i64, lf: &str, m: &BookMeta, match_state: &str, score: Option<f64>) -> anyhow::Result<()> {
    let now = now_ms();
    let mut tx = state.db.write.begin().await?;
    let title = m.title.clone().unwrap_or_default();
    let series_index: Option<i64> = m.series_index.as_deref().and_then(|s| s.split(['.', ',']).next()).and_then(|s| s.trim().parse().ok());
    sqlx::query(
        "UPDATE items SET
           title = CASE WHEN ?2 OR ?3 = '' THEN title ELSE ?3 END,
           sort_title = CASE WHEN ?2 OR ?3 = '' THEN sort_title ELSE ?4 END,
           tagline = ?5,
           original_title = ?16,
           year = CASE WHEN ?6 THEN year ELSE COALESCE(?7, year) END,
           overview = CASE WHEN ?8 THEN overview ELSE ?9 END,
           premiere_date = ?10,
           community_rating = ?11,
           index_number = ?12,
           match_state = ?13,
           match_score = ?14,
           metadata_refreshed_at = ?15,
           updated_at = ?15
         WHERE id = ?1",
    )
    .bind(item_id)
    .bind(locked(lf, "title"))
    .bind(&title)
    .bind(platea_naming::sort_title(&title))
    .bind(&m.subtitle)
    .bind(locked(lf, "year"))
    .bind(m.year)
    .bind(locked(lf, "overview"))
    .bind(&m.overview)
    .bind(&m.date)
    .bind(m.rating)
    .bind(series_index)
    .bind(match_state)
    .bind(score)
    .bind(now)
    // Gli autori stanno nel titolo originale (inutile per un audiolibro): l'indice di ricerca li trova.
    .bind(Some(m.authors.iter().take(3).cloned().collect::<Vec<_>>().join(", ")).filter(|a| !a.is_empty()))
    .execute(&mut *tx)
    .await?;
    if match_state != "review" {
        sqlx::query("DELETE FROM match_candidates WHERE item_id = ?").bind(item_id).execute(&mut *tx).await?;
    }
    if !locked(lf, "genres") {
        sqlx::query("DELETE FROM item_genres WHERE item_id = ?").bind(item_id).execute(&mut *tx).await?;
        for g in m.genres.iter().take(8) {
            sqlx::query("INSERT OR IGNORE INTO genres (name) VALUES (?)").bind(g).execute(&mut *tx).await?;
            sqlx::query("INSERT OR IGNORE INTO item_genres (item_id, genre_id) SELECT ?, id FROM genres WHERE name = ?")
                .bind(item_id)
                .bind(g)
                .execute(&mut *tx)
                .await?;
        }
    }
    sqlx::query("DELETE FROM item_studios WHERE item_id = ?").bind(item_id).execute(&mut *tx).await?;
    if let Some(p) = &m.publisher {
        sqlx::query("INSERT OR IGNORE INTO studios (name) VALUES (?)").bind(p).execute(&mut *tx).await?;
        sqlx::query("INSERT OR IGNORE INTO item_studios (item_id, studio_id) SELECT ?, id FROM studios WHERE name = ?")
            .bind(item_id)
            .bind(p)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("DELETE FROM item_people WHERE item_id = ? AND role IN ('author', 'narrator')").bind(item_id).execute(&mut *tx).await?;
    let mut order = 0i64;
    for (role, list) in [("author", &m.authors), ("narrator", &m.narrators)] {
        for name in list.iter().take(8) {
            add_person(&mut tx, item_id, name, role, order).await?;
            order += 1;
        }
    }
    for (p, v) in [("audible", &m.asin), ("isbn", &m.isbn)] {
        if let Some(v) = v {
            sqlx::query("INSERT OR REPLACE INTO external_ids (item_id, provider, value) VALUES (?, ?, ?)")
                .bind(item_id)
                .bind(p)
                .bind(v)
                .execute(&mut *tx)
                .await?;
        }
    }
    match &m.series {
        Some(s) => {
            collections::link(&mut tx, library_id, item_id, s, None).await?;
        }
        None => {
            sqlx::query(
                "DELETE FROM collection_members WHERE item_id = ? AND collection_id NOT IN (SELECT id FROM items WHERE kind = 'collection' AND match_key LIKE 'manual:%')",
            )
            .bind(item_id)
            .execute(&mut *tx)
            .await?;
            collections::prune(&mut tx).await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

/// Autore o lettore per nome: la stessa persona in tutta la libreria (e, se c'è, quella di TMDB con la foto).
async fn add_person(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, item_id: i64, name: &str, role: &str, order: i64) -> anyhow::Result<()> {
    let found: Option<i64> =
        sqlx::query_scalar("SELECT id FROM people WHERE name = ? ORDER BY tmdb_id IS NULL, id LIMIT 1").bind(name).fetch_optional(&mut **tx).await?;
    let pid = match found {
        Some(id) => id,
        None => sqlx::query("INSERT INTO people (name) VALUES (?)").bind(name).execute(&mut **tx).await?.last_insert_rowid(),
    };
    sqlx::query("INSERT OR IGNORE INTO item_people (item_id, person_id, role, sort_order) VALUES (?, ?, ?, ?)")
        .bind(item_id)
        .bind(pid)
        .bind(role)
        .bind(order)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Copertina: immagine nella cartella, poi Audible, poi quella incorporata nel primo file.
async fn cover(
    state: &Shared,
    item_id: i64,
    library_id: i64,
    lf: &str,
    found: &Found,
    audible_url: Option<String>,
    parts: &[crate::audiobook::Part],
) -> anyhow::Result<()> {
    if locked(lf, "poster") {
        return Ok(());
    }
    let current: Option<(String, String)> =
        sqlx::query_as("SELECT source, source_url FROM images WHERE item_id = ? AND kind = 'poster'").bind(item_id).fetch_optional(&state.db.read).await?;
    if current.as_ref().is_some_and(|c| c.0 == "upload") {
        return Ok(());
    }
    let timeout = super::nfo::fs_timeout(state);
    // 1. Nella cartella.
    if let Some(p) = &found.cover {
        let meta = tokio::time::timeout(timeout, tokio::fs::metadata(p)).await.ok().and_then(Result::ok);
        if let Some(meta) = meta.filter(|m| m.len() > 0 && m.len() < 20 * 1024 * 1024) {
            let id = format!(
                "local:{}:{}:{}",
                p.display(),
                meta.len(),
                meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0)
            );
            if current.as_ref().is_some_and(|c| c.1 == id) {
                return Ok(());
            }
            if let Ok(Ok(bytes)) = tokio::time::timeout(Duration::from_secs(60), tokio::fs::read(p)).await {
                match super::images::store_bytes(state, item_id, "poster", bytes, "local", &id).await {
                    Ok(_) => return Ok(()),
                    Err(e) => tracing::debug!(file = %p.display(), error = %e, "copertina nella cartella non leggibile"),
                }
            }
        }
    }
    // 2. Audible.
    if let Some(u) = audible_url {
        let mut tx = state.db.write.begin().await?;
        let new = set_image_url(&mut tx, item_id, "poster", &u).await?;
        if let Some(id) = new {
            sqlx::query("UPDATE images SET source = 'audible' WHERE id = ?").bind(id).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        if let Some(id) = new {
            jobs::enqueue(state, JobKind::DownloadImage { image_id: id }, priority::IMAGES, Some(library_id)).await?;
        }
        return Ok(());
    }
    // 3. Incorporata nel file.
    let Some(first) = parts.iter().find(|p| p.present) else { return Ok(()) };
    let id = format!("embedded:{}:{}", first.file_id, first.duration_ms);
    if current.as_ref().is_some_and(|c| c.1 == id) {
        return Ok(());
    }
    if let Some(bytes) = embedded_cover(state, first.file_id).await {
        if let Err(e) = super::images::store_bytes(state, item_id, "poster", bytes, "embedded", &id).await {
            tracing::debug!(item = item_id, error = %e, "copertina incorporata non leggibile");
        }
    }
    let _ = &found.dir;
    Ok(())
}

/// Estrae la copertina incorporata (traccia "attached_pic") senza ricodificarla.
pub async fn embedded_cover(state: &Shared, file_id: i64) -> Option<Vec<u8>> {
    let idx: Option<i64> = sqlx::query_scalar(
        "SELECT stream_index FROM media_streams WHERE file_id = ? AND kind = 'attachment' AND codec IN ('mjpeg', 'png', 'jpeg', 'bmp', 'gif', 'webp') ORDER BY stream_index LIMIT 1",
    )
    .bind(file_id)
    .fetch_optional(&state.db.read)
    .await
    .ok()
    .flatten();
    let idx = idx?;
    let (abs, ..) = crate::probe::file_abs_path(state, file_id).await.ok()??;
    let ffmpeg = state.ffmpeg.get().ffmpeg.clone();
    let mut cmd = tokio::process::Command::new(ffmpeg);
    cmd.args(["-v", "error", "-nostdin", "-i"])
        .arg(&abs)
        .args(["-map", &format!("0:{idx}"), "-frames:v", "1", "-c", "copy", "-f", "image2", "pipe:1"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let child = cmd.spawn().ok()?;
    let out = tokio::time::timeout(Duration::from_secs(90), child.wait_with_output()).await.ok()?.ok()?;
    (out.status.success() && out.stdout.len() > 100).then_some(out.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jellyfin_album_nfo() {
        let raw = "\u{feff}<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"yes\"?>\n<album>\n  <review />\n  <title>Delitti di lago</title>\n  <year>2025</year>\n  <genre>Antologie e racconti brevi, Poliziesco</genre>\n  <art><poster>/config/x/folder.jpg</poster></art>\n  <artist>AA.VV.</artist>\n  <albumartist>AA.VV.</albumartist>\n  <track><title>Delitti di lago</title></track>\n</album>";
        let m = from_nfo(raw).unwrap();
        assert_eq!(m.title.as_deref(), Some("Delitti di lago"));
        assert_eq!(m.authors, vec!["AA.VV.".to_string()]);
        assert_eq!(m.year, Some(2025));
        assert_eq!(m.genres, vec!["Antologie e racconti brevi".to_string(), "Poliziesco".to_string()]);
        assert_eq!(m.overview, None);
    }

    #[test]
    fn audible_product_json() {
        let v = serde_json::json!({
            "asin": "B0H6QTVZJ9", "title": "Dignità", "subtitle": "Una storia", "authors": [{"name": "Lea Ypi"}], "narrators": [{"name": "Lea Ypi"}],
            "release_date": "2026-07-17", "runtime_length_min": 600, "language": "italian", "publisher_name": "Feltrinelli",
            "publisher_summary": "<p>Una foto &amp; una storia.</p><p>Seconda riga</p>",
            "series": [{"title": "Saga", "sequence": "2"}],
            "category_ladders": [{"root": "Genres", "ladder": [{"name": "Narrativa"}, {"name": "Storica"}, {"name": "XX secolo"}]}],
            "rating": {"overall_distribution": {"display_average_rating": "4.5"}},
            "product_images": {"500": "https://m/x._SL500_.jpg", "1024": "https://m/x._SL1024_.jpg"}
        });
        let m = from_json(&v.to_string()).unwrap();
        assert_eq!(m.asin.as_deref(), Some("B0H6QTVZJ9"));
        assert_eq!(m.authors, vec!["Lea Ypi".to_string()]);
        assert_eq!(m.overview.as_deref(), Some("Una foto & una storia.\n\nSeconda riga"));
        assert_eq!(m.genres, vec!["Narrativa".to_string(), "Storica".to_string()]);
        assert_eq!(m.rating, Some(9.0));
        assert_eq!(m.year, Some(2026));
        assert_eq!((m.series.as_deref(), m.series_index.as_deref()), (Some("Saga"), Some("2")));
        assert_eq!(m.cover_url.as_deref(), Some("https://m/x._SL1024_.jpg"));
    }

    #[test]
    fn audiobookshelf_json() {
        let raw =
            r#"{"title":"Artemis","authors":["Andy Weir"],"narrators":["Rosario Dawson"],"series":["Saga #3"],"publishedYear":"2017","description":"Moon"}"#;
        let m = from_json(raw).unwrap();
        assert_eq!(m.narrators, vec!["Rosario Dawson".to_string()]);
        assert_eq!((m.series.as_deref(), m.series_index.as_deref()), (Some("Saga"), Some("3")));
        assert_eq!(m.year, Some(2017));
    }

    #[test]
    fn tags_of_audible_rips() {
        let t: HashMap<String, String> = [
            ("title", "33 false verità sull'Europa"),
            ("artist", "Lorenzo Bini Smaghi"),
            ("album", "33 false verità sull'Europa"),
            ("date", "2021"),
            ("comment", "Chapter 38"),
            ("genre", "Audiobook"),
            ("composer", "Mario Rossi, Anna Bianchi"),
        ]
        .into_iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        let m = from_tags(&t);
        assert_eq!(m.authors, vec!["Lorenzo Bini Smaghi".to_string()]);
        assert_eq!(m.narrators, vec!["Mario Rossi".to_string(), "Anna Bianchi".to_string()]);
        assert_eq!(m.overview, None);
        assert!(m.genres.is_empty());
        assert_eq!(m.year, Some(2021));
    }

    #[test]
    fn opf() {
        let raw = r#"<?xml version="1.0"?><package xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf"><metadata>
            <dc:title>Il nome della rosa</dc:title><dc:creator opf:role="aut">Umberto Eco</dc:creator><dc:creator opf:role="nrt">Luca Ward</dc:creator>
            <dc:date>1980-01-01</dc:date><dc:identifier opf:scheme="ISBN">9788845</dc:identifier><meta name="calibre:series" content="Gialli"/></metadata></package>"#;
        let m = from_opf(raw).unwrap();
        assert_eq!(m.authors, vec!["Umberto Eco".to_string()]);
        assert_eq!(m.narrators, vec!["Luca Ward".to_string()]);
        assert_eq!(m.isbn.as_deref(), Some("9788845"));
        assert_eq!(m.series.as_deref(), Some("Gialli"));
        assert_eq!(m.year, Some(1980));
    }

    #[test]
    fn scoring_prefers_same_book_and_length() {
        let q = BookMeta { title: Some("Delitti di lago".into()), authors: vec!["AA.VV.".into()], ..Default::default() };
        let same = BookMeta { title: Some("Delitti di lago".into()), authors: vec!["AA.VV.".into()], runtime_min: Some(429), ..Default::default() };
        let other =
            BookMeta { title: Some("Delitto vista lago".into()), authors: vec!["Mariana D'Amico".into()], runtime_min: Some(267), ..Default::default() };
        let (a, ta) = score(&q, Some(429 * 60_000), &same);
        let (b, _) = score(&q, Some(429 * 60_000), &other);
        assert!(a > 0.95 && ta == 1.0, "{a}");
        assert!(b < 0.7, "{b}");
    }

    #[test]
    fn translators_are_not_authors() {
        let v = serde_json::json!({ "title": "Mr. X", "asin": "B0BJL9ZLHN", "authors": [{"name": "Peter Straub"}, {"name": "Tessa Bernardi - traduttore"}, {"name": "Jean-Paul Sartre - prefazione"}] });
        assert_eq!(from_audible(&v).authors, vec!["Peter Straub".to_string()]);
        assert!(!is_contributor_role("Anne-Marie Smith"));
    }

    #[test]
    fn hosts() {
        assert_eq!(audible_host("auto", "IT"), "api.audible.it");
        assert_eq!(audible_host("", "GB"), "api.audible.co.uk");
        assert_eq!(audible_host("de", "IT"), "api.audible.de");
        assert_eq!(audible_host("auto", "XX"), "api.audible.com");
    }

    #[test]
    fn names_split() {
        assert_eq!(split_names("A, B & C"), vec!["A", "B", "C"]);
        assert_eq!(split_names("Mario Rossi e Anna Bianchi"), vec!["Mario Rossi", "Anna Bianchi"]);
    }
}
