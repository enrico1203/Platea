//! Storico delle risorse per i grafici della Panoramica: CPU, rete, memoria, riproduzioni.
//! Un campione ogni 5 secondi per l'ultima ora, e la media di ogni minuto per le ultime 24 ore. Solo in memoria.

use crate::state::Shared;
use crate::util::now_ms;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

pub const FINE_MS: i64 = 5_000;
const FINE_KEEP: usize = 720;
pub const COARSE_MS: i64 = 60_000;
const COARSE_KEEP: usize = 1440;

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Sample {
    pub t: i64,
    /// CPU di tutta la macchina, in % (0-100).
    pub cpu_host: f32,
    /// CPU di Platea (container: server + FFmpeg), in % della macchina.
    pub cpu_app: f32,
    /// Rete del container, bit al secondo.
    pub rx_bps: f64,
    pub tx_bps: f64,
    /// Memoria usata dalla macchina e da Platea, in byte.
    pub mem_host: u64,
    pub mem_app: u64,
    pub streams: u32,
    pub transcodes: u32,
}

#[derive(Default)]
pub struct Metrics {
    fine: Mutex<VecDeque<Sample>>,
    coarse: Mutex<VecDeque<Sample>>,
}

impl Metrics {
    /// Campioni dal più vecchio; `long` = ultime 24 ore (medie al minuto), altrimenti l'ultima ora.
    pub fn series(&self, long: bool) -> Vec<Sample> {
        let q = if long { self.coarse.lock().unwrap() } else { self.fine.lock().unwrap() };
        q.iter().copied().collect()
    }

    fn push(&self, s: Sample) {
        let mut fine = self.fine.lock().unwrap();
        fine.push_back(s);
        while fine.len() > FINE_KEEP {
            fine.pop_front();
        }
        // Minuto chiuso: media dei campioni di quel minuto.
        let minute = s.t / COARSE_MS;
        let mut coarse = self.coarse.lock().unwrap();
        let last_minute = coarse.back().map(|c| c.t / COARSE_MS);
        let prev = fine.iter().rev().nth(1).map(|p| p.t / COARSE_MS);
        if let Some(pm) = prev.filter(|pm| *pm < minute && Some(*pm) != last_minute) {
            let group: Vec<&Sample> = fine.iter().filter(|x| x.t / COARSE_MS == pm).collect();
            if let Some(avg) = average(&group, pm * COARSE_MS) {
                coarse.push_back(avg);
                while coarse.len() > COARSE_KEEP {
                    coarse.pop_front();
                }
            }
        }
    }
}

/// Media di un gruppo di campioni; per riproduzioni e transcodifiche il massimo (un picco breve resta visibile).
fn average(g: &[&Sample], t: i64) -> Option<Sample> {
    if g.is_empty() {
        return None;
    }
    let n = g.len() as f64;
    let avg = |f: &dyn Fn(&Sample) -> f64| g.iter().map(|s| f(s)).sum::<f64>() / n;
    Some(Sample {
        t,
        cpu_host: avg(&|s| s.cpu_host as f64) as f32,
        cpu_app: avg(&|s| s.cpu_app as f64) as f32,
        rx_bps: avg(&|s| s.rx_bps),
        tx_bps: avg(&|s| s.tx_bps),
        mem_host: avg(&|s| s.mem_host as f64) as u64,
        mem_app: avg(&|s| s.mem_app as f64) as u64,
        streams: g.iter().map(|s| s.streams).max().unwrap_or(0),
        transcodes: g.iter().map(|s| s.transcodes).max().unwrap_or(0),
    })
}

/// Contatori cumulativi letti dal sistema.
#[derive(Default, Clone, Copy)]
struct Counters {
    /// (tempo di CPU attivo, totale) di tutta la macchina, in tick.
    host: Option<(u64, u64)>,
    /// Microsecondi di CPU del cgroup del container.
    app_usec: Option<u64>,
    rx: Option<u64>,
    tx: Option<u64>,
}

fn host_cpu() -> Option<(u64, u64)> {
    let s = std::fs::read_to_string("/proc/stat").ok()?;
    let line = s.lines().next()?.strip_prefix("cpu ")?;
    let v: Vec<u64> = line.split_whitespace().filter_map(|x| x.parse().ok()).collect();
    if v.len() < 4 {
        return None;
    }
    // user nice system idle iowait irq softirq steal (guest già compresi in user).
    let idle = v[3] + v.get(4).copied().unwrap_or(0);
    let total: u64 = v.iter().take(8).sum();
    Some((total - idle, total))
}

/// CPU della macchina (righe `cpuN` di /proc/stat: non dipende dai limiti del container).
pub fn host_cpus() -> usize {
    std::fs::read_to_string("/proc/stat")
        .map(|s| s.lines().filter(|l| l.starts_with("cpu") && l.as_bytes().get(3).is_some_and(|c| c.is_ascii_digit())).count())
        .unwrap_or(0)
        .max(1)
}

