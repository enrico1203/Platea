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

/// Decodifica con limiti: un file piccolo che dichiara dimensioni enormi (bomba di decompressione) viene rifiutato
/// invece di allocare centinaia di MB.
pub fn decode_limited(b: &[u8]) -> anyhow::Result<image::DynamicImage> {
    let mut r = image::ImageReader::new(std::io::Cursor::new(b)).with_guessed_format()?;
    let mut l = image::Limits::default();
    l.max_image_width = Some(16_384);
    l.max_image_height = Some(16_384);
    l.max_alloc = Some(256 * 1024 * 1024);
    r.limits(l);
    Ok(r.decode()?)
}

/// Un indirizzo che non è di TMDB (immagini indicate negli NFO) non deve portare il server dentro la rete locale:
/// niente IP privati, di loopback o link-local, anche dopo la risoluzione del nome.
fn is_public_ip(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            !(v.is_private()
                || v.is_loopback()
                || v.is_link_local()
                || v.is_unspecified()
                || v.is_broadcast()
                || v.is_documentation()
                || o[0] == 0
                || (o[0] == 100 && (o[1] & 0xc0) == 64)
                || (o[0] == 198 && (o[1] & 0xfe) == 18))
        }
        IpAddr::V6(v) => {
            let seg0 = v.segments()[0];
            if let Some(m) = v.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(m));
            }
            !(v.is_loopback() || v.is_unspecified() || (seg0 & 0xfe00) == 0xfc00 || (seg0 & 0xffc0) == 0xfe80)
        }
    }
}

async fn check_external_url(url: &str) -> anyhow::Result<()> {
    let u = reqwest::Url::parse(url).context("indirizzo dell'immagine non valido")?;
    if !matches!(u.scheme(), "http" | "https") {
        bail!("schema non consentito: {}", u.scheme());
    }
    let port = u.port_or_known_default().unwrap_or(443);
    let host = u.host_str().context("indirizzo senza host")?;
    let literal = host.trim_start_matches('[').trim_end_matches(']').parse::<std::net::IpAddr>().ok();
    let addrs: Vec<std::net::IpAddr> = match literal {
        Some(ip) => vec![ip],
        None => tokio::time::timeout(Duration::from_secs(5), tokio::net::lookup_host((host, port)))
            .await
            .context("risoluzione del nome oltre il tempo limite")??
            .map(|a| a.ip())
            .collect(),
    };
    if addrs.is_empty() || !addrs.iter().all(|ip| is_public_ip(*ip)) {
        bail!("immagine su un indirizzo locale o privato: non scaricata");
    }
    Ok(())
}

/// Client per gli indirizzi esterni non TMDB: niente redirect (un redirect potrebbe puntare alla rete locale).
static EXTERNAL: once_cell::sync::Lazy<reqwest::Client> = once_cell::sync::Lazy::new(|| {
    reqwest::Client::builder()
        .user_agent(concat!("Platea/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client HTTP")
});

const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;

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
    let target = fetch_url(&url);
    let tmdb = target.starts_with(crate::metadata::tmdb::IMAGE_BASE.as_str()) || target.starts_with("https://image.tmdb.org/");
    let client = if tmdb {
        &state.http
    } else {
        if let Err(e) = check_external_url(&target).await {
            sqlx::query("UPDATE images SET state = 'failed' WHERE id = ?").bind(image_id).execute(&state.db.write).await?;
            return Err(e);
        }
        &*EXTERNAL
    };
    let mut resp = client.get(&target).timeout(Duration::from_secs(30)).send().await.context("download dell'immagine")?;
    if !resp.status().is_success() {
        sqlx::query("UPDATE images SET state = 'failed' WHERE id = ?").bind(image_id).execute(&state.db.write).await?;
        bail!("immagine non disponibile: {}", resp.status());
    }
    // Letta a pezzi con un tetto: una risposta senza fine non riempie la memoria.
    if resp.content_length().is_some_and(|l| l as usize > MAX_IMAGE_BYTES) {
        bail!("immagine troppo grande");
    }
    let mut bytes: Vec<u8> = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        if bytes.len() + chunk.len() > MAX_IMAGE_BYTES {
            bail!("immagine troppo grande");
        }
        bytes.extend_from_slice(&chunk);
    }
    let path = original_path(state, image_id);
    let b = bytes;
    let p = path.clone();
    let decoded = tokio::task::spawn_blocking(move || -> anyhow::Result<(u32, u32, String)> {
        let img = decode_limited(&b).context("immagine non leggibile")?;
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
        let img = decode_limited(&bytes).context("immagine non leggibile")?;
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
            decode_limited(&bytes)
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

#[cfg(test)]
mod ssrf_tests {
    use super::*;

    #[test]
    fn private_addresses_are_refused() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "192.168.1.1",
            "172.20.0.5",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:192.168.1.1",
        ] {
            assert!(!is_public_ip(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["1.1.1.1", "142.250.180.14", "2606:4700::1111"] {
            assert!(is_public_ip(ip.parse().unwrap()), "{ip}");
        }
    }

    #[tokio::test]
    async fn literal_private_urls_are_refused() {
        assert!(check_external_url("http://192.168.1.1/admin.jpg").await.is_err());
        assert!(check_external_url("http://[::1]:8096/x.png").await.is_err());
        assert!(check_external_url("file:///etc/passwd").await.is_err());
        assert!(check_external_url("http://localhost/x.png").await.is_err());
    }
}
