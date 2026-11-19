//! Direct play: il file originale servito così com'è, con richieste HTTP Range.
//! Lettura a blocchi con contropressione: in memoria c'è solo il blocco in transito.
//! Il descrittore resta aperto per la durata della sessione. FFmpeg non viene mai avviato.

use crate::error::{ApiError, ApiResult};
use axum::body::Body;
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::Response;
use bytes_stream::chunked;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

const CHUNK: usize = 256 * 1024;

/// Descrittori aperti per file, condivisi da tutte le riproduzioni dello stesso file (web, Infuse, più utenti).
/// Restano aperti finché usati e 10 minuti dopo: su un mount cloud riaprire un file costa secondi,
/// e centinaia di sessioni sullo stesso film non consumano centinaia di descrittori.
static OPEN_FILES: once_cell::sync::Lazy<std::sync::Mutex<std::collections::HashMap<i64, (Arc<File>, std::time::Instant)>>> =
    once_cell::sync::Lazy::new(Default::default);
const OPEN_IDLE: std::time::Duration = std::time::Duration::from_secs(600);

/// Apre (o riusa) il file di una libreria, verificando che il percorso resti sotto la radice.
pub async fn open_library_file(file_id: i64, abs: &Path, root: &Path, timeout: std::time::Duration) -> ApiResult<Arc<File>> {
    {
        let mut map = OPEN_FILES.lock().unwrap();
        map.retain(|_, (f, t)| t.elapsed() < OPEN_IDLE || Arc::strong_count(f) > 1);
        if let Some((f, t)) = map.get_mut(&file_id) {
            *t = std::time::Instant::now();
            return Ok(f.clone());
        }
    }
    let (p, root) = (abs.to_path_buf(), root.to_path_buf());
    let opened = tokio::time::timeout(
        timeout,
        tokio::task::spawn_blocking(move || -> std::io::Result<File> {
            // Il percorso viene dal database e deve restare sotto la radice della libreria.
            let canon = std::fs::canonicalize(&p)?;
            if !canon.starts_with(std::fs::canonicalize(&root)?) {
                return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "fuori dalla libreria"));
            }
            File::open(canon)
        }),
    )
    .await
    .map_err(|_| ApiError::coded(StatusCode::GATEWAY_TIMEOUT, "StorageTimeout", "Lo storage non risponde"))?
    .map_err(|e| ApiError::Internal(e.into()))?
    .map_err(|e| ApiError::coded(StatusCode::CONFLICT, "FileUnavailable", format!("File non leggibile: {e}")))?;
    let f = Arc::new(opened);
    // Due richieste in gara possono aprire lo stesso file: vince la prima, l'altro descrittore si chiude.
    let mut map = OPEN_FILES.lock().unwrap();
    let entry = map.entry(file_id).or_insert_with(|| (f.clone(), std::time::Instant::now()));
    Ok(entry.0.clone())
}

/// Descrittori di file della libreria attualmente aperti (per la dashboard e i test).
pub fn open_files_count() -> usize {
    OPEN_FILES.lock().unwrap().len()
}

/// Video di un file `.strm`: il server lo legge dalla sorgente e lo passa al client, con le richieste Range.
/// Così funziona anche quando la sorgente è raggiungibile solo dal server (rete di casa) o è in http e la pagina in https.
pub async fn proxy(http: &reqwest::Client, url: &str, method: &Method, headers: &HeaderMap) -> crate::error::ApiResult<Response> {
    use crate::error::ApiError;
    let mut req = if *method == Method::HEAD { http.head(url) } else { http.get(url) };
    for h in [header::RANGE, header::IF_RANGE] {
        if let Some(v) = headers.get(&h) {
            req = req.header(h.as_str(), v.as_bytes());
        }
    }
    let resp =
        req.send().await.map_err(|e| ApiError::coded(StatusCode::BAD_GATEWAY, "StrmUnreachable", format!("Sorgente del .strm non raggiungibile: {e}")))?;
    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    if !(status.is_success() || status == StatusCode::RANGE_NOT_SATISFIABLE) {
        return Err(ApiError::coded(StatusCode::BAD_GATEWAY, "StrmUnreachable", format!("La sorgente del .strm ha risposto {status}")));
    }
    let mut out = Response::builder().status(status).header(header::CACHE_CONTROL, "no-store");
    for h in [header::CONTENT_TYPE, header::CONTENT_LENGTH, header::CONTENT_RANGE, header::ACCEPT_RANGES, header::LAST_MODIFIED, header::ETAG] {
        if let Some(v) = resp.headers().get(h.as_str()) {
            if let Ok(v) = HeaderValue::from_bytes(v.as_bytes()) {
                out = out.header(h, v);
            }
        }
    }
    let body = if *method == Method::HEAD { Body::empty() } else { Body::from_stream(resp.bytes_stream()) };
    Ok(out.body(body).unwrap())
}

