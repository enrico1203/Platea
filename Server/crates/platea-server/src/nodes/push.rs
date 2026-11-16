//! Modalità push: il server manda al nodo i blocchi del file che il nodo transcodifica, sul collegamento che usa
//! già per comandarlo. Serve quando il nodo non raggiunge il server (NAT, router di sottorete Tailscale, VPN a senso
//! unico). Il nodo dice quali blocchi gli servono (`/node/v1/push/{id}/wants`, attesa lunga) e li tiene nella sua
//! cache a blocchi (`crate::node::fetch`), come quando li legge lui.

use super::client::{NodeCallError, NodeClient};
use super::proto::SourceTransport;
use super::NodeRow;
use crate::state::Shared;
use std::os::unix::fs::FileExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;

/// Il server deve mandare il file a questo nodo? Automatico: sì se il nodo non lo raggiunge (o non si sa dove).
pub fn wanted(state: &Shared, row: &NodeRow) -> bool {
    match row.config.source_transport {
        SourceTransport::Push => true,
        SourceTransport::Pull => false,
        SourceTransport::Auto => state.nodes.live(row.id).reach.is_some_and(|r| r.ok == Some(false)) || super::server_base_for(state, row.id).is_none(),
    }
}

pub struct Job {
    pub client: NodeClient,
    pub remote_id: String,
    pub path: PathBuf,
    pub size: u64,
    pub session_id: String,
    pub parallel: usize,
    pub node: String,
}

/// Manda i blocchi finché il nodo li chiede e la sessione vive.
pub async fn run(state: Shared, j: Job) {
    let file = match std::fs::File::open(&j.path) {
        Ok(f) => Arc::new(f),
        Err(e) => {
            tracing::warn!(node = %j.node, error = %e, "file da mandare al nodo non leggibile");
            return;
        }
    };
    let t0 = std::time::Instant::now();
    let mut set: JoinSet<Result<u64, String>> = JoinSet::new();
    let mut sent = 0u64;
    let mut errors = 0u32;
    let mut complete = false;
    // La sessione del server si registra subito dopo l'avvio sul nodo: si aspetta che compaia, poi si smette
    // quando sparisce (o quando il nodo dice che la sua è chiusa).
    let mut seen = false;
    loop {
        match state.sessions.get(&j.session_id) {
            Some(_) => seen = true,
            None if seen || t0.elapsed() > Duration::from_secs(30) => break,
            None => {}
        }
        while let Some(r) = set.try_join_next() {
            match r {
                Ok(Ok(n)) => {
                    sent += n;
                    errors = 0;
                }
                Ok(Err(e)) => {
                    errors += 1;
                    tracing::warn!(node = %j.node, error = %e, "blocco non mandato al nodo");
                }
                Err(_) => {}
            }
        }
        if errors >= 6 {
            break;
        }
        let w = match j.client.push_wants(&j.remote_id, 20).await {
            Ok(w) => w,
            Err(NodeCallError::Status { status: 404 | 410, .. }) => break,
            Err(e) => {
                errors += 1;
                tracing::warn!(node = %j.node, error = %e, "richiesta dei blocchi al nodo non riuscita");
                if errors >= 6 {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };
        if w.complete {
            complete = true;
            break;
        }
        let chunk = w.chunk_size.max(1);
        for i in w.chunks {
            // Il nodo regola quanti blocchi chiedere (anche gli urgenti oltre il limite): qui solo un tetto di sicurezza.
            while set.len() >= j.parallel + 4 {
                if let Some(Ok(Ok(n))) = set.join_next().await {
                    sent += n;
                }
            }
            let a = i * chunk;
            if a >= j.size {
                continue;
            }
            let len = chunk.min(j.size - a) as usize;
            let (file, client, rid) = (file.clone(), j.client.clone(), j.remote_id.clone());
            set.spawn(async move {
                let buf = tokio::task::spawn_blocking(move || {
                    let mut buf = vec![0u8; len];
                    file.read_exact_at(&mut buf, a).map(|_| buf)
                })
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| format!("lettura del file: {e}"))?;
                client.push_chunk(&rid, i, buf).await.map_err(|e| e.to_string())?;
                Ok(len as u64)
            });
        }
    }
    while let Some(r) = set.join_next().await {
        if let Ok(Ok(n)) = r {
            sent += n;
        }
    }
    let secs = t0.elapsed().as_secs_f64().max(0.001);
    tracing::info!(
        node = %j.node,
        session = %j.session_id,
        mb = sent >> 20,
        mbit_s = format!("{:.0}", sent as f64 * 8.0 / secs / 1e6),
        complete,
        "invio del file al nodo finito"
    );
}
