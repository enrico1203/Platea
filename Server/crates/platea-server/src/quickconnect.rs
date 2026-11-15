//! Quick Connect: accesso con un codice da un altro dispositivo, senza digitare la password sul televisore.
//! Il dispositivo chiede un codice di 6 cifre e riceve anche un segreto; l'utente, già collegato altrove, conferma il codice;
//! il dispositivo, che intanto controlla lo stato col segreto, lo scambia con un token. Tutto in memoria: i codici valgono
//! 10 minuti, il segreto si usa una volta sola, e un riavvio li cancella (basta chiederne un altro).

use crate::util::{now_ms, random_hex};
use std::collections::HashMap;
use std::sync::Mutex;

/// Validità di un codice.
pub const TTL_MS: i64 = 10 * 60_000;
/// Richieste in attesa al massimo: oltre, le nuove vengono rifiutate (nessuna crescita senza limite).
const MAX_PENDING: usize = 256;
/// Richieste in attesa per indirizzo.
const MAX_PER_IP: usize = 5;

#[derive(Debug, Clone)]
pub struct Pending {
    pub code: String,
    pub device_id: String,
    pub device_name: String,
    pub client: String,
    pub client_version: String,
    pub created_at: i64,
    /// Indirizzo che ha chiesto il codice: poche richieste aperte per indirizzo.
    pub ip: String,
    /// Utente che ha confermato il codice.
    pub user_id: Option<i64>,
}

#[derive(Default)]
pub struct QuickConnect {
    by_secret: Mutex<HashMap<String, Pending>>,
}

impl QuickConnect {
    fn prune(map: &mut HashMap<String, Pending>, now: i64) {
        map.retain(|_, p| now - p.created_at < TTL_MS);
    }

    /// Nuova richiesta: (codice, segreto). `None` se ce ne sono già troppe in attesa.
    pub fn initiate(&self, device_id: &str, device_name: &str, client: &str, client_version: &str, ip: &str) -> Option<(String, String, i64)> {
        let now = now_ms();
        let mut map = self.by_secret.lock().unwrap();
        Self::prune(&mut map, now);
        // Un dispositivo che chiede di nuovo sostituisce la sua richiesta precedente.
        map.retain(|_, p| p.device_id != device_id);
        if map.len() >= MAX_PENDING || map.values().filter(|p| p.ip == ip).count() >= MAX_PER_IP {
            return None;
        }
        let code = loop {
            let n = u32::from_str_radix(&random_hex(4), 16).unwrap_or(0) % 1_000_000;
            let c = format!("{n:06}");
            if !map.values().any(|p| p.code == c) {
                break c;
            }
        };
        let secret = random_hex(32);
        map.insert(
            secret.clone(),
            Pending {
                code: code.clone(),
                device_id: device_id.to_string(),
                device_name: device_name.to_string(),
                client: client.to_string(),
                client_version: client_version.to_string(),
                created_at: now,
                ip: ip.to_string(),
                user_id: None,
            },
        );
        Some((code, secret, now + TTL_MS))
    }

    /// Stato di una richiesta dal suo segreto.
    pub fn status(&self, secret: &str) -> Option<Pending> {
        let mut map = self.by_secret.lock().unwrap();
        Self::prune(&mut map, now_ms());
        map.get(secret).cloned()
    }

    /// Conferma di un codice da parte di un utente collegato: restituisce la richiesta confermata.
    pub fn authorize(&self, code: &str, user_id: i64) -> Option<Pending> {
        let code: String = code.chars().filter(|c| c.is_ascii_digit()).collect();
        let mut map = self.by_secret.lock().unwrap();
        Self::prune(&mut map, now_ms());
        let p = map.values_mut().find(|p| p.code == code && p.user_id.is_none())?;
        p.user_id = Some(user_id);
        Some(p.clone())
    }

    /// Scambio del segreto confermato: la richiesta sparisce (un segreto vale un solo token).
    pub fn redeem(&self, secret: &str) -> Option<Pending> {
        let mut map = self.by_secret.lock().unwrap();
        Self::prune(&mut map, now_ms());
        map.get(secret)?.user_id?;
        map.remove(secret)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_flow() {
        let qc = QuickConnect::default();
        let (code, secret, _) = qc.initiate("tv1", "Televisore", "Platea TV", "1.0", "1.2.3.4").unwrap();
        assert_eq!(code.len(), 6);
        assert!(qc.status(&secret).unwrap().user_id.is_none());
        assert!(qc.redeem(&secret).is_none(), "non ancora confermato");
        assert!(qc.authorize("000000x", 7).is_none() || code == "000000");
        assert_eq!(qc.authorize(&format!("{} {}", &code[..3], &code[3..]), 7).unwrap().device_name, "Televisore");
        assert!(qc.authorize(&code, 8).is_none(), "già confermato");
        assert_eq!(qc.redeem(&secret).unwrap().user_id, Some(7));
        assert!(qc.redeem(&secret).is_none(), "un solo uso");
    }

    #[test]
    fn same_device_replaces_request() {
        let qc = QuickConnect::default();
        let (_, s1, _) = qc.initiate("tv1", "TV", "c", "1", "1.2.3.4").unwrap();
        let (_, s2, _) = qc.initiate("tv1", "TV", "c", "1", "1.2.3.4").unwrap();
        assert!(qc.status(&s1).is_none());
        assert!(qc.status(&s2).is_some());
    }

    #[test]
    fn per_ip_limit() {
        let qc = QuickConnect::default();
        for i in 0..MAX_PER_IP {
            assert!(qc.initiate(&format!("d{i}"), "TV", "c", "1", "9.9.9.9").is_some());
        }
        assert!(qc.initiate("altro", "TV", "c", "1", "9.9.9.9").is_none());
        assert!(qc.initiate("altro", "TV", "c", "1", "8.8.8.8").is_some());
    }
}
