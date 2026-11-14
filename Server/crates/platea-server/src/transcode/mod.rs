//! Remux e transcodifica con HLS.
//! La playlist è generata dal server in anticipo con tutti i segmenti: la barra di avanzamento è completa
//! dal primo istante. FFmpeg parte dal segmento richiesto; un salto fuori dalla finestra già prodotta
//! lo ferma e lo riavvia alla nuova posizione. FFmpeg viene sospeso quando è troppo avanti.

pub mod command;
pub mod ffmpeg;
pub mod hw;

use command::{Backend, HwProfile, JobSpec};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

/// Stato globale della transcodifica: capacità rilevate, semafori, fallimenti per backend.
pub struct TranscodeManager {
    pub caps: RwLock<hw::Capabilities>,
    pub hw_slots: RwLock<Arc<Semaphore>>,
    pub sw_slots: RwLock<Arc<Semaphore>>,
    hw_limit: AtomicI64,
    sw_limit: AtomicI64,
    /// Fallimenti consecutivi per backend: dopo 3 il backend viene escluso fino alla prossima prova.
    failures: StdMutex<HashMap<Backend, u32>>,
}

impl TranscodeManager {
    pub fn new(max_hw: u32, max_sw: u32) -> Self {
        TranscodeManager {
            caps: RwLock::new(hw::Capabilities::default()),
            hw_slots: RwLock::new(Arc::new(Semaphore::new(max_hw as usize))),
            sw_slots: RwLock::new(Arc::new(Semaphore::new(max_sw as usize))),
            hw_limit: AtomicI64::new(max_hw as i64),
            sw_limit: AtomicI64::new(max_sw as i64),
            failures: StdMutex::new(HashMap::new()),
        }
    }

    /// Applica a caldo i limiti dalle impostazioni.
    pub fn apply_limits(&self, max_hw: u32, max_sw: u32) {
        if self.hw_limit.swap(max_hw as i64, Ordering::SeqCst) != max_hw as i64 {
            *self.hw_slots.write().unwrap() = Arc::new(Semaphore::new(max_hw.max(1) as usize));
        }
        if self.sw_limit.swap(max_sw as i64, Ordering::SeqCst) != max_sw as i64 {
            *self.sw_slots.write().unwrap() = Arc::new(Semaphore::new(max_sw.max(1) as usize));
        }
    }

    /// Profilo hardware da usare per una nuova sessione, oppure software.
    pub fn current_profile(&self) -> HwProfile {
        let caps = self.caps.read().unwrap();
        match &caps.selected {
            Some(p) if self.failures.lock().unwrap().get(&p.backend).copied().unwrap_or(0) < 3 => p.clone(),
            _ => HwProfile::software(),
        }
    }

    pub fn server_encoders(&self) -> Vec<String> {
        let caps = self.caps.read().unwrap();
        let mut v: Vec<String> = caps.software_encoders.clone();
        if let Some(p) = &caps.selected {
            for e in &p.encode {
                if !v.contains(e) {
                    v.push(e.clone());
                }
            }
        }
        if v.is_empty() {
            v.push("h264".into());
        }
        v
    }

    pub fn record_failure(&self, backend: Backend) {
        let mut f = self.failures.lock().unwrap();
        let n = f.entry(backend).or_insert(0);
        *n += 1;
        tracing::warn!(backend = backend.as_str(), failures = *n, "transcodifica hardware non riuscita");
    }

    pub fn record_success(&self, backend: Backend) {
        self.failures.lock().unwrap().remove(&backend);
    }

    pub fn reset_failures(&self) {
        self.failures.lock().unwrap().clear();
    }

    fn slots(&self, backend: Backend) -> Arc<Semaphore> {
        if backend == Backend::Software {
            self.sw_slots.read().unwrap().clone()
        } else {
            self.hw_slots.read().unwrap().clone()
        }
    }

    pub fn active_counts(&self) -> (usize, usize, usize, usize) {
        let hw = self.hw_slots.read().unwrap();
        let sw = self.sw_slots.read().unwrap();
        let hw_lim = self.hw_limit.load(Ordering::Relaxed).max(0) as usize;
        let sw_lim = self.sw_limit.load(Ordering::Relaxed).max(0) as usize;
        (hw_lim.saturating_sub(hw.available_permits()), hw_lim, sw_lim.saturating_sub(sw.available_permits()), sw_lim)
    }
}

