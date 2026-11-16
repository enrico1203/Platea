//! Chiamate del server verso un nodo. Ogni chiamata ha il suo tempo limite: un nodo lento o sparito non blocca
//! mai una richiesta di un client oltre il necessario.

use super::proto::{CreateHls, CreateHlsReply, Heartbeat, NodeStatus, ProbeReply, ProbeReq, PushConfig, PushWants, StatEntry, StatReq};
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum NodeCallError {
    /// Nodo non raggiungibile, tempo scaduto, risposta troncata.
    #[error("nodo non raggiungibile: {0}")]
    Network(String),
    /// Il nodo ha risposto con un errore.
    #[error("{code}: {message}")]
    Status { status: u16, code: String, message: String },
}

impl NodeCallError {
    pub fn status(&self) -> Option<u16> {
        match self {
            NodeCallError::Status { status, .. } => Some(*status),
            NodeCallError::Network(_) => None,
        }
    }
}

#[derive(Clone)]
pub struct NodeClient {
    pub http: reqwest::Client,
    pub base: String,
    pub key: String,
}

pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("Platea/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(3))
        .pool_idle_timeout(Duration::from_secs(60))
        .pool_max_idle_per_host(16)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client http dei nodi")
}

/// Indirizzo del nodo normalizzato: schema obbligatorio, niente barra finale.
pub fn normalize_url(raw: &str) -> Result<String, String> {
    let u = raw.trim().trim_end_matches('/');
    let with_scheme = if u.starts_with("http://") || u.starts_with("https://") { u.to_string() } else { format!("http://{u}") };
    let parsed = reqwest::Url::parse(&with_scheme).map_err(|e| format!("Indirizzo non valido: {e}"))?;
    if parsed.host_str().is_none_or(|h| h.is_empty()) {
        return Err("Indirizzo senza host".into());
    }
    if parsed.path() != "/" && !parsed.path().is_empty() {
        return Err("L'indirizzo del nodo non deve avere un percorso (es. http://100.64.0.2:8097)".into());
    }
    let mut s = format!("{}://{}", parsed.scheme(), parsed.host_str().unwrap_or_default());
    if parsed.host_str().is_some_and(|h| h.contains(':')) {
        s = format!("{}://[{}]", parsed.scheme(), parsed.host_str().unwrap_or_default());
    }
    Ok(match parsed.port() {
        Some(p) => format!("{s}:{p}"),
        None if !raw.contains("://") => format!("{s}:8097"),
        None => s,
    })
}

impl NodeClient {
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    fn req(&self, method: reqwest::Method, path: &str, timeout: Duration) -> reqwest::RequestBuilder {
        self.http.request(method, self.url(path)).bearer_auth(&self.key).timeout(timeout)
    }

    async fn check(resp: Result<reqwest::Response, reqwest::Error>) -> Result<reqwest::Response, NodeCallError> {
        let resp = resp.map_err(|e| NodeCallError::Network(short(&e)))?;
        if resp.status().is_success() {
            return Ok(resp);
        }
        let status = resp.status().as_u16();
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        Err(NodeCallError::Status {
            status,
            code: body["error"].as_str().unwrap_or("NodeError").to_string(),
            message: body["message"].as_str().unwrap_or("errore del nodo").to_string(),
        })
    }

    async fn json<T: serde::de::DeserializeOwned>(resp: Result<reqwest::Response, reqwest::Error>) -> Result<T, NodeCallError> {
        Self::check(resp).await?.json::<T>().await.map_err(|e| NodeCallError::Network(format!("risposta non valida: {}", short(&e))))
    }

    pub async fn heartbeat(&self, hb: &Heartbeat, timeout: Duration) -> Result<NodeStatus, NodeCallError> {
        Self::json(self.req(reqwest::Method::POST, "/node/v1/heartbeat", timeout).json(hb).send().await).await
    }

