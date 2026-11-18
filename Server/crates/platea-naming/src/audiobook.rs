//! Audiolibri: quali file sono audio, quale libro forma un file, cosa dice il nome della cartella.
//!
//! Regole (le stesse di Audiobookshelf, compatibili con le cartelle preparate per Jellyfin ed Emby):
//! - una cartella con file audio è un libro, i file sono le sue parti (`01.mp3`, `02.mp3`…);
//! - le sottocartelle dei dischi (`CD1`, `Disc 2`, `Disco 3`) appartengono al libro della cartella sopra;
//! - un `.m4b` è un libro completo: più `.m4b` nella stessa cartella sono libri diversi (una serie o una raccolta),
//!   tranne le parti dichiarate (`Libro - Parte 1.m4b`) e le copie (`Libro (1).m4b`), che restano lo stesso libro;
//! - un file nella radice della libreria è un libro da solo.
//!
//! Nessuna funzione qui fa I/O.

use once_cell::sync::Lazy;
use regex::Regex;
use std::cmp::Ordering;

pub const AUDIO_EXTENSIONS: &[&str] = &["mp3", "m4b", "m4a", "aac", "flac", "ogg", "oga", "opus", "wma", "wav", "aif", "aiff", "mka", "ape", "wv", "mp2"];

pub fn is_audio_file(name: &str) -> bool {
    crate::extension(name).is_some_and(|e| AUDIO_EXTENSIONS.contains(&e.as_str()))
}

/// Immagini che fanno da copertina del libro, in ordine di preferenza (Jellyfin, Emby, Audiobookshelf, Plex).
pub const COVER_NAMES: &[&str] = &["cover", "folder", "poster", "front", "album", "thumb"];

/// Cartella di un disco (`CD1`, `Disc 02`, `Disco 3`): le sue tracce sono del libro della cartella sopra.
pub fn is_disc_dir(name: &str) -> bool {
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^(?:cd|disc|disk|disco|dvd)[\s._-]*\d{1,3}$").unwrap());
    RE.is_match(name.trim())
}

fn stem(file: &str) -> &str {
    match file.rsplit_once('.') {
        Some((s, _)) if !s.is_empty() => s,
        _ => file,
    }
}

/// Copie scaricate due volte: `Libro (1)`, `Libro - Copia`, `Libro copy`. Restituisce il nome senza il suffisso
/// e il suffisso, se c'era.
pub fn strip_copy_suffix(stem: &str) -> (&str, Option<&str>) {
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)(\s*\(\d{1,2}\)|\s+-\s+cop(?:ia|y)(?:\s*\(\d+\))?|\s+copy)$").unwrap());
    match RE.find(stem) {
        Some(m) if m.start() > 0 => (stem[..m.start()].trim_end(), Some(m.as_str().trim())),
        _ => (stem, None),
    }
}

/// Parte dichiarata di un libro diviso: `Libro - Parte 1`, `Libro Part 02`, `Libro (2 of 3)`, `Libro CD1`.
/// Restituisce il nome del libro senza il numero della parte.
pub fn strip_part_marker(stem: &str) -> &str {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)(?:[\s._-]*[\(\[]?\s*(?:parte|part|pt|cd|disc|disco|disk)[\s._-]*\d{1,3}(?:\s*(?:of|di|/)\s*\d{1,3})?\s*[\)\]]?|\s*[\(\[]\s*\d{1,3}\s*(?:of|di|/)\s*\d{1,3}\s*[\)\]])$")
            .unwrap()
    });
    match RE.find(stem) {
        Some(m) if m.start() > 0 => stem[..m.start()].trim_end_matches([' ', '-', '_', '.']),
        _ => stem,
    }
}

/// Il libro a cui appartiene un file della libreria.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookKey {
    /// Chiave stabile del libro (va in `items.match_key`).
    pub key: String,
    /// Cartella del libro, relativa alla libreria ("" = radice).
    pub dir: String,
    /// Nome da cui leggere titolo, autore e lettore: la cartella o il nome del `.m4b`.
    pub name: String,
    /// Etichetta della copia (`(1)`), se il file è un doppione di un'altra parte.
    pub copy: Option<String>,
}

