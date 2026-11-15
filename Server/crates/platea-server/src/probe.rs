//! Analisi tecnica dei file con ffprobe: codec, tracce, durata, HDR, keyframe.
//! Gira in coda con concorrenza limitata per libreria, con dimensione di sonda ridotta: bastano le intestazioni.
//! Il risultato vale finché la terna percorso, dimensione, data di modifica non cambia.

use crate::state::Shared;
use crate::util::now_ms;
use anyhow::{bail, Context};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

#[derive(Debug, Deserialize, Default)]
pub struct ProbeOutput {
    #[serde(default)]
    pub streams: Vec<ProbeStream>,
    #[serde(default)]
    pub format: ProbeFormat,
    #[serde(default)]
    pub chapters: Vec<ProbeChapter>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ProbeChapter {
    pub start_time: Option<String>,
    #[serde(default)]
    pub tags: HashMap<String, String>,
}

/// Capitoli in JSON compatto per il database: inizio in ms e titolo (i titoli generici "Chapter 01" restano vuoti).
pub fn chapters_json(chapters: &[ProbeChapter]) -> String {
    let list: Vec<serde_json::Value> = chapters
        .iter()
        .filter_map(|c| {
            let start = c.start_time.as_deref()?.parse::<f64>().ok()?;
            // Titoli generici ("Chapter 01", "Capitolo 3", "00:03:19.199" di mkvmerge) non dicono nulla: restano vuoti.
            let title = c.tags.get("title").map(|t| t.trim().to_string()).filter(|t| {
                let l = t.to_lowercase();
                let generic = l.split_whitespace().count() <= 2 && (l.starts_with("chapter") || l.starts_with("capitolo") || l.starts_with("chapitre"));
                let timestamp = t.chars().all(|c| c.is_ascii_digit() || c == ':' || c == '.');
                !t.is_empty() && !generic && !timestamp
            });
            Some(serde_json::json!({ "start_ms": (start * 1000.0).round() as i64, "title": title }))
        })
        .collect();
    serde_json::to_string(&list).unwrap_or_else(|_| "[]".into())
}

/// Solo i capitoli (lettura dell'intestazione): per i file analizzati prima che si salvassero.
pub async fn read_chapters(ffprobe: &PathBuf, path: &PathBuf, timeout: Duration) -> anyhow::Result<String> {
    let mut cmd = Command::new(ffprobe);
    cmd.args(["-v", "error", "-hide_banner", "-print_format", "json", "-show_chapters"])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let child = cmd.spawn().context("avvio di ffprobe")?;
    let out = tokio::time::timeout(timeout, child.wait_with_output()).await.context("ffprobe oltre il tempo limite")??;
    let parsed: ProbeOutput = serde_json::from_slice(&out.stdout).unwrap_or_default();
    Ok(chapters_json(&parsed.chapters))
}

#[derive(Debug, Deserialize, Default)]
pub struct ProbeFormat {
    #[serde(default)]
    pub format_name: String,
    pub duration: Option<String>,
    pub bit_rate: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ProbeStream {
    pub index: i64,
    #[serde(default)]
    pub codec_type: String,
    #[serde(default)]
    pub codec_name: String,
    pub profile: Option<String>,
    pub level: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub pix_fmt: Option<String>,
    pub bits_per_raw_sample: Option<String>,
    pub avg_frame_rate: Option<String>,
    pub r_frame_rate: Option<String>,
    pub color_transfer: Option<String>,
    pub channels: Option<i64>,
    pub channel_layout: Option<String>,
    pub sample_rate: Option<String>,
    pub bit_rate: Option<String>,
    #[serde(default)]
    pub tags: HashMap<String, String>,
    #[serde(default)]
    pub disposition: HashMap<String, i64>,
    #[serde(default)]
    pub side_data_list: Vec<serde_json::Value>,
}

impl ProbeStream {
    fn tag(&self, k: &str) -> Option<String> {
        self.tags.iter().find(|(key, _)| key.eq_ignore_ascii_case(k)).map(|(_, v)| v.clone())
    }

