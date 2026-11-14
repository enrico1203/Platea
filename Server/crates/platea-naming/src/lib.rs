//! Lettura dei nomi di file e cartelle.
//!
//! Convenzioni supportate (le stesse di Jellyfin ed Emby, così chi arriva da lì non rinomina nulla):
//! - Film: `Titolo (Anno)/Titolo (Anno).mkv`, nomi "da scena" con punti ed etichette di qualità,
//!   più versioni nella stessa cartella (`Titolo (Anno) - 4K.mkv`).
//! - Serie: `Serie/Season 01/Serie S01E02.mkv`, `1x02`, episodi multipli `S01E01-E02`,
//!   episodi per data, numerazione assoluta per gli anime.
//! - Identificativi espliciti: `[tmdbid-603]`, `[imdbid-tt0133093]`, `[tvdbid-12345]`, anche `{tmdb-603}`.
//!
//! Nessuna funzione qui fa I/O: lavorano solo su stringhe.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;

pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "m4v", "avi", "mov", "wmv", "ts", "m2ts", "mts", "webm", "mpg", "mpeg", "flv",
    "ogv", "3gp", "vob", "iso", "divx", "asf",
];

pub const SUBTITLE_EXTENSIONS: &[&str] = &["srt", "vtt", "ass", "ssa", "sub", "sup"];

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ExternalIds {
    pub tmdb: Option<u64>,
    pub imdb: Option<String>,
    pub tvdb: Option<u64>,
}

impl ExternalIds {
    pub fn is_empty(&self) -> bool {
        self.tmdb.is_none() && self.imdb.is_none() && self.tvdb.is_none()
    }
    fn merge(&mut self, other: ExternalIds) {
        if self.tmdb.is_none() {
            self.tmdb = other.tmdb;
        }
        if self.imdb.is_none() {
            self.imdb = other.imdb;
        }
        if self.tvdb.is_none() {
            self.tvdb = other.tvdb;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MovieName {
    pub title: String,
    pub year: Option<u16>,
    pub ids: ExternalIds,
    /// Etichetta della versione quando più file della stessa cartella sono lo stesso film.
    pub version: Option<String>,
    /// Chiave che raggruppa le versioni dello stesso film (cartella o titolo+anno).
    pub group_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EpisodeName {
    pub series_title: String,
    pub series_year: Option<u16>,
    pub series_ids: ExternalIds,
    /// `None` se la stagione non è deducibile (per esempio numerazione assoluta).
    pub season: Option<u32>,
    /// Uno o più episodi (episodi multipli in un file).
    pub episodes: Vec<u32>,
    /// Episodio per data, formato `AAAA-MM-GG`.
    pub air_date: Option<String>,
    /// Numerazione assoluta (anime).
    pub absolute: Option<u32>,
    /// Chiave che identifica la serie: primo componente del percorso o titolo normalizzato.
    pub series_key: String,
}

static RE_TMDB: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)[\[{(]tmdb(?:id)?[-=: ]?(\d+)[\]})]").unwrap());
static RE_IMDB: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)[\[{(]imdb(?:id)?[-=: ]?(tt\d+)[\]})]").unwrap());
static RE_IMDB_BARE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b(tt\d{7,9})\b").unwrap());
static RE_TVDB: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)[\[{(]tvdb(?:id)?[-=: ]?(\d+)[\]})]").unwrap());
static RE_ANY_ID_TAG: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)[\[{(](?:tmdb|imdb|tvdb)(?:id)?[-=: ]?[a-z0-9]+[\]})]").unwrap()
});
static RE_YEAR_PAREN: Lazy<Regex> = Lazy::new(|| Regex::new(r"[(\[]((?:19|20)\d\d)[)\]]").unwrap());
static RE_YEAR: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b((?:19|20)\d\d)\b").unwrap());
static RE_BRACKETS: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[[^\]]*\]|\{[^}]*\}").unwrap());
static RE_SPACES: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").unwrap());