/// Chiave del libro per un file audio (percorso relativo alla libreria). `None` se non è un file audio.
pub fn book_key(rel: &str) -> Option<BookKey> {
    if !is_audio_file(rel) {
        return None;
    }
    let mut parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty()).collect();
    let file = parts.pop()?;
    while parts.last().is_some_and(|d| is_disc_dir(d)) {
        parts.pop();
    }
    let dir = parts.join("/");
    let (base, copy) = strip_copy_suffix(stem(file));
    let single = crate::extension(file).as_deref() == Some("m4b") || parts.is_empty();
    if single {
        let book = strip_part_marker(base);
        let book = if book.trim().is_empty() { base } else { book };
        let key = if dir.is_empty() { format!("ab:{book}") } else { format!("ab:{dir}/{book}") };
        return Some(BookKey { key, dir, name: book.to_string(), copy: copy.map(str::to_string) });
    }
    let name = parts.last().copied().unwrap_or(base).to_string();
    Some(BookKey { key: format!("ab:{dir}"), dir, name, copy: copy.map(str::to_string) })
}

/// Cosa dice il nome di una cartella o di un file di audiolibro.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BookName {
    pub title: String,
    pub author: Option<String>,
    pub narrator: Option<String>,
    pub year: Option<u16>,
    /// Data completa (AAAA-MM-GG) se il nome la riporta.
    pub date: Option<String>,
    pub series: Option<String>,
    pub series_index: Option<String>,
}

/// Legge `Autore - Titolo 2021 Serie 4 Letto da Lettore`, `Autore - Titolo (2019)`, `Autore_Titolo_2020_Narrato_da_X`.
pub fn parse_book_name(name: &str) -> BookName {
    static TAG: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s*@\S+\s*$").unwrap());
    static NARRATOR: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?i)(?:^|\s|\{)(?:letto|narrato|narrata|letta|narrated|read|lu|gelesen|leído)\s+(?:da|by|par|von|por)\b(.*)$").unwrap());
    static DMY: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?:^|\s)(\d{2})[-./](\d{2})[-./]((?:19|20)\d{2})\s*$").unwrap());
    static YMD: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?:^|\s)((?:19|20)\d{2})[-. ](\d{2})[-. ](\d{2})\s*$").unwrap());
    static YEAR: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?:^|\s)[\(\[]?((?:19|20)\d{2})[\)\]]?(?:\s+(.*?))?\s*$").unwrap());
    static SEQ: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(.*?)\s+#?(\d{1,3}(?:\.\d)?)$").unwrap());

    let mut s = name.trim().to_string();
    // Nomi con i trattini bassi al posto degli spazi.
    if s.matches('_').count() > s.matches(' ').count() {
        s = s.replace('_', " ");
    }
    s = TAG.replace(&s, "").to_string();
    let mut out = BookName::default();
    if let Some(c) = NARRATOR.captures(&s) {
        let n = c[1].trim().trim_end_matches('}').trim().to_string();
        if !n.is_empty() {
            out.narrator = Some(n);
        }
        s = s[..c.get(0).unwrap().start()].trim().to_string();
    }
    let s2 = s.clone();
    if let Some(c) = DMY.captures(&s2) {
        out.year = c[3].parse().ok();
        out.date = Some(format!("{}-{}-{}", &c[3], &c[2], &c[1]));
        s = s2[..c.get(0).unwrap().start()].trim().to_string();
    } else if let Some(c) = YMD.captures(&s2) {
        out.year = c[1].parse().ok();
        out.date = Some(format!("{}-{}-{}", &c[1], &c[2], &c[3]));
        s = s2[..c.get(0).unwrap().start()].trim().to_string();
    } else if let Some(c) = YEAR.captures(&s2) {
        let before = s2[..c.get(0).unwrap().start()].trim();
        let tail = c.get(2).map(|m| m.as_str().trim()).unwrap_or("");
        // Il titolo non resta vuoto e la coda dopo l'anno non è un altro anno.
        if !before.is_empty() && !YEAR.is_match(tail) {
            out.year = c[1].parse().ok();
            if !tail.is_empty() {
                match SEQ.captures(tail) {
                    Some(sc) => {
                        out.series = Some(sc[1].trim().to_string());
                        out.series_index = Some(sc[2].to_string());
                    }
                    None => out.series = Some(tail.to_string()),
                }
            }
            s = before.to_string();
        }
    }
    let s = s.trim().trim_matches(['-', ' ', '.', ',']).to_string();
    match s.split_once(" - ") {
        Some((a, t)) if looks_like_author(a) && !t.trim().is_empty() => {
            out.author = Some(a.trim().to_string());
            out.title = t.trim().trim_start_matches("- ").to_string();
        }
        _ => out.title = s.trim_start_matches("- ").to_string(),
    }
    if out.title.is_empty() {
        out.title = name.trim().to_string();
    }
    out
}

