//! Sfogliare, cercare, vedere i dettagli; stato di visione; correzioni dei metadati.
//! Tutti gli elenchi sono paginati con cursore.

use super::{clamp_limit, Access, Api};
use crate::auth::{AdminUser, AuthUser};
use crate::error::{ApiError, ApiResult};
use crate::jobs::{self, priority, JobKind};
use crate::state::Shared;
use crate::util::now_ms;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use axum::Json;
use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};

pub fn routes(api: Api) -> Api {
    api.get("/api/v1/items", "Libreria", Access::User, "Elenco paginato con filtri e ordinamento", list)
        .get("/api/v1/items/{id}", "Libreria", Access::User, "Dettaglio di un elemento con file, tracce, persone", detail)
        .get("/api/v1/items/{id}/children", "Libreria", Access::User, "Stagioni di una serie o episodi di una stagione", children)
        .get("/api/v1/items/{id}/next", "Libreria", Access::User, "Episodio successivo", next_episode)
        .post("/api/v1/items/{id}/images/{kind}", "Metadati", Access::Admin, "Carica locandina, sfondo o logo (il campo resta bloccato)", upload_image)
        .delete("/api/v1/items/{id}/images/{kind}", "Metadati", Access::Admin, "Torna all'immagine dei metadati", reset_image)
        .get("/api/v1/items/{id}/remote-images/{kind}", "Metadati", Access::Admin, "Alternative di TMDB per locandina, sfondo o logo", remote_images)
        .post(
            "/api/v1/items/{id}/remote-images/{kind}",
            "Metadati",
            Access::Admin,
            "Sceglie un'alternativa di TMDB (il campo resta bloccato)",
            choose_remote_image,
        )
        .get("/api/v1/items/{id}/chapters", "Libreria", Access::User, "Capitoli del file (letti dall'intestazione la prima volta)", chapters)
        .get("/api/v1/items/{id}/similar", "Libreria", Access::User, "Titoli simili: stesso tipo, generi in comune, poi voto", similar)
        .get("/api/v1/people/{id}", "Libreria", Access::User, "Una persona con i titoli della libreria in cui compare", person)
        .get("/api/v1/home", "Libreria", Access::User, "Continua a guardare, prossimi episodi, aggiunti di recente", home)
        .get("/api/v1/search", "Libreria", Access::User, "Ricerca per titolo", search)
        .get("/api/v1/genres", "Libreria", Access::User, "Generi presenti in una libreria", genres)
        .get("/api/v1/items/{id}/missing", "Libreria", Access::User, "Episodi già usciti che mancano in libreria (serie o stagione)", missing)
        .post("/api/v1/items/{id}/played", "Stato di visione", Access::User, "Segna come visto (anche intere serie e stagioni)", mark_played)
        .delete("/api/v1/items/{id}/played", "Stato di visione", Access::User, "Segna come non visto", mark_unplayed)
        .post("/api/v1/items/{id}/favorite", "Stato di visione", Access::User, "Aggiunge ai preferiti", fav_add)
        .delete("/api/v1/items/{id}/favorite", "Stato di visione", Access::User, "Toglie dai preferiti", fav_remove)
        .patch("/api/v1/items/{id}", "Metadati", Access::Admin, "Modifica a mano i campi (che restano bloccati)", edit)
        .get("/api/v1/items/{id}/candidates", "Metadati", Access::Admin, "Candidati TMDB per l'associazione", candidates)
        .post("/api/v1/items/{id}/identify", "Metadati", Access::Admin, "Associa l'elemento alla scheda TMDB scelta", identify)
        .post("/api/v1/items/{id}/reject", "Metadati", Access::Admin, "Rifiuta un candidato: non verrà riproposto", reject)
        .post("/api/v1/items/{id}/refresh", "Metadati", Access::Admin, "Aggiorna i metadati da TMDB", refresh)
        .get("/api/v1/images/{id}", "Immagini", Access::Public, "Immagine ridimensionata (identificativo opaco, cache lunga)", image)
}

/// Colonne comuni per gli elenchi. `?1` è sempre l'utente.
pub const ITEM_SELECT: &str = "SELECT i.id, i.library_id, i.kind, i.parent_id, i.series_id, i.title, i.original_title, i.year, i.index_number, i.index_end,
  i.parent_index_number, i.overview, i.runtime_ms, i.community_rating, i.official_rating, i.premiere_date, i.end_date, i.status, i.available,
  i.match_state, i.added_at, i.sort_title,
  (SELECT title FROM items s WHERE s.id = i.series_id) AS series_title,
  (SELECT id FROM images im WHERE im.item_id = i.id AND im.kind = 'poster') AS poster_id,
  (SELECT placeholder_color FROM images im WHERE im.item_id = i.id AND im.kind = 'poster') AS poster_color,
  (SELECT id FROM images im WHERE im.item_id = i.id AND im.kind = 'backdrop') AS backdrop_id,
  (SELECT id FROM images im WHERE im.item_id = i.id AND im.kind = 'still') AS still_id,
  (SELECT id FROM images im WHERE im.item_id = i.series_id AND im.kind = 'poster') AS series_poster_id,
  (SELECT id FROM images im WHERE im.item_id = i.series_id AND im.kind = 'backdrop') AS series_backdrop_id,
  u.position_ms, u.played, u.play_count, u.favorite, u.last_played_at,
  CASE WHEN i.kind IN ('series','season') THEN (SELECT COUNT(*) FROM items c WHERE c.parent_id = i.id)
       WHEN i.kind = 'collection' THEN (SELECT COUNT(*) FROM collection_members cm WHERE cm.collection_id = i.id) END AS child_count,
  CASE WHEN i.kind = 'collection' THEN (SELECT im.id FROM collection_members cm JOIN items m ON m.id = cm.item_id
       JOIN images im ON im.item_id = m.id AND im.kind = 'poster' WHERE cm.collection_id = i.id ORDER BY m.year, m.id LIMIT 1) END AS member_poster_id,
  CASE WHEN i.kind IN ('series','season') THEN (SELECT COUNT(*) FROM items e LEFT JOIN user_item_data ue ON ue.item_id = e.id AND ue.user_id = ?1
     WHERE e.kind = 'episode' AND (e.series_id = i.id OR e.parent_id = i.id) AND COALESCE(ue.played, 0) = 0) END AS unplayed_count
FROM items i LEFT JOIN user_item_data u ON u.item_id = i.id AND u.user_id = ?1";

#[derive(sqlx::FromRow)]
pub struct ItemRow {
    pub id: i64,
    pub library_id: i64,
    pub kind: String,
    pub parent_id: Option<i64>,
    pub series_id: Option<i64>,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i64>,
    pub index_number: Option<i64>,
    pub index_end: Option<i64>,
    pub parent_index_number: Option<i64>,
    pub overview: Option<String>,
    pub runtime_ms: Option<i64>,
    pub community_rating: Option<f64>,
    pub official_rating: Option<String>,
    pub premiere_date: Option<String>,
    pub end_date: Option<String>,
    pub status: Option<String>,
    pub available: i64,
    pub match_state: String,
    pub added_at: i64,
    #[allow(dead_code)]
    pub sort_title: String,
    pub series_title: Option<String>,
    pub poster_id: Option<i64>,
    pub poster_color: Option<String>,
    pub backdrop_id: Option<i64>,
    pub still_id: Option<i64>,
    pub series_poster_id: Option<i64>,
    pub series_backdrop_id: Option<i64>,
    pub position_ms: Option<i64>,
    pub played: Option<i64>,
    pub play_count: Option<i64>,
    pub favorite: Option<i64>,
    pub last_played_at: Option<i64>,
    pub child_count: Option<i64>,
    pub member_poster_id: Option<i64>,
    pub unplayed_count: Option<i64>,
}

