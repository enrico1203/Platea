//! Preferenze scelte dall'utente: lingue di audio e sottotitoli, comportamento del player, home.
//! Salvate in `users.preferences` (JSON), con valori predefiniti per ogni campo mancante.

use platea_playback::{AudioInfo, SubtitleInfo};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleMode {
    /// Le tracce segnate come predefinite o forzate nel file.
    #[default]
    Default,
    /// Sempre i sottotitoli nella lingua preferita.
    Always,
    /// Solo i sottotitoli forzati (parti in lingua straniera).
    OnlyForced,
    /// Mai.
    None,
    /// Se l'audio non è nella lingua dei sottotitoli, sottotitoli completi; altrimenti solo i forzati.
    Smart,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(default)]
pub struct UserPrefs {
    /// Lingua audio preferita (ISO 639-2, es. "ita"); vuoto = traccia predefinita del file.
    pub audio_language: String,
    /// Lingua dei sottotitoli preferita (ISO 639-2); vuoto = lingua dell'interfaccia.
    pub subtitle_language: String,
    pub subtitle_mode: SubtitleMode,
    /// Episodio successivo automatico alla fine, con conto alla rovescia.
    pub autoplay_next: bool,
    pub autoplay_countdown_seconds: u32,
    pub skip_back_seconds: u32,
    pub skip_forward_seconds: u32,
    /// Dimensione dei sottotitoli nel player web: "s", "m", "l", "xl".
    pub subtitle_size: String,
    /// Sottotitoli su fondo scuro.
    pub subtitle_background: bool,
    /// Sezioni della home in ordine; quelle assenti sono nascoste.
    pub home_sections: Vec<String>,
    /// Librerie nascoste dalla home (ID opachi).
    pub home_hidden_libraries: Vec<String>,
    /// Niente titoli già visti tra gli "aggiunti di recente".
    pub hide_played_in_recent: bool,
    /// Tema: "auto", "dark", "light".
    pub theme: String,
    /// Ricordare per ogni film e serie le tracce scelte nel player.
    pub remember_tracks: bool,
}

pub const HOME_SECTIONS: &[&str] = &["resume", "next_up", "recent"];

impl Default for UserPrefs {
    fn default() -> Self {
        UserPrefs {
            audio_language: String::new(),
            subtitle_language: String::new(),
            subtitle_mode: SubtitleMode::Default,
            autoplay_next: true,
            autoplay_countdown_seconds: 10,
            skip_back_seconds: 10,
            skip_forward_seconds: 30,
            subtitle_size: "m".into(),
            subtitle_background: false,
            home_sections: HOME_SECTIONS.iter().map(|s| s.to_string()).collect(),
            home_hidden_libraries: vec![],
            hide_played_in_recent: false,
            theme: "auto".into(),
            remember_tracks: true,
        }
    }
}

impl UserPrefs {
    /// Riporta i valori entro limiti sensati: le preferenze arrivano dal client.
    pub fn sanitize(mut self) -> Self {
        // "it-IT", "ITA", " eng " → "it", "ita", "eng".
        let lang = |s: &str| s.trim().split(|c: char| !c.is_ascii_alphabetic()).next().unwrap_or("").to_ascii_lowercase().chars().take(3).collect::<String>();
        self.audio_language = lang(&self.audio_language);
        self.subtitle_language = lang(&self.subtitle_language);
        self.autoplay_countdown_seconds = self.autoplay_countdown_seconds.clamp(0, 60);
        self.skip_back_seconds = self.skip_back_seconds.clamp(5, 120);
        self.skip_forward_seconds = self.skip_forward_seconds.clamp(5, 300);
        if !["s", "m", "l", "xl"].contains(&self.subtitle_size.as_str()) {
            self.subtitle_size = "m".into();
        }
        if !["auto", "dark", "light"].contains(&self.theme.as_str()) {
            self.theme = "auto".into();
        }
        let mut seen = std::collections::HashSet::new();
        self.home_sections.retain(|s| HOME_SECTIONS.contains(&s.as_str()) && seen.insert(s.clone()));
        self.home_hidden_libraries.truncate(200);
        self
    }

    pub fn parse(json: &str) -> Self {
        serde_json::from_str::<UserPrefs>(json).unwrap_or_default().sanitize()
    }
}

pub async fn load(state: &crate::state::Shared, user_id: i64) -> UserPrefs {
    let raw: Option<String> =
        sqlx::query_scalar("SELECT preferences FROM users WHERE id = ?").bind(user_id).fetch_optional(&state.db.read).await.ok().flatten();
    raw.map(|r| UserPrefs::parse(&r)).unwrap_or_default()
}

/// Codici ISO 639-1 e 639-2 (anche bibliografici) della stessa lingua.
fn lang_code(s: &str) -> String {
    let s = s.trim().to_ascii_lowercase();
    let map: &[(&str, &str)] = &[
        ("it", "ita"),
        ("en", "eng"),
        ("fr", "fra"),
        ("fre", "fra"),
        ("de", "deu"),
        ("ger", "deu"),
        ("es", "spa"),
        ("pt", "por"),
        ("ja", "jpn"),
        ("zh", "zho"),
        ("chi", "zho"),
        ("ru", "rus"),
        ("ko", "kor"),
        ("nl", "nld"),
        ("dut", "nld"),
        ("pl", "pol"),
        ("sv", "swe"),
        ("da", "dan"),
        ("no", "nor"),
        ("fi", "fin"),
        ("ar", "ara"),
        ("tr", "tur"),
        ("el", "ell"),
        ("gre", "ell"),
        ("he", "heb"),
        ("hi", "hin"),
        ("cs", "ces"),
        ("cze", "ces"),
        ("ro", "ron"),
        ("rum", "ron"),
        ("hu", "hun"),
    ];
    map.iter().find(|(a, _)| *a == s).map(|(_, b)| b.to_string()).unwrap_or(s)
}

