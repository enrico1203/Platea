//! Notifiche del file system per le librerie su disco locale (inotify su Linux).
//! Un film copiato o cancellato fa partire, dopo qualche secondo di quiete, la scansione della sola
//! cartella toccata. Sulle librerie remote (rclone, rete) le notifiche non vedono i cambi fatti altrove:
//! lì restano la scansione pianificata e la chiamata `/api/v1/scan/path`.
//!
//! Non blocca mai: il watcher gira sul suo thread, gli eventi passano da una coda limitata; se la coda
//! si riempie (copia di migliaia di file) si perde il dettaglio e si scansiona l'intera libreria.

use crate::jobs::{self, priority, JobKind};
use crate::state::Shared;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Cosa scansionare dopo un evento: la libreria e la cartella di primo livello sotto la radice
/// (`None` = tutta la libreria).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Target {
    pub library_id: i64,
    pub dir: Option<PathBuf>,
}

/// Cartella da scansionare per un percorso cambiato: la prima sotto la radice che lo contiene.
/// Un film è una cartella, una serie anche: riscansionarla tutta costa pochi millisecondi.
pub fn target_for(roots: &[(i64, PathBuf)], changed: &Path) -> Option<Target> {
    // La radice più lunga vince (librerie annidate).
    let (lib, root) = roots.iter().filter(|(_, r)| changed.starts_with(r)).max_by_key(|(_, r)| r.as_os_str().len())?;
    let rel = changed.strip_prefix(root).ok()?;
    let mut comps = rel.components();
    let first = comps.next()?;
    let name = first.as_os_str().to_string_lossy();
    if platea_naming::is_ignored_entry(&name) {
        return None;
    }
    // Una voce direttamente nella radice (file sciolto, cartella di un film aggiunta o cancellata): si
    // scansiona la libreria intera. Una cartella sparita, scansionata da sola, risulterebbe illeggibile
    // e i suoi file non verrebbero tolti; su disco locale la scansione completa costa poco.
    let dir = if comps.next().is_some() { Some(root.join(first)) } else { None };
    Some(Target { library_id: *lib, dir })
}

/// Librerie locali da osservare: (libreria, radice).
async fn local_roots(state: &Shared) -> Vec<(i64, PathBuf)> {
    sqlx::query_as::<_, (i64, String)>(
        "SELECT p.library_id, p.path FROM library_paths p JOIN libraries l ON l.id = p.library_id WHERE l.scan_profile = 'local' ORDER BY p.id",
    )
    .fetch_all(&state.db.read)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|(l, p)| (l, PathBuf::from(p)))
    .collect()
}

enum Msg {
    Path(PathBuf),
    Overflow,
}

fn build_watcher(roots: &[(i64, PathBuf)], tx: mpsc::Sender<Msg>) -> Option<RecommendedWatcher> {
    let handler = move |res: notify::Result<notify::Event>| {
        let Ok(ev) = res else {
            let _ = tx.try_send(Msg::Overflow);
            return;
        };
        if !matches!(ev.kind, EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_)) {
            return;
        }
        if ev.need_rescan() {
            let _ = tx.try_send(Msg::Overflow);
            return;
        }
        for p in ev.paths {
            if tx.try_send(Msg::Path(p)).is_err() {
                let _ = tx.try_send(Msg::Overflow);
                return;
            }
        }
    };
    let mut w = match notify::recommended_watcher(handler) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!(error = %e, "notifiche del file system non disponibili: restano le scansioni pianificate");
            return None;
        }
    };
    for (lib, root) in roots {
        if let Err(e) = w.watch(root, RecursiveMode::Recursive) {
            // Tipicamente il limite di inotify (fs.inotify.max_user_watches): la libreria resta sulle scansioni pianificate.
            tracing::warn!(library = lib, root = %root.display(), error = %e, "impossibile osservare la cartella");
        }
    }
    Some(w)
}

