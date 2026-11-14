//! Piccole funzioni condivise.

use rand::RngCore;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

/// Millisecondi Unix.
pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Stringa casuale di `bytes` byte in esadecimale (256 bit per i token).
pub fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

pub fn sha256_hex(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

/// Codice di primo avvio leggibile: 3 gruppi da 4 cifre.
pub fn setup_code() -> String {
    let mut buf = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut buf);
    let n = u64::from_le_bytes(buf) % 1_000_000_000_000;
    let s = format!("{n:012}");
    format!("{}-{}-{}", &s[0..4], &s[4..8], &s[8..12])
}

/// Confronto a tempo costante.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
