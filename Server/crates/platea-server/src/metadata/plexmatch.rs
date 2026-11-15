//! File `.plexmatch` (formato di Plex): suggerimenti di associazione scritti dall'utente nella cartella
//! di un film o di una serie. Righe `chiave: valore`, commenti con `#`:
//!
//! ```text
//! title: Tron
//! year: 1982
//! tmdbid: 97
//! imdbid: tt0084827
//! guid: tmdb://97
//! ```
//!
//! È la scelta esplicita dell'utente: i suoi identificativi vincono su quelli di un NFO e del nome del file.
//! Come per gli NFO, il file si legge una volta e non si scrive mai.
//!
//! Nella cartella di una serie le righe `ep:` (o `episode:`) assegnano un episodio a un file con un nome qualsiasi;
//! il percorso è relativo alla cartella della serie:
//!
//! ```text
//! ep: S01E01: /Stagione 1/Il pilota.mkv
//! ep: S01E02-E03: /Stagione 1/Doppio.mkv
//! ep: 2x04: Extra/intervista.mkv
//! ep: 12: /Anime/dodicesimo.mkv          (numerazione assoluta, stagione 1)
//! ep: 2023-05-01: /Puntate/primo maggio.mkv
//! ```

use super::nfo::{fs_timeout, read_small, Located};
use crate::state::Shared;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct PlexMatch {
    pub title: Option<String>,
    pub year: Option<i64>,
    pub tmdb_id: Option<i64>,
    pub imdb_id: Option<String>,
    pub tvdb_id: Option<String>,
}

impl PlexMatch {
    pub fn has_ids(&self) -> bool {
        self.tmdb_id.is_some() || self.imdb_id.is_some() || self.tvdb_id.is_some()
    }
    fn useful(&self) -> bool {
        self.has_ids() || self.title.is_some()
    }
}

fn set_guid(out: &mut PlexMatch, v: &str) {
    let Some((scheme, id)) = v.split_once("://") else { return };
    let id = id.trim().trim_end_matches('/');
    match scheme.trim().to_ascii_lowercase().as_str() {
        "tmdb" | "themoviedb" => out.tmdb_id = out.tmdb_id.or(id.parse().ok()),
        "imdb" => out.imdb_id = out.imdb_id.clone().or(Some(id.to_string())),
        "tvdb" | "thetvdb" => out.tvdb_id = out.tvdb_id.clone().or(Some(id.to_string())),
        _ => {}
    }
}

pub fn parse(raw: &str) -> Option<PlexMatch> {
    let mut out = PlexMatch::default();
    for line in raw.trim_start_matches('\u{feff}').lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once(':') else { continue };
        let v = v.trim().trim_matches('"').trim();
        if v.is_empty() {
            continue;
        }
        match k.trim().to_ascii_lowercase().as_str() {
            "title" => out.title = Some(v.to_string()),
            "year" => out.year = v.parse().ok().filter(|y| (1870..2200).contains(y)),
            "tmdbid" => out.tmdb_id = v.parse().ok(),
            "imdbid" => out.imdb_id = Some(v.to_string()).filter(|i| i.starts_with("tt")),
            "tvdbid" => out.tvdb_id = Some(v.to_string()),
            "guid" => set_guid(&mut out, v),
            _ => {}
        }
    }
    out.useful().then_some(out)
}

/// Episodio indicato da una riga `ep:`, per il file al percorso `path` (relativo alla cartella della serie, senza `/` iniziale).
#[derive(Debug, Clone, PartialEq)]
pub struct EpisodeHint {
    pub season: u32,
    pub episodes: Vec<u32>,
    pub air_date: Option<String>,
    pub path: String,
}