pub fn image_url(state: &Shared, id: Option<i64>) -> Option<String> {
    id.map(|i| format!("/api/v1/images/{}", state.ids.encode(i)))
}

pub fn item_json(state: &Shared, r: &ItemRow) -> Value {
    json!({
        "id": state.ids.encode(r.id),
        "library_id": state.ids.encode(r.library_id),
        "kind": r.kind,
        "title": r.title,
        "original_title": r.original_title,
        "year": r.year,
        "index_number": r.index_number,
        "index_end": r.index_end,
        "parent_index_number": r.parent_index_number,
        "parent_id": r.parent_id.map(|p| state.ids.encode(p)),
        "series_id": r.series_id.map(|p| state.ids.encode(p)),
        "series_title": r.series_title,
        "overview": r.overview,
        "runtime_ms": r.runtime_ms,
        "community_rating": r.community_rating,
        "official_rating": r.official_rating,
        "premiere_date": r.premiere_date,
        "end_date": r.end_date,
        "status": r.status,
        "available": r.available != 0,
        "match_state": r.match_state,
        "added_at": r.added_at,
        "images": {
            "poster": image_url(state, r.poster_id.or(if r.kind == "season" || r.kind == "episode" { r.series_poster_id } else { r.member_poster_id })),
            "poster_color": r.poster_color,
            "backdrop": image_url(state, r.backdrop_id.or(r.series_backdrop_id)),
            "still": image_url(state, r.still_id),
        },
        "user_data": {
            "position_ms": r.position_ms.unwrap_or(0),
            "played": r.played.unwrap_or(0) != 0,
            "play_count": r.play_count.unwrap_or(0),
            "favorite": r.favorite.unwrap_or(0) != 0,
            "last_played_at": r.last_played_at,
        },
        "child_count": r.child_count,
        "unplayed_count": r.unplayed_count,
    })
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ListQuery {
    library_id: Option<String>,
    kind: Option<String>,
    parent_id: Option<String>,
    genre: Option<String>,
    /// Studio o rete televisiva (nome esatto).
    studio: Option<String>,
    sort: Option<String>,
    order: Option<String>,
    played: Option<bool>,
    favorite: Option<bool>,
    letter: Option<String>,
    /// Decennio: 1990 = 1990–1999; 1940 comprende anche gli anni precedenti.
    decade: Option<i64>,
    /// Qualità video: 4k, 1080, 720, sd, hdr.
    video: Option<String>,
    /// Testo da cercare nel titolo (anche originale), parola per parola con prefisso.
    q: Option<String>,
    cursor: Option<String>,
    limit: Option<i64>,
}

#[derive(serde::Serialize, Deserialize)]
struct Cursor {
    v: Value,
    id: i64,
}

fn encode_cursor(v: Value, id: i64) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(serde_json::to_vec(&Cursor { v, id }).unwrap_or_default())
}

fn decode_cursor(s: &str) -> Option<Cursor> {
    let b = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s).ok()?;
    serde_json::from_slice(&b).ok()
}

async fn list(State(state): State<Shared>, user: AuthUser, Query(q): Query<ListQuery>) -> ApiResult<Json<Value>> {
    let limit = clamp_limit(q.limit, 60);
    let mut sql = format!("{ITEM_SELECT} WHERE 1 {}", user.library_filter("i.library_id"));
    let mut binds: Vec<Value> = vec![json!(user.user_id)];
    let mut idx = 2;
    let mut bind = |sql: &mut String, frag: &str, v: Value, binds: &mut Vec<Value>| {
        sql.push_str(&frag.replace("?", &format!("?{idx}")));
        binds.push(v);
        idx += 1;
    };
    if let Some(l) = &q.library_id {
        let lid = state.ids.decode(l).ok_or_else(|| ApiError::not_found("Libreria"))?;
        bind(&mut sql, " AND i.library_id = ?", json!(lid), &mut binds);
    }
    if let Some(p) = &q.parent_id {
        let pid = state.ids.decode(p).ok_or_else(|| ApiError::not_found("Elemento"))?;
        bind(&mut sql, " AND i.parent_id = ?", json!(pid), &mut binds);
    }
    match q.kind.as_deref() {
        // Una collezione con un solo film presente non aiuta a sfogliare: si elencano da due in su.
        Some("collection") => sql.push_str(" AND i.kind = 'collection' AND (SELECT COUNT(*) FROM collection_members cm WHERE cm.collection_id = i.id) >= 2"),
        Some(k @ ("movie" | "series" | "season" | "episode")) => bind(&mut sql, " AND i.kind = ?", json!(k), &mut binds),
        Some(_) => return Err(ApiError::BadRequest("kind: movie, series, season, episode, collection".into())),
        None if q.parent_id.is_none() => sql.push_str(" AND i.kind IN ('movie','series')"),
        None => {}
    }
    if let Some(m) = q.q.as_deref().and_then(fts_query) {
        bind(&mut sql, " AND i.id IN (SELECT rowid FROM items_fts WHERE items_fts MATCH ?)", json!(m), &mut binds);
    }
    if let Some(g) = &q.genre {
        bind(
            &mut sql,
            " AND EXISTS (SELECT 1 FROM item_genres ig JOIN genres g ON g.id = ig.genre_id WHERE ig.item_id = i.id AND g.name = ?)",
            json!(g),
            &mut binds,
        );
    }
    if let Some(st) = &q.studio {
        bind(
            &mut sql,
            " AND EXISTS (SELECT 1 FROM item_studios x JOIN studios s ON s.id = x.studio_id WHERE x.item_id = i.id AND s.name = ?)",
            json!(st),
            &mut binds,
        );
    }
    match q.played {
        Some(true) => sql.push_str(" AND COALESCE(u.played, 0) = 1"),
        Some(false) => sql.push_str(" AND COALESCE(u.played, 0) = 0"),
        None => {}
    }
    if q.favorite == Some(true) {
        sql.push_str(" AND COALESCE(u.favorite, 0) = 1");
    }
    if let Some(d) = q.decade.filter(|d| (1800..2200).contains(d)) {
        let from = if d <= 1940 { 0 } else { d };
        sql.push_str(&format!(" AND i.year BETWEEN {from} AND {}", d + 9));
    }
    if let Some(v) = q.video.as_deref() {
        // Sui film i loro file; sulle serie basta un episodio con quella qualità. Solo interi e testo fisso nell'SQL.
        let cond = match v {
            "4k" => "v.width >= 3200 OR v.height >= 2000",
            "1080" => "(v.width >= 1800 OR v.height >= 1000) AND v.width < 3200 AND COALESCE(v.height, 0) < 2000",
            "720" => "(v.width >= 1200 OR v.height >= 700) AND v.width < 1800 AND COALESCE(v.height, 0) < 1000",
            "sd" => "v.width < 1200 AND COALESCE(v.height, 0) < 700",
            "hdr" => "v.hdr IS NOT NULL AND v.hdr <> ''",
            other => return Err(ApiError::BadRequest(format!("video: 4k, 1080, 720, sd, hdr (non {other})"))),
        };
        // Un'unica passata sulle tracce video: gli elementi (film, o la serie dell'episodio) che hanno quella qualità.
        sql.push_str(&format!(
            " AND i.id IN (SELECT COALESCE(e.series_id, e.id) FROM media_streams v JOIN media_files f ON f.id = v.file_id JOIN items e ON e.id = f.item_id
                 WHERE v.kind = 'video' AND f.state = 'present' AND ({cond}))"
        ));
    }
    if let Some(letter) = q.letter.as_deref().and_then(|l| l.chars().next()) {
        if letter == '#' {
            sql.push_str(" AND substr(i.sort_title, 1, 1) NOT BETWEEN 'a' AND 'z'");
        } else {
            bind(&mut sql, " AND substr(i.sort_title, 1, 1) = ?", json!(letter.to_lowercase().to_string()), &mut binds);
        }
    }
    // Ordinamento con chiave stabile (valore, id) per il cursore.
    let (expr, is_text) = match q.sort.as_deref().unwrap_or(if q.parent_id.is_some() { "index" } else { "title" }) {
        "title" => ("i.sort_title", true),
        "added" => ("i.added_at", false),
        "year" => ("COALESCE(i.year, 0)", false),
        "rating" => ("COALESCE(i.community_rating, 0)", false),
        "premiere" => ("COALESCE(i.premiere_date, '')", true),
        "index" => ("COALESCE(i.index_number, 0)", false),
        "played" => ("COALESCE(u.last_played_at, 0)", false),
        other => return Err(ApiError::BadRequest(format!("Ordinamento sconosciuto: {other}"))),
    };
    let desc = q.order.as_deref() == Some("desc");
    if let Some(c) = q.cursor.as_deref().and_then(decode_cursor) {
        let op = if desc { "<" } else { ">" };
        sql.push_str(&format!(" AND ({expr}, i.id) {op} (?{}, ?{})", idx, idx + 1));
        binds.push(c.v);
        binds.push(json!(c.id));
    }
    let dir = if desc { "DESC" } else { "ASC" };
    sql.push_str(&format!(" ORDER BY {expr} {dir}, i.id {dir} LIMIT {}", limit + 1));
    let sort_expr_value = format!("SELECT {expr} FROM items i LEFT JOIN user_item_data u ON u.item_id = i.id AND u.user_id = ?1 WHERE i.id = ?2");

    let mut query = sqlx::query_as::<_, ItemRow>(&sql);
    for b in &binds {
        query = match b {
            Value::Number(n) if n.is_i64() => query.bind(n.as_i64()),
            Value::Number(n) => query.bind(n.as_f64()),
            Value::String(s) => query.bind(s.clone()),
            _ => query.bind(Option::<i64>::None),
        };
    }
    let mut rows = query.fetch_all(&state.db.read).await?;
    let has_more = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    let next = if has_more {
        let last = rows.last().unwrap();
        let v: Value = if is_text {
            sqlx::query_scalar::<_, String>(&sort_expr_value).bind(user.user_id).bind(last.id).fetch_one(&state.db.read).await.map(Value::String)?
        } else {
            sqlx::query_scalar::<_, f64>(&format!(
                "SELECT CAST(({}) AS REAL) FROM items i LEFT JOIN user_item_data u ON u.item_id = i.id AND u.user_id = ?1 WHERE i.id = ?2",
                expr
            ))
            .bind(user.user_id)
            .bind(last.id)
            .fetch_one(&state.db.read)
            .await
            .map(|f| if f.fract() == 0.0 { json!(f as i64) } else { json!(f) })?
        };
        Some(encode_cursor(v, last.id))
    } else {
        None
    };
    let items: Vec<Value> = rows.iter().map(|r| item_json(&state, r)).collect();
    Ok(Json(json!({ "items": items, "next_cursor": next })))
}

