//! Visione dal vivo. Il flusso della sorgente (MPEG-TS continuo) diventa HLS a finestra scorrevole: un solo FFmpeg
//! per canale e variante, condiviso da tutti quelli che lo guardano (una sola connessione al fornitore), fermato
//! 30 secondi dopo l'ultimo spettatore. Le app che leggono MPEG-TS (Infuse, Android) ricevono il flusso così com'è,
//! sempre attraverso Platea: l'indirizzo del fornitore non arriva mai al client.

use crate::state::Shared;
use crate::util::{now_ms, random_hex};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tokio::process::Child;

/// Cosa fa il server con il flusso.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Variant {
    /// Video e audio copiati nei segmenti.
    Copy,
    /// Video copiato, audio convertito in AAC (AC3, MP2 dei canali DVB).
    Audio,
    /// Video ricodificato in H.264 (MPEG-2, codec non supportati, banda limitata).
    Transcode,
    /// MPEG-TS passato così com'è (app che lo leggono da sole).
    Direct,
}

impl Variant {
    pub fn as_str(&self) -> &'static str {
        match self {
            Variant::Copy => "live_copy",
            Variant::Audio => "live_audio",
            Variant::Transcode => "live_transcode",
            Variant::Direct => "live_direct",
        }
    }
    /// Metodo equivalente per il pannello (stessi colori dei file).
    pub fn method(&self) -> &'static str {
        match self {
            Variant::Direct => "direct_play",
            Variant::Copy | Variant::Audio => "remux",
            Variant::Transcode => "transcode",
        }
    }
}

/// Un FFmpeg che produce l'HLS di un canale.
pub struct LiveStream {
    pub channel_id: i64,
    pub source_id: i64,
    pub variant: Variant,
    pub dir: PathBuf,
    input: String,
    max_height: u32,
    bitrate: u64,
    child: tokio::sync::Mutex<Option<Child>>,
    pub last_access: AtomicI64,
    pub restarts: AtomicU32,
    pub failed: AtomicBool,
    /// Ultima riga di errore di FFmpeg.
    pub error: Arc<Mutex<Option<String>>>,
}

pub struct LiveSession {
    pub id: String,
    pub token: String,
    pub user_id: i64,
    pub token_id: i64,
    pub username: String,
    pub device_name: String,
    pub client: String,
    pub ip: Option<String>,
    pub remote: bool,
    pub expires_at: Option<i64>,
    pub channel_id: i64,
    pub source_id: i64,
    pub channel_name: String,
    pub channel_number: Option<f64>,
    pub logo: bool,
    pub variant: Variant,
    pub chain: String,
    pub codecs: serde_json::Value,
    pub started_at: i64,
    pub last_seen: AtomicI64,
    pub bytes_sent: Arc<AtomicU64>,
    pub stream: Option<Arc<LiveStream>>,
    /// Passthrough: stesso flusso per una sola connessione alla volta.
    pub direct_url: Option<String>,
}

impl LiveSession {
    pub fn touch(&self) {
        self.last_seen.store(now_ms(), Ordering::Relaxed);
        if let Some(s) = &self.stream {
            s.last_access.store(now_ms(), Ordering::Relaxed);
        }
    }
}

#[derive(Default)]
pub struct LiveManager {
    streams: Mutex<HashMap<(i64, Variant, u32), Arc<LiveStream>>>,
    sessions: RwLock<HashMap<String, Arc<LiveSession>>>,
    /// Passthrough aperti per sorgente (per il limite di flussi del fornitore).
    direct_open: Mutex<HashMap<i64, usize>>,
}

pub struct DirectGuard {
    state: Shared,
    source_id: i64,
}

impl Drop for DirectGuard {
    fn drop(&mut self) {
        let mut m = self.state.live.direct_open.lock().unwrap();
        if let Some(n) = m.get_mut(&self.source_id) {
            *n = n.saturating_sub(1);
        }
    }
}