fn app_cpu_usec() -> Option<u64> {
    // cgroup v2: tutto il container (server e FFmpeg).
    if let Ok(s) = std::fs::read_to_string("/sys/fs/cgroup/cpu.stat") {
        if let Some(v) = s.lines().find_map(|l| l.strip_prefix("usage_usec ")).and_then(|v| v.trim().parse().ok()) {
            return Some(v);
        }
    }
    // Senza cgroup: solo questo processo e i figli terminati.
    let s = std::fs::read_to_string("/proc/self/stat").ok()?;
    let after = s.rsplit_once(')')?.1;
    let f: Vec<u64> = after.split_whitespace().filter_map(|x| x.parse().ok()).collect();
    // Campi 14-17 (utime stime cutime cstime) → indici 11..15 dopo il nome.
    let ticks: u64 = f.get(10..14)?.iter().sum();
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as u64;
    Some(ticks * 1_000_000 / hz)
}

fn app_memory() -> Option<u64> {
    if let Ok(s) = std::fs::read_to_string("/sys/fs/cgroup/memory.current") {
        if let Ok(total) = s.trim().parse::<u64>() {
            // La cache dei file non è memoria "usata": si toglie come fa `docker stats`.
            let inactive = std::fs::read_to_string("/sys/fs/cgroup/memory.stat")
                .ok()
                .and_then(|st| st.lines().find_map(|l| l.strip_prefix("inactive_file ")).and_then(|v| v.trim().parse::<u64>().ok()))
                .unwrap_or(0);
            return Some(total.saturating_sub(inactive));
        }
    }
    crate::sysinfo::rss()
}

/// Byte ricevuti e inviati da tutte le interfacce tranne `lo`.
fn net() -> (Option<u64>, Option<u64>) {
    let Ok(s) = std::fs::read_to_string("/proc/net/dev") else { return (None, None) };
    let (mut rx, mut tx) = (0u64, 0u64);
    for line in s.lines().skip(2) {
        let Some((name, rest)) = line.split_once(':') else { continue };
        if name.trim() == "lo" {
            continue;
        }
        let f: Vec<u64> = rest.split_whitespace().filter_map(|x| x.parse().ok()).collect();
        if f.len() >= 9 {
            rx += f[0];
            tx += f[8];
        }
    }
    (Some(rx), Some(tx))
}

fn read() -> Counters {
    let (rx, tx) = net();
    Counters { host: host_cpu(), app_usec: app_cpu_usec(), rx, tx }
}

pub async fn run(state: Shared) {
    let cpus = host_cpus() as f64;
    let mut prev = read();
    let mut prev_t = std::time::Instant::now();
    let mut tick = tokio::time::interval(Duration::from_millis(FINE_MS as u64));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    tick.tick().await;
    loop {
        tick.tick().await;
        let cur = read();
        let dt = prev_t.elapsed().as_secs_f64().max(0.001);
        prev_t = std::time::Instant::now();
        let cpu_host = match (prev.host, cur.host) {
            (Some((a0, t0)), Some((a1, t1))) if t1 > t0 => (a1.saturating_sub(a0)) as f64 / (t1 - t0) as f64 * 100.0,
            _ => 0.0,
        };
        let cpu_app = match (prev.app_usec, cur.app_usec) {
            (Some(a), Some(b)) => (b.saturating_sub(a)) as f64 / 1e6 / dt / cpus * 100.0,
            _ => 0.0,
        };
        let rate = |a: Option<u64>, b: Option<u64>| match (a, b) {
            (Some(a), Some(b)) => b.saturating_sub(a) as f64 * 8.0 / dt,
            _ => 0.0,
        };
        let (total, avail) = crate::sysinfo::meminfo();
        let sessions = state.sessions.all();
        let s = Sample {
            t: now_ms(),
            cpu_host: cpu_host.clamp(0.0, 100.0) as f32,
            cpu_app: cpu_app.clamp(0.0, 100.0) as f32,
            rx_bps: rate(prev.rx, cur.rx),
            tx_bps: rate(prev.tx, cur.tx),
            mem_host: match (total, avail) {
                (Some(t), Some(a)) => t.saturating_sub(a),
                _ => 0,
            },
            mem_app: app_memory().unwrap_or(0),
            streams: sessions.len() as u32,
            transcodes: sessions.iter().filter(|s| s.decision.method == platea_playback::Method::Transcode).count() as u32,
        };
        state.metrics.push(s);
        prev = cur;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minute_averages() {
        let m = Metrics::default();
        for i in 0..30 {
            // 12 campioni al minuto: CPU 10 nel primo minuto, 30 nel secondo, poi un campione del terzo.
            let t = 1_000 * COARSE_MS + i * FINE_MS;
            let cpu = if i < 12 { 10.0 } else { 30.0 };
            m.push(Sample { t, cpu_host: cpu, streams: if i == 3 { 4 } else { 1 }, ..Default::default() });
        }
        let c = m.series(true);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].t, 1_000 * COARSE_MS);
        assert!((c[0].cpu_host - 10.0).abs() < 0.01);
        assert_eq!(c[0].streams, 4);
        assert!((c[1].cpu_host - 30.0).abs() < 0.01);
        assert_eq!(m.series(false).len(), 30);
    }

    #[test]
    fn readers_work_here() {
        assert!(host_cpus() >= 1);
        assert!(host_cpu().is_some());
        let (rx, _) = net();
        assert!(rx.is_some());
    }
}
