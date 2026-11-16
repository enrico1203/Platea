//! Lettura a blocchi del file che il nodo riceve dal server.
//!
//! Senza questo modulo FFmpeg legge il file dal server in diretta, al ritmo della transcodifica: ogni rallentamento
//! della rete (o del mount del server) diventa un'attesa di FFmpeg. Qui il nodo scarica il file a blocchi da 8 MiB,
//! con più connessioni in parallelo e in anticipo rispetto a dove legge FFmpeg, e li tiene in una cache su disco.
//! FFmpeg legge dalla cache attraverso un indirizzo locale (`/node/v1/local-source/{token}`, solo da 127.0.0.1) che
//! risponde alle richieste Range e aspetta i blocchi non ancora arrivati (scaricandoli per primi).
//!
//! Modalità (impostazioni del nodo): `stream` (in diretta, come prima), `readahead` (finestra di lettura anticipata,
//! predefinita) e `full` (tutto il file, a partire da dove si guarda). I blocchi restano in cache dopo la sessione:
//! riprendere lo stesso film o tornare indietro non riscarica nulla. La cache ha un tetto e lascia sempre spazio libero.

use crate::nodes::proto::{NodeConfig, PushWants, SourceMode};
use std::collections::HashMap;
use std::os::unix::fs::FileExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;
use tokio::task::JoinSet;

/// Dimensione di un blocco. Abbastanza grande da ammortizzare la latenza di una richiesta su una rete lontana.
pub const CHUNK: u64 = 8 << 20;
/// Spazio libero da lasciare sempre sul disco della cache: 10 GB, o un decimo del disco se è piccolo.
const MIN_FREE: u64 = 10 << 30;

fn reserve(dir: &std::path::Path) -> u64 {
    crate::sysinfo::disk_bytes(dir).map(|t| (t / 10).min(MIN_FREE)).unwrap_or(MIN_FREE)
}
/// Blocchi urgenti (lettori fermi) chiesti insieme oltre il limite dei blocchi in parallelo.
const URGENT_MAX: usize = 4;
/// Errori di fila prima di arrendersi (FFmpeg riceve un errore e il server sposta la sessione).
const MAX_ERRORS: u32 = 12;
/// Attesa massima di un blocco da parte di FFmpeg.
const WAIT_CHUNK: Duration = Duration::from_secs(90);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Chunk {
    Missing,
    Fetching,
    Ready,
}

struct EntryState {
    chunks: Vec<Chunk>,
    ready: u64,
    /// Sessioni che usano il file: (id sessione, indirizzo del server valido per quella sessione).
    sessions: Vec<(String, String)>,
    /// Lettori attivi (connessioni di FFmpeg) → posizione.
    cursors: HashMap<u64, u64>,
    /// Ultima posizione letta: la lettura anticipata continua da qui anche tra una connessione e l'altra.
    last_pos: u64,
    mode: SourceMode,
    readahead: u64,
    parallel: usize,
    errors: u32,
    failed: Option<String>,
    running: bool,
    last_used: i64,
    downloaded: u64,
    started: Option<std::time::Instant>,
    /// Modalità push: blocchi chiesti al server → quando (se non arrivano entro un minuto si richiedono).
    pushed: HashMap<u64, i64>,
}

/// Un blocco chiesto al server in modalità push e non arrivato entro questo tempo torna da chiedere.
const PUSH_TIMEOUT_MS: i64 = 60_000;

pub struct Entry {
    key: String,
    path: PathBuf,
    size: u64,
    file: std::fs::File,
    st: Mutex<EntryState>,
    /// Un blocco è pronto (o è fallito): svegliare i lettori.
    progress: Notify,
    /// Qualcosa è cambiato (lettore nuovo o spostato, sessione aggiunta): svegliare chi scarica.
    wake: Notify,
}

impl Entry {
    pub fn size(&self) -> u64 {
        self.size
    }

    fn chunks(&self) -> u64 {
        self.size.div_ceil(CHUNK)
    }

    fn chunk_range(&self, i: u64) -> (u64, u64) {
        let start = i * CHUNK;
        (start, (start + CHUNK).min(self.size))
    }
}

pub struct Fetcher {
    dir: PathBuf,
    http: reqwest::Client,
    entries: Mutex<HashMap<String, Arc<Entry>>>,
    /// Token locale (indirizzo dato a FFmpeg) → (file, sessione).
    tokens: Mutex<HashMap<String, (Arc<Entry>, String)>>,
    next_cursor: AtomicU64,
}