impl LiveManager {
    pub fn sessions(&self) -> Vec<Arc<LiveSession>> {
        self.sessions.read().unwrap().values().cloned().collect()
    }
    pub fn get(&self, id: &str) -> Option<Arc<LiveSession>> {
        self.sessions.read().unwrap().get(id).cloned()
    }
    pub fn user_count(&self, user_id: i64) -> usize {
        self.sessions.read().unwrap().values().filter(|s| s.user_id == user_id).count()
    }
    pub fn by_device(&self, token_id: i64) -> Vec<String> {
        self.sessions.read().unwrap().values().filter(|s| s.token_id == token_id).map(|s| s.id.clone()).collect()
    }
    pub fn insert(&self, s: Arc<LiveSession>) {
        self.sessions.write().unwrap().insert(s.id.clone(), s);
    }
    pub fn remove(&self, id: &str) -> Option<Arc<LiveSession>> {
        self.sessions.write().unwrap().remove(id)
    }
    /// Connessioni aperte verso una sorgente: un FFmpeg per canale e i passthrough.
    pub fn upstreams(&self, source_id: i64) -> usize {
        let ffmpeg =
            self.streams.lock().unwrap().values().filter(|s| s.source_id == source_id).map(|s| s.channel_id).collect::<std::collections::HashSet<_>>().len();
        ffmpeg + self.direct_open.lock().unwrap().get(&source_id).copied().unwrap_or(0)
    }
    /// Un canale ha già un FFmpeg acceso (vederlo non apre una connessione nuova).
    pub fn channel_running(&self, channel_id: i64) -> bool {
        self.streams.lock().unwrap().values().any(|s| s.channel_id == channel_id)
    }
    pub fn open_direct(state: &Shared, source_id: i64) -> DirectGuard {
        *state.live.direct_open.lock().unwrap().entry(source_id).or_insert(0) += 1;
        DirectGuard { state: state.clone(), source_id }
    }
}

