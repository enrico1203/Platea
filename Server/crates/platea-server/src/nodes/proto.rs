//! Protocollo tra il server e i nodi: tipi scambiati in JSON su `/node/v1`.
//! Il server chiama il nodo, mai il contrario; la versione del protocollo cambia solo con modifiche incompatibili.

use crate::settings::HwBackend;
use crate::transcode::command::{HwProfile, JobSpec};
use crate::transcode::hw::Capabilities;
use serde::{Deserialize, Serialize};

/// Versione del protocollo. Un nodo con una versione diversa viene rifiutato con l'avviso "aggiorna il nodo".
pub const PROTOCOL: u32 = 1;

/// Configurazione del nodo: vive nel database del server e viene spinta al nodo a ogni collegamento.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct NodeConfig {
    pub hw_backend: HwBackend,
    pub vaapi_device: String,
    /// 0 = illimitate.
    pub max_hw_transcodes: u32,
    /// 0 = illimitate.
    pub max_sw_transcodes: u32,
    pub sw_preset: String,
    pub sw_crf: u32,
    /// 0 = automatico.
    pub threads: u32,
}

impl Default for NodeConfig {
    fn default() -> Self {
        NodeConfig {
            hw_backend: HwBackend::Auto,
            vaapi_device: "/dev/dri/renderD128".into(),
            max_hw_transcodes: 0,
            max_sw_transcodes: 0,
            sw_preset: "veryfast".into(),
            sw_crf: 23,
            threads: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushConfig {
    pub rev: i64,
    pub config: NodeConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Heartbeat {
    /// Sessioni che il server considera ancora vive su questo nodo: le altre il nodo le chiude.
    pub sessions: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Slots {
    pub hw_used: usize,
    pub hw_max: usize,
    pub sw_used: usize,
    pub sw_max: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RemoteSessionStatus {
    pub id: String,
    pub paused: bool,
    pub buffered_until_ms: Option<i64>,
    pub backend: String,
}

/// Stato del nodo: risposta a ogni heartbeat.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NodeStatus {
    pub version: String,
    pub protocol: u32,
    pub name: String,
    pub ffmpeg_version: String,
    pub ffmpeg_available: bool,
    pub ffmpeg_jellyfin: bool,
    pub caps: Capabilities,
    /// Profilo che il nodo userebbe adesso (GPU scelta, o software dopo troppi fallimenti).
    pub current: Option<HwProfile>,
    /// Codec video che il nodo sa codificare (software + GPU).
    pub encoders: Vec<String>,
    pub slots: Slots,
    pub cpus: usize,
    pub load_1m: Option<f64>,
    pub memory_total: Option<u64>,
    pub memory_available: Option<u64>,
    pub rss: Option<u64>,
    pub transcode_free_bytes: Option<u64>,
    pub sessions: Vec<RemoteSessionStatus>,
    /// Revisione della configurazione applicata (0 = mai ricevuta dopo l'avvio).
    pub config_rev: i64,
    pub config: Option<NodeConfig>,
    /// Indirizzo da cui il nodo vede arrivare il server: serve a costruire l'indirizzo della sorgente.
    pub peer_ip: Option<String>,
    pub media_roots: Vec<String>,
    pub uptime_ms: i64,
}

/// Da dove FFmpeg del nodo legge il video.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum Source {
    /// File visto dal nodo (cartella copiata o mount): sotto una delle radici ammesse.
    Path(String),
    /// Indirizzo http(s): il server (`/api/v1/node-source/…`) o un `.strm`.
    Url(String),
}

/// Apertura di una sessione HLS sul nodo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateHls {
    pub id: String,
    pub spec: JobSpec,
    pub hw: HwProfile,
    pub cuts: Vec<i64>,
    pub duration_ms: i64,
    pub source: Source,
    /// Rallentamento e pulizia come sul server.
    pub ahead_segments: usize,
    pub keep_behind: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateHlsReply {
    pub id: String,
    pub backend: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatReq {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatEntry {
    pub path: String,
    pub exists: bool,
    pub size: Option<u64>,
    pub is_dir: bool,
    pub error: Option<String>,
}
