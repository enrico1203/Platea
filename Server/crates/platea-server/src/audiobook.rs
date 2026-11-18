//! Audiolibri: le parti di un libro (i suoi file, in ordine), la durata totale e i capitoli sull'intero libro.
//! Tutto dal database: lo storage non viene toccato.

use crate::state::Shared;
use platea_naming::audiobook as ab;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct Part {
    #[serde(skip)]
    pub file_id: i64,
    pub duration_ms: i64,
    /// Inizio della parte nel libro.
    pub offset_ms: i64,
    pub title: String,
    pub present: bool,
    #[serde(skip)]
    pub chapters: Vec<(i64, Option<String>)>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Chapter {
    /// Inizio nel libro (non nella parte).
    pub start_ms: i64,
    pub title: Option<String>,
    /// Indice della parte in cui cade.
    pub part: usize,
}

#[derive(sqlx::FromRow)]
struct Row {
    id: i64,
    relative_path: String,
    duration_ms: Option<i64>,
    chapters: Option<String>,
    tags: Option<String>,
    state: String,
    version_label: Option<String>,
}

fn file_name(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

fn stem(name: &str) -> &str {
    name.rsplit_once('.').map(|(s, _)| s).filter(|s| !s.is_empty()).unwrap_or(name)
}

/// Numero di traccia o disco dai tag (`3`, `3/12`).
fn tag_number(tags: &HashMap<String, String>, keys: &[&str]) -> Option<u32> {
    keys.iter().find_map(|k| tags.get(*k)).and_then(|v| v.split('/').next()).and_then(|v| v.trim().parse().ok())
}

/// Le parti del libro in ordine, una per file (le copie dello stesso file contano una volta sola).
pub async fn parts(state: &Shared, item_id: i64) -> anyhow::Result<Vec<Part>> {
    let rows: Vec<Row> =
        sqlx::query_as("SELECT id, relative_path, duration_ms, chapters, tags, state, version_label FROM media_files WHERE item_id = ? AND extra_of IS NULL")
            .bind(item_id)
            .fetch_all(&state.db.read)
            .await?;
    Ok(build_parts(rows))
}

fn build_parts(rows: Vec<Row>) -> Vec<Part> {
    // Copie (`Libro (1).m4b` accanto a `Libro.m4b`): una per gruppo, la presente e senza etichetta prima.
    let mut groups: HashMap<String, Row> = HashMap::new();
    for r in rows {
        let dir = r.relative_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        let key = format!("{dir}/{}", ab::strip_copy_suffix(stem(file_name(&r.relative_path))).0);
        let rank = |x: &Row| (x.state == "present", x.version_label.is_none(), x.duration_ms.unwrap_or(0) > 0);
        match groups.get(&key) {
            Some(cur) if rank(cur) >= rank(&r) => {}
            _ => {
                groups.insert(key, r);
            }
        }
    }
    let mut rows: Vec<(Row, HashMap<String, String>)> = groups
        .into_values()
        .map(|r| {
            let tags: HashMap<String, String> = r.tags.as_deref().and_then(|t| serde_json::from_str(t).ok()).unwrap_or_default();
            (r, tags)
        })
        .collect();
    order_rows(&mut rows);
    let mut offset = 0i64;
    rows.into_iter()
        .map(|(r, tags)| {
            let duration = r.duration_ms.unwrap_or(0).max(0);
            let chapters: Vec<(i64, Option<String>)> = r
                .chapters
                .as_deref()
                .and_then(|c| serde_json::from_str::<Vec<serde_json::Value>>(c).ok())
                .unwrap_or_default()
                .iter()
                .filter_map(|c| Some((c["start_ms"].as_i64()?, c["title"].as_str().map(str::to_string))))
                .collect();
            let title = tags.get("title").map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).unwrap_or_else(|| clean_stem(file_name(&r.relative_path)));
            let p = Part { file_id: r.id, duration_ms: duration, offset_ms: offset, title, present: r.state == "present", chapters };
            offset += duration;
            p
        })
        .collect()
}

/// Ordine delle parti: numeri di traccia (e disco) se ogni parte ne ha uno diverso, altrimenti la data nel nome
/// se ogni parte ne ha una (le puntate dei podcast), altrimenti il nome in ordine naturale.
fn order_rows(rows: &mut Vec<(Row, HashMap<String, String>)>) {
    let tracks: Option<Vec<(u32, u32)>> =
        rows.iter().map(|(_, t)| tag_number(t, &["track", "tracknumber"]).map(|n| (tag_number(t, &["disc", "discnumber"]).unwrap_or(1), n))).collect();
    let distinct = tracks.as_ref().is_some_and(|t| {
        let mut u = t.clone();
        u.sort();
        u.dedup();
        u.len() == t.len()
    });
    let dates: Option<Vec<String>> = rows.iter().map(|(r, _)| ab::date_in_name(file_name(&r.relative_path))).collect();
    let mut keyed: Vec<((u32, u32), String, (Row, HashMap<String, String>))> = std::mem::take(rows)
        .into_iter()
        .enumerate()
        .map(|(i, x)| {
            let t = if distinct { tracks.as_ref().map(|t| t[i]).unwrap_or((0, 0)) } else { (0, 0) };
            let d = if !distinct { dates.as_ref().map(|d| d[i].clone()).unwrap_or_default() } else { String::new() };
            (t, d, x)
        })
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)).then_with(|| ab::natural_cmp(&a.2 .0.relative_path, &b.2 .0.relative_path)));
    rows.extend(keyed.into_iter().map(|(_, _, x)| x));
}

