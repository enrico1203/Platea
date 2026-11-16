//! Prova di velocità tra server e nodo. Tre misure, ognuna di qualche secondo con dati casuali (incomprimibili):
//! - server → nodo: il server manda dati al nodo sulla connessione verso il nodo (come la modalità push);
//! - nodo → server: il server scarica dal nodo (segmenti transcodificati, copie dei file);
//! - il nodo legge dal server: il nodo scarica dall'indirizzo del server con cui legge i file da transcodificare.

use super::client::NodeCallError;
use super::NodeRow;
use crate::state::Shared;
use crate::util::now_ms;
use axum::body::Bytes;
use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Durata di ogni misura.
pub const SECONDS: u64 = 5;
/// Byte chiesti al massimo per una misura in scaricamento (la misura si ferma comunque dopo `SECONDS`).
pub const MAX_BYTES: u64 = 16 << 30;
const CHUNK: usize = 1 << 20;
/// Validità di un token per la lettura dal server.
const TOKEN_MS: i64 = 60_000;

/// 1 MiB di dati casuali, generato una volta e riusato (Bytes si clona senza copiare).
fn chunk() -> Bytes {
    static C: OnceLock<Bytes> = OnceLock::new();
    C.get_or_init(|| {
        let mut v = vec![0u8; CHUNK];
        rand::Rng::fill(&mut rand::thread_rng(), &mut v[..]);
        Bytes::from(v)
    })
    .clone()
}

/// `total` byte di dati casuali.
pub fn sized(total: u64) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static {
    futures_util::stream::unfold(total, |left| async move {
        if left == 0 {
            return None;
        }
        let mut c = chunk();
        if (c.len() as u64) > left {
            c.truncate(left as usize);
        }
        let n = c.len() as u64;
        Some((Ok(c), left - n))
    })
}

/// Dati casuali fino allo scadere di `dur`.
pub fn timed(dur: Duration) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static {
    let end = Instant::now() + dur;
    futures_util::stream::unfold((), move |_| async move { (Instant::now() < end).then(|| (Ok(chunk()), ())) })
}

/// Esito di una misura, come lo calcola chi riceve.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Measure {
    pub bytes: u64,
    pub ms: u64,
    pub error: Option<String>,
}

impl Measure {
    fn failed(e: impl Into<String>) -> Self {
        Measure { error: Some(e.into()), ..Default::default() }
    }

    pub fn mbps(&self) -> Option<f64> {
        (self.error.is_none() && self.ms > 0).then(|| (self.bytes as f64 * 8.0 / 1000.0 / self.ms as f64 * 10.0).round() / 10.0)
    }
}

/// Riceve uno stream per al massimo `dur`. Il tempo parte dal primo blocco arrivato e i suoi byte non si contano:
/// così la misura è la banda, non il tempo di apertura della connessione.
pub async fn receive<S, B, E>(mut s: S, dur: Duration) -> Measure
where
    S: Stream<Item = Result<B, E>> + Unpin,
    B: AsRef<[u8]>,
    E: std::fmt::Display,
{
    let deadline = tokio::time::Instant::now() + dur;
    let mut start: Option<Instant> = None;
    let mut bytes = 0u64;
    loop {
        match tokio::time::timeout_at(deadline, s.next()).await {
            Err(_) | Ok(None) => break,
            Ok(Some(Ok(b))) => {
                if start.is_some() {
                    bytes += b.as_ref().len() as u64;
                } else {
                    start = Some(Instant::now());
                }
            }
            Ok(Some(Err(e))) => {
                if bytes == 0 {
                    return Measure::failed(e.to_string());
                }
                break;
            }
        }
    }
    match start {
        Some(t) => Measure { bytes, ms: t.elapsed().as_millis().max(1) as u64, error: None },
        None => Measure::failed("nessun dato ricevuto"),
    }
}

/// Richiesta al nodo: scarica dal server a `url` per `seconds`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PullReq {
    pub url: String,
    pub seconds: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Direction {
    pub mbps: Option<f64>,
    pub bytes: u64,
    pub ms: u64,
    pub error: Option<String>,
}