fn looks_like_author(a: &str) -> bool {
    let a = a.trim();
    let words = a.split_whitespace().count();
    !a.is_empty() && words <= 6 && !a.starts_with(|c: char| c.is_ascii_digit())
}

/// Confronto "naturale" dei nomi: `2 - x` prima di `10 - x`, senza distinzione tra maiuscole e minuscole.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut na = String::new();
                while let Some(c) = ai.peek().copied().filter(char::is_ascii_digit) {
                    na.push(c);
                    ai.next();
                }
                let mut nb = String::new();
                while let Some(c) = bi.peek().copied().filter(char::is_ascii_digit) {
                    nb.push(c);
                    bi.next();
                }
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let o = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb)).then_with(|| na.len().cmp(&nb.len()));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                let o = x.to_lowercase().cmp(y.to_lowercase());
                if o != Ordering::Equal {
                    return o;
                }
                ai.next();
                bi.next();
            }
        }
    }
}

/// Data nel nome di una puntata (`… 01-09-2026 …`, `2026-09-01`): ordina i podcast in ordine di uscita.
pub fn date_in_name(name: &str) -> Option<String> {
    static DMY: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?:^|\D)(\d{2})[-.](\d{2})[-.]((?:19|20)\d{2})(?:\D|$)").unwrap());
    static YMD: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?:^|\D)((?:19|20)\d{2})[-.](\d{2})[-.](\d{2})(?:\D|$)").unwrap());
    if let Some(c) = YMD.captures(name) {
        return Some(format!("{}-{}-{}", &c[1], &c[2], &c[3]));
    }
    DMY.captures(name).map(|c| format!("{}-{}-{}", &c[3], &c[2], &c[1]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_files() {
        assert!(is_audio_file("a/b.M4B"));
        assert!(is_audio_file("x.mp3"));
        assert!(!is_audio_file("x.mkv"));
        assert!(!is_audio_file("cover.jpg"));
    }

    #[test]
    fn folder_is_one_book() {
        let a = book_key("Autore/Libro/01 - Inizio.mp3").unwrap();
        let b = book_key("Autore/Libro/CD2/05.mp3").unwrap();
        assert_eq!(a.key, "ab:Autore/Libro");
        assert_eq!(a.key, b.key);
        assert_eq!(a.name, "Libro");
    }

    #[test]
    fn each_m4b_is_a_book() {
        let a = book_key("Serie/Autore - Uno @x.m4b").unwrap();
        let b = book_key("Serie/Autore - Due @x.m4b").unwrap();
        assert_ne!(a.key, b.key);
        assert_eq!(a.dir, "Serie");
        // Copia scaricata due volte: stesso libro, etichetta della copia.
        let c = book_key("Serie/Autore - Uno @x (1).m4b").unwrap();
        assert_eq!(a.key, c.key);
        assert_eq!(c.copy.as_deref(), Some("(1)"));
        // Parti dichiarate: stesso libro.
        let p1 = book_key("D/Libro - Parte 1.m4b").unwrap();
        let p2 = book_key("D/Libro - Parte 2.m4b").unwrap();
        assert_eq!(p1.key, p2.key);
        assert_eq!(p1.name, "Libro");
        assert_eq!(book_key("D/Libro (2 of 3).m4b").unwrap().name, "Libro");
        // Libri numerati di una serie restano distinti.
        assert_ne!(book_key("D/Lezioni di storia 4.m4b").unwrap().key, book_key("D/Lezioni di storia 5.m4b").unwrap().key);
    }

    #[test]
    fn root_files_are_books() {
        let a = book_key("Libro uno.mp3").unwrap();
        assert_eq!(a.key, "ab:Libro uno");
        assert_eq!(a.dir, "");
    }

    #[test]
    fn disc_dirs() {
        assert!(is_disc_dir("CD1"));
        assert!(is_disc_dir("Disc 02"));
        assert!(is_disc_dir("disco_3"));
        assert!(!is_disc_dir("Capitolo 1"));
    }

    #[test]
    fn names_from_audible_downloads() {
        let n = parse_book_name("AA.VV. - Delitti di lago 10-07-2025 Letto da Voce virtuale @audible_maw");
        assert_eq!(n.author.as_deref(), Some("AA.VV."));
        assert_eq!(n.title, "Delitti di lago");
        assert_eq!(n.narrator.as_deref(), Some("Voce virtuale"));
        assert_eq!(n.date.as_deref(), Some("2025-07-10"));
        assert_eq!(n.year, Some(2025));

        let n = parse_book_name("Peter Straub - Mr. X 2022  Narrato da Alvaro Gradella");
        assert_eq!(n.author.as_deref(), Some("Peter Straub"));
        assert_eq!(n.title, "Mr. X");
        assert_eq!(n.year, Some(2022));
        assert_eq!(n.narrator.as_deref(), Some("Alvaro Gradella"));

        let n = parse_book_name("Claudio Pavone - Novecento italiano - 1943. L'8 settembre 2019 Lezioni di Storia 4 Narrato da Claudio Pavone @audible_maw");
        assert_eq!(n.author.as_deref(), Some("Claudio Pavone"));
        assert_eq!(n.title, "Novecento italiano - 1943. L'8 settembre");
        assert_eq!(n.series.as_deref(), Some("Lezioni di Storia"));
        assert_eq!(n.series_index.as_deref(), Some("4"));

        let n = parse_book_name("Felicia_Kingsley_Bugiarde_si_diventa_2022_12_22_Narrato_da_Annalisa");
        assert_eq!(n.narrator.as_deref(), Some("Annalisa"));
        assert_eq!(n.date.as_deref(), Some("2022-12-22"));
        assert_eq!(n.title, "Felicia Kingsley Bugiarde si diventa");

        let n = parse_book_name("33 false verità sull'Europa @audible_maw");
        assert_eq!(n.title, "33 false verità sull'Europa");
        assert_eq!(n.author, None);

        let n = parse_book_name("Andy Weir - Artemis (2019)");
        assert_eq!((n.author.as_deref(), n.title.as_str(), n.year), (Some("Andy Weir"), "Artemis", Some(2019)));

        let n = parse_book_name("1984");
        assert_eq!(n.title, "1984");
        assert_eq!(n.year, None);
    }

    #[test]
    fn natural_order() {
        let mut v = vec!["10 - x.mp3", "2 - x.mp3", "1 - x.mp3", "Capitolo 11.mp3", "capitolo 9.mp3"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["1 - x.mp3", "2 - x.mp3", "10 - x.mp3", "capitolo 9.mp3", "Capitolo 11.mp3"]);
    }

    #[test]
    fn dates_in_episode_names() {
        assert_eq!(date_in_name("- 100 Secondi Con Mentana - 01-09-2026 09 01-09-2026 Letto da").as_deref(), Some("2026-09-01"));
        assert_eq!(date_in_name("Puntata 2024-03-05.mp3").as_deref(), Some("2024-03-05"));
        assert_eq!(date_in_name("Capitolo 12.mp3"), None);
    }
}
