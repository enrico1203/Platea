//! Immagini: scaricate una volta da TMDB, ridimensionate in locale su richiesta,
//! salvate su disco e servite con cache lunga nel browser. Colore segnaposto calcolato al download.

use crate::state::Shared;
use anyhow::{bail, Context};
use std::path::PathBuf;
use std::time::Duration;

/// Larghezze consentite: limita le varianti in cache.
pub const WIDTHS: &[u32] = &[92, 185, 300, 400, 500, 780, 1280, 1920];

pub fn snap_width(w: u32) -> u32 {
    *WIDTHS.iter().find(|x| **x >= w).unwrap_or(WIDTHS.last().unwrap())
}

pub fn original_path(state: &Shared, image_id: i64) -> PathBuf {
    let shard = format!("{:02x}", image_id % 256);
    state.boot.images_dir().join(shard).join(format!("{image_id}.img"))
}

pub fn variant_path(state: &Shared, image_id: i64, width: u32) -> PathBuf {
    let shard = format!("{:02x}", image_id % 256);
    state.boot.images_dir().join(shard).join(format!("{image_id}_w{width}.jpg"))
}

/// Indirizzo da scaricare. I loghi TMDB in SVG non si possono decodificare: TMDB li fornisce anche
/// in PNG, con lo stesso nome e l'estensione cambiata.
pub fn fetch_url(url: &str) -> String {
    let tmdb = url.starts_with(crate::metadata::tmdb::IMAGE_BASE.as_str()) || url.contains("://image.tmdb.org/");
    match url.strip_suffix(".svg") {
        Some(stem) if tmdb => format!("{stem}.png"),
        _ => url.to_string(),
    }
}

pub async fn download_image(state: &Shared, image_id: i64) -> anyhow::Result<()> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT source_url, state FROM images WHERE id = ?").bind(image_id).fetch_optional(&state.db.read).await?;
    let Some((url, st)) = row else { return Ok(()) };
    if st == "ready" && original_path(state, image_id).exists() {
        return Ok(());
    }
    let _permit = state.image_downloads.acquire().await?;
    let resp = state.http.get(fetch_url(&url)).timeout(Duration::from_secs(30)).send().await.context("download dell'immagine")?;
    if !resp.status().is_success() {
        sqlx::query("UPDATE images SET state = 'failed' WHERE id = ?").bind(image_id).execute(&state.db.write).await?;
        bail!("immagine non disponibile: {}", resp.status());
    }
    let bytes = resp.bytes().await?;
    if bytes.len() > 20 * 1024 * 1024 {
        bail!("immagine troppo grande");
    }
    let path = original_path(state, image_id);
    let b = bytes.to_vec();
    let p = path.clone();
    let decoded = tokio::task::spawn_blocking(move || -> anyhow::Result<(u32, u32, String)> {
        let img = image::load_from_memory(&b).context("immagine non leggibile")?;
        let color = average_color(&img);
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = p.with_extension("tmp");
        std::fs::write(&tmp, &b)?;
        std::fs::rename(&tmp, &p)?;
        Ok((img.width(), img.height(), color))
    })
    .await?;
    let (w, h, color) = match decoded {
        Ok(v) => v,
        Err(e) => {
            // Un file che non è un'immagine non lo diventa riprovando.
            sqlx::query("UPDATE images SET state = 'failed' WHERE id = ?").bind(image_id).execute(&state.db.write).await?;
            return Err(e);
        }
    };
    sqlx::query("UPDATE images SET local_path = ?, width = ?, height = ?, placeholder_color = ?, state = 'ready' WHERE id = ?")
        .bind(path.to_string_lossy().to_string())
        .bind(w as i64)
        .bind(h as i64)
        .bind(&color)
        .bind(image_id)
        .execute(&state.db.write)
        .await?;
    Ok(())
}

/// Colore medio scurito: il fondo della locandina finché l'immagine non arriva.
fn average_color(img: &image::DynamicImage) -> String {
    let small = img.thumbnail(8, 8).to_rgb8();
    let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
    for p in small.pixels() {
        r += p[0] as u64;
        g += p[1] as u64;
        b += p[2] as u64;
        n += 1;
    }
    let n = n.max(1);
    // Scurito al 60% perché stia bene sul tema scuro.
    let f = |c: u64| ((c / n) as f64 * 0.6) as u8;
    format!("#{:02x}{:02x}{:02x}", f(r), f(g), f(b))
}