pub async fn load_item(state: &Shared, user: &AuthUser, id: i64) -> ApiResult<ItemRow> {
    let sql = format!("{ITEM_SELECT} WHERE i.id = ?2");
    let row: ItemRow = sqlx::query_as(&sql).bind(user.user_id).bind(id).fetch_optional(&state.db.read).await?.ok_or_else(|| ApiError::not_found("Elemento"))?;
    if !user.can_see_item(state, id, row.library_id).await? {
        return Err(ApiError::not_found("Elemento"));
    }
    Ok(row)
}

fn decode(state: &Shared, id: &str) -> ApiResult<i64> {
    state.ids.decode(id).ok_or_else(|| ApiError::not_found("Elemento"))
}

#[derive(sqlx::FromRow)]
struct StreamDb {
    stream_index: i64,
    kind: String,
    codec: String,
    profile: Option<String>,
    level: Option<i64>,
    bit_depth: Option<i64>,
    width: Option<i64>,
    height: Option<i64>,
    frame_rate: Option<f64>,
    hdr: Option<String>,
    channels: Option<i64>,
    channel_layout: Option<String>,
    bitrate: Option<i64>,
    language: Option<String>,
    title: Option<String>,
    is_default: i64,
    is_forced: i64,
    is_external: i64,
}

pub async fn files_json(state: &Shared, item_id: i64) -> ApiResult<Vec<Value>> {
    let files: Vec<(i64, Option<String>, Option<String>, i64, Option<i64>, Option<i64>, String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT id, version_label, container, size, duration_ms, bitrate, state, probe_state, probe_error, relative_path FROM media_files WHERE item_id = ? ORDER BY size DESC",
    )
    .bind(item_id)
    .fetch_all(&state.db.read)
    .await?;
    let mut out = Vec::new();
    for (fid, label, container, size, dur, br, st, probe, perr, rel) in files {
        let streams: Vec<StreamDb> = sqlx::query_as(
            "SELECT stream_index, kind, codec, profile, level, bit_depth, width, height, frame_rate, hdr, channels, channel_layout, bitrate, language, title, is_default, is_forced, is_external
             FROM media_streams WHERE file_id = ? ORDER BY kind, stream_index",
        )
        .bind(fid)
        .fetch_all(&state.db.read)
        .await?;
        let mut video = Vec::new();
        let mut audio = Vec::new();
        let mut subs = Vec::new();
        for st in streams {
            let StreamDb {
                stream_index: idx,
                kind,
                codec,
                profile,
                level,
                bit_depth: bd,
                width: w,
                height: h,
                frame_rate: fr,
                hdr,
                channels: ch,
                channel_layout: layout,
                bitrate: sbr,
                language: lang,
                title,
                is_default: def,
                is_forced: forced,
                is_external: ext,
            } = st;
            match kind.as_str() {
                "video" => video.push(json!({ "index": idx, "codec": codec, "profile": profile, "level": level, "bit_depth": bd, "width": w, "height": h, "frame_rate": fr, "hdr": hdr, "bitrate": sbr })),
                "audio" => audio.push(json!({ "index": idx, "codec": codec, "channels": ch, "channel_layout": layout, "bitrate": sbr, "language": lang, "title": title, "default": def != 0 })),
                "subtitle" => subs.push(json!({ "index": idx, "codec": codec, "language": lang, "title": title, "default": def != 0, "forced": forced != 0, "external": ext != 0, "image": platea_playback::is_image_subtitle(&codec) })),
                _ => {}
            }
        }
        let name = rel.rsplit('/').next().unwrap_or(&rel).to_string();
        out.push(json!({
            "id": state.ids.encode(fid),
            "name": name,
            "version_label": label,
            "container": container,
            "size": size,
            "duration_ms": dur,
            "bitrate": br,
            "state": st,
            "probe_state": probe,
            "probe_error": perr,
            "video": video,
            "audio": audio,
            "subtitles": subs,
        }));
    }
    Ok(out)
}

