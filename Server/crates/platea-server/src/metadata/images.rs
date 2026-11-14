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

pub async fn download_image(state: &Shared, image_id: i64) -> anyhow::Result<()> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT source_url, state FROM images WHERE id = ?").bind(image_id).fetch_optional(&state.db.read).await?;
    let Some((url, st)) = row else { return Ok(()) };
    if st == "ready" && original_path(state, image_id).exists() {
        return Ok(());
    }
    let _permit = state.image_downloads.acquire().await?;
    let resp = state.http.get(&url).timeout(Duration::from_secs(30)).send().await.context("download dell'immagine")?;
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
    let (w, h, color) = tokio::task::spawn_blocking(move || -> anyhow::Result<(u32, u32, String)> {
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
    .await??;
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
