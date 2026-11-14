//! Stato condiviso del server.

use crate::auth::{LoginLimiter, TokenCache};
use crate::boot::BootConfig;
use crate::db::Db;
use crate::events::EventBus;
use crate::ids::IdCodec;
use crate::jobs::JobQueue;
use crate::metadata::tmdb::Tmdb;
use crate::sessions::SessionManager;
use crate::settings::Settings;
use crate::transcode::ffmpeg::FfmpegLocator;
use crate::transcode::TranscodeManager;
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;

pub struct AppState {
    pub boot: BootConfig,
    pub db: Db,
    pub settings: Settings,
    pub events: EventBus,
    pub ids: IdCodec,
    pub http: reqwest::Client,
    pub token_cache: TokenCache,
    pub login_limiter: LoginLimiter,
    pub jobs: JobQueue,
    pub ffmpeg: FfmpegLocator,
    pub transcoder: Arc<TranscodeManager>,
    pub sessions: SessionManager,
    pub tmdb: Tmdb,
    pub server_id: String,
    /// Codice di primo avvio: presente finché la procedura guidata non crea l'amministratore.
    pub setup_code: Mutex<Option<String>>,
    pub image_downloads: Semaphore,
    pub image_resizes: Semaphore,
    /// Estrazioni di sottotitoli e keyframe su richiesta: risorsa scarsa.
    pub extractions: Semaphore,
    pub started_at: i64,
}

pub type Shared = Arc<AppState>;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