async fn detail(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let iid = decode(&state, &id)?;
    let row = load_item(&state, &user, iid).await?;
    let mut v = item_json(&state, &row);
    let (tagline, trailers, manual): (Option<String>, Option<String>, bool) =
        sqlx::query_as("SELECT tagline, trailers, kind = 'collection' AND COALESCE(match_key, '') LIKE 'manual:%' FROM items WHERE id = ?")
            .bind(iid)
            .fetch_one(&state.db.read)
            .await?;
    let genres: Vec<String> = sqlx::query_scalar("SELECT g.name FROM item_genres ig JOIN genres g ON g.id = ig.genre_id WHERE ig.item_id = ? ORDER BY g.name")
        .bind(row.series_id.filter(|_| row.kind != "movie" && row.kind != "series").unwrap_or(iid))
        .fetch_all(&state.db.read)
        .await?;
    let studios: Vec<String> = sqlx::query_scalar("SELECT s.name FROM item_studios x JOIN studios s ON s.id = x.studio_id WHERE x.item_id = ?")
        .bind(iid)
        .fetch_all(&state.db.read)
        .await?;
    let people: Vec<(i64, String, String, Option<String>, Option<i64>)> = sqlx::query_as(
        "SELECT p.id, p.name, ip.role, ip.character, (SELECT id FROM images im WHERE im.person_id = p.id AND im.kind = 'profile')
         FROM item_people ip JOIN people p ON p.id = ip.person_id WHERE ip.item_id = ? ORDER BY ip.sort_order LIMIT 40",
    )
    .bind(row.series_id.filter(|_| row.kind == "season" || row.kind == "episode").unwrap_or(iid))
    .fetch_all(&state.db.read)
    .await?;
    let ext: Vec<(String, String)> = sqlx::query_as("SELECT provider, value FROM external_ids WHERE item_id = ?").bind(iid).fetch_all(&state.db.read).await?;
    let logo: Option<i64> = sqlx::query_scalar("SELECT id FROM images WHERE item_id = ? AND kind = 'logo'")
        .bind(row.series_id.filter(|_| row.kind != "movie" && row.kind != "series").unwrap_or(iid))
        .fetch_optional(&state.db.read)
        .await?;
    let o = v.as_object_mut().unwrap();
    o.insert("tagline".into(), json!(tagline));
    // Extra del film o della serie (trailer locali, dietro le quinte…): si riproducono con `file_id`.
    let extras: Vec<(i64, String, Option<String>, Option<i64>)> = sqlx::query_as(
        "SELECT id, relative_path, extra_type, duration_ms FROM media_files WHERE extra_of = ? AND state = 'present' ORDER BY extra_type, relative_path",
    )
    .bind(iid)
    .fetch_all(&state.db.read)
    .await?;
    o.insert(
        "extras".into(),
        json!(extras
            .into_iter()
            .map(|(fid, rel, kind, dur)| {
                let title = platea_naming::extra_info(&rel).map(|x| x.title).unwrap_or_else(|| rel.rsplit('/').next().unwrap_or(&rel).to_string());
                json!({ "file_id": state.ids.encode(fid), "title": title, "kind": kind, "duration_ms": dur })
            })
            .collect::<Vec<_>>()),
    );
    // Collezione creata a mano: l'amministratore può rinominarla, cancellarla e toglierne i titoli.
    o.insert("manual_collection".into(), json!(manual));
    o.insert("trailers".into(), trailers.and_then(|t| serde_json::from_str::<Value>(&t).ok()).unwrap_or_else(|| json!([])));
    o.insert("genres".into(), json!(genres));
    o.insert("studios".into(), json!(studios));
    o.insert(
        "people".into(),
        json!(people
            .iter()
            .map(|(id, n, r, c, img)| json!({ "id": state.ids.encode(*id), "name": n, "role": r, "character": c, "image": image_url(&state, *img) }))
            .collect::<Vec<_>>()),
    );
    o.insert("external_ids".into(), json!(ext.into_iter().collect::<std::collections::HashMap<_, _>>()));
    o["images"]["logo"] = json!(image_url(&state, logo));
    if row.kind == "movie" || row.kind == "episode" {
        o.insert("files".into(), json!(files_json(&state, iid).await?));
    }
    if row.kind == "movie" {
        let cols: Vec<(i64, String)> = sqlx::query_as(
            "SELECT c.id, c.title FROM collection_members cm JOIN items c ON c.id = cm.collection_id WHERE cm.item_id = ? ORDER BY c.sort_title",
        )
        .bind(iid)
        .fetch_all(&state.db.read)
        .await?;
        o.insert("collections".into(), json!(cols.iter().map(|(id, t)| json!({ "id": state.ids.encode(*id), "title": t })).collect::<Vec<_>>()));
    }
    if user.is_admin {
        let lf: String = sqlx::query_scalar("SELECT locked_fields FROM items WHERE id = ?").bind(iid).fetch_one(&state.db.read).await?;
        o.insert("locked_fields".into(), json!(lf.split(',').filter(|s| !s.is_empty()).collect::<Vec<_>>()));
    }
    Ok(Json(v))
}

async fn children(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let iid = decode(&state, &id)?;
    let parent = load_item(&state, &user, iid).await?;
    let sql = if parent.kind == "collection" {
        // I film di una collezione, in ordine di uscita.
        format!(
            "{ITEM_SELECT} WHERE i.id IN (SELECT item_id FROM collection_members WHERE collection_id = ?2) {} ORDER BY COALESCE(i.premiere_date, i.year, ''), i.sort_title LIMIT 1000",
            user.library_filter("i.library_id")
        )
    } else {
        format!("{ITEM_SELECT} WHERE i.parent_id = ?2 ORDER BY COALESCE(i.index_number, 0), i.sort_title LIMIT 1000")
    };
    let rows: Vec<ItemRow> = sqlx::query_as(&sql).bind(user.user_id).bind(iid).fetch_all(&state.db.read).await?;
    Ok(Json(json!({ "items": rows.iter().map(|r| item_json(&state, r)).collect::<Vec<_>>() })))
}

fn image_kind(kind: &str) -> ApiResult<&'static str> {
    match kind {
        "poster" => Ok("poster"),
        "backdrop" => Ok("backdrop"),
        "logo" => Ok("logo"),
        _ => Err(ApiError::BadRequest("Tipo d'immagine: poster, backdrop o logo".into())),
    }
}

async fn set_locked(state: &Shared, item_id: i64, field: &str, on: bool) -> ApiResult<()> {
    let lf: String = sqlx::query_scalar("SELECT locked_fields FROM items WHERE id = ?").bind(item_id).fetch_one(&state.db.read).await?;
    let mut list: Vec<String> = lf.split(',').filter(|s| !s.is_empty() && *s != field).map(String::from).collect();
    if on {
        list.push(field.to_string());
    }
    sqlx::query("UPDATE items SET locked_fields = ?, updated_at = ? WHERE id = ?")
        .bind(list.join(","))
        .bind(now_ms())
        .bind(item_id)
        .execute(&state.db.write)
        .await?;
    Ok(())
}

async fn upload_image(State(state): State<Shared>, _a: AdminUser, Path((id, kind)): Path<(String, String)>, body: axum::body::Bytes) -> ApiResult<Json<Value>> {
    let iid = decode(&state, &id)?;
    let kind = image_kind(&kind)?;
    if body.is_empty() || body.len() > 15 * 1024 * 1024 {
        return Err(ApiError::BadRequest("Immagine vuota o troppo grande (massimo 15 MB)".into()));
    }
    let img = crate::metadata::images::store_upload(&state, iid, kind, body.to_vec()).await.map_err(|e| ApiError::BadRequest(format!("{e:#}")))?;
    set_locked(&state, iid, kind, true).await?;
    state.events.publish("item.updated", json!({ "item_id": state.ids.encode(iid) }));
    Ok(Json(json!({ "image": image_url(&state, Some(img)) })))
}

