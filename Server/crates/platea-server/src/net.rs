//! Indirizzo del client, proxy fidati, reti locali.

use std::net::{IpAddr, SocketAddr};

#[derive(Debug, Clone, Copy)]
pub struct Cidr {
    addr: IpAddr,
    prefix: u8,
}

impl Cidr {
    pub fn parse(s: &str) -> Option<Cidr> {
        let s = s.trim();
        let (a, p) = match s.split_once('/') {
            Some((a, p)) => (a, Some(p)),
            None => (s, None),
        };
        let addr: IpAddr = a.parse().ok()?;
        let max = if addr.is_ipv4() { 32 } else { 128 };
        let prefix = match p {
            Some(p) => p.parse().ok().filter(|p| *p <= max)?,
            None => max,
        };
        Some(Cidr { addr, prefix })
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        let ip = match ip {
            IpAddr::V6(v6) => v6.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(ip),
            v4 => v4,
        };
        match (self.addr, ip) {
            (IpAddr::V4(net), IpAddr::V4(ip)) => {
                let mask = if self.prefix == 0 { 0 } else { u32::MAX << (32 - self.prefix) };
                (u32::from(net) & mask) == (u32::from(ip) & mask)
            }
            (IpAddr::V6(net), IpAddr::V6(ip)) => {
                let mask = if self.prefix == 0 { 0 } else { u128::MAX << (128 - self.prefix) };
                (u128::from(net) & mask) == (u128::from(ip) & mask)
            }
            _ => false,
        }
    }
}

pub fn in_any(list: &[String], ip: IpAddr) -> bool {
    list.iter().filter_map(|c| Cidr::parse(c)).any(|c| c.contains(ip))
}

/// Indirizzo reale del client: X-Forwarded-For solo se la connessione arriva da un proxy fidato.
pub fn client_ip(peer: Option<SocketAddr>, headers: &axum::http::HeaderMap, trusted: &[String]) -> Option<IpAddr> {
    let peer_ip = peer.map(|p| p.ip());
    if let Some(ip) = peer_ip {
        if in_any(trusted, ip) {
            if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
                // Il primo indirizzo non fidato partendo da destra.
                for part in xff.split(',').rev() {
                    if let Ok(candidate) = part.trim().parse::<IpAddr>() {
                        if !in_any(trusted, candidate) {
                            return Some(candidate);
                        }
                    }
                }
            }
            if let Some(real) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()).and_then(|v| v.trim().parse().ok()) {
                return Some(real);
            }
        }
    }
    peer_ip
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cidr() {
        let c = Cidr::parse("192.168.0.0/16").unwrap();
        assert!(c.contains("192.168.1.20".parse().unwrap()));
        assert!(!c.contains("10.0.0.1".parse().unwrap()));
        assert!(c.contains("::ffff:192.168.3.4".parse().unwrap()));
        assert!(Cidr::parse("::1").unwrap().contains("::1".parse().unwrap()));
        assert!(Cidr::parse("garbage").is_none());
    }
}
