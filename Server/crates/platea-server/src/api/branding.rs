//! Personalizzazione: nome, logo e icone della web UI.
//!
//! Il nome del server (Impostazioni › Generali) compare ovunque al posto di "Platea"; con "Nascondi Platea" spariscono
//! anche il marchio e ogni altro riferimento. Il logo caricato dall'amministratore sta in `data/branding/`:
//! un SVG si serve com'è; un'immagine raster si riduce a PNG e se ne ricavano le icone quadrate 192 e 512
//! (schermata Home di iPhone e Android). La testata di `index.html` e il manifest dell'app si generano qui,
//! così il nome e l'icona giusti ci sono prima ancora che parta il JavaScript.

use super::{Access, Api};
use crate::auth::AdminUser;
use crate::error::{ApiError, ApiResult};
use crate::state::Shared;
use axum::body::Body;
use axum::extract::State;
use axum::http::header;
use axum::response::Response;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// Segnaposto in `index.html`: nginx lo risolve con SSI, il server (PLATEA_WEB_DIR) lo sostituisce da sé.
pub const HEAD_MARKER: &str = r#"<!--# include virtual="/api/v1/branding/head.html" -->"#;

const MAX_LOGO: usize = 5 * 1024 * 1024;
const MAX_SVG: usize = 1024 * 1024;

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum LogoKind {
    Svg,
    Png,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Logo {
    pub kind: LogoKind,
    /// Cambia a ogni caricamento: entra negli indirizzi, che così si tengono in cache a lungo.
    pub version: String,
}

/// Logo attuale, letto all'avvio e aggiornato a ogni caricamento.
#[derive(Default)]
pub struct Branding {
    logo: RwLock<Option<Logo>>,
}

impl Branding {
    pub async fn load(data_dir: &Path) -> Branding {
        let logo = match tokio::fs::read(dir(data_dir).join("logo.json")).await {
            Ok(b) => serde_json::from_slice::<Logo>(&b).ok(),
            Err(_) => None,
        };
        Branding { logo: RwLock::new(logo) }
    }

    pub fn logo(&self) -> Option<Logo> {
        self.logo.read().unwrap().clone()
    }
}

fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("branding")
}

pub fn routes(api: Api) -> Api {
    api.get("/api/v1/branding/logo", "Sistema", Access::Public, "Logo caricato dall'amministratore (SVG o PNG)", logo)
        .get("/api/v1/branding/icon-192.png", "Sistema", Access::Public, "Icona quadrata 192×192 ricavata dal logo", icon_192)
        .get("/api/v1/branding/icon-512.png", "Sistema", Access::Public, "Icona quadrata 512×512 ricavata dal logo", icon_512)
        .get(
            "/api/v1/branding/icon-maskable-512.png",
            "Sistema",
            Access::Public,
            "Icona 512×512 a pieno riquadro per Android (logo nella zona sicura su fondo scuro)",
            icon_maskable,
        )
        .get("/api/v1/branding/icon.svg", "Sistema", Access::Public, "Icona con l'iniziale del nome del server (senza logo, con Platea nascosto)", monogram)
        .get("/api/v1/branding/manifest.webmanifest", "Sistema", Access::Public, "Manifest della web app con nome e icone del server", manifest)
        .get("/api/v1/branding/head.html", "Sistema", Access::Public, "Titolo, descrizione e icone per la testata di index.html", head)
        .post("/api/v1/admin/branding/logo", "Impostazioni", Access::Admin, "Carica il logo (SVG, PNG, JPEG o WebP; massimo 5 MB)", put_logo)
        .delete("/api/v1/admin/branding/logo", "Impostazioni", Access::Admin, "Toglie il logo", delete_logo)
}

/// Nome mostrato: quello scelto dall'amministratore, "Platea" se vuoto.
pub fn display_name(state: &Shared) -> String {
    let n = state.settings.get().general.server_name.trim().to_string();
    if n.is_empty() {
        "Platea".into()
    } else {
        n
    }
}

