//! Lettura di un file dalla copia migliore: questo server o un nodo che ha la cartella in copia.
//! Le richieste Range passano dal server (il client parla solo con lui); se una copia non risponde si prova la
//! successiva prima di mandare un solo byte al client. Con la consegna diretta attiva, direct play e download
//! ricevono invece un link firmato verso il nodo.

use crate::api::playback::FileInfo;
use crate::error::{ApiError, ApiResult};
use crate::state::Shared;
use axum::body::Body;
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::Response;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Duration;

/// Validità di un link di consegna diretta.
const DIRECT_VALID_S: i64 = 6 * 3600;

pub struct Served {
    pub response: Response,
    /// Copia usata (id del nodo; quello del server per il file locale).
    pub node: i64,
}

/// Cosa può fare la risposta.
#[derive(Clone, Copy, Default)]
pub struct Options {
    /// Copia da preferire (stessa del giro precedente).
    pub sticky: Option<i64>,
    /// Consentito un redirect verso un nodo con indirizzo pubblico.
    pub allow_redirect: bool,
    /// Nome del file per lo scaricamento (consegna diretta).
    pub download: bool,
}

pub async fn serve(state: &Shared, info: &FileInfo, method: &Method, headers: &HeaderMap, opts: Options, sent: Option<Arc<AtomicU64>>) -> ApiResult<Served> {
    let local = state.nodes.local_id();
    if let Some(url) = &info.url {
        return Ok(Served { response: crate::stream::proxy(&state.http, url, method, headers).await?, node: local });
    }
    let order = if state.nodes.has_remote() { super::copy_order(state, info.library_path_id, info.id, opts.sticky, &[]) } else { vec![local] };
    let mut last_err: Option<ApiError> = None;
    for id in order {
        if id == local {
            match serve_local(state, info, method, headers, sent.clone()).await {
                Ok(r) => return Ok(Served { response: r, node: local }),
                Err(e) => {
                    if state.nodes.has_remote() {
                        tracing::warn!(file = info.id, error = %e, "copia locale non leggibile: provo le altre");
                    }
                    last_err = Some(e);
                    continue;
                }
            }
        }
        let Some(row) = state.nodes.row(id) else { continue };
        let Some(root) = state.nodes.node_path(id, info.library_path_id) else { continue };
        let path = super::join_node_path(&root, &info.rel);
        if !super::verify_copy(state, &row, info.id, &path, info.size).await {
            continue;
        }
        let s = state.settings.get();
        if opts.allow_redirect && s.nodes.direct_delivery && !row.public_url.trim().is_empty() {
            let name = info.abs.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "video".into());
            let url = signed_url(&row.public_url, &row.key, &path, &name, opts.download, crate::util::now_ms() / 1000 + DIRECT_VALID_S);
            let r = Response::builder()
                .status(StatusCode::FOUND)
                .header(header::LOCATION, url)
                .header(header::CACHE_CONTROL, "no-store")
                .body(Body::empty())
                .unwrap();
            return Ok(Served { response: r, node: id });
        }
        match row.client(&state.nodes.http).file(&path, method, headers).await {
            Ok(resp) => return Ok(Served { response: to_response(resp, method, sent.clone()), node: id }),
            Err(e) => {
                tracing::warn!(node = %row.name, file = info.id, error = %e, "copia sul nodo non leggibile: provo la successiva");
                state.nodes.record_failure(id, &e.to_string());
                if matches!(e.status(), Some(404)) {
                    state.nodes.stat_store(id, info.id, false);
                }
                last_err = Some(ApiError::coded(StatusCode::BAD_GATEWAY, "NodeUnavailable", e.to_string()));
            }
        }
    }
    Err(last_err.unwrap_or_else(|| ApiError::coded(StatusCode::CONFLICT, "FileUnavailable", "Nessuna copia del file disponibile")))
}

async fn serve_local(state: &Shared, info: &FileInfo, method: &Method, headers: &HeaderMap, sent: Option<Arc<AtomicU64>>) -> ApiResult<Response> {
    let timeout = Duration::from_secs(state.settings.get().scanner.fs_timeout_seconds as u64);
    let f = crate::stream::open_library_file(info.id, &info.abs, &info.root, timeout).await?;
    let ct = crate::stream::content_type_for(&info.media.container, &info.abs);
    crate::stream::serve_file(f, info.size as u64, info.mtime, ct, method, headers, sent)
}

/// Risposta del nodo passata al client così com'è (stato, intestazioni utili, corpo in streaming).
fn to_response(resp: reqwest::Response, method: &Method, sent: Option<Arc<AtomicU64>>) -> Response {
    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut out = Response::builder().status(status).header(header::CACHE_CONTROL, "private, max-age=0");
    for h in [header::CONTENT_TYPE, header::CONTENT_LENGTH, header::CONTENT_RANGE, header::ACCEPT_RANGES, header::LAST_MODIFIED, header::ETAG] {
        if let Some(v) = resp.headers().get(h.as_str()) {
            if let Ok(v) = HeaderValue::from_bytes(v.as_bytes()) {
                out = out.header(h, v);
            }
        }
    }
    let body = if *method == Method::HEAD {
        Body::empty()
    } else {
        Body::from_stream(futures_util::StreamExt::map(resp.bytes_stream(), move |r| {
            if let (Ok(b), Some(n)) = (&r, &sent) {
                n.fetch_add(b.len() as u64, std::sync::atomic::Ordering::Relaxed);
            }
            r
        }))
    };
    out.body(body).unwrap()
}

/// Firma di un link di consegna diretta: HMAC-SHA256 con la chiave del nodo su scadenza e percorso.
pub fn sign(key: &str, exp: i64, path: &str) -> String {
    hex::encode(crate::util::hmac_sha256(key.as_bytes(), format!("{exp}\n{path}").as_bytes()))
}

pub fn signed_url(public_url: &str, key: &str, path: &str, name: &str, download: bool, exp: i64) -> String {
    format!(
        "{}/node/v1/d/{exp}/{}/{}?path={}{}",
        public_url.trim().trim_end_matches('/'),
        sign(key, exp, path),
        urlencoding::encode(name),
        urlencoding::encode(path),
        if download { "&dl=1" } else { "" }
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn signed_links() {
        let u = super::signed_url("https://nodo.example.com/", "k", "/media/film/a b.mkv", "a b.mkv", true, 1000);
        assert!(u.starts_with("https://nodo.example.com/node/v1/d/1000/"));
        assert!(u.ends_with("/a%20b.mkv?path=%2Fmedia%2Ffilm%2Fa%20b.mkv&dl=1"));
        assert_eq!(super::sign("k", 1000, "/x"), super::sign("k", 1000, "/x"));
        assert_ne!(super::sign("k", 1000, "/x"), super::sign("k", 1001, "/x"));
        assert_ne!(super::sign("k", 1000, "/x"), super::sign("j", 1000, "/x"));
    }
}
