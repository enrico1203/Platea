//! Scelta dell'esecutore di una transcodifica e della copia da cui leggere un file.
//! Funzioni pure: lo stato dal vivo (nodi online, sessioni, copie verificate) lo raccoglie il chiamante.
//!
//! Regole:
//! - si scartano gli esecutori che non accettano il lavoro (regole, pieni, spenti, in svuotamento);
//! - vince il livello di priorità più basso che ha qualcuno disponibile; gli altri livelli sono la riserva;
//! - dentro un livello decide la strategia (pesi, carico, ordine fisso), con chi ha il file in locale per primo
//!   se richiesto. L'elenco completo è anche l'ordine di failover.

use crate::settings::NodeStrategy;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum HdrRule {
    #[default]
    Any,
    OnlyHdr,
    OnlySdr,
}

/// "Accetta solo": vuoto o 0 = nessun vincolo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Rules {
    /// Altezza minima della sorgente (2160 = solo 4K).
    pub min_height: u32,
    /// Altezza massima della sorgente.
    pub max_height: u32,
    pub hdr: HdrRule,
    /// Codec video della sorgente ammessi (`hevc`, `h264`, `av1`…).
    pub codecs: Vec<String>,
    /// Librerie ammesse (identificativi del database).
    pub libraries: Vec<i64>,
    /// Accetta transcodifiche video.
    pub transcode: bool,
    /// Accetta remux (video copiato): per un nodo solo se ha il file in una cartella propria.
    pub remux: bool,
    /// Solo se la GPU decodifica la sorgente e codifica l'uscita.
    pub require_hw: bool,
}

impl Default for Rules {
    fn default() -> Self {
        Rules { min_height: 0, max_height: 0, hdr: HdrRule::Any, codecs: vec![], libraries: vec![], transcode: true, remux: true, require_hw: false }
    }
}

/// Il lavoro da assegnare.
#[derive(Debug, Clone, Default)]
pub struct Job {
    pub height: u32,
    pub hdr: bool,
    pub codec: String,
    pub bit_depth: u8,
    pub library_id: i64,
    /// Transcodifica video (altrimenti remux).
    pub transcode: bool,
    /// Codec d'uscita della transcodifica.
    pub out_codec: Option<String>,
}

/// Perché un esecutore non accetta: per la dashboard e il log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    Kind,
    Height,
    Hdr,
    Codec,
    Library,
}