pub async fn run(state: Shared) {
    let (tx, mut rx) = mpsc::channel::<Msg>(4096);
    let mut roots: Vec<(i64, PathBuf)> = Vec::new();
    let mut watcher: Option<RecommendedWatcher> = None;
    let mut enabled = false;
    // Cartella → ultimo evento. Si scansiona dopo un periodo di quiete: una copia lunga genera eventi continui.
    let mut pending: HashMap<Target, Instant> = HashMap::new();
    let mut overflow_libs: HashSet<i64> = HashSet::new();
    let mut refresh = tokio::time::interval(Duration::from_secs(60));
    let mut flush = tokio::time::interval(Duration::from_secs(2));
    loop {
        tokio::select! {
            _ = refresh.tick() => {
                let want = state.settings.get().scanner.watch_local;
                let new_roots = if want { local_roots(&state).await } else { vec![] };
                if new_roots != roots || want != enabled {
                    watcher = None;
                    roots = new_roots;
                    enabled = want;
                    if !roots.is_empty() {
                        let r = roots.clone();
                        let t = tx.clone();
                        // Registrare migliaia di cartelle costa I/O: fuori dal runtime.
                        watcher = tokio::task::spawn_blocking(move || build_watcher(&r, t)).await.ok().flatten();
                        if watcher.is_some() {
                            tracing::info!(roots = roots.len(), "notifiche attive sulle librerie locali");
                        }
                    }
                }
            }
            Some(msg) = rx.recv() => {
                match msg {
                    Msg::Path(p) => {
                        if let Some(t) = target_for(&roots, &p) {
                            pending.insert(t, Instant::now());
                        }
                    }
                    Msg::Overflow => {
                        overflow_libs.extend(roots.iter().map(|(l, _)| *l));
                    }
                }
            }
            _ = flush.tick() => {
                let quiet = Duration::from_secs(state.settings.get().scanner.watch_debounce_seconds.max(1) as u64);
                let ready: Vec<Target> = pending.iter().filter(|(_, t)| t.elapsed() >= quiet).map(|(k, _)| k.clone()).collect();
                for t in ready {
                    pending.remove(&t);
                    if overflow_libs.contains(&t.library_id) {
                        continue;
                    }
                    let subpath = t.dir.as_ref().map(|d| d.to_string_lossy().to_string());
                    tracing::debug!(library = t.library_id, dir = ?subpath, "cambio nel file system: scansione mirata");
                    let _ = jobs::enqueue(&state, JobKind::ScanLibrary { library_id: t.library_id, subpath }, priority::INTERACTIVE, Some(t.library_id)).await;
                }
                if !overflow_libs.is_empty() && pending.is_empty() {
                    for lib in overflow_libs.drain() {
                        tracing::info!(library = lib, "troppi cambi insieme: scansione completa");
                        let _ = jobs::enqueue(&state, JobKind::ScanLibrary { library_id: lib, subpath: None }, priority::INTERACTIVE, Some(lib)).await;
                    }
                }
            }
        }
        let _ = &watcher;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_top_level_folder() {
        let roots = vec![(1, PathBuf::from("/media/Film")), (2, PathBuf::from("/media/Serie"))];
        let t = target_for(&roots, Path::new("/media/Film/Matrix (1999)/Matrix (1999).mkv")).unwrap();
        assert_eq!(t, Target { library_id: 1, dir: Some(PathBuf::from("/media/Film/Matrix (1999)")) });
        let t = target_for(&roots, Path::new("/media/Serie/Dark/Season 01/Dark S01E01.mkv")).unwrap();
        assert_eq!(t.dir, Some(PathBuf::from("/media/Serie/Dark")));
        // Voce direttamente nella radice → libreria intera.
        assert_eq!(target_for(&roots, Path::new("/media/Film/film-sciolto.mkv")).unwrap().dir, None);
        assert_eq!(target_for(&roots, Path::new("/media/Film/Matrix (1999)")).unwrap().dir, None);
        assert!(target_for(&roots, Path::new("/media/Film/.cache/x")).is_none());
        assert!(target_for(&roots, Path::new("/altrove/x.mkv")).is_none());
    }
}