/// Codec del flusso: dalla cache del canale (24 ore) o con ffprobe (pochi secondi di lettura).
pub async fn probe(state: &Shared, channel_id: i64, url: &str) -> anyhow::Result<serde_json::Value> {
    let cached: Option<String> =
        sqlx::query_scalar("SELECT codecs FROM tv_channels WHERE id = ?").bind(channel_id).fetch_optional(&state.db.read).await?.flatten();
    if let Some(v) = cached.as_deref().and_then(|c| serde_json::from_str::<serde_json::Value>(c).ok()) {
        if v["at"].as_i64().is_some_and(|t| now_ms() - t < 86_400_000) {
            return Ok(v);
        }
    }
    let ffprobe = state.ffmpeg.get().ffprobe.clone();
    let mut cmd = tokio::process::Command::new(ffprobe);
    cmd.args(["-v", "error", "-probesize", "4M", "-analyzeduration", "4M", "-rw_timeout", "35000000", "-print_format", "json", "-show_streams"])
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let child = cmd.spawn()?;
    let out = tokio::time::timeout(Duration::from_secs(40), child.wait_with_output()).await.map_err(|_| anyhow::anyhow!("il canale non risponde"))??;
    if !out.status.success() {
        anyhow::bail!("il canale non si apre: {}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("errore sconosciuto"));
    }
    let p: crate::probe::ProbeOutput = serde_json::from_slice(&out.stdout)?;
    let v = p.streams.iter().find(|s| s.codec_type == "video" && s.disposition.get("attached_pic").copied().unwrap_or(0) == 0);
    let a = p.streams.iter().find(|s| s.codec_type == "audio");
    let j = serde_json::json!({
        "video": v.map(|s| s.codec_name.clone()),
        "width": v.and_then(|s| s.width),
        "height": v.and_then(|s| s.height),
        "interlaced": v.and_then(|s| s.field_order.clone()).is_some_and(|f| f != "progressive" && f != "unknown"),
        "audio": a.map(|s| s.codec_name.clone()),
        "channels": a.and_then(|s| s.channels),
        "at": now_ms(),
    });
    sqlx::query("UPDATE tv_channels SET codecs = ? WHERE id = ?").bind(j.to_string()).bind(channel_id).execute(&state.db.write).await?;
    Ok(j)
}

/// Variante per il dispositivo: copia se il client legge i codec, altrimenti audio o video convertiti.
pub fn choose(codecs: &serde_json::Value, profile: &platea_playback::DeviceProfile, max_bitrate: Option<u64>, allow_transcode: bool, force: bool) -> Variant {
    let video = codecs["video"].as_str().unwrap_or("");
    let audio = codecs["audio"].as_str().unwrap_or("");
    let has = |list: &[String], c: &str| list.iter().any(|x| x.eq_ignore_ascii_case(c));
    let video_ok = video.is_empty() || has(&profile.hls_video_codecs, video);
    let audio_ok = audio.is_empty() || has(&profile.hls_audio_codecs, audio);
    // Una banda massima bassa (fuori casa) chiede la ricodifica anche se il video passerebbe.
    let slow = max_bitrate.is_some_and(|b| b < 6_000_000);
    if (force || !video_ok || slow) && allow_transcode && !video.is_empty() {
        return Variant::Transcode;
    }
    if !audio_ok {
        return Variant::Audio;
    }
    Variant::Copy
}

fn args(input: &str, v: Variant, dir: &std::path::Path, max_height: u32, bitrate: u64, interlaced: bool, threads: u32) -> Vec<String> {
    let mut a: Vec<String> = ["-hide_banner", "-loglevel", "error", "-nostdin", "-fflags", "+genpts+discardcorrupt"].iter().map(|s| s.to_string()).collect();
    if input.starts_with("http") {
        a.extend(
            ["-reconnect", "1", "-reconnect_streamed", "1", "-reconnect_on_network_error", "1", "-reconnect_delay_max", "5", "-rw_timeout", "40000000"]
                .map(String::from),
        );
        a.extend(["-user_agent".into(), format!("Platea/{}", crate::state::VERSION)]);
    }
    a.extend(["-i".into(), input.to_string(), "-map".into(), "0:v:0?".into(), "-map".into(), "0:a:0?".into(), "-sn".into(), "-dn".into()]);
    match v {
        Variant::Copy => a.extend(["-c:v", "copy", "-c:a", "copy"].map(String::from)),
        Variant::Audio => a.extend(["-c:v", "copy", "-c:a", "aac", "-ac", "2", "-b:a", "160k"].map(String::from)),
        Variant::Transcode | Variant::Direct => {
            let mut vf = String::new();
            if interlaced {
                vf.push_str("yadif=deint=interlaced,");
            }
            vf.push_str(&format!("scale=-2:'min({max_height},ih)',format=yuv420p"));
            let br = bitrate.max(1_000_000);
            a.extend(["-vf".into(), vf, "-c:v".into(), "libx264".into(), "-preset".into(), "veryfast".into(), "-profile:v".into(), "high".into()]);
            a.extend(["-b:v".into(), br.to_string(), "-maxrate".into(), br.to_string(), "-bufsize".into(), (br * 2).to_string()]);
            a.extend(["-g", "50", "-keyint_min", "50", "-sc_threshold", "0", "-force_key_frames", "expr:gte(t,n_forced*2)"].map(String::from));
            if threads > 0 {
                a.extend(["-threads".into(), threads.to_string()]);
            }
            a.extend(["-c:a", "aac", "-ac", "2", "-b:a", "160k"].map(String::from));
        }
    }
    a.extend(["-f", "hls", "-hls_time", "2", "-hls_list_size", "8", "-hls_delete_threshold", "4"].map(String::from));
    a.extend(["-hls_flags", "delete_segments+independent_segments+omit_endlist+program_date_time"].map(String::from));
    a.extend(["-hls_segment_filename".into(), dir.join("seg%d.ts").to_string_lossy().to_string(), dir.join("index.m3u8").to_string_lossy().to_string()]);
    a
}

impl LiveStream {
    async fn spawn(&self, state: &Shared, interlaced: bool) -> anyhow::Result<()> {
        let _ = tokio::fs::remove_dir_all(&self.dir).await;
        tokio::fs::create_dir_all(&self.dir).await?;
        let ff = state.ffmpeg.get().ffmpeg.clone();
        let threads = state.settings.get().transcode.threads;
        let mut cmd = tokio::process::Command::new(ff);
        cmd.args(args(&self.input, self.variant, &self.dir, self.max_height, self.bitrate, interlaced, threads))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = cmd.spawn()?;
        // Le ultime righe di errore di FFmpeg restano per il pannello e il player.
        if let Some(err) = child.stderr.take() {
            let me = self.error.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncBufReadExt;
                let mut lines = tokio::io::BufReader::new(err).lines();
                while let Ok(Some(l)) = lines.next_line().await {
                    tracing::debug!(line = %l, "ffmpeg (live)");
                    *me.lock().unwrap() = Some(l.chars().take(300).collect());
                }
            });
        }
        *self.child.lock().await = Some(child);
        self.failed.store(false, Ordering::Relaxed);
        Ok(())
    }

    pub fn last_error(&self) -> Option<String> {
        self.error.lock().unwrap().clone()
    }

    /// FFmpeg è uscito (flusso caduto, sorgente spenta).
    pub async fn exited(&self) -> bool {
        let mut c = self.child.lock().await;
        match c.as_mut() {
            Some(ch) => matches!(ch.try_wait(), Ok(Some(_))),
            None => true,
        }
    }

    pub async fn stop(&self) {
        if let Some(mut c) = self.child.lock().await.take() {
            let _ = c.kill().await;
        }
        let _ = tokio::fs::remove_dir_all(&self.dir).await;
    }

    /// Segmenti pronti nella playlist.
    pub async fn ready_segments(&self) -> usize {
        tokio::fs::read_to_string(self.dir.join("index.m3u8")).await.map(|t| t.lines().filter(|l| l.starts_with("#EXTINF")).count()).unwrap_or(0)
    }
}

