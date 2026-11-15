//! File `.strm`: un file di testo con l'indirizzo di un video in rete (Kodi, Jellyfin, plugin IPTV e simili).
//! Si accettano solo indirizzi `http` e `https`: niente percorsi locali né altri protocolli, così un `.strm` non può
//! far leggere al server file della macchina. L'indirizzo si legge una volta, durante l'analisi del file.

use std::path::Path;
use std::time::Duration;

/// Un `.strm` non è un file di testo lungo: oltre questa dimensione non si legge.
const MAX_BYTES: u64 = 16 * 1024;

pub fn is_strm(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("strm"))
}

/// Indirizzo dal contenuto: la prima riga non vuota che non sia un commento (`#`). Solo http(s).
pub fn parse(content: &str) -> Option<String> {
    let line = content.trim_start_matches('\u{feff}').lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#'))?;
    let lower = line.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) || line.len() > 4096 || line.chars().any(|c| c.is_control() || c == ' ') {
        return None;
    }
    Some(line.to_string())
}

/// Legge l'indirizzo di un `.strm`, con un tempo limite (il file può stare su un mount lento).
pub async fn read_url(path: &Path, timeout: Duration) -> anyhow::Result<String> {
    let p = path.to_path_buf();
    let read = async move {
        let meta = tokio::fs::metadata(&p).await?;
        anyhow::ensure!(meta.len() <= MAX_BYTES, "file .strm troppo grande");
        Ok::<_, anyhow::Error>(tokio::fs::read(&p).await?)
    };
    let bytes = tokio::time::timeout(timeout, read).await.map_err(|_| anyhow::anyhow!("lettura del .strm oltre il tempo limite"))??;
    parse(&String::from_utf8_lossy(&bytes)).ok_or_else(|| anyhow::anyhow!("il .strm non contiene un indirizzo http(s) valido"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_urls() {
        assert_eq!(parse("https://example.org/film.mkv\n").as_deref(), Some("https://example.org/film.mkv"));
        assert_eq!(parse("\u{feff}# commento\n\n  http://192.168.1.5:8080/live/1.ts  \n").as_deref(), Some("http://192.168.1.5:8080/live/1.ts"));
        assert_eq!(parse("/mnt/film/segreto.mkv"), None);
        assert_eq!(parse("file:///etc/passwd"), None);
        assert_eq!(parse("concat:/etc/passwd|x"), None);
        assert_eq!(parse("https://x.org/a b"), None);
        assert_eq!(parse(""), None);
        assert!(is_strm(Path::new("Film (2020)/Film (2020).STRM")));
    }
}