/// Nome del prodotto verso i client: Platea, oppure il nome del server se Platea è nascosto.
pub fn product_name(state: &Shared) -> String {
    if state.settings.get().general.white_label {
        display_name(state)
    } else {
        "Platea".into()
    }
}

/// Indirizzo del logo per i client (con la versione), se caricato.
pub fn logo_url(state: &Shared) -> Option<String> {
    state.branding.logo().map(|l| format!("/api/v1/branding/logo?v={}", l.version))
}

struct Icon {
    href: String,
    mime: &'static str,
    sizes: &'static str,
}

/// Icone da usare, in ordine: logo raster (PNG quadrati), logo SVG, iniziale del nome (Platea nascosto), Platea.
fn icons(state: &Shared) -> (Vec<Icon>, Option<String>) {
    let s = state.settings.get();
    match state.branding.logo() {
        Some(Logo { kind: LogoKind::Png, version }) => (
            vec![
                Icon { href: format!("/api/v1/branding/icon-192.png?v={version}"), mime: "image/png", sizes: "192x192" },
                Icon { href: format!("/api/v1/branding/icon-512.png?v={version}"), mime: "image/png", sizes: "512x512" },
            ],
            Some(format!("/api/v1/branding/icon-192.png?v={version}")),
        ),
        Some(Logo { kind: LogoKind::Svg, version }) => {
            (vec![Icon { href: format!("/api/v1/branding/logo?v={version}"), mime: "image/svg+xml", sizes: "any" }], None)
        }
        None if s.general.white_label => {
            let v = &crate::util::sha256_hex(&display_name(state))[..10];
            (vec![Icon { href: format!("/api/v1/branding/icon.svg?v={v}"), mime: "image/svg+xml", sizes: "any" }], None)
        }
        None => (
            vec![
                Icon { href: "/platea-app-icon.svg".into(), mime: "image/svg+xml", sizes: "any" },
                Icon { href: "/icon-192.png".into(), mime: "image/png", sizes: "192x192" },
                Icon { href: "/icon-512.png".into(), mime: "image/png", sizes: "512x512" },
            ],
            Some("/icon-192.png".into()),
        ),
    }
}

fn description(state: &Shared) -> &'static str {
    if state.settings.get().general.white_label {
        "Media server"
    } else {
        "Open source media server"
    }
}

pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// Frammento della testata di index.html.
pub fn head_html(state: &Shared) -> String {
    let name = escape_html(&display_name(state));
    let (icons, apple) = icons(state);
    let first = &icons[0];
    let mut h = format!(
        "<title>{name}</title>\n    <meta name=\"description\" content=\"{}\" />\n    <meta name=\"application-name\" content=\"{name}\" />\n    <meta name=\"apple-mobile-web-app-title\" content=\"{name}\" />\n    <link rel=\"icon\" type=\"{}\" href=\"{}\" />",
        description(state),
        first.mime,
        escape_html(&first.href)
    );
    if let Some(a) = apple {
        h.push_str(&format!("\n    <link rel=\"apple-touch-icon\" href=\"{}\" />", escape_html(&a)));
    }
    h
}

fn no_cache(body: impl Into<Body>, ctype: &str) -> Response {
    Response::builder()
        .header(header::CONTENT_TYPE, ctype)
        .header(header::CACHE_CONTROL, "no-cache")
        .header("X-Content-Type-Options", "nosniff")
        .body(body.into())
        .unwrap()
}

async fn head(State(state): State<Shared>) -> Response {
    no_cache(head_html(&state), "text/html; charset=utf-8")
}