pub fn same_language(a: Option<&str>, b: &str) -> bool {
    !b.is_empty() && a.is_some_and(|a| lang_code(a) == lang_code(b))
}

/// Traccia audio iniziale: la lingua preferita se c'è, altrimenti quella predefinita del file.
pub fn pick_audio(audio: &[AudioInfo], prefs: &UserPrefs) -> Option<u32> {
    if prefs.audio_language.is_empty() {
        return None;
    }
    // A parità di lingua, quella predefinita e poi quella con più canali.
    audio.iter().filter(|a| same_language(a.language.as_deref(), &prefs.audio_language)).max_by_key(|a| (a.default, a.channels)).map(|a| a.index)
}

/// Sottotitolo iniziale secondo la modalità; solo tracce di testo (quelle a immagine richiederebbero la transcodifica).
pub fn pick_subtitle(subs: &[SubtitleInfo], audio_language: Option<&str>, prefs: &UserPrefs, ui_language: &str) -> Option<u32> {
    let text: Vec<&SubtitleInfo> = subs.iter().filter(|s| !s.is_image()).collect();
    let lang = if prefs.subtitle_language.is_empty() { ui_language } else { prefs.subtitle_language.as_str() };
    let in_lang =
        |forced: Option<bool>| text.iter().find(|s| same_language(s.language.as_deref(), lang) && forced.is_none_or(|f| s.forced == f)).map(|s| s.index);
    match prefs.subtitle_mode {
        SubtitleMode::None => None,
        SubtitleMode::Always => in_lang(Some(false)).or_else(|| in_lang(None)),
        SubtitleMode::OnlyForced => in_lang(Some(true)),
        SubtitleMode::Default => {
            text.iter().find(|s| s.forced && same_language(s.language.as_deref(), lang)).or_else(|| text.iter().find(|s| s.default)).map(|s| s.index)
        }
        SubtitleMode::Smart => {
            if audio_language.is_some_and(|a| !same_language(Some(a), lang)) {
                in_lang(Some(false)).or_else(|| in_lang(None))
            } else {
                in_lang(Some(true))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(i: u32, l: &str, def: bool, ch: u32) -> AudioInfo {
        AudioInfo { index: i, codec: "ac3".into(), channels: ch, language: Some(l.into()), default: def, bitrate: None }
    }
    fn s(i: u32, l: &str, forced: bool, codec: &str) -> SubtitleInfo {
        SubtitleInfo { index: i, codec: codec.into(), language: Some(l.into()), forced, default: false, external: false }
    }

    #[test]
    fn audio_by_language() {
        let tracks = vec![a(1, "eng", true, 6), a(2, "ita", false, 2), a(3, "ita", false, 6)];
        let p = UserPrefs { audio_language: "it".into(), ..Default::default() }.sanitize();
        assert_eq!(pick_audio(&tracks, &p), Some(3));
        assert_eq!(pick_audio(&tracks, &UserPrefs::default()), None);
    }

    #[test]
    fn subtitle_modes() {
        let subs = vec![s(4, "ita", true, "subrip"), s(5, "ita", false, "subrip"), s(6, "eng", false, "subrip"), s(7, "ita", false, "hdmv_pgs_subtitle")];
        let mut p = UserPrefs { subtitle_language: "ita".into(), ..Default::default() };
        p.subtitle_mode = SubtitleMode::Always;
        assert_eq!(pick_subtitle(&subs, Some("ita"), &p, "it"), Some(5));
        p.subtitle_mode = SubtitleMode::OnlyForced;
        assert_eq!(pick_subtitle(&subs, Some("ita"), &p, "it"), Some(4));
        p.subtitle_mode = SubtitleMode::Smart;
        assert_eq!(pick_subtitle(&subs, Some("eng"), &p, "it"), Some(5));
        assert_eq!(pick_subtitle(&subs, Some("ita"), &p, "it"), Some(4));
        p.subtitle_mode = SubtitleMode::None;
        assert_eq!(pick_subtitle(&subs, Some("eng"), &p, "it"), None);
        // Lingua dei sottotitoli vuota: quella dell'interfaccia.
        let q = UserPrefs { subtitle_mode: SubtitleMode::Always, ..Default::default() };
        assert_eq!(pick_subtitle(&subs, None, &q, "en"), Some(6));
    }

    #[test]
    fn sanitize_limits() {
        let p = UserPrefs::parse(
            r#"{"skip_forward_seconds": 9999, "theme": "rosa", "home_sections": ["recent", "x", "recent", "resume"], "audio_language": "IT-it"}"#,
        );
        assert_eq!(p.skip_forward_seconds, 300);
        assert_eq!(p.theme, "auto");
        assert_eq!(p.home_sections, vec!["recent", "resume"]);
        assert_eq!(p.audio_language, "it");
        assert!(p.autoplay_next);
    }
}
