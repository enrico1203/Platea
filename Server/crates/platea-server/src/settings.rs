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
    /// Ogni quante ore creare una copia di sicurezza automatica del database in data/backups (0 = mai).
    #[schemars(title = "Copia di sicurezza automatica (ore)", range(min = 0, max = 720))]
    pub backup_interval_hours: u32,
    /// Quante copie automatiche tenere; le manuali restano finché non le cancelli.
    #[schemars(title = "Copie automatiche da tenere", range(min = 1, max = 100))]
    pub backup_keep: u32,
    /// Ora (locale) della manutenzione notturna del database: statistiche, pulizia, spazio libero.
    #[schemars(title = "Ora della manutenzione del database", range(min = 0, max = 23))]
    pub maintenance_hour: u32,
    /// Accesso con un codice di 6 cifre confermato da un dispositivo già collegato (televisori, app).
    #[schemars(title = "Quick Connect")]
    pub quick_connect: bool,
    /// Dopo questo numero di password sbagliate di fila l'account viene disattivato finché un amministratore non lo
    /// riattiva (0 = mai). Gli amministratori non si bloccano: restano solo rallentati.
    #[schemars(title = "Blocco dopo tentativi falliti", range(min = 0, max = 100))]
    pub lockout_after_failures: u32,
    /// Testo mostrato sopra il modulo di accesso (avvisi, contatti). Testo semplice.
    #[schemars(title = "Messaggio nella pagina di accesso")]
    pub login_message: String,
    /// CSS aggiunto alla web UI per tutti gli utenti.
    #[schemars(title = "CSS personalizzato")]
    pub custom_css: String,
    /// Il nome del server (e il logo, se caricato) prende il posto di Platea ovunque: web UI, icona dell'app,
    /// pagina Informazioni, documentazione dell'API, client compatibili con Jellyfin, notifiche.
    #[schemars(title = "Nascondi Platea")]
    pub white_label: bool,
    /// Indirizzo del codice sorgente, mostrato nella pagina Informazioni. Con la licenza AGPL va indicato se si
    /// modifica il codice e lo si offre ad altri via rete.
    #[schemars(title = "Indirizzo del codice sorgente")]
    pub source_url: String,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        GeneralSettings {
            server_name: "Platea".into(),
            public_url: String::new(),
            min_password_length: 8,
            backup_interval_hours: 24,
            backup_keep: 7,
            maintenance_hour: 4,
            quick_connect: true,
            lockout_after_failures: 0,
            login_message: String::new(),
            custom_css: String::new(),
            white_label: false,
            source_url: String::new(),
        }
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
            local_networks: vec![
                "10.0.0.0/8".into(),
                "172.16.0.0/12".into(),
                "192.168.0.0/16".into(),
                "127.0.0.0/8".into(),
                "::1/128".into(),
                "fc00::/7".into(),
            ],
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
    /// Segmenti fMP4 anche quando il video copiato è H.264 (HEVC e AV1 usano fMP4, tranne per le app che li chiedono in MPEG-TS).
    #[schemars(title = "fMP4 anche per H.264")]
    pub fmp4_always: bool,
    /// Ogni quanti secondi una miniatura sulla barra di avanzamento (librerie con le miniature attive).
    #[schemars(title = "Intervallo delle miniature (s)", range(min = 2, max = 60))]
    pub trickplay_interval_seconds: u32,
    /// Larghezza delle miniature in pixel.
    #[schemars(title = "Larghezza delle miniature", range(min = 160, max = 640))]
    pub trickplay_width: u32,
    /// Thread di FFmpeg per la generazione delle miniature (un file alla volta).
    #[schemars(title = "Thread per le miniature", range(min = 1, max = 16))]
    pub trickplay_threads: u32,
    /// Le miniature aspettano quando qualcuno sta guardando.
    #[schemars(title = "Miniature in pausa durante le riproduzioni")]
    pub trickplay_pause_on_play: bool,
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
            fmp4_always: false,
            trickplay_interval_seconds: 10,
            trickplay_width: 320,
            trickplay_threads: 2,
            trickplay_pause_on_play: true,
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
    /// Transcodifiche contemporanee su GPU. 0 = illimitate.
    #[schemars(title = "Transcodifiche hardware contemporanee", range(min = 0, max = 64))]
    pub max_hw_transcodes: u32,
    /// Transcodifiche contemporanee in software. 0 = illimitate (ogni sessione si ferma quando è avanti di qualche
    /// minuto, quindi in media pesa quanto la visione in tempo reale).
    #[schemars(title = "Transcodifiche software contemporanee", range(min = 0, max = 64))]
    pub max_sw_transcodes: u32,
    /// Preset x264 per la codifica software. Più veloce = più transcodifiche insieme, a parità di qualità più
    /// bitrate: su 8 core un 1080p va a circa 5× in veryfast, 8× in superfast, 13× in ultrafast (più sgranato).
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
            max_hw_transcodes: 0,
            max_sw_transcodes: 0,
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
    /// Legge i file .nfo accanto ai video (Kodi, Jellyfin, Radarr): identificativo esatto e, senza chiave TMDB, i metadati.
    /// I file si leggono una volta sola e non si scrivono mai.
    #[schemars(title = "Leggi i file NFO")]
    pub read_nfo: bool,
    /// Legge i file .plexmatch (formato di Plex) nelle cartelle di film e serie: titolo, anno e identificativi scelti dall'utente.
    #[schemars(title = "Leggi i file .plexmatch")]
    pub read_plexmatch: bool,
    /// Chiave API di opensubtitles.com (gratuita, da "API consumers" nel proprio profilo): abilita la ricerca dei sottotitoli.
    #[schemars(title = "Chiave API OpenSubtitles")]
    pub opensubtitles_api_key: String,
    /// Account opensubtitles.com (facoltativo): più download al giorno.
    #[schemars(title = "Utente OpenSubtitles")]
    pub opensubtitles_username: String,
    #[schemars(title = "Password OpenSubtitles")]
    pub opensubtitles_password: String,
    /// Cerca gli audiolibri nel catalogo pubblico di Audible (titolo, autore, ASIN): copertine grandi, trama, lettori,
    /// serie, generi e voto. Spento, valgono solo i metadati locali (metadata.json, NFO, tag dei file, nomi).
    #[schemars(title = "Audiolibri: cerca su Audible")]
    pub audible: bool,
    /// Catalogo Audible da usare: "auto" segue il paese della libreria; altrimenti it, us, uk, de, fr, es, ca, au, jp, in, br.
    #[schemars(title = "Audiolibri: catalogo Audible")]
    pub audible_region: String,
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
            read_nfo: true,
            read_plexmatch: true,
            opensubtitles_api_key: String::new(),
            opensubtitles_username: String::new(),
            opensubtitles_password: String::new(),
            audible: true,
            audible_region: "auto".into(),
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
    /// Sparizione di massa: se in una scansione sparisce almeno questa percentuale dei file…
    #[schemars(title = "Soglia sparizione di massa (%)", range(min = 1, max = 100))]
    pub mass_missing_percent: u32,
    /// …e almeno questo numero di file, nulla viene rimosso finché l'amministratore non conferma.
    #[schemars(title = "Minimo file per la sparizione di massa", range(min = 1, max = 100000))]
    pub mass_missing_min_files: u32,
    /// Notifiche del file system sulle librerie locali: un file aggiunto o tolto fa scansionare subito la sua cartella.
    #[schemars(title = "Notifiche sui dischi locali")]
    pub watch_local: bool,
    /// Secondi di quiete dopo l'ultimo cambio prima della scansione (una copia lunga genera eventi continui).
    #[schemars(title = "Attesa dopo un cambio (s)", range(min = 1, max = 600))]
    pub watch_debounce_seconds: u32,
    /// Cartelle lette in parallelo durante la scansione di una libreria locale.
    #[schemars(title = "Cartelle in parallelo (locale)", range(min = 1, max = 64))]
    pub list_concurrency_local: u32,
    /// Cartelle lette in parallelo durante la scansione di una libreria remota: ogni cartella fuori cache è una richiesta al provider.
    #[schemars(title = "Cartelle in parallelo (remoto)", range(min = 1, max = 64))]
    pub list_concurrency_remote: u32,
    /// Indirizzo del controllo remoto di rclone (es. http://rclone:5572). Vuoto = disattivato.
    #[schemars(title = "rclone rc URL")]
    pub rclone_rc_url: String,
    /// Dove il server vede la radice del mount rclone (es. /media/cloud). Vuoto = la radice di ogni libreria.
    #[schemars(title = "Percorso del mount rclone")]
    pub rclone_mount_path: String,
    /// Più mount rclone, ognuno con il suo controllo remoto: `percorso=indirizzo`, es.
    /// `/media/film=http://host.docker.internal:8573`. Prima di scansionare una cartella si aggiorna il mount
    /// che la contiene (il percorso più lungo vince), anche per le librerie con più cartelle su mount diversi.
    #[schemars(title = "Mount rclone (percorso=indirizzo rc)")]
    pub rclone_mounts: Vec<String>,
    /// Tempo massimo per l'aggiornamento dell'elenco da parte di rclone (s).
    #[schemars(title = "Timeout aggiornamento rclone (s)", range(min = 10, max = 3600))]
    pub rclone_refresh_timeout_seconds: u32,
}