struct Running {
    start_segment: u32,
    pid: Option<u32>,
    paused: bool,
    started: Instant,
    exited: Arc<AtomicBool>,
    exit_ok: Arc<AtomicBool>,
    stderr_tail: Arc<StdMutex<String>>,
    kill: Option<tokio::sync::oneshot::Sender<()>>,
    backend: Backend,
    _permit: Option<OwnedSemaphorePermit>,
}

impl Running {
    fn signal(&self, sig: i32) {
        #[cfg(unix)]
        if let Some(pid) = self.pid {
            if !self.exited.load(Ordering::SeqCst) {
                unsafe {
                    libc::kill(pid as i32, sig);
                }
            }
        }
        #[cfg(not(unix))]
        let _ = sig;
    }

    fn stop(&mut self) {
        if self.paused {
            self.signal(libc_sigcont());
            self.paused = false;
        }
        if let Some(k) = self.kill.take() {
            let _ = k.send(());
        }
    }
}

#[cfg(unix)]
fn libc_sigcont() -> i32 {
    libc::SIGCONT
}
#[cfg(not(unix))]
fn libc_sigcont() -> i32 {
    0
}
#[cfg(unix)]
fn libc_sigstop() -> i32 {
    libc::SIGSTOP
}
#[cfg(not(unix))]
fn libc_sigstop() -> i32 {
    0
}

/// Una sessione HLS: directory dei segmenti, piano dei segmenti, processo FFmpeg sorvegliato.
pub struct HlsRuntime {
    pub dir: PathBuf,
    /// Inizio di ogni segmento in ms.
    pub cuts: Vec<i64>,
    pub duration_ms: i64,
    spec: JobSpec,
    ffmpeg: PathBuf,
    hw: StdMutex<HwProfile>,
    manager: Arc<TranscodeManager>,
    proc: Mutex<Option<Running>>,
    pub last_requested: AtomicI64,
    pub stopped: AtomicBool,
    /// Segmenti completati in qualunque avvio di FFmpeg (i confini non cambiano tra un avvio e l'altro).
    done: StdMutex<std::collections::BTreeSet<usize>>,
    /// Backend effettivo, per la riga tecnica della dashboard.
    pub backend_label: StdMutex<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum HlsError {
    #[error("segmento fuori dalla playlist")]
    OutOfRange,
    #[error("FFmpeg non ha prodotto il segmento: {0}")]
    Failed(String),
    #[error("troppe transcodifiche contemporanee: riprova tra poco")]
    Busy,
    #[error("sessione chiusa")]
    Stopped,
}

impl HlsRuntime {
    pub fn new(dir: PathBuf, cuts: Vec<i64>, duration_ms: i64, spec: JobSpec, ffmpeg: PathBuf, hw: HwProfile, manager: Arc<TranscodeManager>) -> Self {
        let label = if matches!(spec.video_action, platea_playback::VideoAction::Copy) { "copia".to_string() } else { hw.backend.label().to_string() };
        HlsRuntime {
            dir,
            cuts,
            duration_ms,
            spec,
            ffmpeg,
            hw: StdMutex::new(hw),
            manager,
            proc: Mutex::new(None),
            last_requested: AtomicI64::new(0),
            stopped: AtomicBool::new(false),
            done: StdMutex::new(Default::default()),
            backend_label: StdMutex::new(label),
        }
    }

    pub fn segment_duration_ms(&self, n: usize) -> i64 {
        let start = self.cuts[n];
        let end = self.cuts.get(n + 1).copied().unwrap_or(self.duration_ms);
        (end - start).max(1)
    }

    /// Playlist VOD con tutti i segmenti.
    pub fn playlist(&self, segment_url: impl Fn(usize) -> String) -> String {
        let max = (0..self.cuts.len()).map(|i| self.segment_duration_ms(i)).max().unwrap_or(6000);
        let mut out = String::with_capacity(self.cuts.len() * 48 + 200);
        out.push_str("#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-PLAYLIST-TYPE:VOD\n");
        out.push_str(&format!("#EXT-X-TARGETDURATION:{}\n#EXT-X-MEDIA-SEQUENCE:0\n", (max as f64 / 1000.0).ceil() as i64));
        for i in 0..self.cuts.len() {
            out.push_str(&format!("#EXTINF:{:.3},\n{}\n", self.segment_duration_ms(i) as f64 / 1000.0, segment_url(i)));
        }
        out.push_str("#EXT-X-ENDLIST\n");
        out
    }

