//! Impostazioni tipizzate salvate nel database, un gruppo per riga.
//! Da ogni struttura si genera uno schema JSON: la dashboard disegna i moduli dallo schema,
//! quindi una nuova impostazione compare nell'interfaccia senza scrivere codice di interfaccia.
//! Le modifiche si applicano a caldo: chi legge prende sempre l'ultima copia in memoria.

use crate::db::Db;
use crate::util::now_ms;
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
#[schemars(title = "Generali")]
pub struct GeneralSettings {
    /// Nome del server mostrato ai client.
    #[schemars(title = "Nome del server")]
    pub server_name: String,
    /// Indirizzo pubblico del server, se raggiungibile da fuori (es. https://platea.example.com).
    #[schemars(title = "URL pubblico")]
    pub public_url: String,
    /// Lunghezza minima delle password.
    #[schemars(title = "Lunghezza minima password", range(min = 4, max = 128))]
    pub min_password_length: u32,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        GeneralSettings { server_name: "Platea".into(), public_url: String::new(), min_password_length: 8 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
#[schemars(title = "Rete")]
pub struct NetworkSettings {
    /// Origini consentite per le web UI ospitate altrove (CORS). `*` consente tutte.
    #[schemars(title = "Origini consentite")]
    pub allowed_origins: Vec<String>,
    /// Indirizzi dei reverse proxy fidati: solo da questi si legge X-Forwarded-For.
    #[schemars(title = "Proxy fidati")]
    pub trusted_proxies: Vec<String>,
    /// Reti considerate locali (CIDR). Le altre sono "da remoto".
    #[schemars(title = "Reti locali")]
    pub local_networks: Vec<String>,
    /// Limite di banda per stream da remoto, in Mb/s. 0 = nessun limite.
    #[schemars(title = "Bitrate massimo da remoto (Mb/s)")]
    pub remote_max_bitrate_mbps: u32,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        NetworkSettings {
            allowed_origins: vec!["*".into()],
            trusted_proxies: vec!["127.0.0.1".into(), "::1".into(), "172.16.0.0/12".into(), "10.0.0.0/8".into(), "192.168.0.0/16".into()],
            local_networks: vec!["10.0.0.0/8".into(), "172.16.0.0/12".into(), "192.168.0.0/16".into(), "127.0.0.0/8".into(), "::1/128".into(), "fc00::/7".into()],
            remote_max_bitrate_mbps: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
#[schemars(title = "Riproduzione")]
pub struct PlaybackSettings {
    /// Durata dei segmenti HLS in secondi.
    #[schemars(title = "Durata dei segmenti (s)", range(min = 2, max = 10))]
    pub segment_seconds: u32,
    /// FFmpeg viene sospeso quando è avanti di questo margine rispetto al punto di visione.
    #[schemars(title = "Margine di rallentamento (s)", range(min = 30, max = 1800))]
    pub throttle_ahead_seconds: u32,
    /// Una sessione senza richieste per questo tempo viene chiusa.
    #[schemars(title = "Chiusura sessione inattiva (s)", range(min = 15, max = 3600))]
    pub session_idle_seconds: u32,
    /// Quanti segmenti già visti tenere su disco dietro al punto di visione.
    #[schemars(title = "Segmenti conservati dietro la visione", range(min = 2, max = 500))]
    pub keep_segments_behind: u32,
    /// Percentuale oltre la quale un elemento è considerato visto.
    #[schemars(title = "Soglia \"visto\" (%)", range(min = 50, max = 100))]
    pub played_threshold_percent: u32,
    /// Sotto questa posizione (s) la ripresa non viene proposta.
    #[schemars(title = "Ripresa minima (s)", range(min = 0, max = 600))]
    pub resume_min_seconds: u32,
}

impl Default for PlaybackSettings {
    fn default() -> Self {
        PlaybackSettings {
            segment_seconds: 6,
            throttle_ahead_seconds: 180,
            session_idle_seconds: 90,
            keep_segments_behind: 20,
            played_threshold_percent: 90,
            resume_min_seconds: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum HwBackend {
    Auto,
    None,
    Qsv,
    Vaapi,
    Nvenc,
    Amf,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
#[schemars(title = "Transcodifica")]
pub struct TranscodeSettings {
    /// Percorso di ffmpeg. Vuoto = rilevamento automatico (jellyfin-ffmpeg, poi PATH).
    #[schemars(title = "Percorso di FFmpeg")]
    pub ffmpeg_path: String,
    /// Percorso di ffprobe. Vuoto = accanto a ffmpeg.
    #[schemars(title = "Percorso di ffprobe")]
    pub ffprobe_path: String,
    /// Backend hardware. Auto prova Intel, poi NVIDIA, poi AMD, poi software.
    #[schemars(title = "Accelerazione hardware")]
    pub hw_backend: HwBackend,
    /// Dispositivo per VA-API e QSV.
    #[schemars(title = "Dispositivo DRI")]
    pub vaapi_device: String,
    /// Transcodifiche contemporanee su GPU.
    #[schemars(title = "Transcodifiche hardware contemporanee", range(min = 1, max = 64))]
    pub max_hw_transcodes: u32,
    /// Transcodifiche contemporanee in software.
    #[schemars(title = "Transcodifiche software contemporanee", range(min = 1, max = 32))]
    pub max_sw_transcodes: u32,
    /// Preset x264 per la codifica software.
    #[schemars(title = "Preset software")]
    pub sw_preset: String,
    /// Qualità costante (CRF) per la codifica software, con tetto di bitrate.
    #[schemars(title = "CRF software", range(min = 16, max = 35))]
    pub sw_crf: u32,
    /// Thread di FFmpeg per una transcodifica software. 0 = automatico.
    #[schemars(title = "Thread per transcodifica", range(min = 0, max = 64))]
    pub threads: u32,
    /// Tone mapping da HDR a SDR quando il client non supporta l'HDR.
    #[schemars(title = "Tone mapping HDR")]
    pub enable_tonemap: bool,
    /// Uscita HEVC quando il client la dichiara e la GPU la codifica.
    #[schemars(title = "Uscita HEVC")]
    pub allow_hevc_output: bool,
    /// Uscita AV1 quando il client la dichiara e la GPU la codifica.
    #[schemars(title = "Uscita AV1")]
    pub allow_av1_output: bool,
}

impl Default for TranscodeSettings {
    fn default() -> Self {
        TranscodeSettings {
            ffmpeg_path: String::new(),
            ffprobe_path: String::new(),
            hw_backend: HwBackend::Auto,
            vaapi_device: "/dev/dri/renderD128".into(),
            max_hw_transcodes: 8,
            max_sw_transcodes: 2,
            sw_preset: "veryfast".into(),
            sw_crf: 23,
            threads: 0,
            enable_tonemap: true,
            allow_hevc_output: false,
            allow_av1_output: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
#[schemars(title = "Metadati")]
pub struct MetadataSettings {
    /// Chiave API di The Movie Database (v3) o token di lettura (v4).
    #[schemars(title = "Chiave TMDB")]
    pub tmdb_api_key: String,
    /// Lingua predefinita dei metadati per le nuove librerie.
    #[schemars(title = "Lingua predefinita")]
    pub language: String,
    /// Paese per le classificazioni per età.
    #[schemars(title = "Paese")]
    pub country: String,
    /// Sopra questo punteggio l'associazione è automatica; sotto, l'elemento va "da verificare".
    #[schemars(title = "Soglia di associazione automatica", range(min = 0.5, max = 1.0))]
    pub auto_match_threshold: f64,
    /// Richieste al secondo verso TMDB.
    #[schemars(title = "Richieste TMDB al secondo", range(min = 1, max = 40))]
    pub tmdb_requests_per_second: u32,
    /// Ogni quanti giorni aggiornare i metadati già presenti (0 = mai).
    #[schemars(title = "Aggiornamento metadati (giorni)", range(min = 0, max = 365))]
    pub refresh_days: u32,
    /// Larghezza massima delle immagini scaricate.
    #[schemars(title = "Larghezza locandine scaricate")]
    pub poster_download_width: u32,
}

impl Default for MetadataSettings {
    fn default() -> Self {
        MetadataSettings {
            tmdb_api_key: String::new(),
            language: "it-IT".into(),
            country: "IT".into(),
            auto_match_threshold: 0.8,
            tmdb_requests_per_second: 20,
            refresh_days: 30,
            poster_download_width: 780,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
#[schemars(title = "Scansione")]
pub struct ScannerSettings {
    /// Giorni prima di rimuovere un file scomparso. Lo stato di visione si conserva.
    #[schemars(title = "Periodo di grazia file mancanti (giorni)", range(min = 0, max = 365))]
    pub missing_grace_days: u32,
    /// Timeout di una singola operazione sul file system (s). Protegge dai mount bloccati.
    #[schemars(title = "Timeout operazioni su file (s)", range(min = 2, max = 600))]
    pub fs_timeout_seconds: u32,
    /// Timeout di un'analisi ffprobe (s).
    #[schemars(title = "Timeout analisi (s)", range(min = 5, max = 600))]
    pub probe_timeout_seconds: u32,
    /// Indirizzo del controllo remoto di rclone (es. http://rclone:5572). Vuoto = disattivato.
    #[schemars(title = "rclone rc URL")]
    pub rclone_rc_url: String,
}

impl Default for ScannerSettings {
    fn default() -> Self {
        ScannerSettings { missing_grace_days: 7, fs_timeout_seconds: 30, probe_timeout_seconds: 60, rclone_rc_url: String::new() }
    }
}

/// Campi segreti: si scrivono ma l'API non li restituisce.
pub const SECRET_FIELDS: &[(&str, &str)] = &[("metadata", "tmdb_api_key")];

/// Campi che richiedono un riavvio.
pub const RESTART_FIELDS: &[(&str, &str)] = &[];

#[derive(Debug, Clone, Default)]
pub struct AllSettings {
    pub general: GeneralSettings,
    pub network: NetworkSettings,
    pub playback: PlaybackSettings,
    pub transcode: TranscodeSettings,
    pub metadata: MetadataSettings,
    pub scanner: ScannerSettings,
}

pub const GROUPS: &[&str] = &["general", "network", "playback", "transcode", "metadata", "scanner"];

#[derive(Clone)]
pub struct Settings {
    inner: Arc<RwLock<Arc<AllSettings>>>,
    db: Db,
}

fn load_group<T: DeserializeOwned + Default>(rows: &[(String, String)], name: &str) -> T {
    rows.iter()
        .find(|(g, _)| g == name)
        .and_then(|(_, v)| serde_json::from_str(v).ok())
        .unwrap_or_default()
}

impl Settings {
    pub async fn load(db: Db) -> anyhow::Result<Settings> {
        let rows: Vec<(String, String)> = sqlx::query_as("SELECT grp, value FROM settings").fetch_all(&db.read).await?;
        let all = AllSettings {
            general: load_group(&rows, "general"),
            network: load_group(&rows, "network"),
            playback: load_group(&rows, "playback"),
            transcode: load_group(&rows, "transcode"),
            metadata: load_group(&rows, "metadata"),
            scanner: load_group(&rows, "scanner"),
        };
        Ok(Settings { inner: Arc::new(RwLock::new(Arc::new(all))), db })
    }

    /// Copia corrente: un puntatore, nessuna attesa.
    pub fn get(&self) -> Arc<AllSettings> {
        self.inner.read().expect("settings lock").clone()
    }

    pub fn schema() -> Value {
        let mut out = serde_json::Map::new();
        out.insert("general".into(), serde_json::to_value(schemars::schema_for!(GeneralSettings)).unwrap());
        out.insert("network".into(), serde_json::to_value(schemars::schema_for!(NetworkSettings)).unwrap());
        out.insert("playback".into(), serde_json::to_value(schemars::schema_for!(PlaybackSettings)).unwrap());
        out.insert("transcode".into(), serde_json::to_value(schemars::schema_for!(TranscodeSettings)).unwrap());
        out.insert("metadata".into(), serde_json::to_value(schemars::schema_for!(MetadataSettings)).unwrap());
        out.insert("scanner".into(), serde_json::to_value(schemars::schema_for!(ScannerSettings)).unwrap());
        // Segna i segreti e i campi che richiedono riavvio.
        for (grp, field) in SECRET_FIELDS {
            if let Some(p) = out.get_mut(*grp).and_then(|s| s.pointer_mut(&format!("/properties/{field}"))) {
                p["writeOnly"] = Value::Bool(true);
            }
        }
        for (grp, field) in RESTART_FIELDS {
            if let Some(p) = out.get_mut(*grp).and_then(|s| s.pointer_mut(&format!("/properties/{field}"))) {
                p["x-restart"] = Value::Bool(true);
            }
        }
        Value::Object(out)
    }

    /// Valori correnti di tutti i gruppi, con i segreti sostituiti da un indicatore.
    pub fn values_public(&self) -> Value {
        let all = self.get();
        let mut v = serde_json::json!({
            "general": all.general,
            "network": all.network,
            "playback": all.playback,
            "transcode": all.transcode,
            "metadata": all.metadata,
            "scanner": all.scanner,
        });
        for (grp, field) in SECRET_FIELDS {
            if let Some(g) = v.get_mut(*grp).and_then(|g| g.as_object_mut()) {
                let set = g.get(*field).and_then(|x| x.as_str()).is_some_and(|s| !s.is_empty());
                g.insert((*field).to_string(), Value::Null);
                g.insert(format!("{field}_set"), Value::Bool(set));
            }
        }
        v
    }

    fn group_value(all: &AllSettings, grp: &str) -> Option<Value> {
        Some(match grp {
            "general" => serde_json::to_value(&all.general).ok()?,
            "network" => serde_json::to_value(&all.network).ok()?,
            "playback" => serde_json::to_value(&all.playback).ok()?,
            "transcode" => serde_json::to_value(&all.transcode).ok()?,
            "metadata" => serde_json::to_value(&all.metadata).ok()?,
            "scanner" => serde_json::to_value(&all.scanner).ok()?,
            _ => return None,
        })
    }

    /// Aggiorna un gruppo. I campi assenti restano invariati; un segreto `null` resta invariato.
    /// Restituisce il valore precedente (per il registro delle attività).
    pub async fn update(&self, grp: &str, patch: Value) -> Result<(Value, Value), String> {
        let current = self.get();
        let mut merged = Self::group_value(&current, grp).ok_or_else(|| format!("Gruppo di impostazioni sconosciuto: {grp}"))?;
        let previous = merged.clone();
        let Value::Object(patch) = patch else {
            return Err("Il corpo deve essere un oggetto JSON".into());
        };
        let obj = merged.as_object_mut().unwrap();
        for (k, v) in patch {
            if k.ends_with("_set") {
                continue;
            }
            let is_secret = SECRET_FIELDS.iter().any(|(g, f)| *g == grp && *f == k);
            if is_secret && v.is_null() {
                continue;
            }
            if !obj.contains_key(&k) {
                return Err(format!("Impostazione sconosciuta: {grp}.{k}"));
            }
            obj.insert(k, v);
        }
        let mut next = (*current).clone();
        let parse_err = |e: serde_json::Error| format!("Valore non accettato: {e}");
        match grp {
            "general" => next.general = serde_json::from_value(merged.clone()).map_err(parse_err)?,
            "network" => next.network = serde_json::from_value(merged.clone()).map_err(parse_err)?,
            "playback" => next.playback = serde_json::from_value(merged.clone()).map_err(parse_err)?,
            "transcode" => next.transcode = serde_json::from_value(merged.clone()).map_err(parse_err)?,
            "metadata" => next.metadata = serde_json::from_value(merged.clone()).map_err(parse_err)?,
            "scanner" => next.scanner = serde_json::from_value(merged.clone()).map_err(parse_err)?,
            _ => unreachable!(),
        }
        validate(&next)?;
        sqlx::query(
            "INSERT INTO settings (grp, value, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(grp) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(grp)
        .bind(merged.to_string())
        .bind(now_ms())
        .execute(&self.db.write)
        .await
        .map_err(|e| format!("Salvataggio non riuscito: {e}"))?;
        *self.inner.write().expect("settings lock") = Arc::new(next);
        Ok((redact(grp, previous), redact(grp, merged)))
    }

    /// Esportazione completa (segreti inclusi: il file va custodito come un backup).
    pub fn export(&self) -> Value {
        let all = self.get();
        let mut out = serde_json::Map::new();
        for g in GROUPS {
            if let Some(v) = Self::group_value(&all, g) {
                out.insert((*g).to_string(), v);
            }
        }
        Value::Object(out)
    }
}

fn redact(grp: &str, mut v: Value) -> Value {
    for (g, f) in SECRET_FIELDS {
        if *g == grp {
            if let Some(o) = v.as_object_mut() {
                if o.contains_key(*f) {
                    o.insert((*f).to_string(), Value::String("•••".into()));
                }
            }
        }
    }
    v
}

fn validate(s: &AllSettings) -> Result<(), String> {
    let p = &s.playback;
    if !(2..=10).contains(&p.segment_seconds) {
        return Err("La durata dei segmenti deve stare tra 2 e 10 secondi".into());
    }
    if s.metadata.auto_match_threshold < 0.5 || s.metadata.auto_match_threshold > 1.0 {
        return Err("La soglia di associazione deve stare tra 0,5 e 1".into());
    }
    if s.transcode.max_sw_transcodes == 0 || s.transcode.max_hw_transcodes == 0 {
        return Err("Il numero di transcodifiche contemporanee deve essere almeno 1".into());
    }
    if s.general.min_password_length < 4 {
        return Err("La lunghezza minima della password è 4".into());
    }
    Ok(())
}