async fn remote_images(State(state): State<Shared>, _a: AdminUser, Path((id, kind)): Path<(String, String)>) -> ApiResult<Json<Value>> {
    let iid = decode(&state, &id)?;
    let kind = image_kind(&kind)?;
    let list = crate::metadata::remote_images(&state, iid, kind).await.map_err(ApiError::Internal)?;
    let base = crate::metadata::tmdb::IMAGE_BASE.as_str();
    let preview = if kind == "poster" { "w342" } else { "w500" };
    let items: Vec<Value> = list
        .into_iter()
        .map(|mut v| {
            let p = v["path"].as_str().unwrap_or("").to_string();
            v["preview"] = json!(format!("{base}/{preview}{p}"));
            v
        })
        .collect();
    Ok(Json(json!({ "items": items })))
}

#[derive(Deserialize)]
struct ChooseReq {
    path: String,
}

async fn choose_remote_image(
    State(state): State<Shared>,
    _a: AdminUser,
    Path((id, kind)): Path<(String, String)>,
    Json(req): Json<ChooseReq>,
) -> ApiResult<Json<Value>> {
    let iid = decode(&state, &id)?;
    let kind = image_kind(&kind)?;
    // Solo un percorso di TMDB ("/abc123.jpg"): niente indirizzi arbitrari.
    let ok = req.path.starts_with('/')
        && req.path.len() < 80
        && req.path[1..].chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        && [".jpg", ".png", ".svg"].iter().any(|e| req.path.ends_with(e));
    if !ok {
        return Err(ApiError::BadRequest("Percorso d'immagine TMDB non valido".into()));
    }
    let lib: i64 = sqlx::query_scalar("SELECT library_id FROM items WHERE id = ?").bind(iid).fetch_one(&state.db.read).await?;
    if let Some(img) = crate::metadata::choose_remote_image(&state, iid, kind, &req.path).await.map_err(ApiError::Internal)? {
        jobs::enqueue(&state, JobKind::DownloadImage { image_id: img }, priority::INTERACTIVE, Some(lib)).await?;
    }
    set_locked(&state, iid, kind, true).await?;
    state.events.publish("item.updated", json!({ "item_id": state.ids.encode(iid) }));
    Ok(Json(json!({ "ok": true })))
}

async fn reset_image(State(state): State<Shared>, _a: AdminUser, Path((id, kind)): Path<(String, String)>) -> ApiResult<Json<Value>> {
    let iid = decode(&state, &id)?;
    let kind = image_kind(&kind)?;
    let ids: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM images WHERE item_id = ? AND kind = ? AND source = 'upload'").bind(iid).bind(kind).fetch_all(&state.db.read).await?;
    // La riga resta fino al prossimo aggiornamento, che ne crea una nuova (con un identificativo nuovo):
    // cancellarla ora permetterebbe a SQLite di riusarne l'identificativo e ai browser di mostrare la vecchia.
    for img in ids {
        sqlx::query("UPDATE images SET source = 'tmdb', source_url = '', state = 'failed' WHERE id = ?").bind(img).execute(&state.db.write).await?;
    }
    set_locked(&state, iid, kind, false).await?;
    // Il prossimo aggiornamento rimette l'immagine dei metadati (TMDB o NFO).
    let lib: i64 = sqlx::query_scalar("SELECT library_id FROM items WHERE id = ?").bind(iid).fetch_one(&state.db.read).await?;
    jobs::enqueue(&state, JobKind::RefreshMetadata { item_id: iid }, priority::INTERACTIVE, Some(lib)).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ChaptersQuery {
    file_id: Option<String>,
}

/// Capitoli di un film o episodio. Per i file analizzati prima che si salvassero si leggono ora dall'intestazione
/// (poche letture anche sul cloud) e si conservano; con lo storage che non risponde si restituisce un elenco vuoto.
async fn chapters(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>, Query(q): Query<ChaptersQuery>) -> ApiResult<Json<Value>> {
    let iid = decode(&state, &id)?;
    load_item(&state, &user, iid).await?;
    let wanted = q.file_id.as_deref().and_then(|f| state.ids.decode(f));
    let fid = super::download::choose_file(&state, iid, wanted).await?;
    let stored: Option<String> =
        sqlx::query_scalar::<_, Option<String>>("SELECT chapters FROM media_files WHERE id = ?").bind(fid).fetch_optional(&state.db.read).await?.flatten();
    let json_text = match stored {
        Some(s) => s,
        None => {
            let info = super::playback::load_file(&state, fid).await?;
            let ffprobe = state.ffmpeg.get().ffprobe.clone();
            let timeout = std::time::Duration::from_secs(state.settings.get().scanner.fs_timeout_seconds.max(5) as u64);
            match crate::probe::read_chapters(&ffprobe, &info.input(), timeout).await {
                Ok(s) => {
                    sqlx::query("UPDATE media_files SET chapters = ? WHERE id = ?").bind(&s).bind(fid).execute(&state.db.write).await?;
                    s
                }
                Err(e) => {
                    tracing::debug!(file = fid, error = %e, "capitoli non leggibili");
                    "[]".into()
                }
            }
        }
    };
    let mut list: Value = serde_json::from_str(&json_text).unwrap_or(json!([]));
    // Le stesse regole dell'analisi anche sui capitoli salvati prima: un titolo che è solo un orario non dice nulla.
    if let Some(arr) = list.as_array_mut() {
        for c in arr {
            if c["title"].as_str().is_some_and(|t| t.chars().all(|ch| ch.is_ascii_digit() || ch == ':' || ch == '.')) {
                c["title"] = Value::Null;
            }
        }
    }
    Ok(Json(json!({ "items": list })))
}

/// Titoli simili: stesso tipo (film o serie), ordinati per generi in comune, poi per voto.
/// Gli altri film della stessa collezione vengono prima: chi guarda il terzo capitolo cerca gli altri.
async fn similar(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let iid = decode(&state, &id)?;
    let row = load_item(&state, &user, iid).await?;
    let base = match row.kind.as_str() {
        "movie" | "series" => iid,
        "season" | "episode" => row.series_id.unwrap_or(iid),
        _ => return Ok(Json(json!({ "items": [] }))),
    };
    let kind = if row.kind == "movie" { "movie" } else { "series" };
    let sql = format!(
        "{ITEM_SELECT} JOIN (
            SELECT ig.item_id AS sid, COUNT(*) AS shared FROM item_genres ig
            WHERE ig.genre_id IN (SELECT genre_id FROM item_genres WHERE item_id = ?2) AND ig.item_id <> ?2
            GROUP BY ig.item_id
         ) g ON g.sid = i.id
         WHERE i.kind = '{kind}' AND i.available = 1 {}
         ORDER BY EXISTS (SELECT 1 FROM collection_members a JOIN collection_members b ON a.collection_id = b.collection_id WHERE a.item_id = ?2 AND b.item_id = i.id) DESC,
                  g.shared DESC, COALESCE(i.community_rating, 0) DESC
         LIMIT 16",
        user.library_filter("i.library_id")
    );
    let rows: Vec<ItemRow> = sqlx::query_as(&sql).bind(user.user_id).bind(base).fetch_all(&state.db.read).await?;
    Ok(Json(json!({ "items": rows.iter().map(|r| item_json(&state, r)).collect::<Vec<_>>() })))
}