/// Un indirizzo locale pronto per FFmpeg.
pub struct Attached {
    pub token: String,
    pub mode: SourceMode,
}

impl Fetcher {
    /// La cache non sopravvive a un riavvio del nodo (lo stato dei blocchi è in memoria): si riparte puliti.
    pub fn new(dir: &std::path::Path) -> Fetcher {
        let dir = dir.to_path_buf();
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let http = reqwest::Client::builder()
            .user_agent(concat!("Platea-node/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(16)
            .build()
            .expect("client http della cache");
        Fetcher { dir, http, entries: Mutex::new(HashMap::new()), tokens: Mutex::new(HashMap::new()), next_cursor: AtomicU64::new(1) }
    }

    /// Collega una sessione al file `key` (`size` byte, letto dal server a `url`). `None` = modalità in diretta,
    /// o la cache non si può usare: FFmpeg legge `url` come prima.
    /// `push`: i blocchi li manda il server (`url` vuoto); senza cache non si può, quindi niente modalità in diretta.
    pub fn attach(self: &Arc<Self>, key: &str, size: u64, url: &str, session: &str, cfg: &NodeConfig, push: bool) -> Option<Attached> {
        let url = if push { "" } else { url };
        if (cfg.source_mode == SourceMode::Stream && !push) || size == 0 || key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return None;
        }
        let free = crate::sysinfo::free_bytes(&self.dir).unwrap_or(0);
        let mut mode = if cfg.source_mode == SourceMode::Stream { SourceMode::Readahead } else { cfg.source_mode };
        let min_free = reserve(&self.dir);
        if mode == SourceMode::Full && free < size.saturating_add(min_free) {
            tracing::warn!(key, size, free, "spazio insufficiente per la copia completa: lettura anticipata");
            mode = SourceMode::Readahead;
        }
        let readahead = (cfg.readahead_mb.max(16) as u64) << 20;
        if free < readahead.min(size).saturating_add(min_free) {
            tracing::warn!(free, "poco spazio per la cache delle sorgenti: lettura in diretta");
            return None;
        }
        self.evict(cfg, Some(key));
        let entry = {
            let mut entries = self.entries.lock().unwrap();
            match entries.get(key).filter(|e| e.size == size) {
                Some(e) => e.clone(),
                None => {
                    let path = self.dir.join(format!("{key}.bin"));
                    let file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&path).ok()?;
                    file.set_len(size).ok()?;
                    let n = size.div_ceil(CHUNK) as usize;
                    let e = Arc::new(Entry {
                        key: key.to_string(),
                        path,
                        size,
                        file,
                        st: Mutex::new(EntryState {
                            chunks: vec![Chunk::Missing; n],
                            ready: 0,
                            sessions: vec![],
                            cursors: HashMap::new(),
                            last_pos: 0,
                            mode,
                            readahead,
                            parallel: 4,
                            errors: 0,
                            failed: None,
                            running: false,
                            last_used: crate::util::now_ms(),
                            downloaded: 0,
                            started: None,
                            pushed: HashMap::new(),
                        }),
                        progress: Notify::new(),
                        wake: Notify::new(),
                    });
                    entries.insert(key.to_string(), e.clone());
                    e
                }
            }
        };
        let start = {
            let mut st = entry.st.lock().unwrap();
            st.sessions.retain(|(s, _)| s != session);
            st.sessions.push((session.to_string(), url.to_string()));
            st.mode = mode;
            st.readahead = readahead;
            st.parallel = cfg.source_parallel.clamp(1, 16) as usize;
            st.errors = 0;
            st.failed = None;
            st.last_used = crate::util::now_ms();
            // Lo scaricamento serve solo se qualche sessione legge dal server (in push i blocchi arrivano da soli).
            let start = !st.running && st.ready < entry.chunks() && st.sessions.iter().any(|(_, u)| !u.is_empty());
            if start {
                st.running = true;
                st.started = Some(std::time::Instant::now());
                st.downloaded = 0;
            }
            start
        };
        if start {
            let me = self.clone();
            let e = entry.clone();
            tokio::spawn(async move { me.download(e).await });
        }
        entry.wake.notify_one();
        let token = crate::util::random_hex(24);
        self.tokens.lock().unwrap().insert(token.clone(), (entry, session.to_string()));
        Some(Attached { token, mode })
    }