impl EpisodeHint {
    /// Percorso equivalente con un nome standard, dentro la cartella della serie `series_dir` (relativa alla libreria):
    /// riconosciuto come gli altri episodi, quindi con la stessa serie.
    pub fn synthetic_rel(&self, series_dir: &str) -> String {
        let ext = self.path.rsplit_once('.').map(|(_, e)| e).unwrap_or("mkv");
        let name = match &self.air_date {
            Some(d) => d.clone(),
            None => {
                let mut n = format!("S{:02}E{:02}", self.season, self.episodes[0]);
                if let Some(last) = self.episodes.last().filter(|l| **l != self.episodes[0]) {
                    n.push_str(&format!("-E{last:02}"));
                }
                n
            }
        };
        if series_dir.is_empty() {
            format!("{name}.{ext}")
        } else {
            format!("{series_dir}/{name}.{ext}")
        }
    }
}

fn parse_episode_spec(spec: &str) -> Option<(u32, Vec<u32>, Option<String>)> {
    let s = spec.trim().to_ascii_uppercase();
    let range = |a: u32, b: Option<u32>| -> Vec<u32> {
        match b.filter(|b| *b > a && *b - a < 50) {
            Some(b) => (a..=b).collect(),
            None => vec![a],
        }
    };
    // S01E02, S01E02-E03, S01E02-03
    if let Some(rest) = s.strip_prefix('S') {
        let (season, eps) = rest.split_once('E')?;
        let (a, b) = eps.split_once('-').map(|(a, b)| (a, Some(b.trim_start_matches('E')))).unwrap_or((eps, None));
        return Some((season.parse().ok()?, range(a.parse().ok()?, b.and_then(|b| b.parse().ok())), None));
    }
    // 2x04, 2x04-05
    if let Some((season, eps)) = s.split_once('X') {
        let (a, b) = eps.split_once('-').map(|(a, b)| (a, Some(b))).unwrap_or((eps, None));
        return Some((season.parse().ok()?, range(a.parse().ok()?, b.and_then(|b| b.parse().ok())), None));
    }
    // Data AAAA-MM-GG: come gli episodi per data, stagione = anno.
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() == 3 && parts[0].len() == 4 && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())) {
        let year: u32 = parts[0].parse().ok()?;
        return Some((year, vec![0], Some(format!("{}-{:0>2}-{:0>2}", parts[0], parts[1], parts[2]))));
    }
    // Numerazione assoluta.
    let n: u32 = s.parse().ok()?;
    Some((1, vec![n], None))
}

/// Righe `ep:` / `episode:` di un `.plexmatch`.
pub fn parse_episode_hints(raw: &str) -> Vec<EpisodeHint> {
    let mut out = Vec::new();
    for line in raw.trim_start_matches('\u{feff}').lines() {
        let line = line.trim();
        let Some((k, v)) = line.split_once(':') else { continue };
        if !matches!(k.trim().to_ascii_lowercase().as_str(), "ep" | "episode") {
            continue;
        }
        let Some((spec, path)) = v.split_once(':') else { continue };
        let path = path.trim().trim_matches('"').trim().trim_start_matches('/').to_string();
        if path.is_empty() || path.split('/').any(|p| p == "..") {
            continue;
        }
        if let Some((season, episodes, air_date)) = parse_episode_spec(spec) {
            out.push(EpisodeHint { season, episodes, air_date, path });
        }
    }
    out
}

/// `.plexmatch` nella cartella del film o, per una serie, nella cartella della serie (anche maiuscolo).
pub async fn find(state: &Shared, loc: &Located) -> Option<PlexMatch> {
    let timeout = fs_timeout(state);
    for d in &loc.dirs {
        for name in [".plexmatch", ".PlexMatch", ".PLEXMATCH"] {
            if let Some(raw) = read_small(d.join(name), timeout).await {
                if let Some(p) = parse(&raw) {
                    tracing::debug!(dir = %d.display(), "plexmatch letto");
                    return Some(p);
                }
            }
        }
    }
    None
}