    pub fn bit_depth(&self) -> Option<i64> {
        if let Some(b) = self.bits_per_raw_sample.as_deref().and_then(|b| b.parse().ok()) {
            return Some(b);
        }
        let pf = self.pix_fmt.as_deref()?;
        Some(if pf.contains("12") {
            12
        } else if pf.contains("10") {
            10
        } else {
            8
        })
    }

    pub fn hdr(&self) -> Option<String> {
        let dovi = self
            .side_data_list
            .iter()
            .any(|s| s.get("side_data_type").and_then(|t| t.as_str()).is_some_and(|t| t.contains("DOVI") || t.contains("Dolby Vision")));
        if dovi || self.codec_name == "dvhe" || self.codec_name == "dvh1" {
            return Some("dovi".into());
        }
        let hdr10plus = self
            .side_data_list
            .iter()
            .any(|s| s.get("side_data_type").and_then(|t| t.as_str()).is_some_and(|t| t.contains("HDR10+") || t.contains("HDR Dynamic Metadata SMPTE2094-40")));
        match self.color_transfer.as_deref() {
            Some("smpte2084") if hdr10plus => Some("hdr10plus".into()),
            Some("smpte2084") => Some("hdr10".into()),
            Some("arib-std-b67") => Some("hlg".into()),
            _ => None,
        }
    }

    pub fn frame_rate(&self) -> Option<f64> {
        let fr = self.avg_frame_rate.as_deref().filter(|f| *f != "0/0").or(self.r_frame_rate.as_deref())?;
        let (n, d) = fr.split_once('/')?;
        let n: f64 = n.parse().ok()?;
        let d: f64 = d.parse().ok()?;
        (d > 0.0).then(|| n / d)
    }
}

/// Contenitore normalizzato dall'estensione e dal nome di formato di ffprobe.
pub fn normalize_container(ext: &str, format_name: &str) -> String {
    let ext = ext.to_ascii_lowercase();
    match ext.as_str() {
        "mkv" => "mkv".into(),
        "webm" => {
            if format_name.contains("webm") || format_name.contains("matroska") {
                "webm".into()
            } else {
                "mkv".into()
            }
        }
        "mp4" | "m4v" => "mp4".into(),
        "mov" => "mov".into(),
        "avi" | "divx" => "avi".into(),
        "ts" | "m2ts" | "mts" => "ts".into(),
        "mpg" | "mpeg" | "vob" => "mpeg".into(),
        "wmv" | "asf" => "asf".into(),
        "flv" => "flv".into(),
        "ogv" => "ogg".into(),
        "3gp" => "3gp".into(),
        other => {
            if format_name.contains("matroska") {
                "mkv".into()
            } else if format_name.contains("mp4") {
                "mp4".into()
            } else {
                other.to_string()
            }
        }
    }
}

/// Esegue ffprobe con un tempo limite. Il processo viene ucciso se il tempo scade.
pub async fn run_ffprobe(ffprobe: &PathBuf, path: &PathBuf, timeout: Duration) -> anyhow::Result<ProbeOutput> {
    let mut cmd = Command::new(ffprobe);
    cmd.args(["-v", "error", "-hide_banner", "-probesize", "20M", "-analyzeduration", "10M"])
        .args(["-print_format", "json", "-show_format", "-show_streams", "-show_chapters"])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let child = cmd.spawn().context("avvio di ffprobe")?;
    let out = match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(r) => r.context("esecuzione di ffprobe")?,
        Err(_) => bail!("ffprobe non ha risposto entro {} s", timeout.as_secs()),
    };
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("ffprobe: {}", err.lines().last().unwrap_or("errore sconosciuto"));
    }
    serde_json::from_slice(&out.stdout).context("uscita di ffprobe non leggibile")
}