    /// Fine della sessione: il file resta in cache (se c'è posto), lo scaricamento si ferma se nessuno lo usa più.
    pub fn detach(&self, session: &str, cfg: &NodeConfig) {
        let mut touched = Vec::new();
        self.tokens.lock().unwrap().retain(|_, (e, s)| {
            if s == session {
                touched.push(e.clone());
                false
            } else {
                true
            }
        });
        for e in touched {
            let mut st = e.st.lock().unwrap();
            st.sessions.retain(|(s, _)| s != session);
            st.last_used = crate::util::now_ms();
            drop(st);
            e.wake.notify_one();
        }
        self.evict(cfg, None);
    }

    fn entry_of_session(&self, session: &str) -> Option<Arc<Entry>> {
        self.tokens.lock().unwrap().values().find(|(_, s)| s == session).map(|(e, _)| e.clone())
    }

    /// Modalità push: blocchi che il server deve mandare adesso per la sessione. Aspetta fino a `wait` che ce ne siano.
    /// `None` = sessione non presente (chiusa): il server smette.
    pub async fn wants(&self, session: &str, wait: Duration) -> Option<PushWants> {
        let e = self.entry_of_session(session)?;
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            let progress = e.progress.notified();
            let wake = e.wake.notified();
            tokio::pin!(progress, wake);
            progress.as_mut().enable();
            wake.as_mut().enable();
            {
                let mut st = e.st.lock().unwrap();
                let now = crate::util::now_ms();
                let stale: Vec<u64> = st.pushed.iter().filter(|(_, t)| now - **t > PUSH_TIMEOUT_MS).map(|(i, _)| *i).collect();
                for i in stale {
                    st.pushed.remove(&i);
                    if st.chunks[i as usize] == Chunk::Fetching {
                        st.chunks[i as usize] = Chunk::Missing;
                    }
                }
                if st.ready == e.chunks() {
                    return Some(PushWants { chunk_size: CHUNK, chunks: vec![], complete: true });
                }
                let free = st.parallel.saturating_sub(st.pushed.len());
                let todo = wanted(&st, &e, free, true);
                if !todo.is_empty() {
                    for &i in &todo {
                        st.chunks[i as usize] = Chunk::Fetching;
                        st.pushed.insert(i, now);
                    }
                    if st.started.is_none() {
                        st.started = Some(std::time::Instant::now());
                    }
                    return Some(PushWants { chunk_size: CHUNK, chunks: todo, complete: false });
                }
            }
            // Sessione chiusa nel frattempo: `None`.
            self.entry_of_session(session)?;
            tokio::select! {
                _ = progress => {}
                _ = wake => {}
                _ = tokio::time::sleep_until(deadline) => return Some(PushWants { chunk_size: CHUNK, chunks: vec![], complete: false }),
            }
        }
    }

    /// Modalità push: un blocco mandato dal server.
    pub async fn put_chunk(&self, session: &str, i: u64, body: axum::body::Bytes) -> Result<(), String> {
        let e = self.entry_of_session(session).ok_or("sessione non presente")?;
        if i >= e.chunks() {
            return Err("blocco fuori dal file".into());
        }
        let (a, b) = e.chunk_range(i);
        if body.len() as u64 != b - a {
            return Err(format!("blocco di {} byte invece di {}", body.len(), b - a));
        }
        if e.st.lock().unwrap().chunks[i as usize] == Chunk::Ready {
            return Ok(());
        }
        let e2 = e.clone();
        let n = body.len() as u64;
        tokio::task::spawn_blocking(move || e2.file.write_all_at(&body, a)).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        {
            let mut st = e.st.lock().unwrap();
            st.pushed.remove(&i);
            if st.chunks[i as usize] != Chunk::Ready {
                st.chunks[i as usize] = Chunk::Ready;
                st.ready += 1;
                st.downloaded += n;
            }
        }
        e.progress.notify_waiters();
        Ok(())
    }

    pub fn entry_for(&self, token: &str) -> Option<Arc<Entry>> {
        self.tokens.lock().unwrap().get(token).map(|(e, _)| e.clone())
    }

    /// Byte in cache (blocchi pronti) e file in cache.
    pub fn usage(&self) -> (u64, usize) {
        let entries = self.entries.lock().unwrap();
        let bytes = entries.values().map(|e| e.st.lock().unwrap().ready * CHUNK).sum::<u64>();
        (bytes, entries.len())
    }

    /// Toglie i file meno usati di recente (mai quelli in uso) finché la cache sta nel tetto e il disco ha spazio.
    pub fn evict(&self, cfg: &NodeConfig, keep: Option<&str>) {
        let cap = (cfg.source_cache_gb as u64) << 30;
        let mut entries = self.entries.lock().unwrap();
        loop {
            let total: u64 = entries.values().map(|e| e.st.lock().unwrap().ready * CHUNK).sum();
            let free = crate::sysinfo::free_bytes(&self.dir).unwrap_or(u64::MAX);
            let idle: Vec<(i64, String, u64)> = entries
                .values()
                .filter(|e| Some(e.key.as_str()) != keep)
                .filter_map(|e| {
                    let st = e.st.lock().unwrap();
                    (st.sessions.is_empty() && !st.running).then(|| (st.last_used, e.key.clone(), st.ready * CHUNK))
                })
                .collect();
            // Tetto 0 = niente cache tra una sessione e l'altra.
            let over = if cap == 0 { !idle.is_empty() } else { total > cap || free < reserve(&self.dir) };
            if !over {
                break;
            }
            let Some((_, key, bytes)) = idle.into_iter().min() else {
                break;
            };
            if let Some(e) = entries.remove(&key) {
                let _ = std::fs::remove_file(&e.path);
                tracing::info!(key, mb = bytes >> 20, "sorgente tolta dalla cache");
            }
        }
    }

    /// Scarica i blocchi che servono, finché qualche sessione usa il file.
    async fn download(self: Arc<Self>, e: Arc<Entry>) {
        let mut set: JoinSet<(u64, Result<u64, String>)> = JoinSet::new();
        // Dopo un errore si aspetta un po' (sempre di più) prima di richiedere: la sessione appena aperta può non
        // essere ancora registrata sul server, e i blocchi in parallelo non devono bruciare i tentativi in un istante.
        let mut retry_at = tokio::time::Instant::now();
        loop {
            // Blocchi da chiedere adesso, nell'ordine in cui serviranno.
            let (todo, url) = {
                let mut st = e.st.lock().unwrap();
                let url = st.sessions.iter().rev().find(|(_, u)| !u.is_empty()).map(|(_, u)| u.clone());
                if url.is_none() || st.failed.is_some() || st.ready == e.chunks() {
                    if set.is_empty() {
                        // Sotto lo stesso lucchetto di `attach`: una sessione che arriva adesso ne fa partire un altro.
                        st.running = false;
                        break;
                    }
                    (vec![], None)
                } else {
                    let pause = tokio::time::Instant::now() < retry_at;
                    let free_slots = if pause { 0 } else { st.parallel.saturating_sub(set.len()) };
                    let todo = wanted(&st, &e, free_slots, !pause);
                    for &i in &todo {
                        st.chunks[i as usize] = Chunk::Fetching;
                    }
                    (todo, url)
                }
            };
            if let Some(url) = url {
                for i in todo {
                    let (a, b) = e.chunk_range(i);
                    let (http, e2, url) = (self.http.clone(), e.clone(), url.clone());
                    set.spawn(async move { (i, fetch_chunk(&http, &url, &e2, a, b).await) });
                }
            }
            tokio::select! {
                r = set.join_next(), if !set.is_empty() => {
                    if let Some(Ok((i, res))) = r {
                        let mut st = e.st.lock().unwrap();
                        match res {
                            Ok(n) => {
                                st.chunks[i as usize] = Chunk::Ready;
                                st.ready += 1;
                                st.downloaded += n;
                                st.errors = 0;
                            }
                            Err(err) => {
                                st.chunks[i as usize] = Chunk::Missing;
                                st.errors += 1;
                                retry_at = tokio::time::Instant::now() + Duration::from_millis(500 * st.errors as u64);
                                tracing::warn!(key = %e.key, chunk = i, error = %err, "blocco della sorgente non scaricato");
                                if st.errors >= MAX_ERRORS {
                                    st.failed = Some(err);
                                }
                            }
                        }
                        drop(st);
                        e.progress.notify_waiters();
                    }
                }
                _ = e.wake.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(500)) => {}
            }
        }
        // Qui non ci sono blocchi a metà: si esce solo con il JoinSet vuoto.
        let st = e.st.lock().unwrap();
        let secs = st.started.map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0).max(0.001);
        tracing::info!(
            key = %e.key,
            mb = st.downloaded >> 20,
            mbit_s = format!("{:.0}", st.downloaded as f64 * 8.0 / secs / 1e6),
            complete = st.ready == e.chunks(),
            "scaricamento della sorgente fermo"
        );
        drop(st);
        e.progress.notify_waiters();
    }
}