pub async fn manifest(State(state): State<Shared>) -> Response {
    let name = display_name(&state);
    let (icons, _) = icons(&state);
    let mut icons: Vec<Value> = icons.iter().map(|i| json!({ "src": i.href, "sizes": i.sizes, "type": i.mime, "purpose": "any" })).collect();
    // Android: icona a pieno riquadro, altrimenti il logo finisce dentro un cerchio bianco.
    match state.branding.logo() {
        Some(Logo { kind: LogoKind::Png, version }) => icons.push(
            json!({ "src": format!("/api/v1/branding/icon-maskable-512.png?v={version}"), "sizes": "512x512", "type": "image/png", "purpose": "maskable" }),
        ),
        None if !state.settings.get().general.white_label => {
            icons.push(json!({ "src": "/icon-maskable-512.png", "sizes": "512x512", "type": "image/png", "purpose": "maskable" }))
        }
        _ => {}
    }
    let short: String = name.chars().take(12).collect();
    let m = json!({
        "name": name,
        "short_name": short,
        "description": description(&state),
        "id": "/",
        "start_url": "/",
        "scope": "/",
        "display": "standalone",
        "background_color": "#050608",
        "theme_color": "#050608",
        "icons": icons,
    });
    no_cache(serde_json::to_vec_pretty(&m).unwrap_or_default(), "application/manifest+json")
}

/// index.html servito dal server stesso (PLATEA_WEB_DIR): la testata si sostituisce qui, come farebbe nginx con SSI.
pub async fn index_html(State(state): State<Shared>) -> Response {
    let Some(dir) = state.boot.web_dir.clone() else {
        return Response::builder().status(404).body(Body::empty()).unwrap();
    };
    match tokio::fs::read_to_string(dir.join("index.html")).await {
        Ok(html) => {
            let html = html.replace(HEAD_MARKER, &head_html(&state)).replace(r#"<!--# config errmsg="" -->"#, "");
            no_cache(html, "text/html; charset=utf-8")
        }
        Err(_) => Response::builder().status(404).body(Body::empty()).unwrap(),
    }
}

/// Iniziale del nome su un quadrato arrotondato: icona neutra quando Platea è nascosto e manca un logo.
pub fn monogram_svg(name: &str) -> String {
    let initial: String = name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().collect()).unwrap_or_else(|| "•".into());
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" rx="14" fill="#2f6bff"/><text x="32" y="44" text-anchor="middle" font-family="system-ui,-apple-system,Segoe UI,Roboto,sans-serif" font-size="34" font-weight="700" fill="#fff">{}</text></svg>"##,
        escape_html(&initial)
    )
}

async fn monogram(State(state): State<Shared>) -> Response {
    Response::builder()
        .header(header::CONTENT_TYPE, "image/svg+xml")
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .header("X-Content-Type-Options", "nosniff")
        .body(Body::from(monogram_svg(&display_name(&state))))
        .unwrap()
}

/// Un SVG aperto direttamente nel browser potrebbe eseguire script: la CSP lo impedisce (nei tag <img> non girano mai).
const IMAGE_CSP: &str = "default-src 'none'; style-src 'unsafe-inline'; img-src data:; sandbox";

async fn serve_file(state: &Shared, name: &str, ctype: &str) -> ApiResult<Response> {
    let bytes = tokio::fs::read(dir(&state.boot.data_dir).join(name)).await.map_err(|_| ApiError::not_found("Logo"))?;
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, ctype)
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .header("X-Content-Type-Options", "nosniff")
        .header(header::CONTENT_SECURITY_POLICY, IMAGE_CSP)
        .body(Body::from(bytes))
        .unwrap())
}

async fn logo(State(state): State<Shared>) -> ApiResult<Response> {
    match state.branding.logo().map(|l| l.kind) {
        Some(LogoKind::Svg) => serve_file(&state, "logo.svg", "image/svg+xml").await,
        Some(LogoKind::Png) => serve_file(&state, "logo.png", "image/png").await,
        None => Err(ApiError::not_found("Logo")),
    }
}

async fn icon_192(State(state): State<Shared>) -> ApiResult<Response> {
    serve_file(&state, "icon-192.png", "image/png").await
}

async fn icon_512(State(state): State<Shared>) -> ApiResult<Response> {
    serve_file(&state, "icon-512.png", "image/png").await
}