/// Una persona: foto, ruoli e i titoli della libreria (visibili all'utente) in cui compare, dal più recente.
async fn person(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let pid = state.ids.decode(&id).ok_or_else(|| ApiError::not_found("Persona"))?;
    let p: Option<(String, Option<i64>)> = sqlx::query_as("SELECT name, tmdb_id FROM people WHERE id = ?").bind(pid).fetch_optional(&state.db.read).await?;
    let (name, tmdb) = p.ok_or_else(|| ApiError::not_found("Persona"))?;
    let image: Option<i64> =
        sqlx::query_scalar("SELECT id FROM images WHERE person_id = ? AND kind = 'profile'").bind(pid).fetch_optional(&state.db.read).await?;
    let sql = format!(
        "{ITEM_SELECT} WHERE i.id IN (SELECT item_id FROM item_people WHERE person_id = ?2) AND i.kind IN ('movie','series') {}
         ORDER BY COALESCE(i.year, 0) DESC, i.sort_title LIMIT 500",
        user.library_filter("i.library_id")
    );
    let rows: Vec<ItemRow> = sqlx::query_as(&sql).bind(user.user_id).bind(pid).fetch_all(&state.db.read).await?;
    let roles: Vec<(i64, String, Option<String>)> =
        sqlx::query_as("SELECT item_id, role, character FROM item_people WHERE person_id = ?").bind(pid).fetch_all(&state.db.read).await?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let mut v = item_json(&state, r);
            let mine: Vec<&(i64, String, Option<String>)> = roles.iter().filter(|x| x.0 == r.id).collect();
            v["roles"] = json!(mine.iter().map(|x| json!({ "role": x.1, "character": x.2 })).collect::<Vec<_>>());
            v
        })
        .collect();
    Ok(Json(json!({
        "id": id,
        "name": name,
        "image": image_url(&state, image),
        "tmdb_id": tmdb,
        "items": items,
    })))
}

/// Episodio successivo nell'ordine stagione/episodio (speciali esclusi).
pub async fn next_after(state: &Shared, user: &AuthUser, episode_id: i64) -> ApiResult<Option<ItemRow>> {
    let cur: Option<(Option<i64>, Option<i64>, Option<i64>)> =
        sqlx::query_as("SELECT series_id, parent_index_number, index_number FROM items WHERE id = ? AND kind = 'episode'")
            .bind(episode_id)
            .fetch_optional(&state.db.read)
            .await?;
    let Some((Some(series), Some(season), Some(ep))) = cur else { return Ok(None) };
    let sql = format!(
        "{ITEM_SELECT} WHERE i.series_id = ?2 AND i.kind = 'episode' AND i.available = 1 AND COALESCE(i.parent_index_number, 0) > 0
           AND (i.parent_index_number > ?3 OR (i.parent_index_number = ?3 AND i.index_number > ?4))
         ORDER BY i.parent_index_number, i.index_number LIMIT 1"
    );
    Ok(sqlx::query_as(&sql).bind(user.user_id).bind(series).bind(season).bind(ep).fetch_optional(&state.db.read).await?)
}

async fn next_episode(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let iid = decode(&state, &id)?;
    load_item(&state, &user, iid).await?;
    let next = next_after(&state, &user, iid).await?;
    Ok(Json(json!({ "item": next.map(|r| item_json(&state, &r)) })))
}

async fn home(State(state): State<Shared>, user: AuthUser) -> ApiResult<Json<Value>> {
    let lf = user.library_filter("i.library_id");
    let resume_min = state.settings.get().playback.resume_min_seconds as i64 * 1000;
    // Continua a guardare.
    let sql = format!(
        "{ITEM_SELECT} WHERE i.kind IN ('movie','episode') AND u.position_ms > {resume_min} AND COALESCE(u.played,0) = 0 AND i.available = 1 {lf}
         ORDER BY u.last_played_at DESC LIMIT 20"
    );
    let resume: Vec<ItemRow> = sqlx::query_as(&sql).bind(user.user_id).fetch_all(&state.db.read).await?;
    // Prossimi episodi: per le serie viste di recente, il primo episodio non visto dopo l'ultimo visto.
    let sql = format!(
        "SELECT i.id FROM items i JOIN user_item_data u ON u.item_id = i.id AND u.user_id = ?1
         WHERE i.kind = 'series' AND u.last_played_at IS NOT NULL {lf} ORDER BY u.last_played_at DESC LIMIT 20"
    );
    let series: Vec<i64> = sqlx::query_scalar(&sql).bind(user.user_id).fetch_all(&state.db.read).await?;
    let mut next_up = Vec::new();
    let resume_ids: Vec<i64> = resume.iter().map(|r| r.id).collect();
    for sid in series {
        let last: Option<i64> = sqlx::query_scalar(
            "SELECT e.id FROM items e JOIN user_item_data u ON u.item_id = e.id AND u.user_id = ?1
             WHERE e.series_id = ?2 AND e.kind = 'episode' AND u.played = 1 AND COALESCE(e.parent_index_number, 0) > 0
             ORDER BY e.parent_index_number DESC, e.index_number DESC LIMIT 1",
        )
        .bind(user.user_id)
        .bind(sid)
        .fetch_optional(&state.db.read)
        .await?;
        if let Some(last) = last {
            if let Some(n) = next_after(&state, &user, last).await? {
                if n.played.unwrap_or(0) == 0 && !resume_ids.contains(&n.id) {
                    next_up.push(item_json(&state, &n));
                }
            }
        }
    }
    // Aggiunti di recente, per libreria.
    let libs: Vec<(i64, String, String)> =
        sqlx::query_as(&format!("SELECT id, name, kind FROM libraries i WHERE 1 {} ORDER BY name", user.library_filter("i.id")))
            .fetch_all(&state.db.read)
            .await?;
    let prefs = crate::prefs::load(&state, user.user_id).await;
    let hidden: Vec<i64> = prefs.home_hidden_libraries.iter().filter_map(|l| state.ids.decode(l)).collect();
    let played = if prefs.hide_played_in_recent { " AND COALESCE(u.played, 0) = 0" } else { "" };
    let mut recent = Vec::new();
    for (lid, name, kind) in libs {
        if hidden.contains(&lid) {
            continue;
        }
        let k = if kind == "movies" { "movie" } else { "episode" };
        // Anche qui i limiti dell'utente (classificazione): la libreria è consentita, non ogni titolo.
        let sql =
            format!("{ITEM_SELECT} WHERE i.library_id = ?2 AND i.kind = '{k}' AND i.available = 1{played} {lf} ORDER BY i.added_at DESC, i.id DESC LIMIT 20");
        let rows: Vec<ItemRow> = sqlx::query_as(&sql).bind(user.user_id).bind(lid).fetch_all(&state.db.read).await?;
        recent.push(json!({
            "library_id": state.ids.encode(lid),
            "library_name": name,
            "kind": kind,
            "items": rows.iter().map(|r| item_json(&state, r)).collect::<Vec<_>>(),
        }));
    }
    Ok(Json(json!({
        "resume": resume.iter().map(|r| item_json(&state, r)).collect::<Vec<_>>(),
        "next_up": next_up,
        "recent": recent,
    })))
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    limit: Option<i64>,
}