/// Blocchi da chiedere: prima quelli sotto i lettori, poi la finestra di lettura anticipata (o tutto il file).
/// I blocchi sotto un lettore (FFmpeg è fermo ad aspettarli) passano sempre, anche oltre `max`: altrimenti ogni salto
/// nel file (apertura di un MKV: inizio, indice in fondo, punto di ripresa) aspetterebbe i blocchi della lettura
/// anticipata già in viaggio. `urgent` falso (pausa dopo un errore) = solo entro `max`.
fn wanted(st: &EntryState, e: &Entry, max: usize, urgent: bool) -> Vec<u64> {
    let n = e.chunks();
    let mut positions: Vec<u64> = st.cursors.values().copied().collect();
    if positions.is_empty() {
        positions.push(st.last_pos);
    }
    positions.sort_unstable();
    let window = match st.mode {
        SourceMode::Full => n,
        _ => st.readahead.div_ceil(CHUNK).max(2),
    };
    let mut out = Vec::new();
    let push = |i: u64, out: &mut Vec<u64>| {
        if i < n && st.chunks[i as usize] == Chunk::Missing && !out.contains(&i) {
            out.push(i);
        }
    };
    // Il blocco che i lettori aspettano adesso viene prima di tutto (solo lettori veri, non l'ultima posizione).
    if urgent {
        for p in st.cursors.values() {
            if out.len() < URGENT_MAX {
                push(p / CHUNK, &mut out);
            }
        }
    }
    // Con un lettore fermo non si aggiunge altra lettura anticipata: la banda va al blocco che serve adesso.
    if !out.is_empty() {
        return out;
    }
    for p in &positions {
        let first = p / CHUNK;
        for i in first..(first + window).min(n) {
            if out.len() >= max {
                return out;
            }
            push(i, &mut out);
        }
    }
    // Copia completa: anche quello che sta prima del punto di partenza.
    if st.mode == SourceMode::Full {
        for i in 0..n {
            if out.len() >= max {
                break;
            }
            push(i, &mut out);
        }
    }
    out.truncate(max);
    out
}