    /// Il nodo prova a raggiungere il server a `url`. `Status 404` = nodo di una versione senza la prova.
    pub async fn probe(&self, url: &str) -> Result<ProbeReply, NodeCallError> {
        Self::json(self.req(reqwest::Method::POST, "/node/v1/probe", Duration::from_secs(10)).json(&ProbeReq { url: url.to_string() }).send().await).await
    }

    /// Modalità push: blocchi che il nodo aspetta (attesa lunga sul nodo fino a `wait_s`).
    pub async fn push_wants(&self, remote_id: &str, wait_s: u64) -> Result<PushWants, NodeCallError> {
        Self::json(self.req(reqwest::Method::GET, &format!("/node/v1/push/{remote_id}/wants?wait={wait_s}"), Duration::from_secs(wait_s + 15)).send().await)
            .await
    }

    pub async fn push_chunk(&self, remote_id: &str, i: u64, body: Vec<u8>) -> Result<(), NodeCallError> {
        Self::check(self.req(reqwest::Method::PUT, &format!("/node/v1/push/{remote_id}/chunk/{i}"), Duration::from_secs(180)).body(body).send().await)
            .await
            .map(|_| ())
    }

    pub async fn health(&self) -> Result<(), NodeCallError> {
        Self::check(self.req(reqwest::Method::GET, "/node/v1/health", Duration::from_secs(5)).send().await).await.map(|_| ())
    }

    /// Prova di velocità server → nodo: il server manda dati per `dur`, il nodo misura.
    pub async fn speed_sink(&self, dur: Duration) -> Result<super::speed::Measure, NodeCallError> {
        let body = reqwest::Body::wrap_stream(super::speed::timed(dur));
        Self::json(self.req(reqwest::Method::POST, "/node/v1/speed/sink", dur + Duration::from_secs(20)).body(body).send().await).await
    }

    /// Prova di velocità nodo → server: dati casuali dal nodo (si legge solo per il tempo della misura).
    pub async fn speed_source(&self, bytes: u64) -> Result<reqwest::Response, NodeCallError> {
        let timeout = Duration::from_secs(super::speed::SECONDS + 20);
        Self::check(self.req(reqwest::Method::GET, &format!("/node/v1/speed/source?bytes={bytes}"), timeout).send().await).await
    }

    /// Il nodo scarica dal server a `req.url` e dice quanto ha ricevuto.
    pub async fn speed_pull(&self, req: &super::speed::PullReq) -> Result<super::speed::Measure, NodeCallError> {
        let timeout = Duration::from_secs(req.seconds + 30);
        Self::json(self.req(reqwest::Method::POST, "/node/v1/speed/pull", timeout).json(req).send().await).await
    }

    pub async fn push_config(&self, c: &PushConfig) -> Result<(), NodeCallError> {
        Self::check(self.req(reqwest::Method::PUT, "/node/v1/config", Duration::from_secs(5)).json(c).send().await).await.map(|_| ())
    }

    pub async fn hw_test(&self) -> Result<NodeStatus, NodeCallError> {
        Self::json(self.req(reqwest::Method::POST, "/node/v1/hw-test", Duration::from_secs(180)).send().await).await
    }

    pub async fn logs(&self, limit: usize, level: &str) -> Result<serde_json::Value, NodeCallError> {
        Self::json(self.req(reqwest::Method::GET, &format!("/node/v1/logs?limit={limit}&level={level}"), Duration::from_secs(5)).send().await).await
    }

    pub async fn restart(&self) -> Result<(), NodeCallError> {
        Self::check(self.req(reqwest::Method::POST, "/node/v1/restart", Duration::from_secs(5)).send().await).await.map(|_| ())
    }

    pub async fn create_hls(&self, c: &CreateHls) -> Result<CreateHlsReply, NodeCallError> {
        Self::json(self.req(reqwest::Method::POST, "/node/v1/hls", Duration::from_secs(15)).json(c).send().await).await
    }