impl Rules {
    pub fn accepts(&self, job: &Job) -> Result<(), Refusal> {
        if (job.transcode && !self.transcode) || (!job.transcode && !self.remux) {
            return Err(Refusal::Kind);
        }
        if (self.min_height > 0 && job.height < self.min_height) || (self.max_height > 0 && job.height > self.max_height) {
            return Err(Refusal::Height);
        }
        match self.hdr {
            HdrRule::OnlyHdr if !job.hdr => return Err(Refusal::Hdr),
            HdrRule::OnlySdr if job.hdr => return Err(Refusal::Hdr),
            _ => {}
        }
        if !self.codecs.is_empty() && !self.codecs.iter().any(|c| c.eq_ignore_ascii_case(&job.codec)) {
            return Err(Refusal::Codec);
        }
        if !self.libraries.is_empty() && !self.libraries.contains(&job.library_id) {
            return Err(Refusal::Library);
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: i64,
    pub priority: i64,
    pub weight: u32,
    /// Lavori in corso su questo esecutore (per questo ruolo).
    pub active: usize,
    /// Massimo di lavori contemporanei; 0 = illimitati.
    pub limit: usize,
    /// Il file è in una cartella dell'esecutore (non deve riceverlo da un altro).
    pub has_local: bool,
}

impl Candidate {
    fn full(&self) -> bool {
        self.limit > 0 && self.active >= self.limit
    }
}

/// Ordine di prova degli esecutori disponibili: il primo è la scelta, gli altri il failover.
pub fn order(mut cands: Vec<Candidate>, strategy: NodeStrategy, prefer_local: bool) -> Vec<i64> {
    cands.retain(|c| !c.full() && c.weight > 0);
    let key = |c: &Candidate| -> f64 {
        match strategy {
            // Chi è più "indietro" rispetto alla propria quota: con pesi 90 e 10 le sessioni si dividono 90/10.
            NodeStrategy::Weighted => (c.active as f64 + 1.0) / c.weight as f64,
            NodeStrategy::LeastLoaded => {
                if c.limit > 0 {
                    c.active as f64 / c.limit as f64
                } else {
                    c.active as f64 / 1000.0
                }
            }
            NodeStrategy::Ordered => -(c.weight as f64),
        }
    };
    cands.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| if prefer_local { b.has_local.cmp(&a.has_local) } else { std::cmp::Ordering::Equal })
            .then_with(|| key(a).partial_cmp(&key(b)).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| b.weight.cmp(&a.weight))
            .then_with(|| a.id.cmp(&b.id))
    });
    cands.into_iter().map(|c| c.id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(id: i64, priority: i64, weight: u32) -> Candidate {
        Candidate { id, priority, weight, active: 0, limit: 0, has_local: false }
    }

    /// Assegna `n` sessioni una alla volta (ognuna resta aperta) e conta dove finiscono.
    fn simulate(mut cands: Vec<Candidate>, n: usize, strategy: NodeStrategy) -> Vec<(i64, usize)> {
        for _ in 0..n {
            let first = order(cands.clone(), strategy, true).first().copied();
            if let Some(id) = first {
                cands.iter_mut().find(|c| c.id == id).unwrap().active += 1;
            }
        }
        cands.iter().map(|c| (c.id, c.active)).collect()
    }

    #[test]
    fn weights_split_sessions() {
        let r = simulate(vec![c(1, 1, 90), c(2, 1, 10)], 100, NodeStrategy::Weighted);
        assert_eq!(r, vec![(1, 90), (2, 10)]);
        let r = simulate(vec![c(1, 1, 50), c(2, 1, 50)], 20, NodeStrategy::Weighted);
        assert_eq!(r, vec![(1, 10), (2, 10)]);
    }

    #[test]
    fn priority_levels_are_reserves() {
        // Il server (id 1) è riserva: lavora solo quando i nodi del livello 1 sono pieni.
        let mut a = c(2, 1, 100);
        a.limit = 3;
        let r = simulate(vec![c(1, 2, 100), a], 5, NodeStrategy::Weighted);
        assert_eq!(r, vec![(1, 2), (2, 3)]);
        // L'ordine completo è il failover: prima il livello 1, poi il 2.
        assert_eq!(order(vec![c(1, 2, 100), c(2, 1, 100), c(3, 1, 50)], NodeStrategy::Weighted, true), vec![2, 3, 1]);
    }

    #[test]
    fn local_copy_first_within_level() {
        let mut b = c(2, 1, 10);
        b.has_local = true;
        assert_eq!(order(vec![c(1, 1, 90), b.clone()], NodeStrategy::Weighted, true), vec![2, 1]);
        assert_eq!(order(vec![c(1, 1, 90), b], NodeStrategy::Weighted, false), vec![1, 2]);
    }

    #[test]
    fn ordered_and_least_loaded() {
        let mut a = c(1, 1, 10);
        a.active = 0;
        let mut b = c(2, 1, 90);
        b.active = 5;
        assert_eq!(order(vec![a.clone(), b.clone()], NodeStrategy::Ordered, false), vec![2, 1]);
        a.limit = 2;
        a.active = 1;
        b.limit = 10;
        assert_eq!(order(vec![a, b], NodeStrategy::LeastLoaded, false), vec![2, 1]);
        // Peso 0 = mai.
        assert!(order(vec![c(1, 1, 0)], NodeStrategy::Weighted, false).is_empty());
    }

    #[test]
    fn rules() {
        let uhd = Job { height: 2160, hdr: true, codec: "hevc".into(), bit_depth: 10, library_id: 3, transcode: true, out_codec: Some("h264".into()) };
        let hd = Job { height: 1080, hdr: false, codec: "h264".into(), bit_depth: 8, library_id: 4, transcode: true, out_codec: Some("h264".into()) };
        let only_4k = Rules { min_height: 2160, ..Default::default() };
        assert_eq!(only_4k.accepts(&uhd), Ok(()));
        assert_eq!(only_4k.accepts(&hd), Err(Refusal::Height));
        let sdr = Rules { hdr: HdrRule::OnlySdr, ..Default::default() };
        assert_eq!(sdr.accepts(&uhd), Err(Refusal::Hdr));
        let codecs = Rules { codecs: vec!["HEVC".into()], ..Default::default() };
        assert_eq!(codecs.accepts(&uhd), Ok(()));
        assert_eq!(codecs.accepts(&hd), Err(Refusal::Codec));
        let libs = Rules { libraries: vec![4], ..Default::default() };
        assert_eq!(libs.accepts(&uhd), Err(Refusal::Library));
        let no_remux = Rules { remux: false, ..Default::default() };
        assert_eq!(no_remux.accepts(&Job { transcode: false, ..hd.clone() }), Err(Refusal::Kind));
        // Un JSON vuoto (nodo appena creato) accetta tutto.
        let r: Rules = serde_json::from_str("{}").unwrap();
        assert_eq!(r.accepts(&uhd), Ok(()));
    }
}
