//! Miniature per la barra di avanzamento (trickplay).
//! Per ogni file una serie di griglie JPEG (10×10 miniature, una ogni N secondi) in `cache/trickplay/<file_id>/`,
//! mai accanto ai media. Si decodificano solo i keyframe (`-skip_frame nokey`): un film locale richiede pochi secondi,
//! ma il file va letto tutto, quindi sulle librerie remote è un'opzione da scegliere consapevolmente.
//! Un file alla volta, a bassa priorità di CPU, in pausa durante le riproduzioni e nella fascia oraria delle analisi.

use crate::state::Shared;
use crate::util::now_ms;
use anyhow::{bail, Context};
use serde::Serialize;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

pub const COLS: u32 = 10;
pub const ROWS: u32 = 10;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Manifest {
    pub interval_ms: i64,
    pub width: i64,
    pub height: i64,
    pub cols: i64,
    pub rows: i64,
    pub count: i64,
    pub sheets: i64,
}

pub fn root(state: &Shared) -> PathBuf {
    state.boot.cache_dir.join("trickplay")
}

pub fn dir(state: &Shared, file_id: i64) -> PathBuf {
    root(state).join(file_id.to_string())
}

pub async fn manifest(state: &Shared, file_id: i64) -> Option<Manifest> {
    sqlx::query_as("SELECT interval_ms, width, height, cols, rows, count, sheets FROM trickplay WHERE file_id = ?")
        .bind(file_id)
        .fetch_optional(&state.db.read)
        .await
        .ok()
        .flatten()
}

/// Mette in coda i file della libreria che non hanno ancora le miniature (o le hanno con altri parametri).
pub async fn enqueue_library(state: &Shared, library_id: i64) -> anyhow::Result<usize> {
    let s = state.settings.get();
    let interval = s.playback.trickplay_interval_seconds.clamp(2, 60) as i64 * 1000;
    let width = s.playback.trickplay_width.clamp(160, 640) as i64;
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT m.id FROM media_files m JOIN library_paths lp ON lp.id = m.library_path_id
         LEFT JOIN trickplay t ON t.file_id = m.id
         WHERE lp.library_id = ? AND m.state = 'present' AND m.probe_state = 'done' AND m.item_id IS NOT NULL
           AND (t.file_id IS NULL OR t.interval_ms != ? OR t.width != ?)",
    )
    .bind(library_id)
    .bind(interval)
    .bind(width)
    .fetch_all(&state.db.read)
    .await?;
    let n = ids.len();
    let jobs = ids
        .into_iter()
        .map(|file_id| (crate::jobs::JobKind::GenerateTrickplay { file_id }, crate::jobs::priority::MAINTENANCE + 5, Some(library_id)))
        .collect();
    crate::jobs::enqueue_batch(state, jobs).await?;
    Ok(n)
}

/// Dopo l'analisi di un file: miniature in coda se la libreria le prevede.
pub async fn after_probe(state: &Shared, file_id: i64, library_id: i64) {
    let on: Option<i64> =
        sqlx::query_scalar("SELECT trickplay FROM libraries WHERE id = ?").bind(library_id).fetch_optional(&state.db.read).await.ok().flatten();
    if on.unwrap_or(0) != 0 && manifest(state, file_id).await.is_none() {
        let job = crate::jobs::JobKind::GenerateTrickplay { file_id };
        if let Err(e) = crate::jobs::enqueue(state, job, crate::jobs::priority::MAINTENANCE + 5, Some(library_id)).await {
            tracing::warn!(error = %e, "miniature non accodate");
        }
    }
}