/// Espressione FTS5 sicura: ogni parola tra virgolette, con ricerca per prefisso.
pub fn fts_query(q: &str) -> Option<String> {
    let parts: Vec<String> =
        q.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).take(8).map(|w| format!("\"{}\"*", w.replace('"', ""))).collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}

async fn search(State(state): State<Shared>, user: AuthUser, Query(q): Query<SearchQuery>) -> ApiResult<Json<Value>> {
    let Some(m) = fts_query(&q.q) else { return Ok(Json(json!({ "items": [] }))) };
    let limit = clamp_limit(q.limit, 40);
    let sql = format!(
        "{ITEM_SELECT} JOIN items_fts f ON f.rowid = i.id WHERE items_fts MATCH ?2 AND (i.kind IN ('movie','series','episode') OR (i.kind = 'collection' AND (SELECT COUNT(*) FROM collection_members cm WHERE cm.collection_id = i.id) >= 2)) {}
         ORDER BY CASE i.kind WHEN 'episode' THEN 1 ELSE 0 END, bm25(items_fts) LIMIT {limit}",
        user.library_filter("i.library_id")
    );
    let rows: Vec<ItemRow> = sqlx::query_as(&sql).bind(user.user_id).bind(m).fetch_all(&state.db.read).await?;
    Ok(Json(json!({ "items": rows.iter().map(|r| item_json(&state, r)).collect::<Vec<_>>() })))
}

#[derive(Deserialize)]
struct GenreQuery {
    library_id: Option<String>,
}

async fn genres(State(state): State<Shared>, user: AuthUser, Query(q): Query<GenreQuery>) -> ApiResult<Json<Value>> {
    let mut sql = format!(
        "SELECT g.name, COUNT(*) FROM genres g JOIN item_genres ig ON ig.genre_id = g.id JOIN items i ON i.id = ig.item_id WHERE 1 {}",
        user.library_filter("i.library_id")
    );
    let lid = q.library_id.as_deref().and_then(|l| state.ids.decode(l));
    if lid.is_some() {
        sql.push_str(" AND i.library_id = ?");
    }
    sql.push_str(" GROUP BY g.name ORDER BY g.name");
    let mut query = sqlx::query_as::<_, (String, i64)>(&sql);
    if let Some(l) = lid {
        query = query.bind(l);
    }
    let rows = query.fetch_all(&state.db.read).await?;
    Ok(Json(json!({ "items": rows.into_iter().map(|(n, c)| json!({ "name": n, "count": c })).collect::<Vec<_>>() })))
}