async fn fetch_chunk(http: &reqwest::Client, url: &str, e: &Arc<Entry>, a: u64, b: u64) -> Result<u64, String> {
    let resp = http
        .get(url)
        .header(reqwest::header::RANGE, format!("bytes={}-{}", a, b - 1))
        .timeout(Duration::from_secs(120))
        .send()
        .await
        .map_err(|e| crate::nodes::client::short(&e))?;
    if resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(format!("risposta {} dal server", resp.status().as_u16()));
    }
    let body = resp.bytes().await.map_err(|e| crate::nodes::client::short(&e))?;
    if body.len() as u64 != b - a {
        return Err(format!("blocco incompleto: {} byte su {}", body.len(), b - a));
    }
    let e = e.clone();
    let n = body.len() as u64;
    tokio::task::spawn_blocking(move || e.file.write_all_at(&body, a)).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
    Ok(n)
}

/// Un lettore (connessione di FFmpeg) che consuma il file da `pos`: lo scaricamento segue la sua posizione.
pub struct Reader {
    e: Arc<Entry>,
    id: u64,
}

impl Reader {
    pub fn new(f: &Fetcher, e: Arc<Entry>, pos: u64) -> Reader {
        let id = f.next_cursor.fetch_add(1, Ordering::Relaxed);
        {
            let mut st = e.st.lock().unwrap();
            st.cursors.insert(id, pos);
            st.last_pos = pos;
            st.last_used = crate::util::now_ms();
        }
        e.wake.notify_one();
        Reader { e, id }
    }

