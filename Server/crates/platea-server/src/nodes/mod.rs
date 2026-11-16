//! Nodi visti dal server: registro (dal database), stato dal vivo (heartbeat), scelta di chi transcodifica e da
//! quale copia leggere un file. Il nodo vero e proprio (l'altra macchina) è in `crate::node`.

pub mod cdn;
pub mod client;
pub mod hls;
pub mod proto;
pub mod route;

use crate::state::Shared;
use crate::util::now_ms;
use client::{NodeCallError, NodeClient};
use proto::{Heartbeat, NodeConfig, NodeStatus, PushConfig, PROTOCOL};
use route::{Candidate, Job, Rules};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

/// Interruttore: tante transcodifiche fallite in poco tempo → nodo escluso per un po', poi una di prova.
const BREAKER_FAILURES: usize = 3;
const BREAKER_WINDOW_MS: i64 = 5 * 60_000;
const BREAKER_OPEN_MS: i64 = 2 * 60_000;
/// Verifica di una copia (dimensione del file sul nodo uguale a quella nel database): vale per un po'.
const STAT_TTL_MS: i64 = 10 * 60_000;

#[derive(Debug, Clone, Serialize)]
pub struct NodeRow {
    pub id: i64,
    pub name: String,
    pub is_local: bool,
    pub url: String,
    #[serde(skip)]
    pub key: String,
    pub enabled: bool,
    pub draining: bool,
    pub role_transcode: bool,
    pub role_cdn: bool,
    pub tc_priority: i64,
    pub tc_weight: u32,
    pub tc_max_sessions: u32,
    pub rules: Rules,
    pub cdn_priority: i64,
    pub cdn_weight: u32,
    pub cdn_max_streams: u32,
    pub config: NodeConfig,
    pub public_url: String,
    pub updated_at: i64,
}

