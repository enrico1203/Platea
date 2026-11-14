//! Identificativi opachi nell'API: chiavi intere all'interno, stringhe che non rivelano
//! l'ordine di inserimento all'esterno. Cifratura a blocchi Feistel a 64 bit con chiave
//! dell'installazione: nessuna tabella di conversione, nessuna query in più.

use sha2::{Digest, Sha256};

#[derive(Clone)]
pub struct IdCodec {
    keys: [u32; 6],
}

impl IdCodec {
    pub fn new(secret: &[u8]) -> Self {
        let digest = Sha256::digest(secret);
        let mut keys = [0u32; 6];
        for (i, k) in keys.iter_mut().enumerate() {
            *k = u32::from_le_bytes([digest[i * 4], digest[i * 4 + 1], digest[i * 4 + 2], digest[i * 4 + 3]]);
        }
        IdCodec { keys }
    }

    fn round(&self, half: u32, key: u32) -> u32 {
        // Funzione di round semplice e veloce (moltiplicazioni e rotazioni).
        let mut x = half ^ key;
        x = x.wrapping_mul(0x9E37_79B1);
        x ^= x >> 15;
        x = x.wrapping_mul(0x85EB_CA77);
        x ^= x >> 13;
        x.rotate_left(7) ^ key.rotate_right(3)
    }

    fn permute(&self, v: u64) -> u64 {
        let mut l = (v >> 32) as u32;
        let mut r = v as u32;
        for &k in &self.keys {
            let t = r;
            r = l ^ self.round(r, k);
            l = t;
        }
        ((l as u64) << 32) | r as u64
    }

    fn unpermute(&self, v: u64) -> u64 {
        let mut l = (v >> 32) as u32;
        let mut r = v as u32;
        for &k in self.keys.iter().rev() {
            let t = l;
            l = r ^ self.round(l, k);
            r = t;
        }
        ((l as u64) << 32) | r as u64
    }

    pub fn encode(&self, id: i64) -> String {
        format!("{:016x}", self.permute(id as u64))
    }

    pub fn decode(&self, s: &str) -> Option<i64> {
        if s.len() != 16 {
            return None;
        }
        let v = u64::from_str_radix(s, 16).ok()?;
        let id = self.unpermute(v) as i64;
        (id > 0).then_some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let c = IdCodec::new(b"segreto");
        for id in [1i64, 2, 3, 1000, 123_456_789, i64::MAX] {
            let e = c.encode(id);
            assert_eq!(e.len(), 16);
            assert_eq!(c.decode(&e), Some(id));
        }
        assert_ne!(c.encode(1)[..8], c.encode(2)[..8]);
        assert_eq!(c.decode("zz"), None);
        let other = IdCodec::new(b"altro");
        assert_ne!(other.encode(1), c.encode(1));
    }
}