/// Etichette che segnano la fine del titolo in un nome "da scena".
static RE_QUALITY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?ix)\b(
            2160p|1080p|1080i|720p|576p|480p|4k|uhd|8k|
            blu-?ray|bdrip|brrip|bdremux|remux|web-?dl|web-?rip|webrip|web|hdtv|hdrip|dvdrip|dvdscr|dvd|
            x264|x265|h\.?264|h\.?265|hevc|avc|xvid|divx|av1|vp9|
            hdr10\+?|hdr|dv|dovi|dolby\.?vision|sdr|10bit|8bit|
            aac|ac3|eac3|dts(?:-?hd)?|truehd|atmos|flac|ddp?5\.1|dd\+?|5\.1|7\.1|
            proper|repack|internal|limited|extended|unrated|uncut|remastered|imax|
            director'?s\.?cut|theatrical|criterion|
            multi|dual|ita|eng|sub|subs|subita|forced|
            complete
        )\b",
    )
    .unwrap()
});

static RE_SXXEXX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\bs(\d{1,4})[ ._-]?e(\d{1,4})((?:(?:[ ._-]?-?[ ._-]?e|-)\d{1,4})*)").unwrap()
});
static RE_EXTRA_EP: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)(?:e|-)(\d{1,4})").unwrap());
static RE_NXNN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\b(\d{1,2})x(\d{2,3})((?:-(?:\d{1,2}x)?\d{2,3})*)\b").unwrap());
static RE_DATE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b((?:19|20)\d\d)[.\-_ ](\d\d)[.\-_ ](\d\d)\b").unwrap());
static RE_SEASON_DIR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(?:season|stagione|staffel|saison|temporada|series|s)[ ._-]*(\d{1,4})\b").unwrap()
});
static RE_SPECIALS_DIR: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^(?:specials?|speciali|extras? speciali)$").unwrap());
static RE_ABSOLUTE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?:^|[ ._])(?:-|ep|episode|episodio|#)?[ ._]?(\d{2,4})(?:v\d)?(?:[ ._]|$)").unwrap());
static RE_EPISODE_WORD: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\b(?:episode|episodio|ep|e)[ ._-]?(\d{1,4})\b").unwrap());

const EXTRA_DIRS: &[&str] = &[
    "extras", "extra", "featurettes", "behind the scenes", "deleted scenes", "interviews",
    "scenes", "shorts", "trailers", "trailer", "samples", "sample", "clips", "other", "contenuti speciali",
];
const EXTRA_SUFFIXES: &[&str] = &[
    "-trailer", "-sample", "-featurette", "-behindthescenes", "-deleted", "-interview", "-scene",
    "-short", "-clip", "-other", ".sample", "-extra",
];

/// Estensione in minuscolo, senza punto.
pub fn extension(name: &str) -> Option<String> {
    let file = name.rsplit('/').next().unwrap_or(name);
    let (stem, ext) = file.rsplit_once('.')?;
    if stem.is_empty() {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

pub fn is_video_file(name: &str) -> bool {
    extension(name).is_some_and(|e| VIDEO_EXTENSIONS.contains(&e.as_str()))
}

pub fn is_subtitle_file(name: &str) -> bool {
    extension(name).is_some_and(|e| SUBTITLE_EXTENSIONS.contains(&e.as_str()))
}

/// File e cartelle nascosti o di sistema da ignorare durante l'elenco.
pub fn is_ignored_entry(name: &str) -> bool {
    name.starts_with('.')
        || name.starts_with("@eaDir")
        || name.eq_ignore_ascii_case("#recycle")
        || name.eq_ignore_ascii_case("$RECYCLE.BIN")
        || name.eq_ignore_ascii_case("System Volume Information")
        || name.eq_ignore_ascii_case("lost+found")
        || name.ends_with(".part")
        || name.ends_with(".!qB")
}

/// `true` se il file è un contenuto extra (trailer, featurette...) da tenere fuori dall'elenco principale.
pub fn is_extra(rel_path: &str) -> bool {
    let parts: Vec<&str> = rel_path.split('/').collect();
    if parts.len() > 1 {
        for dir in &parts[..parts.len() - 1] {
            let d = dir.to_ascii_lowercase();
            if EXTRA_DIRS.contains(&d.as_str()) {
                return true;
            }
        }
    }
    let stem = file_stem(parts.last().copied().unwrap_or("")).to_ascii_lowercase();
    EXTRA_SUFFIXES.iter().any(|s| stem.ends_with(s)) || stem == "sample" || stem == "trailer"
}

fn file_stem(file: &str) -> &str {
    match file.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && ext.len() <= 5 && !ext.contains(' ') => stem,
        _ => file,
    }
}

/// Estrae gli identificativi espliciti da una stringa.
pub fn extract_ids(s: &str) -> ExternalIds {
    let mut ids = ExternalIds::default();
    if let Some(c) = RE_TMDB.captures(s) {
        ids.tmdb = c[1].parse().ok();
    }
    if let Some(c) = RE_IMDB.captures(s) {
        ids.imdb = Some(c[1].to_ascii_lowercase());
    } else if let Some(c) = RE_IMDB_BARE.captures(s) {
        ids.imdb = Some(c[1].to_string());
    }
    if let Some(c) = RE_TVDB.captures(s) {
        ids.tvdb = c[1].parse().ok();
    }
    ids
}

/// Pulisce un nome: toglie identificativi, parentesi quadre, punti e trattini bassi da scena.
fn base_clean(s: &str) -> String {
    let s = RE_ANY_ID_TAG.replace_all(s, " ");
    let s = RE_BRACKETS.replace_all(&s, " ");
    let mut out = s.to_string();
    // Punti e trattini bassi diventano spazi quando fanno da separatori (nomi da scena).
    if out.contains('.') || out.contains('_') {
        let dots = out.matches('.').count() + out.matches('_').count();
        let spaces = out.matches(' ').count();
        if dots >= spaces || !out.contains(' ') {
            out = replace_scene_separators(&out);
        }
    }
    RE_SPACES.replace_all(out.trim(), " ").to_string()
}

/// Sostituisce `.` e `_` con spazi, ma conserva i punti di sigle come "S.H.I.E.L.D." e "H.264".
fn replace_scene_separators(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    for (i, &c) in chars.iter().enumerate() {
        if c == '_' {
            out.push(' ');
        } else if c == '.' {
            let prev = if i > 0 { chars[i - 1] } else { ' ' };
            let next = chars.get(i + 1).copied().unwrap_or(' ');
            let prev2 = if i > 1 { chars[i - 2] } else { ' ' };
            // Sigla: lettera singola seguita da punto (A.B.C.)
            let acronym = prev.is_alphabetic()
                && (prev2 == '.' || prev2 == ' ' || i == 1)
                && (next.is_alphabetic() && chars.get(i + 2).is_none_or(|c| *c == '.'))
                && prev.is_uppercase();
            // Numero decimale (5.1)
            let decimal = prev.is_ascii_digit() && next.is_ascii_digit();
            if acronym || decimal {
                out.push('.');
            } else {
                out.push(' ');
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Taglia il titolo alla prima etichetta di qualità.
fn cut_at_quality(s: &str) -> &str {
    match RE_QUALITY.find(s) {
        Some(m) if m.start() > 0 => &s[..m.start()],
        _ => s,
    }
}

fn tidy_title(s: &str) -> String {
    let s = s.trim_matches(|c: char| c.is_whitespace() || c == '-' || c == '(' || c == '[' || c == '.' || c == ',');
    RE_SPACES.replace_all(s, " ").trim().to_string()
}

/// Separa titolo e anno da una stringa già pulita.
fn split_title_year(clean: &str) -> (String, Option<u16>) {
    // Anno tra parentesi: il caso più affidabile.
    if let Some(c) = RE_YEAR_PAREN.captures_iter(clean).last() {
        let m = c.get(0).unwrap();
        let year: u16 = c[1].parse().unwrap();
        let title = tidy_title(cut_at_quality(&clean[..m.start()]));
        if !title.is_empty() {
            return (title, Some(year));
        }
    }
    // Anno nudo: l'ultimo che non sia all'inizio del nome ("2001 Odissea nello spazio 1968").
    let years: Vec<_> = RE_YEAR.captures_iter(clean).collect();
    for c in years.iter().rev() {
        let m = c.get(1).unwrap();
        if m.start() == 0 {
            continue;
        }
        let title = tidy_title(cut_at_quality(&clean[..m.start()]));
        if !title.is_empty() {
            return (title, c[1].parse().ok());
        }
    }
    (tidy_title(cut_at_quality(clean)), None)
}

/// Titolo e anno da un nome qualunque (cartella o file senza estensione).
pub fn parse_title_year(name: &str) -> (String, Option<u16>) {
    split_title_year(&base_clean(name))
}

/// Lettura del nome di un film dal percorso relativo alla radice della libreria.
pub fn parse_movie(rel_path: &str) -> MovieName {
    let parts: Vec<&str> = rel_path.split('/').filter(|p| !p.is_empty()).collect();
    let file = parts.last().copied().unwrap_or("");
    let stem = file_stem(file);
    let folder = if parts.len() >= 2 { Some(parts[parts.len() - 2]) } else { None };

    let mut ids = extract_ids(stem);
    let (file_title, file_year) = parse_title_year(stem);

    // La cartella vince quando sembra la cartella del film: contiene un anno o un identificativo.
    let mut use_folder = false;
    let mut folder_title = String::new();
    let mut folder_year = None;
    if let Some(dir) = folder {
        let dir_ids = extract_ids(dir);
        let (t, y) = parse_title_year(dir);
        let looks_like_movie_dir = (y.is_some() || !dir_ids.is_empty()) && !t.is_empty();
        if looks_like_movie_dir {
            use_folder = true;
            folder_title = t;
            folder_year = y;
            ids.merge(dir_ids);
        }
    }

    let (title, year) = if use_folder {
        (folder_title, folder_year.or(file_year))
    } else {
        (file_title, file_year)
    };

    let version = detect_version(stem, folder.filter(|_| use_folder));
    let group_key = if use_folder {
        parts[..parts.len() - 1].join("/").to_lowercase()
    } else {
        format!(
            "{}/{}|{}",
            parts[..parts.len().saturating_sub(1)].join("/").to_lowercase(),
            normalize_title(&title),
            year.map(|y| y.to_string()).unwrap_or_default()
        )
    };

    MovieName { title, year, ids, version, group_key }
}

/// Etichetta di versione: testo dopo " - " che segue il nome della cartella, oppure la risoluzione.
fn detect_version(stem: &str, folder: Option<&str>) -> Option<String> {
    if let Some(dir) = folder {
        let dir_clean = RE_ANY_ID_TAG.replace_all(dir, "");
        let dir_l = dir_clean.trim().to_lowercase();
        let stem_l = stem.to_lowercase();
        if !dir_l.is_empty() && stem_l.starts_with(&dir_l) && stem.is_char_boundary(dir_l.len()) {
            let rest = stem[dir_l.len()..].trim();
            if let Some(label) = rest.strip_prefix('-') {
                let label = label.trim().trim_matches(|c| c == '[' || c == ']').trim();
                if !label.is_empty() {
                    return Some(label.to_string());
                }
            }
            if let Some(label) = rest.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
                if !label.trim().is_empty() {
                    return Some(label.trim().to_string());
                }
            }
        }
    }
    static RE_RES: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\b(2160p|1080p|720p|576p|480p|4k)\b").unwrap());
    RE_RES.captures(stem).map(|c| c[1].to_string())
}

/// Lettura del nome di un episodio dal percorso relativo alla radice della libreria.
/// Restituisce `None` se il file non sembra un episodio.
pub fn parse_episode(rel_path: &str) -> Option<EpisodeName> {
    let parts: Vec<&str> = rel_path.split('/').filter(|p| !p.is_empty()).collect();
    let file = *parts.last()?;
    let stem = file_stem(file);
    let dirs = &parts[..parts.len() - 1];

    // Stagione dalla cartella più vicina che la indica.
    let mut season_from_dir: Option<u32> = None;
    let mut season_dir_index: Option<usize> = None;
    for (i, d) in dirs.iter().enumerate().rev() {
        let clean = base_clean(d);
        if RE_SPECIALS_DIR.is_match(clean.trim()) {
            season_from_dir = Some(0);
            season_dir_index = Some(i);
            break;
        }
        if let Some(c) = RE_SEASON_DIR.captures(clean.trim()) {
            season_from_dir = c[1].parse().ok();
            season_dir_index = Some(i);
            break;
        }
    }

    // Serie: la cartella sopra quella della stagione, oppure il primo componente.
    let series_dir: Option<&str> = match season_dir_index {
        Some(i) if i > 0 => Some(dirs[i - 1]),
        Some(_) => None,
        None => dirs.first().copied(),
    };

    let mut season = None;
    let mut episodes = Vec::new();
    let mut air_date = None;
    let mut absolute = None;
    let mut name_prefix_end = None;

    if let Some(c) = RE_SXXEXX.captures(stem) {
        season = c[1].parse().ok();
        let first: u32 = c[2].parse().ok()?;
        episodes.push(first);
        if let Some(extra) = c.get(3) {
            for e in RE_EXTRA_EP.captures_iter(extra.as_str()) {
                if let Ok(n) = e[1].parse::<u32>() {
                    expand_range(&mut episodes, n);
                }
            }
        }
        name_prefix_end = Some(c.get(0).unwrap().start());
    } else if let Some(c) = RE_NXNN.captures(stem) {
        season = c[1].parse().ok();
        let first: u32 = c[2].parse().ok()?;
        episodes.push(first);
        if let Some(extra) = c.get(3) {
            for piece in extra.as_str().split('-').filter(|p| !p.is_empty()) {
                let num = piece.rsplit('x').next().unwrap_or(piece);
                if let Ok(n) = num.parse::<u32>() {
                    expand_range(&mut episodes, n);
                }
            }
        }
        name_prefix_end = Some(c.get(0).unwrap().start());
    } else if let Some(c) = RE_DATE.captures(stem) {
        air_date = Some(format!("{}-{}-{}", &c[1], &c[2], &c[3]));
        name_prefix_end = Some(c.get(0).unwrap().start());
    } else if season_from_dir.is_some() || series_dir.is_some() {
        // "Episodio 5", "Ep05", oppure numerazione assoluta degli anime "[Gruppo] Titolo - 012".
        let cleaned = RE_BRACKETS.replace_all(stem, " ").to_string();
        if let Some(c) = RE_EPISODE_WORD.captures(&cleaned) {
            let n: u32 = c[1].parse().ok()?;
            if season_from_dir.is_some() {
                episodes.push(n);
            } else {
                absolute = Some(n);
                episodes.push(n);
            }
        } else {
            let without_quality = strip_quality_numbers(&cleaned);
            let caps: Vec<_> = RE_ABSOLUTE.captures_iter(&without_quality).collect();
            let c = caps.last()?;
            let n: u32 = c[1].parse().ok()?;
            if (1900..=2099).contains(&n) && c[1].len() == 4 {
                return None;
            }
            if season_from_dir.is_some() {
                episodes.push(n);
            } else {
                absolute = Some(n);
                episodes.push(n);
            }
        }
    } else {
        return None;
    }

    if season.is_none() {
        season = season_from_dir;
    }

    let (series_title, series_year, series_ids, series_key) = match series_dir {
        Some(dir) => {
            let (t, y) = parse_title_year(dir);
            (t, y, extract_ids(dir), dir.to_lowercase())
        }
        None => {
            let prefix = &stem[..name_prefix_end.unwrap_or(0)];
            let (t, y) = parse_title_year(prefix);
            let key = normalize_title(&t);
            (t, y, extract_ids(stem), key)
        }
    };
    if series_title.is_empty() {
        return None;
    }

    Some(EpisodeName {
        series_title,
        series_year,
        series_ids,
        season,
        episodes,
        air_date,
        absolute,
        series_key,
    })
}

fn expand_range(episodes: &mut Vec<u32>, n: u32) {
    let last = *episodes.last().unwrap_or(&n);
    if n > last && n - last <= 50 {
        for e in last + 1..=n {
            episodes.push(e);
        }
    } else if !episodes.contains(&n) {
        episodes.push(n);
    }
}

fn strip_quality_numbers(s: &str) -> String {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)\b(?:2160p|1080p|1080i|720p|576p|480p|x26[45]|h\.?26[45]|10bit|8bit|5\.1|7\.1|(?:19|20)\d\d)\b").unwrap()
    });
    RE.replace_all(s, " ").to_string()
}

/// Titolo normalizzato per i confronti: minuscolo, senza accenti né punteggiatura.
pub fn normalize_title(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = true;
    for c in s.chars() {
        let c = fold_char(c);
        if c.is_alphanumeric() {
            for l in c.to_lowercase() {
                out.push(l);
            }
            last_space = false;
        } else if c == '&' {
            if !last_space {
                out.push(' ');
            }
            out.push_str("and ");
            last_space = true;
        } else if c == '\'' || c == '’' {
            // "Schindler's" → "schindlers"
        } else if !last_space {
            out.push(' ');
            last_space = true;
        }
    }
    out.trim().to_string()
}

fn fold_char(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => 'a',
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Ā' => 'a',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'È' | 'É' | 'Ê' | 'Ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => 'i',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'Ù' | 'Ú' | 'Û' | 'Ü' => 'u',
        'ñ' | 'Ñ' => 'n',
        'ç' | 'Ç' => 'c',
        'ý' | 'ÿ' | 'Ý' => 'y',
        'ß' => 's',
        _ => c,
    }
}

/// Titolo per l'ordinamento: senza articolo iniziale, normalizzato.
pub fn sort_title(title: &str) -> String {
    const ARTICLES: &[&str] = &[
        "the ", "a ", "an ", "il ", "lo ", "la ", "i ", "gli ", "le ", "un ", "uno ", "una ",
        "l'", "un'", "der ", "die ", "das ", "el ", "los ", "las ", "les ",
    ];
    let lower = title.to_lowercase();
    let mut rest = lower.as_str();
    for a in ARTICLES {
        if let Some(r) = rest.strip_prefix(a) {
            if !r.trim().is_empty() {
                rest = r;
                break;
            }
        }
    }
    normalize_title(rest)
}

/// Somiglianza tra due titoli normalizzati, tra 0 e 1 (Dice sui bigrammi di caratteri).
pub fn title_similarity(a: &str, b: &str) -> f64 {
    let a = normalize_title(a).replace(' ', "");
    let b = normalize_title(b).replace(' ', "");
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    if a.chars().count() < 2 || b.chars().count() < 2 {
        return 0.0;
    }
    let bigrams = |s: &str| -> Vec<(char, char)> {
        let c: Vec<char> = s.chars().collect();
        c.windows(2).map(|w| (w[0], w[1])).collect()
    };
    let ba = bigrams(&a);
    let mut bb = bigrams(&b);
    let total = ba.len() + bb.len();
    let mut hits = 0;
    for x in &ba {
        if let Some(pos) = bb.iter().position(|y| y == x) {
            bb.swap_remove(pos);
            hits += 1;
        }
    }
    (2 * hits) as f64 / total as f64
}

/// Lingua dal nome di un file di sottotitoli esterno: `Film.it.forced.srt` → ("it", forced).
pub fn subtitle_language(file: &str) -> (Option<String>, bool, bool) {
    let stem = file_stem(file.rsplit('/').next().unwrap_or(file));
    let mut lang = None;
    let mut forced = false;
    let mut sdh = false;
    for part in stem.rsplit('.').take(3) {
        let p = part.to_ascii_lowercase();
        match p.as_str() {
            "forced" | "forzati" => forced = true,
            "sdh" | "cc" | "hi" => sdh = true,
            _ if (p.len() == 2 || p.len() == 3) && p.chars().all(|c| c.is_ascii_alphabetic()) && lang.is_none() => {
                lang = Some(p)
            }
            _ => {}
        }
    }
    (lang, forced, sdh)
}

#[cfg(test)]
mod tests;
