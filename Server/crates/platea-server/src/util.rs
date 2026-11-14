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

/// Per i campi `Option<Option<T>>` delle PATCH: campo assente = `None`, `null` = `Some(None)`.
/// Senza questo serde tratta `null` come assente e non si può azzerare un valore.
pub fn double_option<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    serde::Deserialize::deserialize(d).map(Some)
}

/// Confronto a tempo costante.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    #[derive(serde::Deserialize, Default)]
    #[serde(default)]
    struct P {
        #[serde(deserialize_with = "super::double_option")]
        v: Option<Option<i64>>,
    }
    #[test]
    fn double_option_distinguishes_null() {
        assert_eq!(serde_json::from_str::<P>("{}").unwrap().v, None);
        assert_eq!(serde_json::from_str::<P>(r#"{"v":null}"#).unwrap().v, Some(None));
        assert_eq!(serde_json::from_str::<P>(r#"{"v":5}"#).unwrap().v, Some(Some(5)));
    }
}