/// Trova o avvia l'FFmpeg del canale per questa variante.
#[allow(clippy::too_many_arguments)]
pub async fn ensure_stream(
    state: &Shared,
    channel_id: i64,
    source_id: i64,
    input: &str,
    variant: Variant,
    max_height: u32,
    bitrate: u64,
    interlaced: bool,
) -> anyhow::Result<Arc<LiveStream>> {
    let key = (channel_id, variant, if variant == Variant::Transcode { max_height } else { 0 });
    let existing = state.live.streams.lock().unwrap().get(&key).cloned();
    if let Some(s) = existing {
        if !s.exited().await {
            s.last_access.store(now_ms(), Ordering::Relaxed);
            return Ok(s);
        }
        // Uscito: si riaccende al posto suo.
        s.restarts.fetch_add(1, Ordering::Relaxed);
        s.spawn(state, interlaced).await?;
        s.last_access.store(now_ms(), Ordering::Relaxed);
        return Ok(s);
    }
    let dir = state.boot.transcode_dir.join("live").join(format!("{channel_id}-{}-{}", variant.as_str(), random_hex(4)));
    let s = Arc::new(LiveStream {
        channel_id,
        source_id,
        variant,
        dir,
        input: input.to_string(),
        max_height,
        bitrate,
        child: tokio::sync::Mutex::new(None),
        last_access: AtomicI64::new(now_ms()),
        restarts: AtomicU32::new(0),
        failed: AtomicBool::new(false),
        error: Arc::new(Mutex::new(None)),
    });
    s.spawn(state, interlaced).await?;
    state.live.streams.lock().unwrap().insert(key, s.clone());
    tracing::info!(channel = channel_id, variant = variant.as_str(), "Live TV: canale acceso");
    Ok(s)
}

