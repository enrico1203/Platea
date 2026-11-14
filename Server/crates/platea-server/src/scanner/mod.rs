//! Scansione leggera in tre fasi.
//! 1. Elenco: nome, dimensione e data di modifica di ogni voce, confronto con il database.
//! 2. Identificazione: film, serie, stagione, episodio dal nome. Nessun accesso allo storage.
//! 3. Analisi: in coda, a parte (modulo `probe`).
//!
//! Una scansione senza modifiche non apre nessun file: legge solo l'elenco delle cartelle.
//! Se la radice di una libreria è vuota o irraggiungibile la scansione si ferma e non cancella nulla.

use crate::jobs::{self, priority, JobKind};
use crate::state::Shared;
use crate::util::now_ms;
use anyhow::Context;
use platea_naming as naming;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct Entry {
    pub rel: String,
    pub size: i64,
    pub mtime: i64,
}

#[derive(Debug, Default)]
pub struct Listing {
    pub videos: Vec<Entry>,
    /// Sottotitoli esterni per cartella relativa.
    pub subtitles: HashMap<String, Vec<String>>,
    pub dirs: usize,
    pub errors: Vec<String>,
    /// Cartelle (relative) che non si sono potute leggere: i file noti lì sotto non contano come spariti.
    pub error_dirs: Vec<String>,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct ScanStats {
    pub listed: usize,
    pub added: usize,
    pub changed: usize,
    pub moved: usize,
    pub missing: usize,
    pub returned: usize,
    pub unrecognized: usize,
    pub dirs: usize,
    pub files_opened: usize,
    /// File spariti ma non segnati come mancanti: troppi insieme, si attende la conferma.
    pub held: usize,
}

/// Elenco ricorsivo con `workers` thread bloccanti, un livello di cartelle alla volta.
/// Usa solo `read_dir` e `metadata` (stat): nessun file aperto. Sui mount cloud ogni cartella
/// non in cache costa una richiesta al provider: leggerne diverse insieme divide il tempo.
pub fn list_tree(root: &Path, start_rel: &str, workers: usize) -> Listing {
    let mut out = Listing::default();
    let mut frontier: Vec<String> = vec![start_rel.trim_matches('/').to_string()];
    let workers = workers.clamp(1, 64);
    while !frontier.is_empty() {
        let next = std::sync::atomic::AtomicUsize::new(0);
        let n = workers.min(frontier.len());
        let parts: Vec<(Listing, Vec<String>)> = std::thread::scope(|sc| {
            let handles: Vec<_> = (0..n)
                .map(|_| {
                    sc.spawn(|| {
                        let mut l = Listing::default();
                        let mut sub = Vec::new();
                        loop {
                            let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            let Some(rel_dir) = frontier.get(i) else { break };
                            list_dir(root, rel_dir, &mut l, &mut sub);
                        }
                        (l, sub)
                    })
                })
                .collect();
            handles.into_iter().filter_map(|h| h.join().ok()).collect()
        });
        frontier = Vec::new();
        for (l, sub) in parts {
            out.videos.extend(l.videos);
            for (k, v) in l.subtitles {
                out.subtitles.entry(k).or_default().extend(v);
            }
            out.dirs += l.dirs;
            out.errors.extend(l.errors);
            out.error_dirs.extend(l.error_dirs);
            frontier.extend(sub);
        }
    }
    out.videos.sort_by(|a, b| a.rel.cmp(&b.rel));
    out
}

/// Una cartella: i video, i sottotitoli e le sottocartelle da leggere al livello successivo.
fn list_dir(root: &Path, rel_dir: &str, out: &mut Listing, sub: &mut Vec<String>) {
    let abs = if rel_dir.is_empty() { root.to_path_buf() } else { root.join(rel_dir) };
    let rd = match std::fs::read_dir(&abs) {
        Ok(rd) => rd,
        Err(e) => {
            out.errors.push(format!("{}: {e}", abs.display()));
            out.error_dirs.push(rel_dir.to_string());
            return;
        }
    };
    let mut videos = Vec::new();
    let mut subs = Vec::new();
    let mut dirs = Vec::new();
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // Un file `.plateaignore` esclude la cartella e tutto ciò che contiene. Non si apre: basta che ci sia.
        if name == ".plateaignore" {
            return;
        }
        if naming::is_ignored_entry(&name) {
            continue;
        }
        let rel = if rel_dir.is_empty() { name.clone() } else { format!("{rel_dir}/{name}") };
        // file_type() non segue i link; per i link simbolici serve metadata().
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            dirs.push(rel);
            continue;
        }
        let is_video = naming::is_video_file(&name);
        let is_sub = !is_video && naming::is_subtitle_file(&name);
        if !is_video && !is_sub && !ft.is_symlink() {
            continue;
        }
        let meta = if ft.is_symlink() { std::fs::metadata(entry.path()) } else { entry.metadata() };
        let Ok(meta) = meta else { continue };
        if meta.is_dir() {
            dirs.push(rel);
        } else if meta.is_file() {
            if is_video {
                let mtime = meta.modified().ok().and_then(|m| m.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
                videos.push(Entry { rel, size: meta.len() as i64, mtime });
            } else if is_sub {
                subs.push(name);
            }
        }
    }
    out.dirs += 1;
    out.videos.extend(videos);
    if !subs.is_empty() {
        out.subtitles.entry(rel_dir.to_string()).or_default().extend(subs);
    }
    sub.extend(dirs);
}