impl Default for ScannerSettings {
    fn default() -> Self {
        ScannerSettings {
            missing_grace_days: 7,
            fs_timeout_seconds: 30,
            probe_timeout_seconds: 60,
            watch_local: true,
            watch_debounce_seconds: 10,
            mass_missing_percent: 20,
            mass_missing_min_files: 10,
            list_concurrency_local: 4,
            list_concurrency_remote: 8,
            rclone_rc_url: String::new(),
            rclone_mount_path: String::new(),
            rclone_mounts: vec![],
            rclone_refresh_timeout_seconds: 600,
        }
    }
}

/// Campi segreti: si scrivono ma l'API non li restituisce.
pub const SECRET_FIELDS: &[(&str, &str)] = &[("metadata", "tmdb_api_key"), ("metadata", "opensubtitles_api_key"), ("metadata", "opensubtitles_password")];

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
#[schemars(title = "Notifiche")]
pub struct NotificationSettings {
    /// Indirizzi a cui inviare gli eventi con una richiesta POST in JSON. Un indirizzo di Discord riceve un messaggio
    /// già formattato; per ntfy usa l'indirizzo dell'argomento (es. https://ntfy.sh/mio-argomento).
    #[schemars(title = "Webhook")]
    pub webhook_urls: Vec<String>,
    /// Inizio e fine delle riproduzioni.
    #[schemars(title = "Riproduzioni")]
    pub on_playback: bool,
    /// Film e serie nuovi in libreria (alla prima associazione).
    #[schemars(title = "Titoli nuovi")]
    pub on_item_added: bool,
    /// Accessi, accessi non riusciti, account bloccati.
    #[schemars(title = "Accessi")]
    pub on_login: bool,
    /// Mount assente, sparizione di massa in attesa di conferma, scansioni non riuscite.
    #[schemars(title = "Problemi")]
    pub on_problem: bool,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        NotificationSettings { webhook_urls: vec![], on_playback: false, on_item_added: true, on_login: false, on_problem: true }
    }
}