    fn seg_path(&self, n: usize) -> PathBuf {
        self.dir.join(format!("seg_{n:05}.ts"))
    }

    /// Segmenti completati, dall'elenco scritto da FFmpeg.
    fn completed(&self) -> Vec<usize> {
        let mut done = self.done.lock().unwrap();
        if let Ok(text) = std::fs::read_to_string(self.dir.join("segments.csv")) {
            for n in text
                .lines()
                .filter_map(|l| l.split(',').next())
                .filter_map(|name| name.strip_prefix("seg_").and_then(|r| r.strip_suffix(".ts")).and_then(|n| n.parse::<usize>().ok()))
            {
                done.insert(n);
            }
        }
        done.iter().copied().collect()
    }

    fn is_ready(&self, n: usize, completed: &[usize], proc_exited_ok: bool) -> bool {
        (completed.contains(&n) || (proc_exited_ok && n + 1 == self.cuts.len())) && self.seg_path(n).exists()
    }

    async fn launch(&self, n: usize, guard: &mut Option<Running>) -> Result<(), HlsError> {
        if let Some(mut r) = guard.take() {
            r.stop();
        }
        // Segmenti e elenco del processo precedente non valgono più per il nuovo avvio.
        let _ = std::fs::remove_file(self.dir.join("segments.csv"));
        let hw = self.hw.lock().unwrap().clone();
        let transcode = matches!(self.spec.video_action, platea_playback::VideoAction::Transcode { .. });
        let permit = if transcode {
            let sem = self.manager.slots(hw.backend);
            match sem.try_acquire_owned() {
                Ok(p) => Some(p),
                Err(_) => return Err(HlsError::Busy),
            }
        } else {
            None
        };
        let mut spec = self.spec.clone();
        spec.start_ms = self.cuts[n];
        spec.start_segment = n as u32;
        spec.cut_points_ms = self.cuts[n + 1..].to_vec();
        let args = command::build_args(&spec, &hw);
        tracing::debug!(segment = n, args = ?args, "avvio di FFmpeg");
        let mut cmd = tokio::process::Command::new(&self.ffmpeg);
        cmd.args(&args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).kill_on_drop(true);
        let mut child = cmd.spawn().map_err(|e| HlsError::Failed(format!("avvio di FFmpeg: {e}")))?;
        let pid = child.id();
        let exited = Arc::new(AtomicBool::new(false));
        let exit_ok = Arc::new(AtomicBool::new(false));
        let tail = Arc::new(StdMutex::new(String::new()));
        let (kill_tx, kill_rx) = tokio::sync::oneshot::channel::<()>();
        if let Some(stderr) = child.stderr.take() {
            let tail = tail.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncBufReadExt;
                let mut lines = tokio::io::BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let mut t = tail.lock().unwrap();
                    if t.len() > 4000 {
                        let cut = t.len() - 2000;
                        *t = t[cut..].to_string();
                    }
                    t.push_str(&line);
                    t.push('\n');
                }
            });
        }
        {
            let exited = exited.clone();
            let exit_ok = exit_ok.clone();
            tokio::spawn(async move {
                tokio::select! {
                    status = child.wait() => {
                        exit_ok.store(status.map(|s| s.success()).unwrap_or(false), Ordering::SeqCst);
                    }
                    _ = kill_rx => {
                        let _ = child.kill().await;
                    }
                }
                exited.store(true, Ordering::SeqCst);
            });
        }
        *guard = Some(Running {
            start_segment: n as u32,
            pid,
            paused: false,
            started: Instant::now(),
            exited,
            exit_ok,
            stderr_tail: tail,
            kill: Some(kill_tx),
            backend: hw.backend,
            _permit: permit,
        });
        Ok(())
    }

    /// Restituisce il percorso del segmento `n`, avviando o riposizionando FFmpeg se serve.
    pub async fn segment(&self, n: usize) -> Result<PathBuf, HlsError> {
        if n >= self.cuts.len() {
            return Err(HlsError::OutOfRange);
        }
        self.last_requested.store(n as i64, Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut hw_retry_done = false;
        loop {
            if self.stopped.load(Ordering::SeqCst) {
                return Err(HlsError::Stopped);
            }
            let completed = self.completed();
            {
                let mut guard = self.proc.lock().await;
                let exited_ok = guard.as_ref().is_some_and(|r| r.exited.load(Ordering::SeqCst) && r.exit_ok.load(Ordering::SeqCst));
                if self.is_ready(n, &completed, exited_ok) {
                    return Ok(self.seg_path(n));
                }
                let need_launch = match guard.as_ref() {
                    None => true,
                    Some(r) => {
                        let start = r.start_segment as usize;
                        let frontier = contiguous_frontier(&completed, start);
                        if r.exited.load(Ordering::SeqCst) {
                            let ok = r.exit_ok.load(Ordering::SeqCst);
                            if !ok {
                                let err = r.stderr_tail.lock().unwrap().clone();
                                let backend = r.backend;
                                // Ripiego totale: la GPU ha fallito, si riparte in software.
                                if backend != Backend::Software && !hw_retry_done {
                                    self.manager.record_failure(backend);
                                    *self.hw.lock().unwrap() = HwProfile::software();
                                    *self.backend_label.lock().unwrap() = "Software (ripiego)".into();
                                    hw_retry_done = true;
                                    tracing::warn!(error = %err.lines().last().unwrap_or(""), "ripiego su software");
                                    true
                                } else {
                                    return Err(HlsError::Failed(err.lines().last().unwrap_or("errore sconosciuto").to_string()));
                                }
                            } else {
                                true
                            }
                        } else {
                            n < start || n > frontier + 3
                        }
                    }
                };
                if need_launch {
                    self.launch(n, &mut guard).await?;
                } else if let Some(r) = guard.as_mut() {
                    if r.paused {
                        r.signal(libc_sigcont());
                        r.paused = false;
                    }
                    if r.started.elapsed() > Duration::from_secs(8) && r.backend != Backend::Software && !completed.is_empty() {
                        self.manager.record_success(r.backend);
                    }
                }
            }
            if Instant::now() > deadline {
                return Err(HlsError::Failed("tempo scaduto in attesa del segmento".into()));
            }
            tokio::time::sleep(Duration::from_millis(80)).await;
        }
    }

    /// Rallentamento e pulizia: chiamata periodicamente.
    pub async fn maintain(&self, ahead_segments: usize, keep_behind: usize) {
        let last = self.last_requested.load(Ordering::SeqCst).max(0) as usize;
        let completed = self.completed();
        {
            let mut guard = self.proc.lock().await;
            if let Some(r) = guard.as_mut() {
                if !r.exited.load(Ordering::SeqCst) {
                    let start = r.start_segment as usize;
                    let produced = contiguous_frontier(&completed, start);
                    let ahead = produced.saturating_sub(last);
                    if !r.paused && ahead > ahead_segments {
                        r.signal(libc_sigstop());
                        r.paused = true;
                    } else if r.paused && ahead < ahead_segments / 2 {
                        r.signal(libc_sigcont());
                        r.paused = false;
                    }
                }
            }
        }
        // Segmenti già visti: si cancellano, tenendone alcuni dietro per piccoli salti indietro.
        if last > keep_behind {
            let limit = last - keep_behind;
            if let Ok(rd) = std::fs::read_dir(&self.dir) {
                for e in rd.flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    if let Some(n) = name.strip_prefix("seg_").and_then(|r| r.strip_suffix(".ts")).and_then(|n| n.parse::<usize>().ok()) {
                        if n < limit || n > last + ahead_segments * 4 + 50 {
                            let _ = std::fs::remove_file(e.path());
                            self.done.lock().unwrap().remove(&n);
                        }
                    }
                }
            }
        }
    }

    pub async fn is_paused(&self) -> bool {
        self.proc.lock().await.as_ref().is_some_and(|r| r.paused)
    }

    pub async fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Some(mut r) = self.proc.lock().await.take() {
            r.stop();
        }
        let dir = self.dir.clone();
        // Un attimo per far uscire FFmpeg, poi via la directory.
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let _ = tokio::fs::remove_dir_all(dir).await;
        });
    }
}

/// Primo segmento da `start` in poi non ancora completato.
fn contiguous_frontier(completed: &[usize], start: usize) -> usize {
    let set: std::collections::HashSet<usize> = completed.iter().copied().collect();
    let mut f = start;
    while set.contains(&f) {
        f += 1;
    }
    f
}