/// Salva gli identificativi: sostituiscono quelli dedotti da nome del file o NFO.
pub async fn store_ids(state: &Shared, item_id: i64, p: &PlexMatch) -> anyhow::Result<()> {
    for (prov, v) in [("tmdb", p.tmdb_id.map(|v| v.to_string())), ("imdb", p.imdb_id.clone()), ("tvdb", p.tvdb_id.clone())] {
        if let Some(v) = v {
            sqlx::query("INSERT OR REPLACE INTO external_ids (item_id, provider, value) VALUES (?, ?, ?)")
                .bind(item_id)
                .bind(prov)
                .bind(v)
                .execute(&state.db.write)
                .await?;
        }
    }
    // Titolo e anno indicati dall'utente servono alla ricerca su TMDB quando mancano gli identificativi.
    if p.title.is_some() || p.year.is_some() {
        sqlx::query(
            "UPDATE items SET title = COALESCE(?, title), sort_title = COALESCE(?, sort_title), year = COALESCE(?, year) WHERE id = ? AND match_state NOT IN ('matched', 'manual')",
        )
        .bind(&p.title)
        .bind(p.title.as_deref().map(platea_naming::sort_title))
        .bind(p.year)
        .bind(item_id)
        .execute(&state.db.write)
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plexmatch() {
        let p = parse("# commento\nTitle: Tron\nyear: 1982\ntmdbid: 97\nimdbid: tt0084827\n").unwrap();
        assert_eq!(p.title.as_deref(), Some("Tron"));
        assert_eq!(p.year, Some(1982));
        assert_eq!(p.tmdb_id, Some(97));
        assert_eq!(p.imdb_id.as_deref(), Some("tt0084827"));
    }

    #[test]
    fn guid_schemes() {
        let p = parse("guid: tmdb://603\n").unwrap();
        assert_eq!(p.tmdb_id, Some(603));
        let p = parse("guid: tvdb://81189\nep: S01E01: pilot.mkv\n").unwrap();
        assert_eq!(p.tvdb_id.as_deref(), Some("81189"));
        assert!(parse("guid: plex://movie/5d776\n").is_none());
        assert!(parse("\n# solo commenti\n").is_none());
    }

    #[test]
    fn episode_hints() {
        let raw = "title: X\nep: S01E01: /Stagione 1/Il pilota.mkv\nepisode: S01E02-E03: Stagione 1/Doppio.mp4\nep: 2x04: Extra/intervista.mkv\nep: 12: /Anime/dodici.mkv\nep: 2023-05-01: /Puntate/maggio.mkv\nep: S01E09: /../fuori.mkv\nep: rotto\n";
        let h = parse_episode_hints(raw);
        assert_eq!(h.len(), 5);
        assert_eq!((h[0].season, h[0].episodes.clone(), h[0].path.as_str()), (1, vec![1], "Stagione 1/Il pilota.mkv"));
        assert_eq!(h[1].episodes, vec![2, 3]);
        assert_eq!((h[2].season, h[2].episodes.clone()), (2, vec![4]));
        assert_eq!((h[3].season, h[3].episodes.clone()), (1, vec![12]));
        assert_eq!(h[4].air_date.as_deref(), Some("2023-05-01"));
        assert_eq!(h[0].synthetic_rel("Serie (2020)"), "Serie (2020)/S01E01.mkv");
        assert_eq!(h[1].synthetic_rel("Serie (2020)"), "Serie (2020)/S01E02-E03.mp4");
        assert_eq!(h[4].synthetic_rel("Serie (2020)"), "Serie (2020)/2023-05-01.mkv");
        // Il percorso equivalente si riconosce con i numeri indicati e la serie della cartella.
        let e = platea_naming::parse_episode(&h[1].synthetic_rel("Serie (2020)")).unwrap();
        assert_eq!((e.series_title.as_str(), e.season, e.episodes.clone()), ("Serie", Some(1), vec![2, 3]));
        let e = platea_naming::parse_episode(&h[4].synthetic_rel("Serie (2020)")).unwrap();
        assert_eq!(e.air_date.as_deref(), Some("2023-05-01"));
    }
}
