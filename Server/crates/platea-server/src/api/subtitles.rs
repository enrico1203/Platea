//! Sottotitoli aggiunti da Platea a un film o episodio: caricati dall'amministratore o scaricati da OpenSubtitles.
//! Stanno in `cache/subtitles/added/` (mai nelle cartelle dei media) e compaiono come sottotitoli esterni con un
//! percorso `@platea/…`, che la scansione non tocca. Indici da 2000 in su.

use super::playback::ADDED_PREFIX;
use super::{Access, Api};
use crate::auth::AdminUser;
use crate::error::{ApiError, ApiResult};
use crate::state::{Shared, VERSION};
use crate::util::random_hex;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

const MAX_BYTES: usize = 2 * 1024 * 1024;
const FIRST_INDEX: i64 = 2000;

pub fn routes(api: Api) -> Api {
    api.post("/api/v1/items/{id}/subtitles", "Metadati", Access::Admin, "Carica un sottotitolo (SRT, VTT, ASS) per un film o episodio", upload)
        .delete("/api/v1/items/{id}/subtitles/{index}", "Metadati", Access::Admin, "Toglie un sottotitolo aggiunto dal server", remove)
        .get("/api/v1/items/{id}/subtitles/search", "Metadati", Access::Admin, "Cerca sottotitoli su OpenSubtitles", search)
        .post("/api/v1/items/{id}/subtitles/download", "Metadati", Access::Admin, "Scarica un sottotitolo da OpenSubtitles", download)
}

fn base() -> String {
    std::env::var("PLATEA_OPENSUBTITLES_BASE").unwrap_or_else(|_| "https://api.opensubtitles.com/api/v1".to_string())
}

/// Testo del sottotitolo in UTF-8: i file vecchi sono spesso in Windows-1252/Latin-1 (lettere accentate italiane).
pub fn to_utf8(bytes: &[u8]) -> String {
    let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => b.iter().map(|&c| c as char).collect(),
    }
}

/// Formato riconosciuto dal contenuto: (estensione, codec).
pub fn detect_format(text: &str, hint: Option<&str>) -> Option<(&'static str, &'static str)> {
    let t = text.trim_start();
    if t.starts_with("WEBVTT") {
        return Some(("vtt", "webvtt"));
    }
    if t.contains("[Script Info]") || t.contains("[Events]") {
        return Some(if hint == Some("ssa") { ("ssa", "ssa") } else { ("ass", "ass") });
    }
    if t.contains("-->") {
        return Some(("srt", "subrip"));
    }
    None
}

/// Lingua ISO 639-2 da un codice qualsiasi ("it", "ita", "it-IT", "pt-BR").
fn lang3(l: &str) -> String {
    let l = l.trim().to_ascii_lowercase();
    let short = l.split(['-', '_']).next().unwrap_or("");
    match short {
        "it" | "ita" => "ita",
        "en" | "eng" => "eng",
        "fr" | "fre" | "fra" => "fra",
        "de" | "ger" | "deu" => "deu",
        "es" | "spa" => "spa",
        "pt" | "por" => "por",
        "nl" | "dut" | "nld" => "nld",
        "ru" | "rus" => "rus",
        "ja" | "jpn" => "jpn",
        "zh" | "chi" | "zho" => "zho",
        other => return other.chars().take(3).collect(),
    }
    .to_string()
}

async fn target_file(state: &Shared, item: &str, file: Option<&str>) -> ApiResult<(i64, i64)> {
    let iid = state.ids.decode(item).ok_or_else(|| ApiError::not_found("Elemento"))?;
    let kind: Option<String> = sqlx::query_scalar("SELECT kind FROM items WHERE id = ?").bind(iid).fetch_optional(&state.db.read).await?;
    if !matches!(kind.as_deref(), Some("movie" | "episode")) {
        return Err(ApiError::BadRequest("I sottotitoli si aggiungono a un film o a un episodio".into()));
    }
    let wanted = file.and_then(|f| state.ids.decode(f));
    let fid = super::download::choose_file(state, iid, wanted).await?;
    Ok((iid, fid))
}