/// Icona "maskable" di Android: si genera la prima volta dal logo PNG e resta nella cartella del branding.
async fn icon_maskable(State(state): State<Shared>) -> ApiResult<Response> {
    if !matches!(state.branding.logo().map(|l| l.kind), Some(LogoKind::Png)) {
        return Err(ApiError::not_found("Icona"));
    }
    let d = dir(&state.boot.data_dir);
    if !d.join(MASKABLE).exists() {
        let d2 = d.clone();
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let img = image::open(d2.join("logo.png"))?;
            let mut out = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(maskable_icon(&img, 512)).write_to(&mut out, image::ImageFormat::Png)?;
            let tmp = d2.join(format!("{MASKABLE}.tmp"));
            std::fs::write(&tmp, out.into_inner())?;
            std::fs::rename(&tmp, d2.join(MASKABLE))?;
            Ok(())
        })
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .map_err(ApiError::Internal)?;
    }
    serve_file(&state, MASKABLE, "image/png").await
}

const MASKABLE: &str = "icon-maskable-512.png";

/// Fondo pieno con il logo dentro la zona sicura (il cerchio centrale dell'80%): Android ritaglia il resto.
fn maskable_icon(img: &image::DynamicImage, size: u32) -> image::RgbaImage {
    let inner = size * 3 / 5;
    let fit = img.resize(inner, inner, image::imageops::FilterType::Lanczos3).to_rgba8();
    let mut canvas = image::RgbaImage::from_pixel(size, size, image::Rgba([0x05, 0x06, 0x08, 0xff]));
    let (x, y) = ((size - fit.width()) / 2, (size - fit.height()) / 2);
    image::imageops::overlay(&mut canvas, &fit, x as i64, y as i64);
    canvas
}

/// Un SVG si riconosce dal contenuto: testo UTF-8 con un elemento <svg>.
fn looks_like_svg(b: &[u8]) -> bool {
    let head = &b[..b.len().min(4096)];
    std::str::from_utf8(head).map(|s| s.to_ascii_lowercase().contains("<svg")).unwrap_or(false)
}

/// Quadrato trasparente con il logo al centro, con un po' di margine.
fn square_icon(img: &image::DynamicImage, size: u32) -> image::RgbaImage {
    let inner = size * 7 / 8;
    let fit = img.resize(inner, inner, image::imageops::FilterType::Lanczos3).to_rgba8();
    let mut canvas = image::RgbaImage::new(size, size);
    let (x, y) = ((size - fit.width()) / 2, (size - fit.height()) / 2);
    image::imageops::overlay(&mut canvas, &fit, x as i64, y as i64);
    canvas
}

fn store_logo(d: &Path, body: &[u8]) -> anyhow::Result<LogoKind> {
    std::fs::create_dir_all(d)?;
    let write = |name: &str, bytes: &[u8]| -> anyhow::Result<()> {
        let tmp = d.join(format!("{name}.tmp"));
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, d.join(name))?;
        Ok(())
    };
    if looks_like_svg(body) {
        anyhow::ensure!(body.len() <= MAX_SVG, "SVG troppo grande (massimo 1 MB)");
        write("logo.svg", body)?;
        for f in ["logo.png", "icon-192.png", "icon-512.png", MASKABLE] {
            let _ = std::fs::remove_file(d.join(f));
        }
        return Ok(LogoKind::Svg);
    }
    let img = crate::metadata::images::decode_limited(body)?;
    let logo = if img.width() > 1024 || img.height() > 1024 { img.resize(1024, 1024, image::imageops::FilterType::Lanczos3) } else { img.clone() };
    let png = |i: &image::DynamicImage| -> anyhow::Result<Vec<u8>> {
        let mut out = std::io::Cursor::new(Vec::new());
        i.write_to(&mut out, image::ImageFormat::Png)?;
        Ok(out.into_inner())
    };
    write("logo.png", &png(&logo)?)?;
    write("icon-192.png", &png(&image::DynamicImage::ImageRgba8(square_icon(&img, 192)))?)?;
    write("icon-512.png", &png(&image::DynamicImage::ImageRgba8(square_icon(&img, 512)))?)?;
    let _ = std::fs::remove_file(d.join(MASKABLE));
    let _ = std::fs::remove_file(d.join("logo.svg"));
    Ok(LogoKind::Png)
}

