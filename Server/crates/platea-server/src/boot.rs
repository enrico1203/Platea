//! File di avvio: solo ciò che serve prima che il database sia aperto.
//! Indirizzo di ascolto, directory di dati, cache e transcodifica, livello di log.
//! Tutto il resto vive nel database ed è modificabile dalla dashboard.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct BootConfig {
    pub bind: String,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub transcode_dir: PathBuf,
    pub log: String,
    /// Cartella statica della web UI da servire su `/` (opzionale).
    pub web_dir: Option<PathBuf>,
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

impl BootConfig {
    pub fn from_env() -> Self {
        let data_dir = PathBuf::from(env("PLATEA_DATA_DIR").unwrap_or_else(|| "./data".into()));
        BootConfig {
            bind: env("PLATEA_BIND").unwrap_or_else(|| "0.0.0.0:8096".into()),
            cache_dir: PathBuf::from(env("PLATEA_CACHE_DIR").unwrap_or_else(|| "./cache".into())),
            transcode_dir: PathBuf::from(env("PLATEA_TRANSCODE_DIR").unwrap_or_else(|| "./transcode".into())),
            log: env("PLATEA_LOG").or_else(|| env("RUST_LOG")).unwrap_or_else(|| "info".into()),
            web_dir: env("PLATEA_WEB_DIR").map(PathBuf::from),
            data_dir,
        }
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("platea.db")
    }

    pub fn images_dir(&self) -> PathBuf {
        self.cache_dir.join("images")
    }

    pub fn subtitles_dir(&self) -> PathBuf {
        self.cache_dir.join("subtitles")
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        for d in [&self.data_dir, &self.cache_dir, &self.transcode_dir, &self.images_dir(), &self.subtitles_dir()] {
            std::fs::create_dir_all(d)?;
        }
        Ok(())
    }

    /// Segreto dell'installazione: generato al primo avvio, salvato in `data/`.
    pub fn load_secret(&self) -> std::io::Result<Vec<u8>> {
        let path = self.data_dir.join("secret.key");
        match std::fs::read(&path) {
            Ok(v) if v.len() >= 32 => Ok(v),
            _ => {
                let s = crate::util::random_hex(32);
                std::fs::write(&path, &s)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
                }
                Ok(s.into_bytes())
            }
        }
    }
}