    /// Segmento o init: il nodo aspetta FFmpeg fino a 60 s.
    pub async fn hls_bytes(&self, id: &str, part: &str) -> Result<(Vec<u8>, Option<String>), NodeCallError> {
        let resp = Self::check(self.req(reqwest::Method::GET, &format!("/node/v1/hls/{id}/{part}"), Duration::from_secs(90)).send().await).await?;
        let backend = resp.headers().get("x-platea-backend").and_then(|v| v.to_str().ok()).map(String::from);
        let bytes = resp.bytes().await.map_err(|e| NodeCallError::Network(short(&e)))?;
        Ok((bytes.to_vec(), backend))
    }

    pub async fn delete_hls(&self, id: &str) {
        let _ = self.req(reqwest::Method::DELETE, &format!("/node/v1/hls/{id}"), Duration::from_secs(5)).send().await;
    }

    pub async fn stat(&self, paths: Vec<String>) -> Result<Vec<StatEntry>, NodeCallError> {
        Self::json(self.req(reqwest::Method::POST, "/node/v1/fs/stat", Duration::from_secs(30)).json(&StatReq { paths }).send().await).await
    }

    pub async fn browse(&self, path: Option<&str>) -> Result<serde_json::Value, NodeCallError> {
        let q = path.map(|p| format!("?path={}", urlencoding::encode(p))).unwrap_or_default();
        Self::json(self.req(reqwest::Method::GET, &format!("/node/v1/fs/browse{q}"), Duration::from_secs(20)).send().await).await
    }

    /// Richiesta di un file (con Range): la risposta si passa al client così com'è, in streaming.
    pub async fn file(&self, path: &str, method: &axum::http::Method, headers: &axum::http::HeaderMap) -> Result<reqwest::Response, NodeCallError> {
        let m = if *method == axum::http::Method::HEAD { reqwest::Method::HEAD } else { reqwest::Method::GET };
        // Nessun tempo limite sull'intero corpo (un film intero); la connessione ha il suo.
        let mut rb = self.http.request(m, self.url(&format!("/node/v1/file?path={}", urlencoding::encode(path)))).bearer_auth(&self.key);
        for h in [axum::http::header::RANGE, axum::http::header::IF_RANGE] {
            if let Some(v) = headers.get(&h) {
                rb = rb.header(h.as_str(), v.as_bytes());
            }
        }
        let resp = tokio::time::timeout(Duration::from_secs(30), rb.send()).await.map_err(|_| NodeCallError::Network("tempo scaduto".into()))?;
        let resp = resp.map_err(|e| NodeCallError::Network(short(&e)))?;
        let st = resp.status();
        if st.is_success() || st.as_u16() == 416 || st.as_u16() == 304 {
            return Ok(resp);
        }
        Self::check(Ok(resp)).await
    }
}

/// Errore di reqwest senza l'URL completo (che può contenere percorsi di file).
pub(crate) fn short(e: &reqwest::Error) -> String {
    let kind = if e.is_timeout() {
        "tempo scaduto"
    } else if e.is_connect() {
        "connessione rifiutata o host irraggiungibile"
    } else if e.is_decode() || e.is_body() {
        "risposta interrotta"
    } else {
        "errore di rete"
    };
    let inner = std::error::Error::source(e).map(|s| s.to_string()).unwrap_or_default();
    if inner.is_empty() {
        kind.to_string()
    } else {
        format!("{kind} ({})", inner.chars().take(160).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn urls() {
        assert_eq!(super::normalize_url("100.64.0.2").unwrap(), "http://100.64.0.2:8097");
        assert_eq!(super::normalize_url("100.64.0.2:9000/").unwrap(), "http://100.64.0.2:9000");
        assert_eq!(super::normalize_url("http://nodo.lan:8097").unwrap(), "http://nodo.lan:8097");
        assert_eq!(super::normalize_url("https://nodo.example.com").unwrap(), "https://nodo.example.com");
        assert!(super::normalize_url("http://nodo/x").is_err());
        assert!(super::normalize_url("").is_err());
    }
}
