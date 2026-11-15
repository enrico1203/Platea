//! Piccole funzioni condivise.

use rand::RngCore;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

/// Millisecondi Unix.
pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Ora locale del server (fuso da TZ, ora legale compresa): (giorno della settimana 0 = domenica, minuti dalla mezzanotte).
pub fn local_now() -> (u8, u32) {
    #[cfg(unix)]
    unsafe {
        let t: libc::time_t = (now_ms() / 1000) as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        if !libc::localtime_r(&t, &mut tm).is_null() {
            return (tm.tm_wday as u8, (tm.tm_hour * 60 + tm.tm_min) as u32);
        }
    }
    let secs = now_ms() / 1000;
    // 1/1/1970 era giovedì.
    (((secs / 86_400 + 4) % 7) as u8, ((secs % 86_400) / 60) as u32)
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

/// HMAC-SHA256 (RFC 2104), per i link firmati verso i nodi.
pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut k = [0u8; BLOCK];
    if key.len() > BLOCK {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let inner = Sha256::new().chain_update(ipad).chain_update(msg).finalize();
    Sha256::new().chain_update(opad).chain_update(inner).finalize().into()
}

/// Confronto a tempo costante.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Data e ora ISO 8601 in UTC (formato di Jellyfin), senza dipendenze esterne.
pub fn iso(ms: i64) -> String {
    // Data ISO 8601 in UTC senza dipendenze esterne.
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Algoritmo civile di Howard Hinnant.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}.0000000Z")
}

/// Data di oggi in UTC, `AAAA-MM-GG`.
pub fn today_utc() -> String {
    iso(now_ms())[..10].to_string()
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
    fn hmac_rfc4231() {
        // RFC 4231, caso 2.
        assert_eq!(
            hex::encode(super::hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn double_option_distinguishes_null() {
        assert_eq!(serde_json::from_str::<P>("{}").unwrap().v, None);
        assert_eq!(serde_json::from_str::<P>(r#"{"v":null}"#).unwrap().v, Some(None));
        assert_eq!(serde_json::from_str::<P>(r#"{"v":5}"#).unwrap().v, Some(Some(5)));
    }
}