    /// Fino a `max` byte da `pos`, appena il blocco è pronto.
    pub async fn read(&self, pos: u64, max: usize) -> Result<Vec<u8>, String> {
        let i = pos / CHUNK;
        {
            let mut st = self.e.st.lock().unwrap();
            st.cursors.insert(self.id, pos);
            st.last_pos = pos;
        }
        let deadline = tokio::time::Instant::now() + WAIT_CHUNK;
        loop {
            let waiter = self.e.progress.notified();
            tokio::pin!(waiter);
            waiter.as_mut().enable();
            {
                let st = self.e.st.lock().unwrap();
                if st.chunks[i as usize] == Chunk::Ready {
                    break;
                }
                if let Some(err) = &st.failed {
                    return Err(format!("sorgente non scaricabile: {err}"));
                }
                if st.sessions.is_empty() {
                    return Err("sessione chiusa".into());
                }
            }
            self.e.wake.notify_one();
            if tokio::time::timeout_at(deadline, waiter).await.is_err() {
                return Err("blocco della sorgente non arrivato in tempo".into());
            }
        }
        let (_, end) = self.e.chunk_range(i);
        let len = ((end - pos) as usize).min(max);
        let e = self.e.clone();
        tokio::task::spawn_blocking(move || {
            let mut buf = vec![0u8; len];
            e.file.read_exact_at(&mut buf, pos).map(|_| buf)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        self.e.st.lock().unwrap().cursors.remove(&self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(size: u64, mode: SourceMode, readahead: u64) -> Entry {
        let dir = std::env::temp_dir().join(format!("platea-fetch-{}", crate::util::random_hex(6)));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x.bin");
        let file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&path).unwrap();
        Entry {
            key: "k".into(),
            path,
            size,
            file,
            st: Mutex::new(EntryState {
                chunks: vec![Chunk::Missing; size.div_ceil(CHUNK) as usize],
                ready: 0,
                sessions: vec![],
                cursors: HashMap::new(),
                last_pos: 0,
                mode,
                readahead,
                parallel: 4,
                errors: 0,
                failed: None,
                running: false,
                last_used: 0,
                downloaded: 0,
                started: None,
                pushed: HashMap::new(),
            }),
            progress: Notify::new(),
            wake: Notify::new(),
        }
    }

    #[test]
    fn readahead_follows_the_reader() {
        let e = entry(100 * CHUNK, SourceMode::Readahead, 4 * CHUNK);
        let mut st = e.st.lock().unwrap();
        assert_eq!(wanted(&st, &e, 8, true), vec![0, 1, 2, 3]);
        // Salto avanti: il blocco sotto il lettore da solo (urgente), poi la finestra da lì; niente da prima.
        st.cursors.insert(1, 50 * CHUNK + 10);
        assert_eq!(wanted(&st, &e, 8, true), vec![50]);
        st.chunks[50] = Chunk::Ready;
        assert_eq!(wanted(&st, &e, 8, true), vec![51, 52, 53]);
        st.chunks[51] = Chunk::Fetching;
        assert_eq!(wanted(&st, &e, 1, true), vec![52]);
        // Fine del file: la finestra si ferma lì.
        st.cursors.insert(1, 99 * CHUNK);
        assert_eq!(wanted(&st, &e, 8, true), vec![99]);
    }

    #[test]
    fn blocked_reader_jumps_the_queue() {
        let e = entry(100 * CHUNK, SourceMode::Readahead, 8 * CHUNK);
        let mut st = e.st.lock().unwrap();
        // Tutti gli slot occupati dalla lettura anticipata: il blocco di un lettore fermo passa lo stesso.
        st.cursors.insert(1, 90 * CHUNK);
        assert_eq!(wanted(&st, &e, 0, true), vec![90]);
        // In pausa dopo un errore no.
        assert_eq!(wanted(&st, &e, 0, false), Vec::<u64>::new());
        // Due lettori fermi (inizio del file e indice in fondo): tutti e due subito.
        st.cursors.insert(2, 0);
        let mut w = wanted(&st, &e, 0, true);
        w.sort_unstable();
        assert_eq!(w, vec![0, 90]);
    }

    #[test]
    fn full_copy_starts_where_you_watch() {
        let e = entry(6 * CHUNK + 1, SourceMode::Full, 0);
        let mut st = e.st.lock().unwrap();
        st.cursors.insert(1, 4 * CHUNK);
        st.chunks[4] = Chunk::Ready;
        assert_eq!(wanted(&st, &e, 10, true), vec![5, 6, 0, 1, 2, 3]);
        assert_eq!(wanted(&st, &e, 0, true), Vec::<u64>::new());
    }
}
