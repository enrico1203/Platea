//! Log in tempo reale per la dashboard: le ultime righe in memoria e un canale per i nuovi eventi.

use serde::Serialize;
use std::collections::VecDeque;
use std::fmt::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tokio::sync::broadcast;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

const KEEP: usize = 2000;

#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    pub seq: u64,
    pub at: i64,
    pub level: &'static str,
    pub message: String,
}

pub struct LogBuffer {
    lines: Mutex<VecDeque<LogLine>>,
    seq: AtomicU64,
    pub tx: broadcast::Sender<LogLine>,
}

pub static LOGS: once_cell::sync::Lazy<LogBuffer> =
    once_cell::sync::Lazy::new(|| LogBuffer { lines: Mutex::new(VecDeque::with_capacity(KEEP)), seq: AtomicU64::new(0), tx: broadcast::channel(512).0 });

impl LogBuffer {
    pub fn recent(&self, min_level: u8, limit: usize) -> Vec<LogLine> {
        let lines = self.lines.lock().unwrap();
        let mut out: Vec<LogLine> = lines.iter().rev().filter(|l| level_rank(l.level) >= min_level).take(limit).cloned().collect();
        out.reverse();
        out
    }
}

pub fn level_rank(l: &str) -> u8 {
    match l {
        "ERROR" => 4,
        "WARN" => 3,
        "INFO" => 2,
        "DEBUG" => 1,
        _ => 0,
    }
}

/// Messaggio e campi raccolti a parte: la riga è "messaggio campo=valore ...".
#[derive(Default)]
struct Fields {
    message: String,
    rest: String,
}

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.message, "{value:?}");
        } else {
            let _ = write!(self.rest, " {}={value:?}", field.name());
        }
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message.push_str(value);
        } else {
            let _ = write!(self.rest, " {}={value}", field.name());
        }
    }
}

pub struct DashboardLayer;

impl<S: Subscriber> Layer<S> for DashboardLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let level = match *event.metadata().level() {
            tracing::Level::ERROR => "ERROR",
            tracing::Level::WARN => "WARN",
            tracing::Level::INFO => "INFO",
            tracing::Level::DEBUG => "DEBUG",
            tracing::Level::TRACE => return,
        };
        let mut f = Fields::default();
        event.record(&mut f);
        let line = LogLine { seq: LOGS.seq.fetch_add(1, Ordering::Relaxed) + 1, at: crate::util::now_ms(), level, message: f.message + &f.rest };
        {
            let mut lines = LOGS.lines.lock().unwrap();
            if lines.len() >= KEEP {
                lines.pop_front();
            }
            lines.push_back(line.clone());
        }
        let _ = LOGS.tx.send(line);
    }
}