/// Attende che ci siano almeno due segmenti (o che FFmpeg esca con un errore).
pub async fn wait_ready(s: &LiveStream, timeout: Duration) -> anyhow::Result<()> {
    let t0 = std::time::Instant::now();
    while t0.elapsed() < timeout {
        if s.ready_segments().await >= 2 {
            return Ok(());
        }
        if s.exited().await {
            anyhow::bail!("il canale non trasmette: {}", s.last_error().unwrap_or_else(|| "flusso interrotto".into()));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    anyhow::bail!("il canale non ha prodotto video entro {} s", timeout.as_secs())
}

pub fn new_ids() -> (String, String) {
    (random_hex(12), random_hex(24))
}

/// Ogni 5 secondi: sessioni sparite (niente richieste da 60 s), canali senza spettatori da 30 s, FFmpeg caduti.
pub async fn run_maintenance(state: Shared) {
    let mut tick = tokio::time::interval(Duration::from_secs(5));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Cartelle rimaste da un'esecuzione precedente.
    let _ = tokio::fs::remove_dir_all(state.boot.transcode_dir.join("live")).await;
    loop {
        tick.tick().await;
        let now = now_ms();
        let idle: Vec<String> = state
            .live
            .sessions()
            .iter()
            .filter(|s| now - s.last_seen.load(Ordering::Relaxed) > 60_000 || s.expires_at.is_some_and(|e| now >= e))
            .map(|s| s.id.clone())
            .collect();
        for id in idle {
            stop_session(&state, &id);
        }
        let in_use: std::collections::HashSet<usize> =
            state.live.sessions().iter().filter_map(|s| s.stream.as_ref().map(|x| Arc::as_ptr(x) as usize)).collect();
        let dead: Vec<((i64, Variant, u32), Arc<LiveStream>)> = state
            .live
            .streams
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, s)| !in_use.contains(&(Arc::as_ptr(s) as usize)) && now - s.last_access.load(Ordering::Relaxed) > 30_000)
            .map(|(k, s)| (*k, s.clone()))
            .collect();
        for (k, s) in dead {
            state.live.streams.lock().unwrap().remove(&k);
            s.stop().await;
            tracing::info!(channel = s.channel_id, "Live TV: canale spento (nessuno spettatore)");
        }
    }
}

pub fn stop_session(state: &Shared, id: &str) -> Option<Arc<LiveSession>> {
    let s = state.live.remove(id)?;
    state.events.admin("sessions.changed", serde_json::json!({ "stopped": id }));
    tracing::info!(user = %s.username, channel = %s.channel_name, "Live TV: visione terminata");
    Some(s)
}

