//! API pubblica sotto `/api/v1`. È il prodotto: la web UI ufficiale non usa nulla che un client
//! di terzi non possa usare. Ogni route è registrata insieme alla sua descrizione, da cui nasce
//! il documento OpenAPI: codice e documentazione non possono divergere.

pub mod admin;
pub mod auth;
pub mod download;
pub mod events;
pub mod items;
pub mod jellyfin;
pub mod libraries;
pub mod playback;
pub mod playlists;
pub mod setup;
pub mod system;
pub mod users;

use crate::state::Shared;
use axum::body::Body;
use axum::extract::State;
use axum::handler::Handler;
use axum::http::{header, HeaderValue, Method, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::MethodRouter;
use axum::Router;
use serde_json::{json, Value};
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;

#[derive(Clone, Copy, PartialEq)]
pub enum Access {
    Public,
    User,
    Admin,
    /// Token breve della sessione di riproduzione nell'URL.
    Stream,
}

pub struct RouteDoc {
    pub method: &'static str,
    pub path: &'static str,
    pub summary: &'static str,
    pub tag: &'static str,
    pub access: Access,
}

pub struct Api {
    pub router: Router<Shared>,
    pub docs: Vec<RouteDoc>,
}

impl Api {
    pub fn new() -> Self {
        Api { router: Router::new(), docs: Vec::new() }
    }

    fn add(mut self, method: &'static str, path: &'static str, tag: &'static str, access: Access, summary: &'static str, mr: MethodRouter<Shared>) -> Self {
        self.router = self.router.route(path, mr);
        self.docs.push(RouteDoc { method, path, summary, tag, access });
        self
    }

    pub fn get<H: Handler<T, Shared>, T: 'static>(self, path: &'static str, tag: &'static str, access: Access, summary: &'static str, h: H) -> Self {
        self.add("get", path, tag, access, summary, axum::routing::get(h))
    }
    pub fn post<H: Handler<T, Shared>, T: 'static>(self, path: &'static str, tag: &'static str, access: Access, summary: &'static str, h: H) -> Self {
        self.add("post", path, tag, access, summary, axum::routing::post(h))
    }
    pub fn put<H: Handler<T, Shared>, T: 'static>(self, path: &'static str, tag: &'static str, access: Access, summary: &'static str, h: H) -> Self {
        self.add("put", path, tag, access, summary, axum::routing::put(h))
    }
    pub fn patch<H: Handler<T, Shared>, T: 'static>(self, path: &'static str, tag: &'static str, access: Access, summary: &'static str, h: H) -> Self {
        self.add("patch", path, tag, access, summary, axum::routing::patch(h))
    }
    pub fn delete<H: Handler<T, Shared>, T: 'static>(self, path: &'static str, tag: &'static str, access: Access, summary: &'static str, h: H) -> Self {
        self.add("delete", path, tag, access, summary, axum::routing::delete(h))
    }
}

impl Default for Api {
    fn default() -> Self {
        Self::new()
    }
}

fn openapi(docs: &[RouteDoc]) -> Value {
    let mut paths = serde_json::Map::new();
    for d in docs {
        let params: Vec<Value> = d
            .path
            .split('/')
            .filter_map(|seg| seg.strip_prefix('{').and_then(|s| s.strip_suffix('}')))
            .map(|name| json!({ "name": name, "in": "path", "required": true, "schema": { "type": "string" } }))
            .collect();
        let mut op = json!({
            "summary": d.summary,
            "tags": [d.tag],
            "parameters": params,
            "responses": {
                "200": { "description": "OK" },
                "400": { "description": "Richiesta non valida", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Error" } } } },
                "401": { "description": "Accesso richiesto" },
            }
        });
        match d.access {
            Access::Public => {}
            Access::User => op["security"] = json!([{ "token": [] }]),
            Access::Admin => {
                op["security"] = json!([{ "token": [] }]);
                op["x-admin"] = json!(true);
            }
            Access::Stream => op["security"] = json!([{ "streamToken": [] }]),
        }
        let entry = paths.entry(d.path.to_string()).or_insert_with(|| json!({}));
        entry[d.method] = op;
    }
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Platea API",
            "version": crate::state::VERSION,
            "description": "API pubblica di Platea, open source media server. Tutti i client (web UI, app, script) usano questa API.",
            "license": { "name": "Apache-2.0", "identifier": "Apache-2.0" }
        },
        "servers": [{ "url": "/" }],
        "components": {
            "securitySchemes": {
                "token": { "type": "http", "scheme": "bearer", "description": "Token restituito da POST /api/v1/auth/login" },
                "streamToken": { "type": "apiKey", "in": "query", "name": "t", "description": "Token breve della sessione di riproduzione" }
            },
            "schemas": {
                "Error": { "type": "object", "properties": { "error": { "type": "string" }, "message": { "type": "string" } }, "required": ["error", "message"] }
            }
        },
        "paths": paths
    })
}