/// Campi che richiedono un riavvio.
pub const RESTART_FIELDS: &[(&str, &str)] = &[];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeStrategy {
    /// Sessioni in corso divise in proporzione ai pesi.
    Weighted,
    /// Chi ha meno lavoro in rapporto ai propri posti.
    LeastLoaded,
    /// Sempre il primo dell'elenco (per priorità, poi per peso) finché ha posto.
    Ordered,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
#[schemars(title = "Nodi")]
pub struct NodeSettings {
    /// Indirizzo con cui i nodi raggiungono questo server per leggere i file (es. http://100.64.0.1:8096).
    /// Vuoto = l'indirizzo da cui il nodo vede arrivare il server, con la porta qui sotto.
    #[schemars(title = "Indirizzo del server per i nodi")]
    pub server_url: String,
    /// Porta del server vista dai nodi quando l'indirizzo è automatico (quella pubblicata da Docker).
    #[schemars(title = "Porta del server per i nodi", range(min = 1, max = 65535))]
    pub server_port: u32,
    /// Come dividere le transcodifiche tra esecutori con la stessa priorità.
    #[schemars(title = "Bilanciamento")]
    pub strategy: NodeStrategy,
    /// A parità di priorità, prima chi ha il file in una cartella propria (non lo riceve dal server).
    #[schemars(title = "Preferisci chi ha il file in locale")]
    pub prefer_local_copy: bool,
    /// Se nessun esecutore accetta il lavoro (regole, nodi spenti o pieni), transcodifica comunque questo server.
    #[schemars(title = "Ripiego su questo server")]
    pub fallback_local: bool,
    /// Ogni quanti secondi il server controlla i nodi.
    #[schemars(title = "Controllo dei nodi (s)", range(min = 2, max = 60))]
    pub heartbeat_seconds: u32,
    /// Controlli falliti di fila dopo cui un nodo è considerato offline.
    #[schemars(title = "Offline dopo controlli falliti", range(min = 1, max = 20))]
    pub offline_after: u32,
    /// Direct play e download con un link firmato verso il nodo, invece di passare dal server. Solo per i nodi con un
    /// indirizzo pubblico impostato; consuma la banda del nodo invece di quella del server.
    #[schemars(title = "Consegna diretta dai nodi")]
    pub direct_delivery: bool,
}

impl Default for NodeSettings {
    fn default() -> Self {
        NodeSettings {
            server_url: String::new(),
            server_port: 8096,
            strategy: NodeStrategy::Weighted,
            prefer_local_copy: true,
            fallback_local: true,
            heartbeat_seconds: 5,
            offline_after: 3,
            direct_delivery: false,
        }
    }
}

/// Campi di testo lunghi: nella dashboard diventano aree di testo.
pub const MULTILINE_FIELDS: &[(&str, &str)] = &[("general", "login_message"), ("general", "custom_css")];

#[derive(Debug, Clone, Default)]
pub struct AllSettings {
    pub general: GeneralSettings,
    pub network: NetworkSettings,
    pub playback: PlaybackSettings,
    pub transcode: TranscodeSettings,
    pub metadata: MetadataSettings,
    pub scanner: ScannerSettings,
    pub notifications: NotificationSettings,
    pub nodes: NodeSettings,
}

pub const GROUPS: &[&str] = &["general", "network", "playback", "transcode", "metadata", "scanner", "notifications", "nodes"];

#[derive(Clone)]
pub struct Settings {
    inner: Arc<RwLock<Arc<AllSettings>>>,
    db: Db,
}

fn load_group<T: DeserializeOwned + Default>(rows: &[(String, String)], name: &str) -> T {
    rows.iter().find(|(g, _)| g == name).and_then(|(_, v)| serde_json::from_str(v).ok()).unwrap_or_default()
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
            notifications: load_group(&rows, "notifications"),
            nodes: load_group(&rows, "nodes"),
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
        out.insert("notifications".into(), serde_json::to_value(schemars::schema_for!(NotificationSettings)).unwrap());
        out.insert("nodes".into(), serde_json::to_value(schemars::schema_for!(NodeSettings)).unwrap());
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
        for (grp, field) in MULTILINE_FIELDS {
            if let Some(p) = out.get_mut(*grp).and_then(|s| s.pointer_mut(&format!("/properties/{field}"))) {
                p["x-multiline"] = Value::Bool(true);
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
            "notifications": all.notifications,
            "nodes": all.nodes,
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
            "notifications" => serde_json::to_value(&all.notifications).ok()?,
            "nodes" => serde_json::to_value(&all.nodes).ok()?,
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
            "notifications" => next.notifications = serde_json::from_value(merged.clone()).map_err(parse_err)?,
            "nodes" => next.nodes = serde_json::from_value(merged.clone()).map_err(parse_err)?,
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
    // max_sw_transcodes / max_hw_transcodes: 0 = illimitate, ogni valore è valido.
    for u in &s.notifications.webhook_urls {
        if !(u.starts_with("http://") || u.starts_with("https://")) {
            return Err(format!("Webhook non valido (serve http:// o https://): {u}"));
        }
    }
    if s.notifications.webhook_urls.len() > 10 {
        return Err("Al massimo 10 webhook".into());
    }
    if s.general.login_message.chars().count() > 2000 {
        return Err("Il messaggio della pagina di accesso può avere al massimo 2000 caratteri".into());
    }
    if s.general.custom_css.len() > 64 * 1024 {
        return Err("Il CSS personalizzato può essere al massimo di 64 KB".into());
    }
    let u = s.nodes.server_url.trim();
    if !u.is_empty() && !(u.starts_with("http://") || u.starts_with("https://")) {
        return Err("Indirizzo del server per i nodi non valido (serve http:// o https://)".into());
    }
    if s.general.min_password_length < 4 {
        return Err("La lunghezza minima della password è 4".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// I valori predefiniti devono passare la validazione: altrimenti nessun gruppo si potrebbe salvare
    /// (la validazione guarda tutte le impostazioni, non solo il gruppo modificato).
    #[test]
    fn defaults_are_valid() {
        assert_eq!(validate(&AllSettings::default()), Ok(()));
    }
}