impl From<Measure> for Direction {
    fn from(m: Measure) -> Self {
        Direction { mbps: m.mbps(), bytes: m.bytes, ms: m.ms, error: m.error }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SpeedResult {
    pub at: i64,
    /// Tempo di risposta del nodo (il migliore di qualche prova).
    pub latency_ms: Option<u64>,
    pub to_node: Direction,
    pub from_node: Direction,
    /// Il nodo legge dal server all'indirizzo `server_url`. None = indirizzo del server non ancora noto.
    pub node_reads: Option<Direction>,
    pub server_url: Option<String>,
}

fn call_error(e: NodeCallError) -> String {
    match e.status() {
        Some(404) => "il nodo non ha la prova di velocità: aggiornalo".into(),
        _ => e.to_string(),
    }
}

fn tokens() -> &'static Mutex<HashMap<String, i64>> {
    static T: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();
    T.get_or_init(Default::default)
}

/// Token per `/api/v1/node-speed/{token}`: vale una volta sola, per poco.
fn new_token() -> String {
    let t = crate::util::random_hex(24);
    let mut m = tokens().lock().unwrap();
    let now = now_ms();
    m.retain(|_, exp| *exp > now);
    m.insert(t.clone(), now + TOKEN_MS);
    t
}

pub fn take_token(t: &str) -> bool {
    let mut m = tokens().lock().unwrap();
    m.remove(t).is_some_and(|exp| exp > now_ms())
}

/// Una prova alla volta per nodo: due insieme si dividerebbero la banda.
struct Running(i64);

impl Running {
    fn start(id: i64) -> Option<Running> {
        running().lock().unwrap().insert(id).then_some(Running(id))
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        running().lock().unwrap().remove(&self.0);
    }
}

fn running() -> &'static Mutex<HashSet<i64>> {
    static R: OnceLock<Mutex<HashSet<i64>>> = OnceLock::new();
    R.get_or_init(Default::default)
}

/// Prova completa (circa 3 × `SECONDS`). None = c'è già una prova in corso su questo nodo.
pub async fn run(state: &Shared, r: &NodeRow) -> Option<SpeedResult> {
    let _guard = Running::start(r.id)?;
    let client = r.client(&state.nodes.http);
    let dur = Duration::from_secs(SECONDS);

    let mut latency: Option<u64> = None;
    for _ in 0..5 {
        let t0 = Instant::now();
        if client.health().await.is_ok() {
            let ms = t0.elapsed().as_millis() as u64;
            latency = Some(latency.map_or(ms, |l| l.min(ms)));
        }
    }

    let to_node = match client.speed_sink(dur).await {
        Ok(m) => m,
        Err(e) => Measure::failed(call_error(e)),
    };
    let from_node = match client.speed_source(MAX_BYTES).await {
        Ok(resp) => receive(resp.bytes_stream(), dur).await,
        Err(e) => Measure::failed(call_error(e)),
    };
    let server_url = super::server_base_for(state, r.id);
    let node_reads = match &server_url {
        None => None,
        Some(base) => {
            let url = format!("{base}/api/v1/node-speed/{}", new_token());
            Some(match client.speed_pull(&PullReq { url, seconds: SECONDS }).await {
                Ok(m) => m,
                Err(e) => Measure::failed(call_error(e)),
            })
        }
    };
    let res = SpeedResult {
        at: now_ms(),
        latency_ms: latency,
        to_node: to_node.into(),
        from_node: from_node.into(),
        node_reads: node_reads.map(Into::into),
        server_url,
    };
    let fmt = |d: &Direction| d.mbps.map(|m| format!("{m} Mbit/s")).unwrap_or_else(|| d.error.clone().unwrap_or_default());
    tracing::info!(
        node = %r.name,
        to_node = %fmt(&res.to_node),
        from_node = %fmt(&res.from_node),
        node_reads = %res.node_reads.as_ref().map(fmt).unwrap_or_else(|| "—".into()),
        "prova di velocità del nodo"
    );
    state.nodes.live.lock().unwrap().entry(r.id).or_default().speed = Some(res.clone());
    Some(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sized_and_receive() {
        let total: usize = futures_util::StreamExt::collect::<Vec<_>>(sized(3 * CHUNK as u64 + 5)).await.into_iter().map(|r| r.unwrap().len()).sum();
        assert_eq!(total, 3 * CHUNK + 5);
        let m = receive(Box::pin(sized(4 * CHUNK as u64)), Duration::from_secs(5)).await;
        assert!(m.error.is_none());
        // Il primo blocco non si conta.
        assert_eq!(m.bytes, 3 * CHUNK as u64);
        let empty = receive(Box::pin(sized(0)), Duration::from_secs(1)).await;
        assert!(empty.error.is_some());
    }

    #[test]
    fn mbps() {
        let m = Measure { bytes: 125_000_000, ms: 1000, error: None };
        assert_eq!(m.mbps(), Some(1000.0));
        assert_eq!(Measure::failed("x").mbps(), None);
    }

    #[test]
    fn tokens_once() {
        let t = new_token();
        assert!(take_token(&t));
        assert!(!take_token(&t));
        assert!(!take_token("nope"));
    }
}
