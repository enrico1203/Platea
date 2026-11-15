//! Ricerca di FFmpeg: jellyfin-ffmpeg se presente, poi il PATH. Il percorso si cambia dalla dashboard.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FfmpegInfo {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub version: String,
    pub is_jellyfin: bool,
    pub available: bool,
}

#[derive(Clone)]
pub struct FfmpegLocator {
    inner: Arc<RwLock<Arc<FfmpegInfo>>>,
}

const JELLYFIN_DIR: &str = "/usr/lib/jellyfin-ffmpeg";

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(name)).find(|p| p.is_file())
}

fn version_of(ffmpeg: &Path) -> Option<String> {
    let out = std::process::Command::new(ffmpeg).arg("-version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let first = text.lines().next()?;
    // "ffmpeg version 8.1.1-Jellyfin Copyright ..."
    Some(first.split_whitespace().nth(2).unwrap_or(first).to_string())
}

impl FfmpegLocator {
    pub fn new() -> Self {
        FfmpegLocator {
            inner: Arc::new(RwLock::new(Arc::new(FfmpegInfo {
                ffmpeg: "ffmpeg".into(),
                ffprobe: "ffprobe".into(),
                version: String::new(),
                is_jellyfin: false,
                available: false,
            }))),
        }
    }

    pub fn get(&self) -> Arc<FfmpegInfo> {
        self.inner.read().unwrap().clone()
    }

    /// Risolve i percorsi. Bloccante (esegue `ffmpeg -version`): chiamare da spawn_blocking.
    pub fn resolve(&self, ffmpeg_setting: &str, ffprobe_setting: &str) -> Arc<FfmpegInfo> {
        let ffmpeg = if !ffmpeg_setting.trim().is_empty() {
            PathBuf::from(ffmpeg_setting.trim())
        } else if let Ok(p) = std::env::var("PLATEA_FFMPEG") {
            PathBuf::from(p)
        } else if Path::new(JELLYFIN_DIR).join("ffmpeg").is_file() {
            Path::new(JELLYFIN_DIR).join("ffmpeg")
        } else {
            which("ffmpeg").unwrap_or_else(|| "ffmpeg".into())
        };
        let ffprobe = if !ffprobe_setting.trim().is_empty() {
            PathBuf::from(ffprobe_setting.trim())
        } else {
            let sibling = ffmpeg.with_file_name("ffprobe");
            if sibling.is_file() {
                sibling
            } else {
                which("ffprobe").unwrap_or_else(|| "ffprobe".into())
            }
        };
        let version = version_of(&ffmpeg);
        let info = Arc::new(FfmpegInfo {
            is_jellyfin: version.as_deref().is_some_and(|v| v.to_lowercase().contains("jellyfin")),
            available: version.is_some(),
            version: version.unwrap_or_default(),
            ffmpeg,
            ffprobe,
        });
        if info.available {
            tracing::info!(ffmpeg = %info.ffmpeg.display(), version = %info.version, "FFmpeg trovato");
        } else {
            tracing::warn!(ffmpeg = %info.ffmpeg.display(), "FFmpeg non trovato: analisi e transcodifica non disponibili");
        }
        *self.inner.write().unwrap() = info.clone();
        info
    }
}

impl Default for FfmpegLocator {
    fn default() -> Self {
        Self::new()
    }
}