async fn put_logo(State(state): State<Shared>, AdminUser(admin): AdminUser, body: axum::body::Bytes) -> ApiResult<Json<Value>> {
    if body.is_empty() || body.len() > MAX_LOGO {
        return Err(ApiError::BadRequest("Immagine vuota o troppo grande (massimo 5 MB)".into()));
    }
    let d = dir(&state.boot.data_dir);
    let d2 = d.clone();
    let kind = tokio::task::spawn_blocking(move || store_logo(&d2, &body))
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .map_err(|e| ApiError::BadRequest(format!("Immagine non leggibile: {e}")))?;
    let logo = Logo { kind, version: format!("{:x}", crate::util::now_ms()) };
    let meta = serde_json::to_vec(&logo).map_err(|e| ApiError::Internal(e.into()))?;
    tokio::fs::write(d.join("logo.json"), meta).await.map_err(|e| ApiError::Internal(e.into()))?;
    *state.branding.logo.write().unwrap() = Some(logo);
    state.db.activity(Some(admin.user_id), "settings", &format!("{} ha caricato un nuovo logo", admin.username), None).await;
    Ok(Json(json!({ "logo": logo_url(&state) })))
}

async fn delete_logo(State(state): State<Shared>, AdminUser(admin): AdminUser) -> ApiResult<Json<Value>> {
    let d = dir(&state.boot.data_dir);
    for f in ["logo.json", "logo.svg", "logo.png", "icon-192.png", "icon-512.png", MASKABLE] {
        let _ = tokio::fs::remove_file(d.join(f)).await;
    }
    *state.branding.logo.write().unwrap() = None;
    state.db.activity(Some(admin.user_id), "settings", &format!("{} ha tolto il logo", admin.username), None).await;
    Ok(Json(json!({ "logo": null })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maskable_icon_fills_square_and_keeps_logo_in_safe_zone() {
        let logo = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(300, 100, image::Rgba([255, 0, 0, 255])));
        let icon = maskable_icon(&logo, 512);
        assert_eq!(icon.dimensions(), (512, 512));
        assert_eq!(icon.get_pixel(0, 0).0, [0x05, 0x06, 0x08, 0xff]);
        assert_eq!(icon.get_pixel(256, 256).0, [255, 0, 0, 255]);
        // Fuori dal 60% centrale resta il fondo.
        assert_eq!(icon.get_pixel(256, 60).0, [0x05, 0x06, 0x08, 0xff]);
        assert_eq!(icon.get_pixel(90, 256).0, [0x05, 0x06, 0x08, 0xff]);
    }

    #[test]
    fn svg_detection() {
        assert!(looks_like_svg(br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg"></svg>"#));
        assert!(!looks_like_svg(&[0x89, b'P', b'N', b'G', 0, 0]));
    }

    #[test]
    fn monogram_uses_initial_and_escapes() {
        assert!(monogram_svg("itdrive").contains(">I</text>"));
        assert!(monogram_svg("<x>").contains(">X</text>"));
        assert!(monogram_svg("&&").contains(">•</text>"));
    }

    #[test]
    fn raster_logo_makes_square_icons() {
        let d = tempfile::tempdir().unwrap();
        let img = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(300, 100, image::Rgba([255, 0, 0, 255])));
        let mut png = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png, image::ImageFormat::Png).unwrap();
        assert_eq!(store_logo(d.path(), png.get_ref()).unwrap(), LogoKind::Png);
        let icon = image::open(d.path().join("icon-512.png")).unwrap();
        assert_eq!((icon.width(), icon.height()), (512, 512));
        assert!(d.path().join("logo.png").exists());
        // Un SVG dopo un PNG toglie le icone raster.
        assert_eq!(store_logo(d.path(), b"<svg xmlns='http://www.w3.org/2000/svg'/>").unwrap(), LogoKind::Svg);
        assert!(!d.path().join("icon-512.png").exists());
    }
}