/// La sessione come riga del pannello (stessa forma delle riproduzioni dei file).
pub fn session_view(state: &Shared, s: &LiveSession, program: Option<String>) -> crate::sessions::SessionView {
    let now = now_ms();
    let video = s.codecs["video"].as_str().unwrap_or("").to_string();
    let audio = s.codecs["audio"].as_str().unwrap_or("").to_string();
    let out_video = match s.variant {
        Variant::Transcode => {
            serde_json::json!({ "action": "transcode", "codec": "h264", "max_width": 0, "max_height": 720, "bitrate": 4_000_000, "tonemap": false, "burn_subtitle": false })
        }
        _ => serde_json::json!({ "action": "copy" }),
    };
    let out_audio = match s.variant {
        Variant::Audio | Variant::Transcode => serde_json::json!({ "action": "transcode", "codec": "aac", "channels": 2, "bitrate": 160_000 }),
        _ => serde_json::json!({ "action": "copy" }),
    };
    crate::sessions::SessionView {
        id: s.id.clone(),
        user: s.username.clone(),
        item_id: String::new(),
        title: match s.channel_number {
            Some(n) => format!("{} · {}", if n.fract() == 0.0 { format!("{}", n as i64) } else { n.to_string() }, s.channel_name),
            None => s.channel_name.clone(),
        },
        subtitle: program,
        poster_url: s.logo.then(|| format!("/api/v1/livetv/channels/{}/logo", state.ids.encode(s.channel_id))),
        device_name: s.device_name.clone(),
        client: s.client.clone(),
        method: s.variant.method().to_string(),
        reasons: vec![s.variant.as_str().to_string()],
        chain: s.chain.clone(),
        bitrate: None,
        position_ms: now - s.started_at,
        duration_ms: 0,
        paused: false,
        throttled: false,
        started_at: s.started_at,
        server_now: now,
        position_at: now,
        paused_since: None,
        last_seen: s.last_seen.load(Ordering::Relaxed),
        ip: s.ip.clone(),
        remote: s.remote,
        details: serde_json::json!({
            "file": { "name": format!("Live TV · {}", s.channel_name), "size": 0, "container": "mpegts", "bitrate": null },
            "video": if video.is_empty() { serde_json::Value::Null } else { serde_json::json!({ "codec": video, "profile": null, "width": s.codecs["width"], "height": s.codecs["height"], "bit_depth": null, "hdr": null, "bitrate": null }) },
            "audio": if audio.is_empty() { serde_json::Value::Null } else { serde_json::json!({ "index": 0, "codec": audio, "channels": s.codecs["channels"].as_i64().unwrap_or(2), "language": null, "bitrate": null }) },
            "audio_tracks": 1,
            "subtitle": null,
            "subtitle_tracks": 0,
            "live": true,
            "out": { "video": out_video, "audio": out_audio, "subtitle": "none", "container": if s.variant == Variant::Direct { "file" } else { "hls_ts" } },
        }),
        buffered_until_ms: None,
        backend: Some(if s.variant == Variant::Transcode { "Software".into() } else { "copia".into() }),
        executor: None,
        source: None,
        bandwidth_bps: 0,
        bytes_sent: s.bytes_sent.load(Ordering::Relaxed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(video: &[&str], audio: &[&str]) -> platea_playback::DeviceProfile {
        platea_playback::DeviceProfile {
            hls_video_codecs: video.iter().map(|s| s.to_string()).collect(),
            hls_audio_codecs: audio.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn variants() {
        let p = profile(&["h264"], &["aac", "mp3"]);
        let c = |v: &str, a: &str| serde_json::json!({ "video": v, "audio": a });
        assert_eq!(choose(&c("h264", "aac"), &p, None, true, false), Variant::Copy);
        assert_eq!(choose(&c("h264", "ac3"), &p, None, true, false), Variant::Audio);
        assert_eq!(choose(&c("mpeg2video", "mp2"), &p, None, true, false), Variant::Transcode);
        assert_eq!(choose(&c("h264", "aac"), &p, Some(3_000_000), true, false), Variant::Transcode);
        // Senza permesso di transcodifica: si prova con l'audio convertito.
        assert_eq!(choose(&c("mpeg2video", "mp2"), &p, None, false, false), Variant::Audio);
    }

    #[test]
    fn ffmpeg_args() {
        let a = args("http://d/proxy/ts/stream/x", Variant::Copy, std::path::Path::new("/t/live/1"), 720, 4_000_000, false, 0).join(" ");
        assert!(a.contains("-c:v copy -c:a copy"));
        assert!(a.contains("-f hls -hls_time 2"));
        assert!(a.contains("-reconnect 1"));
        let t = args("udp://x", Variant::Transcode, std::path::Path::new("/t"), 720, 4_000_000, true, 2).join(" ");
        assert!(t.contains("yadif"));
        assert!(t.contains("libx264"));
        assert!(!t.contains("-reconnect"));
    }
}