/// Salva il sottotitolo in cache e lo registra come traccia esterna del file. Restituisce l'indice.
async fn store(state: &Shared, file_id: i64, text: &str, ext: &str, codec: &str, language: &str, forced: bool, title: &str) -> ApiResult<i64> {
    let name = format!("{file_id}_{}.{ext}", random_hex(6));
    let dir = state.boot.subtitles_dir().join("added");
    tokio::fs::create_dir_all(&dir).await.map_err(|e| ApiError::Internal(e.into()))?;
    tokio::fs::write(dir.join(&name), text.as_bytes()).await.map_err(|e| ApiError::Internal(e.into()))?;
    let next: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(stream_index) + 1, ?) FROM media_streams WHERE file_id = ? AND stream_index >= ?")
        .bind(FIRST_INDEX)
        .bind(file_id)
        .bind(FIRST_INDEX)
        .fetch_one(&state.db.read)
        .await?;
    sqlx::query(
        "INSERT INTO media_streams (file_id, stream_index, kind, codec, language, title, is_default, is_forced, is_external, external_path)
         VALUES (?, ?, 'subtitle', ?, ?, ?, 0, ?, 1, ?)",
    )
    .bind(file_id)
    .bind(next)
    .bind(codec)
    .bind(lang3(language))
    .bind(title)
    .bind(forced as i64)
    .bind(format!("{ADDED_PREFIX}{name}"))
    .execute(&state.db.write)
    .await?;
    Ok(next)
}

#[derive(Deserialize)]
struct UploadQuery {
    language: String,
    #[serde(default)]
    forced: bool,
    #[serde(default)]
    file_id: Option<String>,
    /// Nome del file caricato: dà l'estensione (ASS o SSA) e il titolo della traccia.
    #[serde(default)]
    name: Option<String>,
}

async fn upload(
    State(state): State<Shared>,
    _a: AdminUser,
    Path(id): Path<String>,
    Query(q): Query<UploadQuery>,
    body: axum::body::Bytes,
) -> ApiResult<Json<Value>> {
    if body.is_empty() || body.len() > MAX_BYTES {
        return Err(ApiError::BadRequest("Sottotitolo vuoto o troppo grande (massimo 2 MB)".into()));
    }
    let (iid, fid) = target_file(&state, &id, q.file_id.as_deref()).await?;
    let text = to_utf8(&body);
    let hint = q.name.as_deref().and_then(platea_naming::extension).map(|e| e.to_ascii_lowercase());
    let (ext, codec) =
        detect_format(&text, hint.as_deref()).ok_or_else(|| ApiError::BadRequest("Formato non riconosciuto: servono SRT, WebVTT o ASS".into()))?;
    let title = q.name.as_deref().map(|n| n.chars().take(120).collect::<String>()).unwrap_or_else(|| "Caricato".into());
    let index = store(&state, fid, &text, ext, codec, &q.language, q.forced, &title).await?;
    state.events.publish("item.updated", json!({ "item_id": state.ids.encode(iid) }));
    Ok(Json(json!({ "index": index })))
}

async fn remove(State(state): State<Shared>, _a: AdminUser, Path((id, index)): Path<(String, i64)>) -> ApiResult<Json<Value>> {
    if index < FIRST_INDEX {
        return Err(ApiError::BadRequest("Si possono togliere solo i sottotitoli aggiunti dal server".into()));
    }
    let iid = state.ids.decode(&id).ok_or_else(|| ApiError::not_found("Elemento"))?;
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT s.id, s.external_path FROM media_streams s JOIN media_files f ON f.id = s.file_id
         WHERE f.item_id = ? AND s.stream_index = ? AND s.external_path LIKE '@platea/%'",
    )
    .bind(iid)
    .bind(index)
    .fetch_all(&state.db.read)
    .await?;
    if rows.is_empty() {
        return Err(ApiError::not_found("Sottotitolo"));
    }
    for (sid, path) in rows {
        sqlx::query("DELETE FROM media_streams WHERE id = ?").bind(sid).execute(&state.db.write).await?;
        if let Some(name) = path.strip_prefix(ADDED_PREFIX).filter(|n| !n.contains('/') && !n.contains("..")) {
            let _ = tokio::fs::remove_file(state.boot.subtitles_dir().join("added").join(name)).await;
        }
    }
    state.events.publish("item.updated", json!({ "item_id": id }));
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------------------------
// OpenSubtitles

/// Hash di OpenSubtitles: dimensione + somma delle parole a 64 bit dei primi e degli ultimi 64 KB. Poche letture.
pub fn opensubtitles_hash<R: std::io::Read + std::io::Seek>(mut r: R, size: u64) -> Option<String> {
    const CHUNK: u64 = 65536;
    if size < CHUNK * 2 {
        return None;
    }
    let mut h: u64 = size;
    let mut buf = vec![0u8; CHUNK as usize];
    for offset in [0, size - CHUNK] {
        r.seek(std::io::SeekFrom::Start(offset)).ok()?;
        r.read_exact(&mut buf).ok()?;
        for w in buf.as_chunks::<8>().0 {
            h = h.wrapping_add(u64::from_le_bytes(*w));
        }
    }
    Some(format!("{h:016x}"))
}

fn os_key(state: &Shared) -> ApiResult<String> {
    let k = state.settings.get().metadata.opensubtitles_api_key.trim().to_string();
    if k.is_empty() {
        return Err(ApiError::coded(StatusCode::CONFLICT, "OpenSubtitlesNotConfigured", "Manca la chiave API di OpenSubtitles (Impostazioni › Metadati)"));
    }
    Ok(k)
}