/// Restituisce il percorso del file da servire per la larghezza richiesta, generandolo se serve.
/// Toglie un'immagine e i suoi file (originale e varianti).
pub async fn remove_image(state: &Shared, image_id: i64) -> anyhow::Result<()> {
    let orig = original_path(state, image_id);
    if let Some(dir) = orig.parent() {
        if let Ok(rd) = std::fs::read_dir(dir) {
            let prefix = format!("{image_id}_w");
            for e in rd.flatten() {
                if e.file_name().to_string_lossy().starts_with(&prefix) {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
    }
    let _ = std::fs::remove_file(orig);
    sqlx::query("DELETE FROM images WHERE id = ?").bind(image_id).execute(&state.db.write).await?;
    Ok(())
}

/// Immagine caricata dall'amministratore (locandina, sfondo, logo): nuovo identificativo, così la cache lunga dei
/// browser non mostra quella vecchia. Il file resta nella cache del server, mai nelle cartelle dei media.
pub async fn store_upload(state: &Shared, item_id: i64, kind: &str, bytes: Vec<u8>) -> anyhow::Result<i64> {
    let (w, h, color, b) = tokio::task::spawn_blocking(move || -> anyhow::Result<(u32, u32, String, Vec<u8>)> {
        let img = image::load_from_memory(&bytes).context("immagine non leggibile")?;
        Ok((img.width(), img.height(), average_color(&img), bytes))
    })
    .await??;
    let old: Vec<i64> = sqlx::query_scalar("SELECT id FROM images WHERE item_id = ? AND kind = ?").bind(item_id).bind(kind).fetch_all(&state.db.read).await?;
    // La nuova riga prima di togliere le vecchie: identificativo sempre nuovo (vedi set_image_url).
    let id = sqlx::query(
        "INSERT INTO images (item_id, kind, source, source_url, state, width, height, placeholder_color) VALUES (?, ?, 'upload', 'upload', 'ready', ?, ?, ?)",
    )
    .bind(item_id)
    .bind(kind)
    .bind(w as i64)
    .bind(h as i64)
    .bind(&color)
    .execute(&state.db.write)
    .await?
    .last_insert_rowid();
    let path = original_path(state, id);
    if let Some(d) = path.parent() {
        tokio::fs::create_dir_all(d).await?;
    }
    tokio::fs::write(&path, &b).await?;
    for o in old {
        remove_image(state, o).await?;
    }
    sqlx::query("UPDATE images SET local_path = ? WHERE id = ?").bind(path.to_string_lossy().to_string()).bind(id).execute(&state.db.write).await?;
    Ok(id)
}

pub async fn serve_path(state: &Shared, image_id: i64, width: Option<u32>) -> anyhow::Result<PathBuf> {
    let orig = original_path(state, image_id);
    if !orig.exists() {
        // Scaricata solo quando una pagina la mostra (per esempio le foto degli attori).
        download_image(state, image_id).await?;
    }
    let Some(w) = width else { return Ok(orig) };
    let w = snap_width(w);
    let variant = variant_path(state, image_id, w);
    if variant.exists() {
        return Ok(variant);
    }
    let _permit = state.image_resizes.acquire().await?;
    if variant.exists() {
        return Ok(variant);
    }
    let o = orig.clone();
    let v = variant.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let img = image::open(&o).or_else(|_| {
            let bytes = std::fs::read(&o)?;
            image::load_from_memory(&bytes).map_err(anyhow::Error::from)
        })?;
        let out = if img.width() > w { img.resize(w, u32::MAX, image::imageops::FilterType::Triangle) } else { img };
        let tmp = v.with_extension("tmp");
        let mut f = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut f, 85);
        out.to_rgb8().write_with_encoder(enc)?;
        drop(f);
        std::fs::rename(&tmp, &v)?;
        Ok(())
    })
    .await??;
    Ok(variant)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmdb_svg_logos_as_png() {
        assert_eq!(fetch_url("https://image.tmdb.org/t/p/w500/abc.svg"), "https://image.tmdb.org/t/p/w500/abc.png");
        assert_eq!(fetch_url("https://image.tmdb.org/t/p/w500/abc.png"), "https://image.tmdb.org/t/p/w500/abc.png");
        assert_eq!(fetch_url("https://altro.example/logo.svg"), "https://altro.example/logo.svg");
    }
}