impl NodeRow {
    pub fn client(&self, http: &reqwest::Client) -> NodeClient {
        NodeClient { http: http.clone(), base: self.url.clone(), key: self.key.clone() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Health {
    /// Mai raggiunto dall'avvio del server.
    Unknown,
    Online,
    Offline,
    /// Raggiungibile ma con una versione del protocollo diversa: va aggiornato.
    Incompatible,
    /// Chiave sbagliata o rete non ammessa.
    Unauthorized,
}

#[derive(Debug, Clone)]
pub struct Live {
    pub health: Health,
    pub fails: u32,
    pub last_ok: i64,
    pub last_error: Option<String>,
    pub latency_ms: u64,
    pub status: Option<NodeStatus>,
    /// Revisione della configurazione già spinta (updated_at della riga).
    pub config_sent: i64,
    pub breaker_until: i64,
    pub failures: VecDeque<i64>,
}

impl Default for Live {
    fn default() -> Self {
        Live {
            health: Health::Unknown,
            fails: 0,
            last_ok: 0,
            last_error: None,
            latency_ms: 0,
            status: None,
            config_sent: 0,
            breaker_until: 0,
            failures: VecDeque::new(),
        }
    }
}

/// Sorgente letta da un nodo tramite il server (`/api/v1/node-source/{token}`): valida finché la sessione vive.
#[derive(Clone)]
pub struct SourceGrant {
    pub file_id: i64,
    pub session_id: String,
}

pub struct NodeManager {
    rows: RwLock<Arc<Vec<NodeRow>>>,
    /// library_path_id → [(node_id, cartella sul nodo)].
    paths: RwLock<Arc<HashMap<i64, Vec<(i64, String)>>>>,
    live: Mutex<HashMap<i64, Live>>,
    pub http: reqwest::Client,
    pub sources: Mutex<HashMap<String, SourceGrant>>,
    /// (node_id, file_id) → (copia valida, quando verificata).
    stats: Mutex<HashMap<(i64, i64), (bool, i64)>>,
    /// Ultima copia usata per (dispositivo, file): le richieste Range dello stesso player restano sulla stessa copia.
    sticky: Mutex<HashMap<(i64, i64), (i64, i64)>>,
    /// Stato delle sessioni remote dall'ultimo heartbeat: id remoto → stato.
    pub remote_status: Mutex<HashMap<String, proto::RemoteSessionStatus>>,
}

impl Default for NodeManager {
    fn default() -> Self {
        NodeManager {
            rows: Default::default(),
            paths: Default::default(),
            live: Default::default(),
            http: client::http_client(),
            sources: Default::default(),
            stats: Default::default(),
            sticky: Default::default(),
            remote_status: Default::default(),
        }
    }
}

#[derive(sqlx::FromRow)]
struct DbRow {
    id: i64,
    name: String,
    is_local: i64,
    url: String,
    key: String,
    enabled: i64,
    draining: i64,
    role_transcode: i64,
    role_cdn: i64,
    tc_priority: i64,
    tc_weight: i64,
    tc_max_sessions: i64,
    rules: String,
    cdn_priority: i64,
    cdn_weight: i64,
    cdn_max_streams: i64,
    config: String,
    public_url: String,
    updated_at: i64,
}

impl NodeManager {
    pub async fn reload(&self, db: &crate::db::Db) -> anyhow::Result<()> {
        let rows: Vec<DbRow> = sqlx::query_as(
            "SELECT id, name, is_local, url, key, enabled, draining, role_transcode, role_cdn, tc_priority, tc_weight, tc_max_sessions, rules,
                    cdn_priority, cdn_weight, cdn_max_streams, config, public_url, updated_at FROM nodes ORDER BY is_local DESC, id",
        )
        .fetch_all(&db.read)
        .await?;
        let rows: Vec<NodeRow> = rows
            .into_iter()
            .map(|r| NodeRow {
                id: r.id,
                name: r.name,
                is_local: r.is_local != 0,
                url: r.url,
                key: r.key,
                enabled: r.enabled != 0,
                draining: r.draining != 0,
                role_transcode: r.role_transcode != 0,
                role_cdn: r.role_cdn != 0,
                tc_priority: r.tc_priority,
                tc_weight: r.tc_weight.max(0) as u32,
                tc_max_sessions: r.tc_max_sessions.max(0) as u32,
                rules: serde_json::from_str(&r.rules).unwrap_or_default(),
                cdn_priority: r.cdn_priority,
                cdn_weight: r.cdn_weight.max(0) as u32,
                cdn_max_streams: r.cdn_max_streams.max(0) as u32,
                config: serde_json::from_str(&r.config).unwrap_or_default(),
                public_url: r.public_url,
                updated_at: r.updated_at,
            })
            .collect();
        let paths: Vec<(i64, i64, String)> = sqlx::query_as("SELECT node_id, library_path_id, node_path FROM node_paths").fetch_all(&db.read).await?;
        let mut map: HashMap<i64, Vec<(i64, String)>> = HashMap::new();
        for (node, lp, p) in paths {
            map.entry(lp).or_default().push((node, p));
        }
        let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
        self.live.lock().unwrap().retain(|k, _| ids.contains(k));
        *self.rows.write().unwrap() = Arc::new(rows);
        *self.paths.write().unwrap() = Arc::new(map);
        self.stats.lock().unwrap().clear();
        Ok(())
    }

    pub fn rows(&self) -> Arc<Vec<NodeRow>> {
        self.rows.read().unwrap().clone()
    }

    pub fn row(&self, id: i64) -> Option<NodeRow> {
        self.rows().iter().find(|r| r.id == id).cloned()
    }

    pub fn local_id(&self) -> i64 {
        self.rows().iter().find(|r| r.is_local).map(|r| r.id).unwrap_or(0)
    }

    /// Ci sono nodi remoti attivi? Senza, tutto resta com'era (nessun costo aggiunto).
    pub fn has_remote(&self) -> bool {
        self.rows().iter().any(|r| !r.is_local && r.enabled)
    }

    /// Ci sono nodi configurati (anche spenti)? Senza nessuno il server fa tutto da sé, come prima dei nodi.
    pub fn has_any_remote(&self) -> bool {
        self.rows().iter().any(|r| !r.is_local)
    }

    pub fn live(&self, id: i64) -> Live {
        self.live.lock().unwrap().get(&id).cloned().unwrap_or_default()
    }

    pub fn online(&self, id: i64) -> bool {
        let l = self.live.lock().unwrap();
        l.get(&id).is_some_and(|l| l.health == Health::Online)
    }

    fn breaker_closed(&self, id: i64) -> bool {
        self.live.lock().unwrap().get(&id).is_none_or(|l| l.breaker_until <= now_ms())
    }

    /// Un lavoro fallito su un nodo (transcodifica, lettura): dopo troppi in poco tempo il nodo resta fuori un po'.
    pub fn record_failure(&self, id: i64, why: &str) -> bool {
        let now = now_ms();
        let mut l = self.live.lock().unwrap();
        let e = l.entry(id).or_default();
        e.failures.push_back(now);
        while e.failures.front().is_some_and(|t| now - t > BREAKER_WINDOW_MS) {
            e.failures.pop_front();
        }
        e.last_error = Some(why.chars().take(300).collect());
        if e.failures.len() >= BREAKER_FAILURES && e.breaker_until <= now {
            e.breaker_until = now + BREAKER_OPEN_MS;
            e.failures.clear();
            tracing::warn!(node = id, error = why, "troppi errori: nodo escluso per 2 minuti");
            return true;
        }
        false
    }

    /// Cartella del nodo per una cartella di libreria, se la ha in copia.
    pub fn node_path(&self, node_id: i64, library_path_id: i64) -> Option<String> {
        self.paths.read().unwrap().get(&library_path_id)?.iter().find(|(n, _)| *n == node_id).map(|(_, p)| p.clone())
    }

    pub fn copies_of(&self, library_path_id: i64) -> Vec<(i64, String)> {
        self.paths.read().unwrap().get(&library_path_id).cloned().unwrap_or_default()
    }

    pub fn stat_cached(&self, node: i64, file: i64) -> Option<bool> {
        let s = self.stats.lock().unwrap();
        s.get(&(node, file)).filter(|(_, at)| now_ms() - at < STAT_TTL_MS).map(|(ok, _)| *ok)
    }

    pub fn stat_store(&self, node: i64, file: i64, ok: bool) {
        let mut s = self.stats.lock().unwrap();
        if s.len() > 50_000 {
            s.clear();
        }
        s.insert((node, file), (ok, now_ms()));
    }

    pub fn sticky_get(&self, key: (i64, i64)) -> Option<i64> {
        let s = self.sticky.lock().unwrap();
        s.get(&key).filter(|(_, at)| now_ms() - at < 30 * 60_000).map(|(n, _)| *n)
    }

    pub fn sticky_set(&self, key: (i64, i64), node: i64) {
        let mut s = self.sticky.lock().unwrap();
        let now = now_ms();
        if s.len() > 10_000 {
            s.retain(|_, (_, at)| now - *at < 30 * 60_000);
        }
        s.insert(key, (node, now));
    }

    /// Nodo che può ricevere lavoro adesso (escluso il server, sempre disponibile).
    fn usable(&self, r: &NodeRow) -> bool {
        r.enabled && !r.draining && (r.is_local || (self.online(r.id) && self.breaker_closed(r.id)))
    }
}

/// Almeno un nodo di transcodifica raggiungibile con FFmpeg (anche se il server non ha FFmpeg).
pub fn any_transcoder_online(state: &Shared) -> bool {
    state
        .nodes
        .rows()
        .iter()
        .filter(|r| !r.is_local && r.enabled && r.role_transcode && !r.draining)
        .any(|r| state.nodes.online(r.id) && state.nodes.live(r.id).status.is_some_and(|s| s.ffmpeg_available))
}

/// Lavori in corso per esecutore: (transcodifiche e remux, letture dalle copie).
pub fn active_counts(state: &Shared) -> (HashMap<i64, usize>, HashMap<i64, usize>) {
    let mut tc: HashMap<i64, usize> = HashMap::new();
    let mut cdn: HashMap<i64, usize> = HashMap::new();
    for s in state.sessions.all() {
        if let Some(h) = &s.hls {
            *tc.entry(h.executor_id()).or_default() += 1;
        }
        let c = s.copy_node.load(std::sync::atomic::Ordering::Relaxed);
        if c != 0 {
            *cdn.entry(c).or_default() += 1;
        }
    }
    (tc, cdn)
}

/// Esecutori che accettano il lavoro, nell'ordine in cui provarli (il primo è la scelta, poi il failover).
/// `local_copy`: per ogni nodo, se ha il file in una cartella propria.
pub fn transcode_order(state: &Shared, job: &Job, library_path_id: i64, file_id: i64, exclude: &[i64]) -> Vec<i64> {
    let m = &state.nodes;
    let s = state.settings.get();
    let (active, _) = active_counts(state);
    let local_encoders = crate::api::playback::allowed_encoders(state);
    let mut cands = Vec::new();
    for r in m.rows().iter() {
        if !r.role_transcode || exclude.contains(&r.id) || !m.usable(r) {
            continue;
        }
        if r.rules.accepts(job).is_err() {
            continue;
        }
        let has_local = if r.is_local { true } else { m.node_path(r.id, library_path_id).is_some() && m.stat_cached(r.id, file_id) != Some(false) };
        // Un remux su un nodo ha senso solo se il nodo ha il file: altrimenti farebbe solo da passacarte.
        if !job.transcode && !has_local {
            continue;
        }
        if job.transcode {
            let (encoders, hw) = if r.is_local {
                (local_encoders.clone(), Some(state.transcoder.current_profile()))
            } else {
                let st = m.live(r.id).status;
                (st.as_ref().map(|s| s.encoders.clone()).unwrap_or_default(), st.and_then(|s| s.current))
            };
            if let Some(out) = &job.out_codec {
                if !encoders.contains(out) {
                    continue;
                }
                if r.rules.require_hw {
                    let ok = hw.is_some_and(|h| {
                        h.backend != crate::transcode::command::Backend::Software && h.encode.contains(out) && h.can_decode(&job.codec, job.bit_depth)
                    });
                    if !ok {
                        continue;
                    }
                }
            }
        }
        cands.push(Candidate {
            id: r.id,
            priority: r.tc_priority,
            weight: r.tc_weight,
            active: active.get(&r.id).copied().unwrap_or(0),
            limit: r.tc_max_sessions as usize,
            has_local: has_local && !r.is_local,
            is_server: r.is_local,
        });
    }
    route::order(cands, s.nodes.strategy, s.nodes.prefer_local_copy)
}

/// Copie da cui leggere un file, in ordine di prova. Il server (se ha il ruolo CDN o come ultima risorsa) è sempre
/// in fondo all'elenco se non c'è già.
pub fn copy_order(state: &Shared, library_path_id: i64, file_id: i64, sticky: Option<i64>, exclude: &[i64]) -> Vec<i64> {
    let m = &state.nodes;
    let s = state.settings.get();
    let local = m.local_id();
    let copies = m.copies_of(library_path_id);
    if copies.is_empty() {
        return vec![local];
    }
    let (_, active) = active_counts(state);
    let rows = m.rows();
    let mut cands = Vec::new();
    for r in rows.iter() {
        if !r.role_cdn || exclude.contains(&r.id) || !m.usable(r) {
            continue;
        }
        if !r.is_local && (!copies.iter().any(|(n, _)| *n == r.id) || m.stat_cached(r.id, file_id) == Some(false)) {
            continue;
        }
        cands.push(Candidate {
            id: r.id,
            priority: r.cdn_priority,
            weight: r.cdn_weight,
            active: active.get(&r.id).copied().unwrap_or(0),
            limit: r.cdn_max_streams as usize,
            has_local: false,
            is_server: r.is_local,
        });
    }
    let mut order = route::order(cands, s.nodes.strategy, false);
    if let Some(st) = sticky.filter(|s| order.contains(s)) {
        order.retain(|x| *x != st);
        order.insert(0, st);
    }
    if !order.contains(&local) && !exclude.contains(&local) {
        order.push(local);
    }
    order
}

/// Controllo periodico dei nodi: heartbeat, configurazione, stato. Gira sempre, costa nulla senza nodi.
pub async fn run_health(state: Shared) {
    if let Err(e) = state.nodes.reload(&state.db).await {
        tracing::warn!(error = %e, "lettura dei nodi non riuscita");
    }
    loop {
        let every = state.settings.get().nodes.heartbeat_seconds.clamp(2, 60) as u64;
        let rows = state.nodes.rows();
        let mut tasks = Vec::new();
        for r in rows.iter().filter(|r| !r.is_local && r.enabled) {
            let r = r.clone();
            let st = state.clone();
            tasks.push(tokio::spawn(async move { check_node(&st, &r).await }));
        }
        for t in tasks {
            let _ = t.await;
        }
        tokio::time::sleep(Duration::from_secs(every)).await;
    }
}

/// Un controllo: heartbeat con le sessioni vive, stato, eventuale configurazione da spingere.
pub async fn check_node(state: &Shared, r: &NodeRow) -> Health {
    let sessions: Vec<String> = state.sessions.all().iter().filter_map(|s| s.hls.as_ref().and_then(|h| h.remote_id_on(r.id))).collect();
    let c = r.client(&state.nodes.http);
    let t0 = std::time::Instant::now();
    let res = c.heartbeat(&Heartbeat { sessions }, Duration::from_secs(4)).await;
    let latency = t0.elapsed().as_millis() as u64;
    let offline_after = state.settings.get().nodes.offline_after.max(1);
    let (before, after, push) = {
        let mut lives = state.nodes.live.lock().unwrap();
        let l = lives.entry(r.id).or_default();
        let before = l.health;
        let mut push = false;
        match &res {
            Ok(st) => {
                l.fails = 0;
                l.last_ok = now_ms();
                l.latency_ms = latency;
                if st.protocol != PROTOCOL {
                    l.health = Health::Incompatible;
                    l.last_error = Some(format!("Protocollo {} sul nodo, {} sul server: aggiorna il nodo", st.protocol, PROTOCOL));
                } else {
                    l.health = Health::Online;
                    if l.health != before {
                        l.last_error = None;
                    }
                    // Il nodo si è riavviato (rev 0) o la configurazione è cambiata.
                    push = st.config_rev != r.updated_at;
                }
                let mut rs = state.nodes.remote_status.lock().unwrap();
                for s in &st.sessions {
                    rs.insert(s.id.clone(), s.clone());
                }
                l.status = Some(st.clone());
            }
            Err(NodeCallError::Status { status: 401 | 403, message, .. }) => {
                l.fails += 1;
                l.health = Health::Unauthorized;
                l.last_error = Some(format!("Chiave rifiutata dal nodo: {message}"));
            }
            Err(e) => {
                l.fails += 1;
                l.last_error = Some(e.to_string());
                if l.fails >= offline_after || before == Health::Unknown {
                    l.health = Health::Offline;
                }
            }
        }
        (before, l.health, push)
    };
    if push {
        match c.push_config(&PushConfig { rev: r.updated_at, config: r.config.clone() }).await {
            Ok(()) => state.nodes.live.lock().unwrap().entry(r.id).or_default().config_sent = r.updated_at,
            Err(e) => tracing::warn!(node = %r.name, error = %e, "configurazione del nodo non inviata"),
        }
    }
    if before != after {
        on_health_change(state, r, before, after);
    }
    after
}

fn on_health_change(state: &Shared, r: &NodeRow, before: Health, after: Health) {
    state.events.admin("nodes.changed", serde_json::json!({ "id": r.id, "health": after }));
    let err = state.nodes.live(r.id).last_error.unwrap_or_default();
    match after {
        Health::Online => {
            tracing::info!(node = %r.name, "nodo online");
            if before != Health::Unknown {
                crate::webhooks::emit(
                    state,
                    crate::webhooks::Kind::Problem,
                    "node.online",
                    format!("Nodo {} di nuovo online", r.name),
                    serde_json::json!({ "node": r.name }),
                );
            }
        }
        Health::Offline | Health::Unauthorized | Health::Incompatible => {
            tracing::warn!(node = %r.name, error = %err, "nodo non disponibile");
            if before == Health::Online {
                crate::webhooks::emit(
                    state,
                    crate::webhooks::Kind::Problem,
                    "node.offline",
                    format!("Nodo {} non disponibile: {err}", r.name),
                    serde_json::json!({ "node": r.name, "error": err }),
                );
                let st = state.clone();
                let msg = format!("Nodo {} non disponibile: {err}", r.name);
                tokio::spawn(async move { st.db.activity(None, "node_offline", &msg, None).await });
            }
        }
        Health::Unknown => {}
    }
}

/// Stato ricevuto fuori dall'heartbeat (prova delle GPU).
pub fn store_status(state: &Shared, id: i64, st: NodeStatus) {
    let mut lives = state.nodes.live.lock().unwrap();
    let l = lives.entry(id).or_default();
    l.last_ok = now_ms();
    l.status = Some(st);
}

/// Indirizzo con cui un nodo raggiunge questo server.
pub fn server_base_for(state: &Shared, node_id: i64) -> Option<String> {
    let s = state.settings.get();
    let u = s.nodes.server_url.trim().trim_end_matches('/');
    if !u.is_empty() {
        return Some(u.to_string());
    }
    let ip: std::net::IpAddr = state.nodes.live(node_id).status?.peer_ip?.parse().ok()?;
    Some(match ip {
        std::net::IpAddr::V6(v6) if v6.to_ipv4_mapped().is_none() => format!("http://[{v6}]:{}", s.nodes.server_port),
        std::net::IpAddr::V6(v6) => format!("http://{}:{}", v6.to_ipv4_mapped().unwrap(), s.nodes.server_port),
        v4 => format!("http://{v4}:{}", s.nodes.server_port),
    })
}

/// Verifica una copia sul nodo: il file esiste con la stessa dimensione del database. Il risultato resta in cache.
pub async fn verify_copy(state: &Shared, node: &NodeRow, file_id: i64, node_file: &str, size: i64) -> bool {
    if let Some(ok) = state.nodes.stat_cached(node.id, file_id) {
        return ok;
    }
    let ok = match node.client(&state.nodes.http).stat(vec![node_file.to_string()]).await {
        Ok(v) => v.first().is_some_and(|e| e.exists && e.size == Some(size as u64)),
        Err(e) => {
            tracing::warn!(node = %node.name, error = %e, "verifica della copia non riuscita");
            return false;
        }
    };
    if !ok {
        tracing::warn!(node = %node.name, file = node_file, "copia assente o diversa sul nodo: si usa un'altra copia");
    }
    state.nodes.stat_store(node.id, file_id, ok);
    ok
}

/// Percorso del file sul nodo: cartella in copia + percorso relativo nella libreria.
pub fn join_node_path(node_root: &str, rel: &str) -> String {
    format!("{}/{}", node_root.trim_end_matches('/'), rel.trim_start_matches('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breaker_opens_after_repeated_failures() {
        let m = NodeManager::default();
        assert!(m.breaker_closed(7));
        assert!(!m.record_failure(7, "a"));
        assert!(!m.record_failure(7, "b"));
        assert!(m.record_failure(7, "c"));
        assert!(!m.breaker_closed(7));
        // Altri nodi non ne risentono.
        assert!(m.breaker_closed(8));
    }

    #[test]
    fn node_paths_join() {
        assert_eq!(join_node_path("/media/film/", "/A (2000)/a.mkv"), "/media/film/A (2000)/a.mkv");
        assert_eq!(join_node_path("/media/film", "a.mkv"), "/media/film/a.mkv");
    }
}