/// Verifica che la radice esista e non sia vuota, entro un tempo limite (mount bloccati).
async fn root_ok(root: PathBuf, timeout: Duration) -> Result<(), String> {
    let check = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let mut rd = std::fs::read_dir(&root).map_err(|e| format!("{}: {e}", root.display()))?;
        if rd.next().is_none() {
            return Err(format!("{} è vuota: il mount potrebbe essere assente", root.display()));
        }
        Ok(())
    });
    match tokio::time::timeout(timeout, check).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("la radice non risponde entro il tempo limite: mount bloccato?".into()),
    }
}

#[derive(sqlx::FromRow, Clone)]
struct DbFile {
    id: i64,
    relative_path: String,
    size: i64,
    mtime: i64,
    state: String,
    item_id: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct DbItem {
    id: i64,
    kind: String,
    parent_id: Option<i64>,
    index_number: Option<i64>,
    match_key: Option<String>,
}

/// Indici in memoria degli elementi della libreria, per non interrogare il database file per file.
#[derive(Default)]
struct ItemIndex {
    movies: HashMap<String, i64>,
    series: HashMap<String, i64>,
    seasons: HashMap<(i64, i64), i64>,
    episodes: HashMap<(i64, i64), i64>,
}

pub async fn scan_library(state: &Shared, library_id: i64, subpath: Option<String>) -> anyhow::Result<()> {
    let lib: Option<(String, String, String)> =
        sqlx::query_as("SELECT name, kind, scan_profile FROM libraries WHERE id = ?").bind(library_id).fetch_optional(&state.db.read).await?;
    let Some((lib_name, kind, profile)) = lib else { return Ok(()) };
    let paths: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, path FROM library_paths WHERE library_id = ?").bind(library_id).fetch_all(&state.db.read).await?;
    let settings = state.settings.get();
    let fs_timeout = Duration::from_secs(settings.scanner.fs_timeout_seconds.max(2) as u64);
    let t0 = std::time::Instant::now();
    state.events.admin("scan.started", serde_json::json!({ "library_id": state.ids.encode(library_id), "name": lib_name }));

    let mut total = ScanStats::default();
    let mut problems: Vec<String> = Vec::new();
    for (path_id, root) in paths {
        let root_path = PathBuf::from(&root);
        if let Err(e) = root_ok(root_path.clone(), fs_timeout).await {
            tracing::warn!(library = %lib_name, root = %root, error = %e, "scansione fermata: radice non disponibile, nessuna cancellazione");
            problems.push(e);
            continue;
        }
        // Scansione mirata: solo se la sottocartella cade sotto questa radice.
        let start_rel = match &subpath {
            Some(sp) => {
                let sp_path = Path::new(sp);
                if sp_path.is_absolute() {
                    match sp_path.strip_prefix(&root_path) {
                        Ok(r) => r.to_string_lossy().to_string(),
                        Err(_) => continue,
                    }
                } else {
                    sp.trim_matches('/').to_string()
                }
            }
            None => String::new(),
        };
        if start_rel.split('/').any(|p| p == "..") {
            continue;
        }
        // rclone: chiede di aggiornare l'elenco della cartella prima di leggerla.
        crate::scanner::rclone::refresh(state, &root_path, &start_rel).await;

        let rp = root_path.clone();
        let sr = start_rel.clone();
        // L'elenco gira su un thread bloccante: un mount lento consuma quel thread, non il runtime.
        let workers = if profile == "remote" { settings.scanner.list_concurrency_remote } else { settings.scanner.list_concurrency_local } as usize;
        let listing = tokio::task::spawn_blocking(move || list_tree(&rp, &sr, workers)).await.context("elenco delle cartelle")?;
        if !listing.errors.is_empty() {
            tracing::warn!(library = %lib_name, errors = listing.errors.len(), first = %listing.errors[0], "cartelle non leggibili durante l'elenco");
        }
        let stats = apply_listing(state, library_id, &kind, path_id, &start_rel, listing).await?;
        total.listed += stats.listed;
        total.added += stats.added;
        total.changed += stats.changed;
        total.moved += stats.moved;
        total.missing += stats.missing;
        total.returned += stats.returned;
        total.unrecognized += stats.unrecognized;
        total.dirs += stats.dirs;
        total.held += stats.held;
    }

    let status = if !problems.is_empty() {
        "mount_missing"
    } else if total.held > 0 {
        "missing_held"
    } else {
        "ok"
    };
    if total.held > 0 {
        tracing::warn!(library = %lib_name, files = total.held, "troppi file spariti insieme: rimozione sospesa in attesa di conferma");
    }
    let message = if total.held > 0 {
        format!(
            "{} file spariti insieme: possibile mount non funzionante, nessun file rimosso. Se l'eliminazione è voluta, confermala dalla dashboard.",
            total.held
        )
    } else if problems.is_empty() {
        format!(
            "{} file in {} cartelle, {} nuovi, {} modificati, {} spostati, {} mancanti, {} non riconosciuti, {} ms",
            total.listed,
            total.dirs,
            total.added,
            total.changed,
            total.moved,
            total.missing,
            total.unrecognized,
            t0.elapsed().as_millis()
        )
    } else {
        problems.join("; ")
    };
    // La conferma vale per una sola scansione completa.
    sqlx::query("UPDATE libraries SET last_scan_at = ?, last_scan_status = ?, last_scan_message = ?, missing_held = ?, confirm_missing = CASE WHEN ? THEN confirm_missing ELSE 0 END WHERE id = ?")
        .bind(now_ms())
        .bind(status)
        .bind(&message)
        .bind(total.held as i64)
        .bind(!problems.is_empty())
        .bind(library_id)
        .execute(&state.db.write)
        .await?;
    tracing::info!(library = %lib_name, status, %message, "scansione completata");
    state
        .events
        .publish("library.updated", serde_json::json!({ "library_id": state.ids.encode(library_id), "stats": total, "status": status, "message": message }));
    Ok(())
}

async fn load_index(state: &Shared, library_id: i64) -> anyhow::Result<ItemIndex> {
    let items: Vec<DbItem> =
        sqlx::query_as("SELECT id, kind, parent_id, index_number, match_key FROM items WHERE library_id = ? AND kind IN ('movie','series','season','episode')")
            .bind(library_id)
            .fetch_all(&state.db.read)
            .await?;
    let mut idx = ItemIndex::default();
    for it in items {
        match it.kind.as_str() {
            "movie" => {
                if let Some(k) = it.match_key {
                    idx.movies.insert(k, it.id);
                }
            }
            "series" => {
                if let Some(k) = it.match_key {
                    idx.series.insert(k, it.id);
                }
            }
            "season" => {
                if let (Some(p), Some(n)) = (it.parent_id, it.index_number) {
                    idx.seasons.insert((p, n), it.id);
                }
            }
            "episode" => {
                if let (Some(p), Some(n)) = (it.parent_id, it.index_number) {
                    idx.episodes.insert((p, n), it.id);
                }
            }
            _ => {}
        }
    }
    Ok(idx)
}

/// Troppi file spariti in una volta sola per essere una cancellazione voluta.
pub fn mass_missing(gone: usize, present_before: usize, min_files: usize, percent: u32) -> bool {
    if gone == 0 || present_before == 0 || gone < min_files.max(1) {
        return false;
    }
    gone * 100 >= present_before * percent.clamp(1, 100) as usize
}

fn parent_dir(rel: &str) -> &str {
    rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
}

async fn apply_listing(state: &Shared, library_id: i64, kind: &str, path_id: i64, start_rel: &str, listing: Listing) -> anyhow::Result<ScanStats> {
    let mut stats = ScanStats { listed: listing.videos.len(), dirs: listing.dirs, ..Default::default() };
    let in_scope = |rel: &str| start_rel.is_empty() || rel == start_rel || rel.starts_with(&format!("{start_rel}/"));

    let db_files: Vec<DbFile> = sqlx::query_as("SELECT id, relative_path, size, mtime, state, item_id FROM media_files WHERE library_path_id = ?")
        .bind(path_id)
        .fetch_all(&state.db.read)
        .await?;
    let mut by_path: HashMap<String, DbFile> = db_files.into_iter().map(|f| (f.relative_path.clone(), f)).collect();

    let seen: HashSet<&str> = listing.videos.iter().map(|e| e.rel.as_str()).collect();
    // Sotto una cartella illeggibile non si sa nulla: quei file non sono spariti.
    let unreadable = |rel: &str| listing.error_dirs.iter().any(|d| d.is_empty() || rel.starts_with(&format!("{d}/")));
    // File noti ma non più presenti (nell'ambito della scansione): candidati per gli spostamenti.
    let mut vanished: HashMap<(i64, i64), Vec<DbFile>> = HashMap::new();
    for f in by_path.values() {
        if in_scope(&f.relative_path) && !seen.contains(f.relative_path.as_str()) && !unreadable(&f.relative_path) {
            vanished.entry((f.size, f.mtime)).or_default().push(f.clone());
        }
    }

    let mut new_entries: Vec<Entry> = Vec::new();
    let mut changed: Vec<(i64, Entry)> = Vec::new();
    let mut returned: Vec<i64> = Vec::new();
    for e in &listing.videos {
        if naming::is_extra(&e.rel) {
            continue;
        }
        match by_path.get(&e.rel) {
            Some(f) if f.size == e.size && f.mtime == e.mtime => {
                if f.state == "missing" {
                    returned.push(f.id);
                }
            }
            Some(f) => changed.push((f.id, e.clone())),
            None => new_entries.push(e.clone()),
        }
    }

    let mut idx = load_index(state, library_id).await?;
    let now = now_ms();
    let mut probe_ids: Vec<i64> = Vec::new();
    let mut match_ids: HashSet<i64> = HashSet::new();
    let mut touched_items: HashSet<i64> = HashSet::new();

    // Raggruppa per cartella: una transazione per cartella, non una per file.
    let mut groups: HashMap<String, Vec<Entry>> = HashMap::new();
    for e in new_entries {
        groups.entry(parent_dir(&e.rel).to_string()).or_default().push(e);
    }
    let mut dirs: Vec<_> = groups.into_iter().collect();
    dirs.sort_by(|a, b| a.0.cmp(&b.0));

    for (_dir, entries) in dirs {
        let mut tx = state.db.write.begin().await?;
        for e in entries {
            // Spostamento o rinomina: stessa dimensione e data di un file scomparso.
            if let Some(cands) = vanished.get_mut(&(e.size, e.mtime)) {
                let file_name = e.rel.rsplit('/').next().unwrap_or("");
                let pos = cands.iter().position(|c| c.relative_path.rsplit('/').next() == Some(file_name)).or(if cands.is_empty() { None } else { Some(0) });
                if let Some(pos) = pos {
                    let old = cands.swap_remove(pos);
                    sqlx::query("UPDATE media_files SET relative_path = ?, state = 'present', missing_since = NULL WHERE id = ?")
                        .bind(&e.rel)
                        .bind(old.id)
                        .execute(&mut *tx)
                        .await?;
                    by_path.remove(&old.relative_path);
                    if let Some(item) = old.item_id {
                        touched_items.insert(item);
                    }
                    stats.moved += 1;
                    continue;
                }
            }
            let item = identify(&mut tx, &mut idx, library_id, kind, &e.rel, now, &mut match_ids).await?;
            let Some((item_id, version)) = item else {
                stats.unrecognized += 1;
                tracing::debug!(path = %e.rel, "nome non riconosciuto");
                continue;
            };
            let id = sqlx::query(
                "INSERT INTO media_files (item_id, library_path_id, relative_path, size, mtime, version_label, state, probe_state, added_at)
                 VALUES (?, ?, ?, ?, ?, ?, 'present', 'pending', ?)",
            )
            .bind(item_id)
            .bind(path_id)
            .bind(&e.rel)
            .bind(e.size)
            .bind(e.mtime)
            .bind(version)
            .bind(now)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();
            touched_items.insert(item_id);
            probe_ids.push(id);
            stats.added += 1;
        }
        tx.commit().await?;
    }

    // File modificati: l'analisi precedente non vale più.
    if !changed.is_empty() {
        let mut tx = state.db.write.begin().await?;
        for (id, e) in &changed {
            sqlx::query(
                "UPDATE media_files SET size = ?, mtime = ?, probe_state = 'pending', probed_at = NULL, state = 'present', missing_since = NULL WHERE id = ?",
            )
            .bind(e.size)
            .bind(e.mtime)
            .bind(id)
            .execute(&mut *tx)
            .await?;
            sqlx::query("DELETE FROM keyframes WHERE file_id = ?").bind(id).execute(&mut *tx).await?;
            probe_ids.push(*id);
        }
        tx.commit().await?;
        stats.changed = changed.len();
    }

    // File tornati disponibili e file scomparsi (non spostati).
    let mut tx = state.db.write.begin().await?;
    for id in &returned {
        sqlx::query("UPDATE media_files SET state = 'present', missing_since = NULL WHERE id = ?").bind(id).execute(&mut *tx).await?;
    }
    stats.returned = returned.len();
    // Sparizione di massa: un mount a metà o un provider che risponde con cartelle vuote somigliano a una
    // cancellazione. Oltre la soglia non si segna nulla finché l'amministratore non conferma.
    let present_before = by_path.values().filter(|f| f.state == "present" && in_scope(&f.relative_path)).count();
    let gone = vanished.values().flatten().filter(|f| f.state == "present").count();
    let sc = state.settings.get().scanner.clone();
    let confirmed: i64 = sqlx::query_scalar("SELECT confirm_missing FROM libraries WHERE id = ?").bind(library_id).fetch_one(&state.db.read).await.unwrap_or(0);
    let hold = confirmed == 0 && mass_missing(gone, present_before, sc.mass_missing_min_files as usize, sc.mass_missing_percent);
    if hold {
        stats.held = gone;
        vanished.clear();
    }
    for cands in vanished.values() {
        for f in cands {
            if f.state == "present" {
                sqlx::query("UPDATE media_files SET state = 'missing', missing_since = ? WHERE id = ?").bind(now).bind(f.id).execute(&mut *tx).await?;
                stats.missing += 1;
            }
            if let Some(i) = f.item_id {
                touched_items.insert(i);
            }
        }
    }
    tx.commit().await?;
    for id in &returned {
        if let Some(f) = by_path.values().find(|f| f.id == *id) {
            if let Some(i) = f.item_id {
                touched_items.insert(i);
            }
        }
    }

    // Disponibilità degli elementi toccati, risalendo a stagioni e serie.
    if !touched_items.is_empty() {
        update_availability(state, &touched_items).await?;
    }

    // Sottotitoli esterni accanto ai video.
    sync_external_subtitles(state, path_id, &listing.subtitles, &listing.videos).await?;

    let mut batch: Vec<(JobKind, i64, Option<i64>)> = Vec::with_capacity(probe_ids.len() + match_ids.len());
    batch.extend(match_ids.into_iter().map(|id| (JobKind::MatchItem { item_id: id }, priority::METADATA, Some(library_id))));
    batch.extend(probe_ids.into_iter().map(|id| (JobKind::ProbeFile { file_id: id }, priority::PROBE, Some(library_id))));
    jobs::enqueue_batch(state, batch).await?;
    Ok(stats)
}

/// Crea o trova l'elemento logico per un file. Restituisce (item_id, etichetta versione).
async fn identify(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    idx: &mut ItemIndex,
    library_id: i64,
    kind: &str,
    rel: &str,
    now: i64,
    match_ids: &mut HashSet<i64>,
) -> anyhow::Result<Option<(i64, Option<String>)>> {
    if kind == "movies" {
        let m = naming::parse_movie(rel);
        if m.title.is_empty() {
            return Ok(None);
        }
        if let Some(id) = idx.movies.get(&m.group_key) {
            return Ok(Some((*id, m.version)));
        }
        let id = insert_item(tx, library_id, "movie", None, None, &m.title, m.year, None, Some(&m.group_key), now).await?;
        insert_ids(tx, id, &m.ids).await?;
        idx.movies.insert(m.group_key.clone(), id);
        match_ids.insert(id);
        return Ok(Some((id, m.version)));
    }

    let Some(e) = naming::parse_episode(rel) else { return Ok(None) };
    let series_id = match idx.series.get(&e.series_key) {
        Some(id) => *id,
        None => {
            let id = insert_item(tx, library_id, "series", None, None, &e.series_title, e.series_year, None, Some(&e.series_key), now).await?;
            insert_ids(tx, id, &e.series_ids).await?;
            idx.series.insert(e.series_key.clone(), id);
            match_ids.insert(id);
            id
        }
    };
    // Stagione: episodi per data vanno nella stagione dell'anno; numerazione assoluta nella stagione 1.
    let (season_no, ep_no) = if let Some(date) = &e.air_date {
        let year: i64 = date[0..4].parse().unwrap_or(0);
        let md: i64 = format!("{}{}", &date[5..7], &date[8..10]).parse().unwrap_or(0);
        (year, md)
    } else {
        (e.season.unwrap_or(1) as i64, *e.episodes.first().unwrap_or(&0) as i64)
    };
    let season_id = match idx.seasons.get(&(series_id, season_no)) {
        Some(id) => *id,
        None => {
            let title = if season_no == 0 { "Speciali".to_string() } else { format!("Stagione {season_no}") };
            let id = insert_item(tx, library_id, "season", Some(series_id), Some(series_id), &title, None, Some(season_no), None, now).await?;
            idx.seasons.insert((series_id, season_no), id);
            id
        }
    };
    if let Some(id) = idx.episodes.get(&(season_id, ep_no)) {
        return Ok(Some((*id, None)));
    }
    let title = format!("Episodio {ep_no}");
    let id = insert_item(tx, library_id, "episode", Some(season_id), Some(series_id), &title, None, Some(ep_no), None, now).await?;
    let end = e.episodes.last().copied().map(|x| x as i64).filter(|x| *x != ep_no);
    sqlx::query("UPDATE items SET parent_index_number = ?, index_end = ?, premiere_date = ? WHERE id = ?")
        .bind(season_no)
        .bind(end)
        .bind(&e.air_date)
        .bind(id)
        .execute(&mut **tx)
        .await?;
    idx.episodes.insert((season_id, ep_no), id);
    Ok(Some((id, None)))
}

#[allow(clippy::too_many_arguments)]
async fn insert_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    library_id: i64,
    kind: &str,
    parent: Option<i64>,
    series: Option<i64>,
    title: &str,
    year: Option<u16>,
    index: Option<i64>,
    match_key: Option<&str>,
    now: i64,
) -> anyhow::Result<i64> {
    Ok(sqlx::query(
        "INSERT INTO items (library_id, kind, parent_id, series_id, title, sort_title, year, index_number, match_key, added_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(library_id)
    .bind(kind)
    .bind(parent)
    .bind(series)
    .bind(title)
    .bind(naming::sort_title(title))
    .bind(year.map(|y| y as i64))
    .bind(index)
    .bind(match_key)
    .bind(now)
    .bind(now)
    .execute(&mut **tx)
    .await?
    .last_insert_rowid())
}

async fn insert_ids(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, item_id: i64, ids: &naming::ExternalIds) -> anyhow::Result<()> {
    let pairs = [("tmdb", ids.tmdb.map(|v| v.to_string())), ("imdb", ids.imdb.clone()), ("tvdb", ids.tvdb.map(|v| v.to_string()))];
    for (p, v) in pairs {
        if let Some(v) = v {
            sqlx::query("INSERT OR REPLACE INTO external_ids (item_id, provider, value) VALUES (?, ?, ?)")
                .bind(item_id)
                .bind(p)
                .bind(v)
                .execute(&mut **tx)
                .await?;
        }
    }
    Ok(())
}

/// Un elemento è disponibile se almeno un suo file è presente; stagioni e serie se almeno un figlio lo è.
pub async fn update_availability(state: &Shared, items: &HashSet<i64>) -> anyhow::Result<()> {
    let mut tx = state.db.write.begin().await?;
    let mut parents: HashSet<i64> = HashSet::new();
    for id in items {
        sqlx::query("UPDATE items SET available = EXISTS (SELECT 1 FROM media_files f WHERE f.item_id = items.id AND f.state = 'present') WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let p: Option<(Option<i64>, Option<i64>)> =
            sqlx::query_as("SELECT parent_id, series_id FROM items WHERE id = ?").bind(id).fetch_optional(&mut *tx).await?;
        if let Some((parent, series)) = p {
            parents.extend(parent);
            parents.extend(series);
        }
    }
    // Prima le stagioni, poi le serie.
    let mut ordered: Vec<i64> = parents.into_iter().collect();
    ordered.sort();
    for _ in 0..2 {
        for id in &ordered {
            sqlx::query(
                "UPDATE items SET available = EXISTS (SELECT 1 FROM items c WHERE c.parent_id = items.id AND c.available = 1)
                 WHERE id = ? AND kind IN ('season', 'series')",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

/// Allinea i sottotitoli esterni (`Film.it.srt` accanto a `Film.mkv`) alle tracce nel database.
async fn sync_external_subtitles(state: &Shared, path_id: i64, subs: &HashMap<String, Vec<String>>, videos: &[Entry]) -> anyhow::Result<()> {
    // Sidecar attesi per ogni video.
    let mut expected: HashMap<String, Vec<String>> = HashMap::new();
    for v in videos {
        let dir = parent_dir(&v.rel);
        let Some(names) = subs.get(dir) else { continue };
        let file = v.rel.rsplit('/').next().unwrap_or(&v.rel);
        let stem = file.rsplit_once('.').map(|(s, _)| s).unwrap_or(file);
        let mut list: Vec<String> = names
            .iter()
            .filter(|n| n.starts_with(stem) && n.len() > stem.len() && n.as_bytes()[stem.len()] == b'.')
            .map(|n| if dir.is_empty() { n.clone() } else { format!("{dir}/{n}") })
            .collect();
        list.sort();
        if !list.is_empty() {
            expected.insert(v.rel.clone(), list);
        }
    }
    // Sidecar attuali nel database (una query sola).
    let rows: Vec<(i64, String, Option<String>)> = sqlx::query_as(
        "SELECT f.id, f.relative_path, s.external_path FROM media_files f
         LEFT JOIN media_streams s ON s.file_id = f.id AND s.is_external = 1
         WHERE f.library_path_id = ?",
    )
    .bind(path_id)
    .fetch_all(&state.db.read)
    .await?;
    let mut current: HashMap<String, (i64, Vec<String>)> = HashMap::new();
    for (id, rel, ext) in rows {
        let e = current.entry(rel).or_insert((id, vec![]));
        if let Some(x) = ext {
            e.1.push(x);
        }
    }
    let mut tx = state.db.write.begin().await?;
    let mut writes = 0;
    for (rel, (file_id, mut have)) in current {
        have.sort();
        let want = expected.get(&rel).cloned().unwrap_or_default();
        if have == want {
            continue;
        }
        writes += 1;
        sqlx::query("DELETE FROM media_streams WHERE file_id = ? AND is_external = 1").bind(file_id).execute(&mut *tx).await?;
        for (i, p) in want.iter().enumerate() {
            let name = p.rsplit('/').next().unwrap_or(p);
            let (lang, forced, _sdh) = naming::subtitle_language(name);
            let codec = match naming::extension(name).as_deref() {
                Some("srt") => "subrip",
                Some("vtt") => "webvtt",
                Some("ass") => "ass",
                Some("ssa") => "ssa",
                Some("sup") => "hdmv_pgs_subtitle",
                Some("sub") => "dvd_subtitle",
                _ => "subrip",
            };
            sqlx::query(
                "INSERT INTO media_streams (file_id, stream_index, kind, codec, language, title, is_default, is_forced, is_external, external_path)
                 VALUES (?, ?, 'subtitle', ?, ?, ?, 0, ?, 1, ?)",
            )
            .bind(file_id)
            .bind(1000 + i as i64)
            .bind(codec)
            .bind(lang)
            .bind(name)
            .bind(forced as i64)
            .bind(p)
            .execute(&mut *tx)
            .await?;
        }
    }
    if writes > 0 {
        tx.commit().await?;
    }
    Ok(())
}

/// Rimuove i file mancanti oltre il periodo di grazia e gli elementi rimasti senza file.
pub async fn cleanup_missing(state: &Shared) -> anyhow::Result<()> {
    let grace_days = state.settings.get().scanner.missing_grace_days as i64;
    let limit = now_ms() - grace_days * 86_400_000;
    let mut tx = state.db.write.begin().await?;
    let removed = sqlx::query("DELETE FROM media_files WHERE state = 'missing' AND missing_since < ?").bind(limit).execute(&mut *tx).await?.rows_affected();
    // Elementi senza file, poi stagioni senza episodi, poi serie senza stagioni.
    sqlx::query("DELETE FROM items WHERE kind IN ('movie','episode') AND NOT EXISTS (SELECT 1 FROM media_files f WHERE f.item_id = items.id)")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM items WHERE kind = 'season' AND NOT EXISTS (SELECT 1 FROM items c WHERE c.parent_id = items.id)").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM items WHERE kind = 'series' AND NOT EXISTS (SELECT 1 FROM items c WHERE c.parent_id = items.id)").execute(&mut *tx).await?;
    tx.commit().await?;
    if removed > 0 {
        tracing::info!(files = removed, "file mancanti rimossi dopo il periodo di grazia");
        state.events.publish("library.updated", serde_json::json!({}));
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn mass_missing_threshold() {
    // Cancellare qualche film è normale.
    assert!(!mass_missing(3, 8500, 10, 20));
    assert!(!mass_missing(500, 8500, 10, 20));
    // Metà libreria sparita in un colpo: sospetto.
    assert!(mass_missing(4250, 8500, 10, 20));
    // Libreria piccola: sotto il minimo assoluto non si blocca.
    assert!(!mass_missing(5, 10, 10, 20));
    assert!(mass_missing(10, 12, 10, 20));
}

pub mod rclone {
    //! Integrazione con il controllo remoto di rclone: `vfs/refresh` prima di leggere una cartella.
    use crate::state::Shared;
    use std::path::Path;

    /// Cartella da chiedere a rclone: il percorso relativo alla radice del mount rclone.
    /// Se la libreria non sta sotto il mount indicato, non c'è niente da aggiornare.
    pub fn remote_dir(mount: &str, root: &Path, rel: &str) -> Option<String> {
        let full = if rel.is_empty() { root.to_path_buf() } else { root.join(rel) };
        let mount = mount.trim();
        if mount.is_empty() {
            return Some(rel.to_string());
        }
        let r = full.strip_prefix(mount).ok()?;
        Some(r.to_string_lossy().trim_matches('/').to_string())
    }

    pub async fn refresh(state: &Shared, root: &Path, rel: &str) {
        let sc = state.settings.get().scanner.clone();
        let url = sc.rclone_rc_url.trim().to_string();
        if url.is_empty() {
            return;
        }
        let Some(dir) = remote_dir(&sc.rclone_mount_path, root, rel) else { return };
        let body = if dir.is_empty() { serde_json::json!({ "recursive": "true" }) } else { serde_json::json!({ "dir": dir, "recursive": "true" }) };
        let t0 = std::time::Instant::now();
        let r = state
            .http
            .post(format!("{}/vfs/refresh", url.trim_end_matches('/')))
            .json(&body)
            .timeout(std::time::Duration::from_secs(sc.rclone_refresh_timeout_seconds.max(10) as u64))
            .send()
            .await;
        match r {
            Ok(resp) if resp.status().is_success() => {
                tracing::info!(dir = %dir, ms = t0.elapsed().as_millis() as u64, "rclone: elenco delle cartelle aggiornato")
            }
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                tracing::warn!(%status, body = %text.chars().take(300).collect::<String>(), "rclone vfs/refresh non riuscito")
            }
            Err(e) => tracing::warn!(error = %e, "rclone non raggiungibile"),
        }
    }

    #[cfg(test)]
    #[test]
    fn maps_library_to_remote_dir() {
        assert_eq!(remote_dir("/media/cloud", Path::new("/media/cloud/Film"), "").as_deref(), Some("Film"));
        assert_eq!(remote_dir("/media/cloud/", Path::new("/media/cloud"), "").as_deref(), Some(""));
        assert_eq!(remote_dir("/media/cloud", Path::new("/media/cloud"), "Saghe/007").as_deref(), Some("Saghe/007"));
        assert_eq!(remote_dir("/media/altro", Path::new("/media/cloud"), ""), None);
        assert_eq!(remote_dir("", Path::new("/media/cloud"), "X").as_deref(), Some("X"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_tree_reads_only_listing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("Matrix (1999)")).unwrap();
        std::fs::write(root.join("Matrix (1999)/Matrix (1999).mkv"), b"x").unwrap();
        std::fs::write(root.join("Matrix (1999)/Matrix (1999).it.srt"), b"x").unwrap();
        std::fs::write(root.join("Matrix (1999)/cover.jpg"), b"x").unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::write(root.join(".hidden/x.mkv"), b"x").unwrap();
        std::fs::create_dir_all(root.join("Ignored")).unwrap();
        std::fs::write(root.join("Ignored/.plateaignore"), b"").unwrap();
        std::fs::write(root.join("Ignored/y.mkv"), b"x").unwrap();
        let l = list_tree(root, "", 3);
        assert_eq!(l.videos.len(), 1);
        assert_eq!(l.videos[0].rel, "Matrix (1999)/Matrix (1999).mkv");
        assert_eq!(l.subtitles.get("Matrix (1999)").unwrap(), &vec!["Matrix (1999).it.srt".to_string()]);
    }
}