/// Nome del file leggibile: senza estensione, con gli spazi al posto dei trattini bassi.
fn clean_stem(name: &str) -> String {
    let s = stem(name);
    if s.matches('_').count() > s.matches(' ').count() {
        s.replace('_', " ")
    } else {
        s.to_string()
    }
}

/// Capitoli sull'intero libro: quelli dei file (m4b, mp3 con capitoli) spostati all'inizio della loro parte;
/// una parte senza capitoli diventa un capitolo, con il suo titolo, se il libro ha più parti.
pub fn chapters(parts: &[Part]) -> Vec<Chapter> {
    let mut out = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        if !p.chapters.is_empty() {
            for (start, title) in &p.chapters {
                out.push(Chapter { start_ms: p.offset_ms + start.max(&0), title: title.clone(), part: i });
            }
        } else if parts.len() > 1 {
            out.push(Chapter { start_ms: p.offset_ms, title: Some(p.title.clone()), part: i });
        }
    }
    out
}

/// Durata del libro: la somma delle parti. Si aggiorna dopo ogni analisi di un file.
pub async fn update_runtime(state: &Shared, item_id: i64) -> anyhow::Result<()> {
    let p = parts(state, item_id).await?;
    let total: i64 = p.iter().map(|x| x.duration_ms).sum();
    if total > 0 {
        sqlx::query("UPDATE items SET runtime_ms = ? WHERE id = ? AND kind = 'audiobook'").bind(total).bind(item_id).execute(&state.db.write).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i64, rel: &str, dur: i64, tags: &str, chapters: &str) -> Row {
        Row {
            id,
            relative_path: rel.into(),
            duration_ms: Some(dur),
            chapters: Some(chapters.into()),
            tags: Some(tags.into()),
            state: "present".into(),
            version_label: None,
        }
    }

    #[test]
    fn parts_in_natural_order_with_offsets() {
        let p = build_parts(vec![row(1, "L/10.mp3", 100, "{}", "[]"), row(2, "L/2.mp3", 50, "{}", "[]"), row(3, "L/1.mp3", 10, "{}", "[]")]);
        assert_eq!(p.iter().map(|x| x.file_id).collect::<Vec<_>>(), vec![3, 2, 1]);
        assert_eq!(p.iter().map(|x| x.offset_ms).collect::<Vec<_>>(), vec![0, 10, 60]);
        let c = chapters(&p);
        assert_eq!(c.len(), 3);
        assert_eq!(c[2].start_ms, 60);
        assert_eq!(c[0].title.as_deref(), Some("1"));
    }

    #[test]
    fn track_tags_win_over_names() {
        let p = build_parts(vec![row(1, "L/a.mp3", 1, r#"{"track":"2/2"}"#, "[]"), row(2, "L/b.mp3", 1, r#"{"track":"1/2"}"#, "[]")]);
        assert_eq!(p[0].file_id, 2);
        // Tracce tutte uguali: si torna al nome.
        let p = build_parts(vec![row(1, "L/b.mp3", 1, r#"{"track":"1"}"#, "[]"), row(2, "L/a.mp3", 1, r#"{"track":"1"}"#, "[]")]);
        assert_eq!(p[0].file_id, 2);
    }

    #[test]
    fn podcast_episodes_by_date() {
        let p = build_parts(vec![row(1, "P/Puntata 02-01-2024.mp3", 1, "{}", "[]"), row(2, "P/Puntata 31-12-2023.mp3", 1, "{}", "[]")]);
        assert_eq!(p[0].file_id, 2);
    }

    #[test]
    fn copies_count_once() {
        let mut copy = row(2, "L/Libro (1).m4b", 100, "{}", "[]");
        copy.version_label = Some("(1)".into());
        let p = build_parts(vec![row(1, "L/Libro.m4b", 100, "{}", r#"[{"start_ms":0,"title":null},{"start_ms":40,"title":"Due"}]"#), copy]);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].file_id, 1);
        let c = chapters(&p);
        assert_eq!(c, vec![Chapter { start_ms: 0, title: None, part: 0 }, Chapter { start_ms: 40, title: Some("Due".into()), part: 0 }]);
    }
}