/// Percorso assoluto di un file della libreria, verificando che resti sotto la radice.
pub async fn file_abs_path(state: &Shared, file_id: i64) -> anyhow::Result<Option<(PathBuf, String, i64, i64, i64)>> {
    let row: Option<(String, String, i64, i64, i64)> = sqlx::query_as(
        "SELECT p.path, f.relative_path, f.size, f.mtime, p.library_id FROM media_files f JOIN library_paths p ON p.id = f.library_path_id WHERE f.id = ?",
    )
    .bind(file_id)
    .fetch_optional(&state.db.read)
    .await?;
    let Some((root, rel, size, mtime, library_id)) = row else { return Ok(None) };
    if rel.split('/').any(|p| p == ".." || p.is_empty()) {
        bail!("percorso non valido nel database");
    }
    Ok(Some((PathBuf::from(&root).join(&rel), rel, size, mtime, library_id)))
}

pub async fn probe_file_job(state: &Shared, file_id: i64) -> anyhow::Result<()> {
    let Some((abs, rel, size, _mtime, library_id)) = file_abs_path(state, file_id).await? else { return Ok(()) };
    let probe_state: Option<String> =
        sqlx::query_scalar("SELECT probe_state FROM media_files WHERE id = ?").bind(file_id).fetch_optional(&state.db.read).await?;
    if probe_state.as_deref() == Some("done") {
        return Ok(());
    }
    let ffprobe = state.ffmpeg.get().ffprobe.clone();
    let timeout = Duration::from_secs(state.settings.get().scanner.probe_timeout_seconds.max(5) as u64);
    let t0 = std::time::Instant::now();
    // .strm: si analizza l'indirizzo che contiene, e lo si salva per la riproduzione.
    let input = match media_input(state, file_id, &abs).await {
        Ok(i) => i,
        Err(e) => {
            sqlx::query("UPDATE media_files SET probe_state = 'failed', probe_error = ?, probed_at = ? WHERE id = ?")
                .bind(format!("{e:#}"))
                .bind(now_ms())
                .bind(file_id)
                .execute(&state.db.write)
                .await?;
            return Err(e);
        }
    };
    let out = match run_ffprobe(&ffprobe, &input, timeout).await {
        Ok(o) => o,
        Err(e) => {
            sqlx::query("UPDATE media_files SET probe_state = 'failed', probe_error = ?, probed_at = ? WHERE id = ?")
                .bind(format!("{e:#}"))
                .bind(now_ms())
                .bind(file_id)
                .execute(&state.db.write)
                .await?;
            return Err(e);
        }
    };
    save_probe(state, file_id, &rel, &out).await?;
    tracing::debug!(file = %rel, ms = t0.elapsed().as_millis() as u64, "analisi completata");

    // MKV e MP4: l'indice dei keyframe viene dal contenitore (Cues, moov), costa poco anche sul cloud e accelera il primo remux.
    if !matches!(sqlx::query_scalar::<_, i64>("SELECT 1 FROM keyframes WHERE file_id = ?").bind(file_id).fetch_optional(&state.db.read).await, Ok(Some(_))) {
        if let Some(times) = container_keyframes(state, &abs, &rel, size).await {
            save_keyframes(state, file_id, &times).await?;
        }
    }
    // Indice dei keyframe per il remux: subito sui dischi locali se la libreria lo prevede.
    let lib: Option<(i64, String)> =
        sqlx::query_as("SELECT extract_keyframes, scan_profile FROM libraries WHERE id = ?").bind(library_id).fetch_optional(&state.db.read).await?;
    if let Some((kf, profile)) = lib {
        if kf != 0 && profile == "local" {
            crate::jobs::enqueue(state, crate::jobs::JobKind::ExtractKeyframes { file_id }, crate::jobs::priority::MAINTENANCE, Some(library_id)).await?;
        }
    }
    crate::trickplay::after_probe(state, file_id, library_id).await;
    state.events.publish("item.probed", serde_json::json!({ "file_id": state.ids.encode(file_id) }));
    Ok(())
}