fn os_get(state: &Shared, key: &str, path: &str) -> reqwest::RequestBuilder {
    state
        .http
        .get(format!("{}{path}", base()))
        .header("Api-Key", key)
        .header("User-Agent", format!("Platea v{VERSION}"))
        .header("Accept", "application/json")
        .timeout(Duration::from_secs(20))
}

fn os_post(state: &Shared, key: &str, path: &str) -> reqwest::RequestBuilder {
    state
        .http
        .post(format!("{}{path}", base()))
        .header("Api-Key", key)
        .header("User-Agent", format!("Platea v{VERSION}"))
        .header("Accept", "application/json")
        .timeout(Duration::from_secs(20))
}

#[derive(Deserialize)]
struct SearchQuery {
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    file_id: Option<String>,
}

async fn search(State(state): State<Shared>, _a: AdminUser, Path(id): Path<String>, Query(q): Query<SearchQuery>) -> ApiResult<Json<Value>> {
    let key = os_key(&state)?;
    let (iid, fid) = target_file(&state, &id, q.file_id.as_deref()).await?;
    let lang = q.language.as_deref().unwrap_or("it").split(['-', '_']).next().unwrap_or("it").to_ascii_lowercase();
    // Identificativi: del film, oppure della serie con stagione ed episodio.
    let (kind, series, season, episode): (String, Option<i64>, Option<i64>, Option<i64>) =
        sqlx::query_as("SELECT kind, series_id, parent_index_number, index_number FROM items WHERE id = ?").bind(iid).fetch_one(&state.db.read).await?;
    let owner = if kind == "episode" { series.unwrap_or(iid) } else { iid };
    let ids: Vec<(String, String)> = sqlx::query_as("SELECT provider, value FROM external_ids WHERE item_id = ?").bind(owner).fetch_all(&state.db.read).await?;
    let id_of = |p: &str| ids.iter().find(|(k, _)| k == p).map(|(_, v)| v.trim_start_matches("tt").to_string());
    let mut params: Vec<(String, String)> = vec![("languages".into(), lang.clone())];
    let prefix = if kind == "episode" { "parent_" } else { "" };
    if let Some(t) = id_of("tmdb") {
        params.push((format!("{prefix}tmdb_id"), t));
    } else if let Some(i) = id_of("imdb") {
        params.push((format!("{prefix}imdb_id"), i));
    } else {
        let title: String = sqlx::query_scalar("SELECT title FROM items WHERE id = ?").bind(owner).fetch_one(&state.db.read).await?;
        params.push(("query".into(), title));
    }
    if kind == "episode" {
        if let (Some(s), Some(e)) = (season, episode) {
            params.push(("season_number".into(), s.to_string()));
            params.push(("episode_number".into(), e.to_string()));
        }
    }
    // Hash del file: segna i sottotitoli fatti esattamente per questa versione. Poche letture, con un tempo limite.
    if let Some((abs, _, size, ..)) = crate::probe::file_abs_path(&state, fid).await.map_err(ApiError::Internal)? {
        let task = tokio::task::spawn_blocking(move || opensubtitles_hash(std::fs::File::open(abs).ok()?, size.max(0) as u64));
        if let Ok(Ok(Some(h))) = tokio::time::timeout(Duration::from_secs(15), task).await {
            params.push(("moviehash".into(), h));
        }
    }
    params.sort();
    let resp = os_get(&state, &key, "/subtitles").query(&params).send().await.map_err(|e| ApiError::Internal(e.into()))?;
    if !resp.status().is_success() {
        return Err(ApiError::coded(StatusCode::BAD_GATEWAY, "OpenSubtitlesError", format!("OpenSubtitles ha risposto {}", resp.status())));
    }
    let d: Value = resp.json().await.map_err(|e| ApiError::Internal(e.into()))?;
    let items: Vec<Value> = d["data"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    let at = &x["attributes"];
                    let f = at["files"].as_array()?.first()?;
                    Some(json!({
                        "file_id": f["file_id"],
                        "file_name": f["file_name"],
                        "release": at["release"],
                        "language": at["language"],
                        "downloads": at["download_count"],
                        "hearing_impaired": at["hearing_impaired"],
                        "forced": at["foreign_parts_only"],
                        "hash_match": at["moviehash_match"],
                        "uploader": at["uploader"]["name"],
                    }))
                })
                .take(40)
                .collect()
        })
        .unwrap_or_default();
    Ok(Json(json!({ "items": items })))
}

#[derive(Deserialize)]
struct DownloadReq {
    /// `file_id` di OpenSubtitles.
    subtitle_file_id: i64,
    language: String,
    #[serde(default)]
    forced: bool,
    #[serde(default)]
    name: Option<String>,
    /// File della libreria (versione) a cui aggiungerlo.
    #[serde(default)]
    file_id: Option<String>,
}

