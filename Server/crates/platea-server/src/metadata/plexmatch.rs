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
//! Come per gli NFO, il file si legge una volta e non si scrive mai. Le righe `ep:` (episodi rinominati)
//! non sono ancora usate.

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
}