pub fn content_type_for(container: &str, path: &Path) -> &'static str {
    match container {
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "ts" => "video/mp2t",
        "mpeg" => "video/mpeg",
        "ogg" => "video/ogg",
        _ => match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
            Some("mp4") | Some("m4v") => "video/mp4",
            Some("mkv") => "video/x-matroska",
            Some("webm") => "video/webm",
            _ => "application/octet-stream",
        },
    }
}

/// Interpreta `Range: bytes=a-b`. Restituisce (inizio, fine inclusa).
pub fn parse_range(h: &str, size: u64) -> Option<(u64, u64)> {
    let spec = h.trim().strip_prefix("bytes=")?;
    let first = spec.split(',').next()?.trim();
    let (a, b) = first.split_once('-')?;
    if size == 0 {
        return None;
    }
    if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        if n == 0 {
            return None;
        }
        let start = size.saturating_sub(n);
        return Some((start, size - 1));
    }
    let start: u64 = a.parse().ok()?;
    if start >= size {
        return None;
    }
    let end = if b.is_empty() { size - 1 } else { b.parse::<u64>().ok()?.min(size - 1) };
    (end >= start).then_some((start, end))
}

pub fn etag(size: u64, mtime: i64) -> String {
    format!("\"{size:x}-{mtime:x}\"")
}

/// Risposta per un file già aperto.
/// `sent`: contatore dei byte inviati (banda della sessione), aggiornato a ogni blocco.
pub fn serve_file(
    file: Arc<File>,
    size: u64,
    mtime: i64,
    content_type: &str,
    method: &Method,
    headers: &HeaderMap,
    sent: Option<Arc<std::sync::atomic::AtomicU64>>,
) -> ApiResult<Response> {
    let tag = etag(size, mtime);
    // 304 solo per le richieste intere. A una richiesta a pezzi (Range) si risponde sempre con i byte: Chrome
    // riconvalida i pezzi già in cache con If-None-Match e, se la sua copia è parziale, un 304 lo lascia senza i byte
    // che cercava (audiolibri .m4b in diretta interrotti dopo pochi secondi, con il moov in fondo al file).
    if let Some(inm) = headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()).filter(|_| !headers.contains_key(header::RANGE)) {
        if inm == tag {
            return Ok(Response::builder().status(StatusCode::NOT_MODIFIED).header(header::ETAG, &tag).body(Body::empty()).unwrap());
        }
    }
    // If-Range: se l'ETag non coincide si risponde con il file intero.
    let range_ok = headers.get(header::IF_RANGE).and_then(|v| v.to_str().ok()).is_none_or(|v| v == tag);
    let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok()).filter(|_| range_ok).map(|r| parse_range(r, size));

    let mut builder = Response::builder()
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::ETAG, &tag)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "private, max-age=0");

    let (status, start, end) = match range {
        Some(Some((s, e))) => {
            builder = builder.header(header::CONTENT_RANGE, format!("bytes {s}-{e}/{size}"));
            (StatusCode::PARTIAL_CONTENT, s, e)
        }
        Some(None) => {
            return Ok(Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::CONTENT_RANGE, format!("bytes */{size}"))
                .body(Body::empty())
                .unwrap());
        }
        None => (StatusCode::OK, 0, size.saturating_sub(1)),
    };
    let len = if size == 0 { 0 } else { end - start + 1 };
    builder = builder.status(status).header(header::CONTENT_LENGTH, HeaderValue::from(len));
    if method == Method::HEAD || len == 0 {
        return builder.body(Body::empty()).map_err(|e| ApiError::Internal(e.into()));
    }
    let body = Body::from_stream(futures_util::TryStreamExt::inspect_ok(chunked(file, start, len, CHUNK), move |c: &Vec<u8>| {
        if let Some(n) = &sent {
            n.fetch_add(c.len() as u64, std::sync::atomic::Ordering::Relaxed);
        }
    }));
    builder.body(body).map_err(|e| ApiError::Internal(e.into()))
}

/// Risposta per un contenuto già in memoria (segmenti HLS), con le richieste `Range`.
///
/// Il player nativo di Safari (iPhone, iPad, Mac) legge i segmenti a pezzi con `Range: bytes=a-b`: se arriva il
/// segmento intero al posto del pezzo, lo interpreta come i byte da `a` in poi, lo rifiuta e richiede lo stesso
/// segmento in loop ("Media failed to decode"). hls.js scarica i segmenti interi e non se ne accorge.
/// Restituisce la risposta e i byte effettivamente inviati.
pub fn serve_bytes(bytes: Vec<u8>, content_type: &str, cache_control: &str, headers: &HeaderMap) -> (Response, u64) {
    let size = bytes.len() as u64;
    let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok()).map(|r| parse_range(r, size));
    let builder =
        Response::builder().header(header::ACCEPT_RANGES, "bytes").header(header::CONTENT_TYPE, content_type).header(header::CACHE_CONTROL, cache_control);
    match range {
        Some(Some((s, e))) => {
            let part = bytes[s as usize..=e as usize].to_vec();
            let n = part.len() as u64;
            let r = builder
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_RANGE, format!("bytes {s}-{e}/{size}"))
                .header(header::CONTENT_LENGTH, HeaderValue::from(n))
                .body(Body::from(part))
                .unwrap();
            (r, n)
        }
        Some(None) => {
            let r = Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_RANGE, format!("bytes */{size}"))
                .body(Body::empty())
                .unwrap();
            (r, 0)
        }
        None => (builder.header(header::CONTENT_LENGTH, HeaderValue::from(size)).body(Body::from(bytes)).unwrap(), size),
    }
}