pub async fn save_probe(state: &Shared, file_id: i64, rel: &str, out: &ProbeOutput) -> anyhow::Result<()> {
    let ext = platea_naming::extension(rel).unwrap_or_default();
    let container = normalize_container(&ext, &out.format.format_name);
    let duration_ms = out.format.duration.as_deref().and_then(|d| d.parse::<f64>().ok()).map(|d| (d * 1000.0) as i64);
    let bitrate = out.format.bit_rate.as_deref().and_then(|b| b.parse::<i64>().ok());
    let now = now_ms();

    let mut tx = state.db.write.begin().await?;
    sqlx::query("DELETE FROM media_streams WHERE file_id = ? AND is_external = 0").bind(file_id).execute(&mut *tx).await?;
    for s in &out.streams {
        let kind = match s.codec_type.as_str() {
            "video" => {
                // Le copertine incorporate sono tracce video "attached_pic": non contano.
                if s.disposition.get("attached_pic").copied().unwrap_or(0) == 1 {
                    "attachment"
                } else {
                    "video"
                }
            }
            "audio" => "audio",
            "subtitle" => "subtitle",
            "attachment" => "attachment",
            _ => "data",
        };
        sqlx::query(
            "INSERT INTO media_streams (file_id, stream_index, kind, codec, profile, level, bit_depth, width, height, frame_rate, hdr,
               channels, channel_layout, sample_rate, bitrate, language, title, is_default, is_forced, is_external)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0)",
        )
        .bind(file_id)
        .bind(s.index)
        .bind(kind)
        .bind(if s.codec_name.is_empty() { "unknown" } else { &s.codec_name })
        .bind(s.profile.as_ref().map(|p| p.to_lowercase()))
        .bind(s.level.filter(|l| *l > 0))
        .bind(if kind == "video" { s.bit_depth() } else { None })
        .bind(s.width)
        .bind(s.height)
        .bind(s.frame_rate())
        .bind(if kind == "video" { s.hdr() } else { None })
        .bind(s.channels)
        .bind(&s.channel_layout)
        .bind(s.sample_rate.as_deref().and_then(|r| r.parse::<i64>().ok()))
        .bind(s.bit_rate.as_deref().and_then(|b| b.parse::<i64>().ok()).or_else(|| s.tag("BPS").and_then(|b| b.parse().ok())))
        .bind(s.tag("language").filter(|l| l != "und"))
        .bind(s.tag("title"))
        .bind(s.disposition.get("default").copied().unwrap_or(0))
        .bind(s.disposition.get("forced").copied().unwrap_or(0))
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("UPDATE media_files SET container = ?, duration_ms = ?, bitrate = ?, chapters = ?, probe_state = 'done', probe_error = NULL, probed_at = ? WHERE id = ?")
        .bind(&container)
        .bind(duration_ms)
        .bind(bitrate)
        .bind(chapters_json(&out.chapters))
        .bind(now)
        .bind(file_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE items SET runtime_ms = COALESCE(runtime_ms, ?) WHERE id = (SELECT item_id FROM media_files WHERE id = ?)")
        .bind(duration_ms)
        .bind(file_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Ingresso per FFmpeg/ffprobe: il file stesso, oppure per un `.strm` l'indirizzo che contiene (salvato nel database).
pub async fn media_input(state: &Shared, file_id: i64, abs: &std::path::Path) -> anyhow::Result<PathBuf> {
    if !crate::strm::is_strm(abs) {
        return Ok(abs.to_path_buf());
    }
    let timeout = Duration::from_secs(state.settings.get().scanner.fs_timeout_seconds.max(2) as u64);
    let url = crate::strm::read_url(abs, timeout).await?;
    sqlx::query("UPDATE media_files SET stream_url = ? WHERE id = ?").bind(&url).bind(file_id).execute(&state.db.write).await?;
    Ok(PathBuf::from(url))
}

/// Analisi immediata con priorità massima: per un file riprodotto prima della sua analisi in coda.
pub async fn ensure_probed(state: &Shared, file_id: i64) -> anyhow::Result<()> {
    let st: Option<String> = sqlx::query_scalar("SELECT probe_state FROM media_files WHERE id = ?").bind(file_id).fetch_optional(&state.db.read).await?;
    match st.as_deref() {
        Some("done") => Ok(()),
        None => bail!("file non trovato"),
        _ => {
            let Some((abs, rel, ..)) = file_abs_path(state, file_id).await? else { bail!("file non trovato") };
            let ffprobe = state.ffmpeg.get().ffprobe.clone();
            let input = media_input(state, file_id, &abs).await?;
            let out = run_ffprobe(&ffprobe, &input, Duration::from_secs(60)).await?;
            save_probe(state, file_id, &rel, &out).await
        }
    }
}

/// Indice dei keyframe del primo video, in millisecondi. Legge i pacchetti senza decodificare.
pub async fn extract_keyframes_job(state: &Shared, file_id: i64) -> anyhow::Result<Vec<i64>> {
    if let Some(blob) =
        sqlx::query_scalar::<_, Vec<u8>>("SELECT times_ms FROM keyframes WHERE file_id = ?").bind(file_id).fetch_optional(&state.db.read).await?
    {
        return Ok(decode_keyframes(&blob));
    }
    let Some((abs, rel, size, _mtime, library_id)) = file_abs_path(state, file_id).await? else { bail!("file non trovato") };
    // MKV e MP4: l'indice del contenitore basta e costa poche letture.
    if let Some(times) = container_keyframes(state, &abs, &rel, size).await {
        save_keyframes(state, file_id, &times).await?;
        return Ok(times);
    }
    // Un .strm è un flusso in rete: leggerlo tutto per i keyframe non ha senso.
    if crate::strm::is_strm(&abs) {
        bail!("keyframe non disponibili per un .strm");
    }
    // Altrimenti si leggono i pacchetti, cioè tutto il file: solo sui dischi locali.
    let profile: Option<String> = sqlx::query_scalar("SELECT scan_profile FROM libraries WHERE id = ?").bind(library_id).fetch_optional(&state.db.read).await?;
    if profile.as_deref() == Some("remote") {
        bail!("keyframe non disponibili senza leggere tutto il file remoto");
    }
    let ffprobe = state.ffmpeg.get().ffprobe.clone();
    let mut cmd = Command::new(&ffprobe);
    cmd.args(["-v", "error", "-select_streams", "v:0", "-show_entries", "packet=pts_time,flags", "-of", "csv=print_section=0"])
        .arg(&abs)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let child = cmd.spawn().context("avvio di ffprobe per i keyframe")?;
    let out = tokio::time::timeout(Duration::from_secs(900), child.wait_with_output()).await.context("estrazione dei keyframe oltre il tempo limite")??;
    if !out.status.success() {
        bail!("ffprobe non è riuscito a leggere i keyframe");
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut times: Vec<i64> = text
        .lines()
        .filter_map(|l| {
            let (pts, flags) = l.split_once(',')?;
            if !flags.starts_with('K') {
                return None;
            }
            pts.parse::<f64>().ok().map(|t| (t * 1000.0).round() as i64)
        })
        .collect();
    times.sort_unstable();
    times.dedup();
    if times.is_empty() {
        bail!("nessun keyframe trovato");
    }
    save_keyframes(state, file_id, &times).await?;
    Ok(times)
}

async fn save_keyframes(state: &Shared, file_id: i64, times: &[i64]) -> anyhow::Result<()> {
    sqlx::query("INSERT OR REPLACE INTO keyframes (file_id, times_ms, extracted_at) VALUES (?, ?, ?)")
        .bind(file_id)
        .bind(encode_keyframes(times))
        .bind(now_ms())
        .execute(&state.db.write)
        .await?;
    Ok(())
}

/// Keyframe dall'indice del contenitore (Cues di un MKV, `moov` di un MP4/MOV), con un tempo limite:
/// poche letture anche su un mount cloud, e un mount lento non blocca nulla.
async fn container_keyframes(state: &Shared, abs: &std::path::Path, rel: &str, size: i64) -> Option<Vec<i64>> {
    let ext = platea_naming::extension(rel).unwrap_or_default().to_ascii_lowercase();
    let mkv = matches!(ext.as_str(), "mkv" | "webm" | "mk3d");
    if !mkv && !matches!(ext.as_str(), "mp4" | "m4v" | "mov") {
        return None;
    }
    let path = abs.to_path_buf();
    let timeout = Duration::from_secs(state.settings.get().scanner.fs_timeout_seconds.max(5) as u64 * 2);
    let t0 = std::time::Instant::now();
    let task = tokio::task::spawn_blocking(move || {
        let f = std::io::BufReader::with_capacity(64 * 1024, std::fs::File::open(&path).ok()?);
        if mkv {
            crate::mkv::cue_keyframes(f, size.max(0) as u64)
        } else {
            crate::mp4::sync_keyframes(f, size.max(0) as u64)
        }
    });
    let r = tokio::time::timeout(timeout, task).await.ok()?.ok()?;
    tracing::debug!(file = rel, ms = t0.elapsed().as_millis() as u64, keyframes = r.as_ref().map(|v| v.len()), "indice dei keyframe dal contenitore");
    r
}

pub fn encode_keyframes(times: &[i64]) -> Vec<u8> {
    let mut out = Vec::with_capacity(times.len() * 4);
    for t in times {
        out.extend_from_slice(&(*t.max(&0) as u32).to_le_bytes());
    }
    out
}

pub fn decode_keyframes(b: &[u8]) -> Vec<i64> {
    b.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c) as i64).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keyframes_roundtrip() {
        let v = vec![0, 2002, 4004, 10_000_000];
        assert_eq!(decode_keyframes(&encode_keyframes(&v)), v);
    }
    #[test]
    fn containers() {
        assert_eq!(normalize_container("mkv", "matroska,webm"), "mkv");
        assert_eq!(normalize_container("webm", "matroska,webm"), "webm");
        assert_eq!(normalize_container("m4v", "mov,mp4,m4a,3gp,3g2,mj2"), "mp4");
    }
    #[test]
    fn hdr_detection() {
        let s = ProbeStream { color_transfer: Some("smpte2084".into()), ..Default::default() };
        assert_eq!(s.hdr().as_deref(), Some("hdr10"));
        let s = ProbeStream { side_data_list: vec![serde_json::json!({"side_data_type": "DOVI configuration record"})], ..Default::default() };
        assert_eq!(s.hdr().as_deref(), Some("dovi"));
        let s = ProbeStream { pix_fmt: Some("yuv420p10le".into()), ..Default::default() };
        assert_eq!(s.bit_depth(), Some(10));
    }
}

#[cfg(test)]
mod chapter_tests {
    use super::*;

    #[test]
    fn generic_chapter_titles_are_dropped() {
        let ch = |t: &str, s: &str| ProbeChapter { start_time: Some(s.into()), tags: [("title".to_string(), t.to_string())].into_iter().collect() };
        let j = chapters_json(&[ch("00:03:19.199", "199.241"), ch("Chapter 02", "300"), ch("La rapina a Fort Knox", "600.5")]);
        let v: Vec<serde_json::Value> = serde_json::from_str(&j).unwrap();
        assert_eq!(v[0]["title"], serde_json::Value::Null);
        assert_eq!(v[0]["start_ms"], 199241);
        assert_eq!(v[1]["title"], serde_json::Value::Null);
        assert_eq!(v[2]["title"], "La rapina a Fort Knox");
    }
}