async fn set_played(state: &Shared, user: &AuthUser, id: i64, played: bool) -> ApiResult<()> {
    let row = load_item(state, user, id).await?;
    let targets: Vec<i64> = match row.kind.as_str() {
        "series" => sqlx::query_scalar("SELECT id FROM items WHERE series_id = ? AND kind = 'episode'").bind(id).fetch_all(&state.db.read).await?,
        "season" => sqlx::query_scalar("SELECT id FROM items WHERE parent_id = ? AND kind = 'episode'").bind(id).fetch_all(&state.db.read).await?,
        _ => vec![id],
    };
    let now = now_ms();
    let mut tx = state.db.write.begin().await?;
    for t in targets {
        sqlx::query(
            "INSERT INTO user_item_data (user_id, item_id, played, position_ms, play_count, last_played_at) VALUES (?1, ?2, ?3, 0, ?3, ?4)
             ON CONFLICT(user_id, item_id) DO UPDATE SET played = ?3, position_ms = 0,
               play_count = play_count + CASE WHEN ?3 = 1 AND played = 0 THEN 1 ELSE 0 END,
               last_played_at = CASE WHEN ?3 = 1 THEN ?4 ELSE last_played_at END",
        )
        .bind(user.user_id)
        .bind(t)
        .bind(played as i64)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }
    if played {
        if let Some(series) = row.series_id.or(if row.kind == "series" { Some(id) } else { None }) {
            sqlx::query(
                "INSERT INTO user_item_data (user_id, item_id, last_played_at) VALUES (?, ?, ?) ON CONFLICT(user_id, item_id) DO UPDATE SET last_played_at = excluded.last_played_at",
            )
            .bind(user.user_id)
            .bind(series)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    state.events.to_user(user.user_id, "user_data.changed", json!({ "item_id": state.ids.encode(id) }));
    Ok(())
}

async fn mark_played(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    set_played(&state, &user, decode(&state, &id)?, true).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn mark_unplayed(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    set_played(&state, &user, decode(&state, &id)?, false).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn set_fav(state: &Shared, user: &AuthUser, id: i64, fav: bool) -> ApiResult<()> {
    load_item(state, user, id).await?;
    sqlx::query("INSERT INTO user_item_data (user_id, item_id, favorite) VALUES (?1, ?2, ?3) ON CONFLICT(user_id, item_id) DO UPDATE SET favorite = ?3")
        .bind(user.user_id)
        .bind(id)
        .bind(fav as i64)
        .execute(&state.db.write)
        .await?;
    Ok(())
}

async fn fav_add(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    set_fav(&state, &user, decode(&state, &id)?, true).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn fav_remove(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    set_fav(&state, &user, decode(&state, &id)?, false).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct EditReq {
    title: Option<String>,
    overview: Option<String>,
    year: Option<i64>,
    official_rating: Option<String>,
    /// Campi da sbloccare (tornano ad aggiornarsi da TMDB).
    unlock: Vec<String>,
}

async fn edit(State(state): State<Shared>, AdminUser(admin): AdminUser, Path(id): Path<String>, Json(req): Json<EditReq>) -> ApiResult<Json<Value>> {
    let iid = decode(&state, &id)?;
    let lf: String = sqlx::query_scalar("SELECT locked_fields FROM items WHERE id = ?")
        .bind(iid)
        .fetch_optional(&state.db.read)
        .await?
        .ok_or_else(|| ApiError::not_found("Elemento"))?;
    let mut locked: Vec<String> = lf.split(',').filter(|s| !s.is_empty()).map(String::from).collect();
    let mut tx = state.db.write.begin().await?;
    let mut lock = |f: &str| {
        if !locked.iter().any(|x| x == f) {
            locked.push(f.to_string());
        }
    };
    if let Some(t) = req.title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        sqlx::query("UPDATE items SET title = ?, sort_title = ? WHERE id = ?").bind(t).bind(platea_naming::sort_title(t)).bind(iid).execute(&mut *tx).await?;
        lock("title");
    }
    if let Some(o) = &req.overview {
        sqlx::query("UPDATE items SET overview = ? WHERE id = ?").bind(o).bind(iid).execute(&mut *tx).await?;
        lock("overview");
    }
    if let Some(y) = req.year {
        sqlx::query("UPDATE items SET year = ? WHERE id = ?").bind(y).bind(iid).execute(&mut *tx).await?;
        lock("year");
    }
    if let Some(r) = &req.official_rating {
        sqlx::query("UPDATE items SET official_rating = ? WHERE id = ?").bind(r).bind(iid).execute(&mut *tx).await?;
        lock("official_rating");
    }
    locked.retain(|f| !req.unlock.contains(f));
    sqlx::query("UPDATE items SET locked_fields = ?, updated_at = ? WHERE id = ?").bind(locked.join(",")).bind(now_ms()).bind(iid).execute(&mut *tx).await?;
    tx.commit().await?;
    state.db.activity(Some(admin.user_id), "item_edited", &format!("{} ha modificato i metadati di un elemento", admin.username), None).await;
    Ok(Json(json!({ "ok": true, "locked_fields": locked })))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct CandQuery {
    q: Option<String>,
    year: Option<i64>,
}

async fn candidates(State(state): State<Shared>, _a: AdminUser, Path(id): Path<String>, Query(q): Query<CandQuery>) -> ApiResult<Json<Value>> {
    let iid = decode(&state, &id)?;
    let (kind, title, year, lib): (String, String, Option<i64>, i64) = sqlx::query_as("SELECT kind, title, year, library_id FROM items WHERE id = ?")
        .bind(iid)
        .fetch_optional(&state.db.read)
        .await?
        .ok_or_else(|| ApiError::not_found("Elemento"))?;
    if kind != "movie" && kind != "series" {
        return Err(ApiError::BadRequest("Si identificano solo film e serie".into()));
    }
    let lang: String = sqlx::query_scalar("SELECT language FROM libraries WHERE id = ?").bind(lib).fetch_one(&state.db.read).await?;
    if q.q.is_none() {
        // Prima i candidati salvati dall'associazione automatica.
        let saved: Vec<(String, String, Option<i64>, f64, Option<String>, Option<String>)> =
            sqlx::query_as("SELECT provider_id, title, year, score, overview, poster_path FROM match_candidates WHERE item_id = ? ORDER BY score DESC")
                .bind(iid)
                .fetch_all(&state.db.read)
                .await?;
        if !saved.is_empty() {
            return Ok(Json(json!({ "items": saved.into_iter().map(|(p, t, y, s, o, pp)| json!({
                "provider_id": p, "title": t, "year": y, "score": s, "overview": o,
                "poster": pp.map(|x| format!("{}/w185{x}", crate::metadata::tmdb::IMAGE_BASE.as_str())),
            })).collect::<Vec<_>>() })));
        }
    }
    let rejected = std::collections::HashSet::new();
    let c = crate::metadata::search(&state, &kind, q.q.as_deref().unwrap_or(&title), q.year.or(if q.q.is_some() { None } else { year }), &lang, &rejected)
        .await
        .map_err(|e| ApiError::BadRequest(format!("Ricerca TMDB non riuscita: {e}")))?;
    Ok(Json(json!({ "items": c.into_iter().map(|c| json!({
        "provider_id": c.provider_id, "title": c.title, "year": c.year, "score": c.score, "overview": c.overview,
        "poster": c.poster_path.map(|x| format!("{}/w185{x}", crate::metadata::tmdb::IMAGE_BASE.as_str())),
    })).collect::<Vec<_>>() })))
}

#[derive(Deserialize)]
struct IdentifyReq {
    provider_id: String,
}

async fn identify(State(state): State<Shared>, AdminUser(admin): AdminUser, Path(id): Path<String>, Json(req): Json<IdentifyReq>) -> ApiResult<Json<Value>> {
    let iid = decode(&state, &id)?;
    let tmdb: i64 = req.provider_id.trim().parse().map_err(|_| ApiError::BadRequest("Identificativo TMDB non valido".into()))?;
    crate::metadata::apply_tmdb(&state, iid, tmdb, 1.0, "manual").await.map_err(|e| ApiError::BadRequest(format!("{e:#}")))?;
    state.db.activity(Some(admin.user_id), "item_identified", &format!("{} ha identificato un elemento (TMDB {tmdb})", admin.username), None).await;
    Ok(Json(json!({ "ok": true })))
}

async fn reject(State(state): State<Shared>, _a: AdminUser, Path(id): Path<String>, Json(req): Json<IdentifyReq>) -> ApiResult<Json<Value>> {
    let iid = decode(&state, &id)?;
    sqlx::query("INSERT OR IGNORE INTO rejected_matches (item_id, provider_id) VALUES (?, ?)")
        .bind(iid)
        .bind(&req.provider_id)
        .execute(&state.db.write)
        .await?;
    sqlx::query("DELETE FROM match_candidates WHERE item_id = ? AND provider_id = ?").bind(iid).bind(&req.provider_id).execute(&state.db.write).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn refresh(State(state): State<Shared>, _a: AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let iid = decode(&state, &id)?;
    let lib: Option<i64> = sqlx::query_scalar("SELECT library_id FROM items WHERE id = ?").bind(iid).fetch_optional(&state.db.read).await?;
    let lib = lib.ok_or_else(|| ApiError::not_found("Elemento"))?;
    jobs::enqueue(&state, JobKind::RefreshMetadata { item_id: iid }, priority::INTERACTIVE, Some(lib)).await?;
    Ok(Json(json!({ "queued": true })))
}

#[derive(Deserialize)]
struct ImageQuery {
    w: Option<u32>,
}

async fn image(State(state): State<Shared>, Path(id): Path<String>, Query(q): Query<ImageQuery>, headers: HeaderMap) -> ApiResult<Response> {
    let iid = state.ids.decode(&id).ok_or_else(|| ApiError::not_found("Immagine"))?;
    let etag = format!("\"{}-{}\"", id, q.w.map(crate::metadata::images::snap_width).unwrap_or(0));
    if headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) == Some(etag.as_str()) {
        return Ok(Response::builder().status(StatusCode::NOT_MODIFIED).body(Body::empty()).unwrap());
    }
    let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM images WHERE id = ?").bind(iid).fetch_optional(&state.db.read).await?;
    if exists.is_none() {
        return Err(ApiError::not_found("Immagine"));
    }
    let path = crate::metadata::images::serve_path(&state, iid, q.w).await.map_err(|e| ApiError::NotFound(format!("Immagine non disponibile: {e}")))?;
    let bytes = tokio::fs::read(&path).await.map_err(|e| ApiError::Internal(e.into()))?;
    let ct = if path.extension().is_some_and(|e| e == "jpg") {
        "image/jpeg".to_string()
    } else {
        image::guess_format(&bytes).ok().map(|f| f.to_mime_type().to_string()).unwrap_or_else(|| "image/jpeg".into())
    };
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, ct)
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .header(header::ETAG, etag)
        .body(Body::from(bytes))
        .unwrap())
}

/// Episodi mancanti di una serie, o di una sola stagione se `id` è una stagione.
async fn missing(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let iid = decode(&state, &id)?;
    let row = load_item(&state, &user, iid).await?;
    let (series, season) = match row.kind.as_str() {
        "series" => (iid, None),
        "season" => (row.series_id.unwrap_or(iid), row.index_number),
        _ => return Ok(Json(json!({ "items": [] }))),
    };
    let rows: Vec<(i64, i64, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT season, episode, title, overview, air_date FROM missing_episodes WHERE series_id = ?1 AND (?2 IS NULL OR season = ?2) ORDER BY season, episode",
    )
    .bind(series)
    .bind(season)
    .fetch_all(&state.db.read)
    .await?;
    let items: Vec<Value> = rows.into_iter().map(|(s, e, t, o, d)| json!({ "season": s, "episode": e, "title": t, "overview": o, "air_date": d })).collect();
    Ok(Json(json!({ "items": items })))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fts() {
        assert_eq!(fts_query("matrix reloaded").as_deref(), Some("\"matrix\"* \"reloaded\"*"));
        assert_eq!(fts_query("\"; DROP").as_deref(), Some("\"DROP\"*"));
        assert_eq!(fts_query("  "), None);
    }
    #[test]
    fn cursor_roundtrip() {
        let c = encode_cursor(json!("matrix"), 42);
        let d = decode_cursor(&c).unwrap();
        assert_eq!(d.id, 42);
        assert_eq!(d.v, json!("matrix"));
    }
}