pub async fn generate_job(state: &Shared, file_id: i64) -> anyhow::Result<()> {
    let s = state.settings.get();
    let interval_s = s.playback.trickplay_interval_seconds.clamp(2, 60);
    let width = s.playback.trickplay_width.clamp(160, 640) & !1;
    let threads = s.playback.trickplay_threads.clamp(1, 16);
    let tonemap_on = s.transcode.enable_tonemap;
    drop(s);
    if let Some(m) = manifest(state, file_id).await {
        if m.interval_ms == interval_s as i64 * 1000 && m.width == width as i64 {
            return Ok(());
        }
    }
    crate::probe::ensure_probed(state, file_id).await?;
    let Some((abs, rel, ..)) = crate::probe::file_abs_path(state, file_id).await? else { bail!("file non trovato") };
    let video: Option<(Option<String>,)> =
        sqlx::query_as("SELECT hdr FROM media_streams WHERE file_id = ? AND kind = 'video' AND is_external = 0 ORDER BY stream_index LIMIT 1")
            .bind(file_id)
            .fetch_optional(&state.db.read)
            .await?;
    let Some((hdr,)) = video else { bail!("nessuna traccia video") };
    let duration_ms: Option<i64> = sqlx::query_scalar("SELECT duration_ms FROM media_files WHERE id = ?").bind(file_id).fetch_one(&state.db.read).await?;
    let duration_ms = duration_ms.filter(|d| *d > 0).context("durata sconosciuta")?;

    let ff = state.ffmpeg.get();
    // Dimensioni con l'aspetto di visualizzazione (DVD anamorfici compresi), sempre pari.
    let mut vf = format!("fps=1/{interval_s}:start_time=0,scale={width}:'trunc(ow/dar/2)*2',setsar=1");
    if tonemap_on && hdr.is_some() {
        vf.push(',');
        vf.push_str(crate::transcode::command::sw_tonemap(ff.is_jellyfin));
    }
    vf.push_str(&format!(",format=yuvj420p,tile={COLS}x{ROWS}"));

    let final_dir = dir(state, file_id);
    let tmp = root(state).join(format!("{file_id}.tmp-{}", crate::util::random_hex(4)));
    let t0 = std::time::Instant::now();
    // Prima solo i keyframe; se il decoder li restituisce fuori ordine (certi HEVC a GOP aperto) e non esce nulla,
    // si ripete decodificando tutto: più lento, ma sempre corretto.
    let mut outcome = Err(anyhow::anyhow!("nessuna miniatura prodotta"));
    for keyframes_only in [true, false] {
        let _ = tokio::fs::remove_dir_all(&tmp).await;
        tokio::fs::create_dir_all(&tmp).await?;
        outcome = run_ffmpeg(&ff.ffmpeg, &abs, &tmp, &vf, threads, keyframes_only, interval_s, duration_ms).await;
        match &outcome {
            Ok(_) => break,
            Err(e) if keyframes_only && e.to_string().starts_with("nessuna miniatura") => {
                tracing::info!(file = %rel, "miniature: nessun fotogramma dai soli keyframe, si decodifica tutto");
            }
            Err(_) => break,
        }
    }
    let m = match outcome {
        Ok(m) => m,
        Err(e) => {
            let _ = tokio::fs::remove_dir_all(&tmp).await;
            return Err(e);
        }
    };
    let _ = tokio::fs::remove_dir_all(&final_dir).await;
    tokio::fs::rename(&tmp, &final_dir).await?;
    sqlx::query(
        "INSERT OR REPLACE INTO trickplay (file_id, interval_ms, width, height, cols, rows, count, sheets, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(file_id)
    .bind(m.interval_ms)
    .bind(m.width)
    .bind(m.height)
    .bind(m.cols)
    .bind(m.rows)
    .bind(m.count)
    .bind(m.sheets)
    .bind(now_ms())
    .execute(&state.db.write)
    .await?;
    tracing::info!(file = %rel, ms = t0.elapsed().as_millis() as u64, miniature = m.count, "miniature generate");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_ffmpeg(
    ffmpeg: &std::path::Path,
    input: &std::path::Path,
    tmp: &std::path::Path,
    vf: &str,
    threads: u32,
    keyframes_only: bool,
    interval_s: u32,
    duration_ms: i64,
) -> anyhow::Result<Manifest> {
    let threads = threads.to_string();
    let mut cmd = tokio::process::Command::new("nice");
    cmd.args(["-n", "15"]).arg(ffmpeg).args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-threads", &threads]);
    if keyframes_only {
        cmd.args(["-skip_frame", "nokey"]);
    }
    cmd.arg("-i")
        .arg(input)
        .args(["-map", "0:V:0", "-an", "-sn", "-dn", "-vf", vf, "-threads", &threads, "-q:v", "5", "-start_number", "0", "-f", "image2"])
        .arg(tmp.join("%d.jpg"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // Il file va letto tutto: da un mount remoto può richiedere molto.
    let out = tokio::time::timeout(Duration::from_secs(4 * 3600), cmd.output())
        .await
        .map_err(|_| anyhow::anyhow!("miniature oltre il tempo limite"))?
        .context("avvio di FFmpeg per le miniature")?;
    if !out.status.success() {
        bail!("FFmpeg: {}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("errore"));
    }
    let mut sheets = 0i64;
    while tokio::fs::try_exists(tmp.join(format!("{sheets}.jpg"))).await.unwrap_or(false) {
        sheets += 1;
    }
    if sheets == 0 {
        bail!("nessuna miniatura prodotta");
    }
    let first = tokio::fs::read(tmp.join("0.jpg")).await?;
    let (w, h) = jpeg_size(&first).context("griglia JPEG non leggibile")?;
    let per_sheet = (COLS * ROWS) as i64;
    let interval_ms = interval_s as i64 * 1000;
    let count = (duration_ms / interval_ms + 1).min(sheets * per_sheet).max(1);
    Ok(Manifest { interval_ms, width: (w / COLS) as i64, height: (h / ROWS) as i64, cols: COLS as i64, rows: ROWS as i64, count, sheets })
}

/// Cartelle di miniature senza più il loro file (file rimossi dalla libreria) e resti di generazioni interrotte.
pub async fn cleanup_orphans(state: &Shared) -> anyhow::Result<usize> {
    let known: std::collections::HashSet<i64> =
        sqlx::query_scalar::<_, i64>("SELECT file_id FROM trickplay").fetch_all(&state.db.read).await?.into_iter().collect();
    let root = root(state);
    let mut rd = match tokio::fs::read_dir(&root).await {
        Ok(r) => r,
        Err(_) => return Ok(0),
    };
    let mut removed = 0;
    while let Some(e) = rd.next_entry().await? {
        let name = e.file_name().to_string_lossy().to_string();
        let keep = name.parse::<i64>().is_ok_and(|id| known.contains(&id));
        // Una generazione in corso ha la sua cartella temporanea: la si lascia se è recente.
        let fresh_tmp = name.contains(".tmp-")
            && e.metadata().await.ok().and_then(|m| m.modified().ok()).is_some_and(|t| t.elapsed().unwrap_or_default() < Duration::from_secs(5 * 3600));
        if !keep && !fresh_tmp {
            let _ = tokio::fs::remove_dir_all(e.path()).await;
            removed += 1;
        }
    }
    Ok(removed)
}

/// Larghezza e altezza di un JPEG dal suo marcatore SOF.
pub fn jpeg_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.get(0..2)? != [0xFF, 0xD8] {
        return None;
    }
    let mut i = 2;
    while i + 4 <= b.len() {
        if b[i] != 0xFF {
            return None;
        }
        let marker = b[i + 1];
        if marker == 0xFF {
            i += 1;
            continue;
        }
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        // SOF0..SOF15 tranne DHT (C4), JPG (C8) e DAC (CC).
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            let h = u16::from_be_bytes([*b.get(i + 5)?, *b.get(i + 6)?]) as u32;
            let w = u16::from_be_bytes([*b.get(i + 7)?, *b.get(i + 8)?]) as u32;
            return Some((w, h));
        }
        i += 2 + len;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jpeg_dimensions() {
        // SOI, APP0 di 16 byte, SOF0 con altezza 1800 e larghezza 3200.
        let mut b = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        b.extend_from_slice(&[0u8; 14]);
        b.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08, 0x07, 0x08, 0x0C, 0x80, 0x03]);
        assert_eq!(jpeg_size(&b), Some((3200, 1800)));
        assert_eq!(jpeg_size(&[0, 1, 2]), None);
    }
}