pub fn router(state: Shared) -> Router {
    let api = Api::new();
    let api = system::routes(api);
    let api = setup::routes(api);
    let api = auth::routes(api);
    let api = users::routes(api);
    let api = libraries::routes(api);
    let api = items::routes(api);
    let api = playback::routes(api);
    let api = playlists::routes(api);
    let api = download::routes(api);
    let api = admin::routes(api);
    let api = events::routes(api);
    let doc = openapi(&api.docs);
    let doc_route = axum::routing::get(move || {
        let d = doc.clone();
        async move { axum::Json(d) }
    });

    // Limite di 16 MB a tutte le richieste tranne il caricamento di un database da ripristinare.
    let restore = Router::new()
        .route("/api/v1/admin/restore", axum::routing::post(admin::restore_upload))
        .layer(axum::extract::DefaultBodyLimit::disable())
        .layer(tower_http::limit::RequestBodyLimitLayer::new(64 * 1024 * 1024 * 1024));
    let mut app: Router<Shared> = api
        .router
        .route("/api/v1/openapi.json", doc_route)
        .merge(jellyfin::router())
        .layer(tower_http::limit::RequestBodyLimitLayer::new(16 * 1024 * 1024))
        .merge(restore)
        .fallback(fallback);

    if let Some(dir) = state.boot.web_dir.clone() {
        let index = dir.join("index.html");
        app = app.fallback_service(tower_http::services::ServeDir::new(dir).fallback(tower_http::services::ServeFile::new(index)));
    }

    app.layer(middleware::from_fn_with_state(state.clone(), cors))
        .layer(CompressionLayer::new().gzip(true).compress_when(compress_predicate()))
        .layer(TraceLayer::new_for_http().make_span_with(|req: &Request<Body>| {
            // Nel log solo il percorso: mai la query, che può contenere un token.
            tracing::info_span!("http", method = %req.method(), path = %req.uri().path())
        }))
        .with_state(state)
}

fn compress_predicate() -> impl tower_http::compression::Predicate {
    use tower_http::compression::predicate::{NotForContentType, Predicate, SizeAbove};
    // Niente compressione per video e segmenti: sarebbe CPU sprecata.
    SizeAbove::new(1024)
        .and(NotForContentType::new("video/"))
        .and(NotForContentType::new("image/"))
        .and(NotForContentType::const_new("application/vnd.apple.mpegurl"))
        .and(NotForContentType::const_new("application/octet-stream"))
}

async fn fallback(req: Request<Body>) -> Response {
    let path = req.uri().path().to_string();
    // Un client (Infuse, un'app Jellyfin) che chiama un percorso non implementato: utile per capire cosa manca.
    tracing::info!(method = %req.method(), path = %path, "percorso non gestito");
    (StatusCode::NOT_FOUND, axum::Json(json!({ "error": "NotFound", "message": format!("Percorso non trovato: {path}") }))).into_response()
}

/// CORS con origini consentite dalla dashboard, applicate a caldo.
async fn cors(State(state): State<Shared>, req: Request<Body>, next: Next) -> Response {
    let origin = req.headers().get(header::ORIGIN).and_then(|o| o.to_str().ok()).map(String::from);
    let allowed = origin.as_deref().is_some_and(|o| {
        let list = &state.settings.get().network.allowed_origins;
        list.iter().any(|a| a == "*" || a.eq_ignore_ascii_case(o))
    });
    if req.method() == Method::OPTIONS && origin.is_some() {
        let mut r = StatusCode::NO_CONTENT.into_response();
        if allowed {
            add_cors(r.headers_mut(), origin.as_deref().unwrap());
        }
        return r;
    }
    let mut r = next.run(req).await;
    if allowed {
        add_cors(r.headers_mut(), origin.as_deref().unwrap());
    }
    r
}

fn add_cors(h: &mut axum::http::HeaderMap, origin: &str) {
    if let Ok(v) = HeaderValue::from_str(origin) {
        h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, v);
    }
    h.insert(header::VARY, HeaderValue::from_static("Origin"));
    h.insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("GET, POST, PUT, PATCH, DELETE, OPTIONS, HEAD"));
    h.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("authorization, content-type, range, x-platea-token, x-emby-authorization, x-emby-token"),
    );
    h.insert(header::ACCESS_CONTROL_EXPOSE_HEADERS, HeaderValue::from_static("content-range, content-length, accept-ranges, etag"));
    h.insert(header::ACCESS_CONTROL_MAX_AGE, HeaderValue::from_static("86400"));
}

/// Paginazione con cursore: nessun endpoint restituisce l'intera libreria.
pub fn clamp_limit(limit: Option<i64>, default: i64) -> i64 {
    limit.unwrap_or(default).clamp(1, 200)
}
