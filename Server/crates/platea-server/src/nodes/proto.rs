//! Protocollo tra il server e i nodi: tipi scambiati in JSON su `/node/v1`.
//! Il server chiama il nodo, mai il contrario; la versione del protocollo cambia solo con modifiche incompatibili.

use crate::settings::HwBackend;
use crate::transcode::command::{HwProfile, JobSpec};
use crate::transcode::hw::Capabilities;
use serde::{Deserialize, Serialize};

/// Versione del protocollo. Un nodo con una versione diversa viene rifiutato con l'avviso "aggiorna il nodo".
pub const PROTOCOL: u32 = 1;

/// Come il nodo legge un file che non ha in copia (lo riceve dal server).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SourceMode {
    /// FFmpeg legge dal server in diretta, al ritmo della transcodifica.
    Stream,
    /// A blocchi in parallelo, in anticipo su FFmpeg (finestra `readahead_mb`), con cache su disco.
    #[default]
    Readahead,
    /// Tutto il file, a partire da dove si guarda, con cache su disco.
    Full,
}

/// Chi muove il file dal server al nodo.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SourceTransport {
    /// Il nodo legge dal server se la prova di collegamento riesce, altrimenti il server manda.
    #[default]
    Auto,
    /// Il nodo legge dal server (serve che il nodo raggiunga il server).
    Pull,
    /// Il server manda i blocchi al nodo sul collegamento che usa già per comandarlo (nodo dietro NAT, VPN a senso unico).
    Push,
}

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
    pub source_mode: SourceMode,
    pub source_transport: SourceTransport,
    /// Quanto leggere in anticipo rispetto a FFmpeg (modalità `readahead`).
    pub readahead_mb: u32,
    /// Blocchi scaricati insieme.
    pub source_parallel: u32,
    /// Tetto della cache dei file ricevuti dal server; 0 = niente cache tra una sessione e l'altra.
    pub source_cache_gb: u32,
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
            source_mode: SourceMode::Readahead,
            source_transport: SourceTransport::Auto,
            readahead_mb: 1024,
            source_parallel: 4,
            source_cache_gb: 50,
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
    /// Cache dei file ricevuti dal server: byte e file.
    pub source_cache_bytes: u64,
    pub source_cache_files: usize,
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
    /// Per `Source::Url` dal server: dimensione e identità del file (cache del nodo). Assenti = lettura in diretta.
    #[serde(default)]
    pub source_size: Option<u64>,
    #[serde(default)]
    pub source_key: Option<String>,
    /// I blocchi li manda il server (`/node/v1/push/…`): il nodo non deve raggiungere il server.
    #[serde(default)]
    pub source_push: bool,
    /// Rallentamento e pulizia come sul server.
    pub ahead_segments: usize,
    pub keep_behind: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateHlsReply {
    pub id: String,
    pub backend: String,
}

/// Prova di collegamento dal nodo verso il server: il nodo chiama `{url}/api/v1/health`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeReq {
    pub url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProbeReply {
    pub ok: bool,
    pub error: Option<String>,
    pub ms: u64,
}

/// Blocchi che il nodo aspetta dal server (modalità push). `gone` = sessione chiusa: il server smette di mandare.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PushWants {
    pub chunk_size: u64,
    pub chunks: Vec<u64>,
    pub complete: bool,
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