mod bytes_stream {
    //! Flusso di blocchi letti con `read_at` (pread) su un thread bloccante: nessuno stato di posizione
    //! condiviso, più richieste Range in parallelo sullo stesso descrittore.
    use futures_util::stream::{self, Stream};
    use std::fs::File;
    use std::sync::Arc;

    pub fn chunked(file: Arc<File>, start: u64, len: u64, chunk: usize) -> impl Stream<Item = Result<Vec<u8>, std::io::Error>> {
        stream::try_unfold((file, start, len), move |(file, pos, remaining)| async move {
            if remaining == 0 {
                return Ok(None);
            }
            let want = (remaining as usize).min(chunk);
            let f = file.clone();
            let buf = tokio::task::spawn_blocking(move || -> std::io::Result<Vec<u8>> {
                let mut buf = vec![0u8; want];
                let mut filled = 0;
                while filled < want {
                    #[cfg(unix)]
                    let n = std::os::unix::fs::FileExt::read_at(&*f, &mut buf[filled..], pos + filled as u64)?;
                    #[cfg(windows)]
                    let n = std::os::windows::fs::FileExt::seek_read(&*f, &mut buf[filled..], pos + filled as u64)?;
                    if n == 0 {
                        break;
                    }
                    filled += n;
                }
                buf.truncate(filled);
                Ok(buf)
            })
            .await
            .map_err(std::io::Error::other)??;
            if buf.is_empty() {
                return Ok(None);
            }
            let n = buf.len() as u64;
            Ok(Some((buf, (file, pos + n, remaining - n))))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=0-", 100), Some((0, 99)));
        assert_eq!(parse_range("bytes=10-19", 100), Some((10, 19)));
        assert_eq!(parse_range("bytes=-10", 100), Some((90, 99)));
        assert_eq!(parse_range("bytes=90-200", 100), Some((90, 99)));
        assert_eq!(parse_range("bytes=100-", 100), None);
        assert_eq!(parse_range("bytes=5-1", 100), None);
        assert_eq!(parse_range("items=0-1", 100), None);
    }

    #[test]
    fn bytes_with_range() {
        let data: Vec<u8> = (0..100u8).collect();
        let mut h = HeaderMap::new();
        let (r, n) = serve_bytes(data.clone(), "video/mp4", "no-cache", &h);
        assert_eq!((r.status(), n), (StatusCode::OK, 100));
        assert_eq!(r.headers()[header::ACCEPT_RANGES], "bytes");
        h.insert(header::RANGE, HeaderValue::from_static("bytes=10-19"));
        let (r, n) = serve_bytes(data.clone(), "video/mp4", "no-cache", &h);
        assert_eq!((r.status(), n), (StatusCode::PARTIAL_CONTENT, 10));
        assert_eq!(r.headers()[header::CONTENT_RANGE], "bytes 10-19/100");
        h.insert(header::RANGE, HeaderValue::from_static("bytes=200-"));
        let (r, _) = serve_bytes(data, "video/mp4", "no-cache", &h);
        assert_eq!(r.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    }

    #[test]
    fn not_modified_only_without_range() {
        let path = std::env::temp_dir().join(format!("platea-etag-{}", std::process::id()));
        std::fs::write(&path, [7u8; 64]).unwrap();
        let file = Arc::new(File::open(&path).unwrap());
        let tag = etag(64, 5);
        let mut h = HeaderMap::new();
        h.insert(header::IF_NONE_MATCH, HeaderValue::from_str(&tag).unwrap());
        let r = serve_file(file.clone(), 64, 5, "audio/mp4", &Method::GET, &h, None).unwrap();
        assert_eq!(r.status(), StatusCode::NOT_MODIFIED);
        // Chrome riconvalida i pezzi in cache: con Range si mandano sempre i byte.
        h.insert(header::RANGE, HeaderValue::from_static("bytes=32-"));
        let r = serve_file(file, 64, 5, "audio/mp4", &Method::GET, &h, None).unwrap();
        assert_eq!(r.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(r.headers()[header::CONTENT_RANGE], "bytes 32-63/64");
        let _ = std::fs::remove_file(path);
    }
}