/// Token di accesso di OpenSubtitles, se l'account è configurato (più download al giorno).
async fn os_token(state: &Shared, key: &str) -> Option<String> {
    let s = state.settings.get();
    let (u, p) = (s.metadata.opensubtitles_username.trim().to_string(), s.metadata.opensubtitles_password.clone());
    drop(s);
    if u.is_empty() || p.is_empty() {
        return None;
    }
    let r = os_post(state, key, "/login").json(&json!({ "username": u, "password": p })).send().await.ok()?;
    let v: Value = r.json().await.ok()?;
    v["token"].as_str().map(String::from)
}

async fn download(State(state): State<Shared>, _a: AdminUser, Path(id): Path<String>, Json(req): Json<DownloadReq>) -> ApiResult<Json<Value>> {
    let key = os_key(&state)?;
    let (iid, fid) = target_file(&state, &id, req.file_id.as_deref()).await?;
    let mut call = os_post(&state, &key, "/download").json(&json!({ "file_id": req.subtitle_file_id, "sub_format": "srt" }));
    if let Some(t) = os_token(&state, &key).await {
        call = call.bearer_auth(t);
    }
    let resp = call.send().await.map_err(|e| ApiError::Internal(e.into()))?;
    let status = resp.status();
    let d: Value = resp.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        let msg = d["message"].as_str().unwrap_or("errore");
        return Err(ApiError::coded(StatusCode::BAD_GATEWAY, "OpenSubtitlesError", format!("OpenSubtitles: {msg} ({status})")));
    }
    let link = d["link"].as_str().ok_or_else(|| ApiError::coded(StatusCode::BAD_GATEWAY, "OpenSubtitlesError", "Risposta senza link"))?;
    if !link.starts_with("https://") && !link.starts_with(&base()) {
        return Err(ApiError::coded(StatusCode::BAD_GATEWAY, "OpenSubtitlesError", "Link di download non valido"));
    }
    let mut r = state.http.get(link).timeout(Duration::from_secs(30)).send().await.map_err(|e| ApiError::Internal(e.into()))?;
    let mut bytes = Vec::new();
    while let Some(chunk) = r.chunk().await.map_err(|e| ApiError::Internal(e.into()))? {
        if bytes.len() + chunk.len() > MAX_BYTES {
            return Err(ApiError::BadRequest("Sottotitolo troppo grande".into()));
        }
        bytes.extend_from_slice(&chunk);
    }
    let text = to_utf8(&bytes);
    let (ext, codec) =
        detect_format(&text, None).ok_or_else(|| ApiError::coded(StatusCode::BAD_GATEWAY, "OpenSubtitlesError", "Il file scaricato non è un sottotitolo"))?;
    let title = req.name.as_deref().or(d["file_name"].as_str()).map(|n| n.chars().take(120).collect::<String>()).unwrap_or_else(|| "OpenSubtitles".into());
    let index = store(&state, fid, &text, ext, codec, &req.language, req.forced, &title).await?;
    state.events.publish("item.updated", json!({ "item_id": state.ids.encode(iid) }));
    Ok(Json(json!({ "index": index, "remaining": d["remaining"] })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_and_encoding() {
        assert_eq!(detect_format("1\n00:00:01,000 --> 00:00:02,000\nCiao\n", None), Some(("srt", "subrip")));
        assert_eq!(detect_format("WEBVTT\n\n00:01.000 --> 00:02.000\nx", None), Some(("vtt", "webvtt")));
        assert_eq!(detect_format("[Script Info]\nTitle: x\n[Events]\n", None), Some(("ass", "ass")));
        assert_eq!(detect_format("<html>no</html>", None), None);
        // "è" in Latin-1 (0xE8) diventa UTF-8; il BOM sparisce.
        assert_eq!(to_utf8(&[0x63, 0x27, 0xE8]), "c'è");
        assert_eq!(to_utf8(&[0xEF, 0xBB, 0xBF, b'o', b'k']), "ok");
        assert_eq!(lang3("it-IT"), "ita");
        assert_eq!(lang3("pt-BR"), "por");
    }

    #[test]
    fn hash_matches_reference() {
        // Riferimento di OpenSubtitles: per un file di soli zeri l'hash è la dimensione.
        let size = 200_000u64;
        let data = vec![0u8; size as usize];
        assert_eq!(opensubtitles_hash(std::io::Cursor::new(&data), size).as_deref(), Some("0000000000030d40"));
        let mut data = data;
        data[0] = 1;
        assert_eq!(opensubtitles_hash(std::io::Cursor::new(&data), size).as_deref(), Some("0000000000030d41"));
    }
}
