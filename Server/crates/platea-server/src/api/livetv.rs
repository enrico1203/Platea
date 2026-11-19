//! API della Live TV: canali, guida, ricerca nei programmi, preferiti, visione dal vivo, registrazioni (con
//! Dispatcharr) e, per l'amministratore, le sorgenti.

use super::{Access, Api};
use crate::auth::{AdminUser, AuthUser};
use crate::error::{ApiError, ApiResult};
use crate::livetv::stream::{self as live, LiveSession, Variant};
use crate::livetv::{self, dispatcharr, Source};
use crate::state::Shared;
use crate::util::now_ms;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub fn routes(api: Api) -> Api {
    api.get("/api/v1/livetv/status", "Live TV", Access::User, "Live TV disponibile per l'utente, sorgenti e registrazioni", status)
        .get("/api/v1/livetv/channels", "Live TV", Access::User, "Canali con il programma in onda e il successivo", channels)
        .get("/api/v1/livetv/groups", "Live TV", Access::User, "Gruppi di canali", groups)
        .get("/api/v1/livetv/guide", "Live TV", Access::User, "Guida: programmi dei canali in un intervallo di tempo", guide)
        .get("/api/v1/livetv/search", "Live TV", Access::User, "Ricerca nei titoli dei programmi (in onda e futuri)", search)
        .get("/api/v1/livetv/programs/{id}", "Live TV", Access::User, "Un programma con il suo canale", program)
        .get("/api/v1/livetv/channels/{id}/logo", "Live TV", Access::Public, "Logo del canale (dalla sorgente, in cache)", logo)
        .post("/api/v1/livetv/channels/{id}/favorite", "Live TV", Access::User, "Aggiunge il canale ai preferiti", fav_add)
        .delete("/api/v1/livetv/channels/{id}/favorite", "Live TV", Access::User, "Toglie il canale dai preferiti", fav_remove)
        .post("/api/v1/livetv/channels/{id}/play", "Live TV", Access::User, "Avvia la visione di un canale (HLS dal vivo o MPEG-TS)", play)
        .post("/api/v1/livetv/sessions/{id}/ping", "Live TV", Access::User, "Tiene viva la sessione (il player la chiama ogni 30 s)", ping)
        .post("/api/v1/livetv/sessions/{id}/stop", "Live TV", Access::User, "Fine della visione", stop)
        .get("/api/v1/live/{session}/{file}", "Live TV", Access::Stream, "Playlist (index.m3u8), segmenti (segN.ts) o flusso diretto (stream.ts)", serve)
        .get("/api/v1/livetv/recordings", "Live TV", Access::User, "Registrazioni (con Dispatcharr)", recordings)
        .post("/api/v1/livetv/recordings", "Live TV", Access::User, "Registra un programma o tutta la serie", record)
        .delete("/api/v1/livetv/recordings/{id}", "Live TV", Access::User, "Cancella una registrazione o una regola di serie", unrecord)
        .get("/api/v1/admin/livetv/sources", "Live TV", Access::Admin, "Sorgenti della Live TV", sources)
        .post("/api/v1/admin/livetv/sources", "Live TV", Access::Admin, "Aggiunge una sorgente", source_create)
        .post("/api/v1/admin/livetv/test", "Live TV", Access::Admin, "Prova una sorgente senza salvarla", source_test)
        .patch("/api/v1/admin/livetv/sources/{id}", "Live TV", Access::Admin, "Modifica una sorgente", source_update)
        .delete("/api/v1/admin/livetv/sources/{id}", "Live TV", Access::Admin, "Elimina una sorgente con i suoi canali", source_delete)
        .post("/api/v1/admin/livetv/sources/{id}/refresh", "Live TV", Access::Admin, "Aggiorna canali e guida della sorgente", source_refresh)
        .get("/api/v1/admin/livetv/channels", "Live TV", Access::Admin, "Tutti i canali, anche nascosti", admin_channels)
        .patch("/api/v1/admin/livetv/channels/{id}", "Live TV", Access::Admin, "Nasconde o mostra un canale", admin_channel_update)
}

fn decode(state: &Shared, id: &str, what: &str) -> ApiResult<i64> {
    state.ids.decode(id).ok_or_else(|| ApiError::not_found(what))
}

fn require_tv(user: &AuthUser) -> ApiResult<()> {
    if !user.perms.allow_livetv {
        return Err(ApiError::coded(StatusCode::FORBIDDEN, "LiveTvNotAllowed", "La Live TV non è abilitata per questo account"));
    }
    Ok(())
}

#[derive(sqlx::FromRow, Clone)]
struct ChannelRow {
    id: i64,
    source_id: i64,
    number: Option<f64>,
    name: String,
    group_name: Option<String>,
    logo_url: Option<String>,
    stream_url: String,
    radio: i64,
    catchup_days: i64,
    remote_pk: Option<i64>,
    hidden: i64,
    favorite: i64,
}

const CH_COLS: &str = "c.id, c.source_id, c.number, c.name, c.group_name, c.logo_url, c.stream_url, c.radio, c.catchup_days, c.remote_pk, c.hidden,
  EXISTS (SELECT 1 FROM user_tv_favorites f WHERE f.channel_id = c.id AND f.user_id = ?1) AS favorite";

fn channel_json(state: &Shared, c: &ChannelRow) -> Value {
    json!({
        "id": state.ids.encode(c.id),
        "number": c.number,
        "name": c.name,
        "group": c.group_name,
        "logo": c.logo_url.as_ref().map(|_| format!("/api/v1/livetv/channels/{}/logo", state.ids.encode(c.id))),
        "radio": c.radio != 0,
        "catchup_days": c.catchup_days,
        "favorite": c.favorite != 0,
        "recordable": c.remote_pk.is_some(),
    })
}

#[derive(sqlx::FromRow, Clone)]
struct ProgramRow {
    id: i64,
    channel_id: i64,
    start_ms: i64,
    end_ms: i64,
    title: String,
    subtitle: Option<String>,
    description: Option<String>,
    category: Option<String>,
    season: Option<i64>,
    episode: Option<i64>,
    image_url: Option<String>,
    is_new: i64,
    rating: Option<String>,
}

const PR_COLS: &str =
    "p.id, p.channel_id, p.start_ms, p.end_ms, p.title, p.subtitle, p.description, p.category, p.season, p.episode, p.image_url, p.is_new, p.rating";

fn program_json(state: &Shared, p: &ProgramRow, full: bool) -> Value {
    let mut v = json!({
        "id": state.ids.encode(p.id),
        "channel_id": state.ids.encode(p.channel_id),
        "start_ms": p.start_ms,
        "end_ms": p.end_ms,
        "title": p.title,
        "subtitle": p.subtitle,
        "category": p.category,
        "season": p.season,
        "episode": p.episode,
        "is_new": p.is_new != 0,
    });
    if full {
        v["description"] = json!(p.description);
        v["image"] = json!(p.image_url);
        v["rating"] = json!(p.rating);
    }
    v
}

/// Canali visibili all'utente, nell'ordine della guida.
async fn visible_channels(state: &Shared, user: &AuthUser, group: Option<&str>, favorites: bool, ids: Option<&[i64]>) -> ApiResult<Vec<ChannelRow>> {
    let (filter, binds) = livetv::channel_filter(user, "c");
    let mut sql = format!("SELECT {CH_COLS} FROM tv_channels c WHERE 1 {filter}");
    if group.is_some() {
        sql.push_str(" AND COALESCE(c.group_name, '') = ?");
    }
    if favorites {
        sql.push_str(" AND EXISTS (SELECT 1 FROM user_tv_favorites f WHERE f.channel_id = c.id AND f.user_id = ?1)");
    }
    if let Some(ids) = ids {
        sql.push_str(&format!(" AND c.id IN ({})", if ids.is_empty() { "0".into() } else { ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",") }));
    }
    sql.push_str(" ORDER BY c.number IS NULL, c.number, c.name LIMIT 20000");
    let sql = livetv::number_params(&sql, 2);
    let mut q = sqlx::query_as::<_, ChannelRow>(&sql).bind(user.user_id);
    for b in &binds {
        q = q.bind(b);
    }
    if let Some(g) = group {
        q = q.bind(g);
    }
    Ok(q.fetch_all(&state.db.read).await?)
}

async fn status(State(state): State<Shared>, user: AuthUser) -> ApiResult<Json<Value>> {
    if !user.perms.allow_livetv {
        return Ok(Json(json!({ "enabled": false, "channels": 0, "can_record": false })));
    }
    let n = visible_channels(&state, &user, None, false, None).await?.len();
    let can_record: bool = user.perms.allow_recording
        && sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tv_sources WHERE kind = 'dispatcharr' AND enabled = 1 AND COALESCE(api_key, '') <> ''")
            .fetch_one(&state.db.read)
            .await?
            > 0;
    Ok(Json(json!({ "enabled": n > 0, "channels": n, "can_record": can_record })))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ChannelsQuery {
    group: Option<String>,
    favorites: bool,
}

/// Programma in onda e successivo per una serie di canali (una query).
async fn now_next(state: &Shared, ids: &[i64]) -> ApiResult<HashMap<i64, (Option<ProgramRow>, Option<ProgramRow>)>> {
    let now = now_ms();
    let mut out: HashMap<i64, (Option<ProgramRow>, Option<ProgramRow>)> = HashMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    for chunk in ids.chunks(900) {
        let list = chunk.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
        let rows: Vec<ProgramRow> = sqlx::query_as(&format!(
            "SELECT {PR_COLS} FROM tv_programs p WHERE p.channel_id IN ({list}) AND p.end_ms > ?1 AND p.start_ms < ?1 + 6 * 3600000 ORDER BY p.channel_id, p.start_ms"
        ))
        .bind(now)
        .fetch_all(&state.db.read)
        .await?;
        for p in rows {
            let e = out.entry(p.channel_id).or_default();
            if p.start_ms <= now && e.0.is_none() {
                e.0 = Some(p);
            } else if p.start_ms > now && e.1.is_none() {
                e.1 = Some(p);
            }
        }
    }
    Ok(out)
}

async fn channels(State(state): State<Shared>, user: AuthUser, Query(q): Query<ChannelsQuery>) -> ApiResult<Json<Value>> {
    require_tv(&user)?;
    let list = visible_channels(&state, &user, q.group.as_deref(), q.favorites, None).await?;
    let nn = now_next(&state, &list.iter().map(|c| c.id).collect::<Vec<_>>()).await?;
    let items: Vec<Value> = list
        .iter()
        .map(|c| {
            let mut v = channel_json(&state, c);
            let (now, next) = nn.get(&c.id).cloned().unwrap_or_default();
            v["now"] = now.map(|p| program_json(&state, &p, false)).unwrap_or(Value::Null);
            v["next"] = next.map(|p| program_json(&state, &p, false)).unwrap_or(Value::Null);
            v
        })
        .collect();
    Ok(Json(json!({ "items": items })))
}

async fn groups(State(state): State<Shared>, user: AuthUser) -> ApiResult<Json<Value>> {
    require_tv(&user)?;
    let list = visible_channels(&state, &user, None, false, None).await?;
    let mut counts: Vec<(String, usize)> = Vec::new();
    for c in list {
        let g = c.group_name.unwrap_or_default();
        match counts.iter_mut().find(|x| x.0 == g) {
            Some(x) => x.1 += 1,
            None => counts.push((g, 1)),
        }
    }
    Ok(Json(json!({ "items": counts.into_iter().map(|(name, count)| json!({ "name": name, "count": count })).collect::<Vec<_>>() })))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct GuideQuery {
    start: Option<i64>,
    end: Option<i64>,
    group: Option<String>,
    favorites: bool,
    /// Canali (id separati da virgole): per caricare la guida a pezzi.
    channels: Option<String>,
}

async fn guide(State(state): State<Shared>, user: AuthUser, Query(q): Query<GuideQuery>) -> ApiResult<Json<Value>> {
    require_tv(&user)?;
    let now = now_ms();
    let start = q.start.unwrap_or(now - 3_600_000);
    // Al massimo 2 giorni per richiesta: la guida si carica a finestre.
    let end = q.end.unwrap_or(start + 6 * 3_600_000).min(start + 48 * 3_600_000).max(start);
    let only: Option<Vec<i64>> = q.channels.as_deref().map(|s| s.split(',').filter_map(|x| state.ids.decode(x.trim())).collect());
    let list = visible_channels(&state, &user, q.group.as_deref(), q.favorites, only.as_deref()).await?;
    let mut by: HashMap<i64, Vec<Value>> = HashMap::new();
    let ids: Vec<i64> = list.iter().map(|c| c.id).collect();
    for chunk in ids.chunks(900) {
        let l = chunk.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
        let rows: Vec<ProgramRow> =
            sqlx::query_as(&format!("SELECT {PR_COLS} FROM tv_programs p WHERE p.channel_id IN ({l}) AND p.end_ms > ? AND p.start_ms < ? ORDER BY p.start_ms"))
                .bind(start)
                .bind(end)
                .fetch_all(&state.db.read)
                .await?;
        for p in rows {
            by.entry(p.channel_id).or_default().push(program_json(&state, &p, false));
        }
    }
    let channels: Vec<Value> = list
        .iter()
        .map(|c| {
            let mut v = channel_json(&state, c);
            v["programs"] = json!(by.remove(&c.id).unwrap_or_default());
            v
        })
        .collect();
    Ok(Json(json!({ "start": start, "end": end, "now": now, "channels": channels })))
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
}

async fn search(State(state): State<Shared>, user: AuthUser, Query(q): Query<SearchQuery>) -> ApiResult<Json<Value>> {
    require_tv(&user)?;
    let Some(m) = super::items::fts_query(&q.q) else { return Ok(Json(json!({ "items": [] }))) };
    let (filter, binds) = livetv::channel_filter(&user, "c");
    let sql = format!(
        "SELECT {PR_COLS} FROM tv_programs p JOIN tv_programs_fts f ON f.rowid = p.id JOIN tv_channels c ON c.id = p.channel_id
         WHERE tv_programs_fts MATCH ? AND p.end_ms > ? {filter} ORDER BY p.start_ms LIMIT 60"
    );
    let mut qq = sqlx::query_as::<_, ProgramRow>(&sql).bind(m).bind(now_ms());
    for b in &binds {
        qq = qq.bind(b);
    }
    let rows = qq.fetch_all(&state.db.read).await?;
    let ch_ids: Vec<i64> = rows.iter().map(|p| p.channel_id).collect();
    let chans: HashMap<i64, Value> =
        visible_channels(&state, &user, None, false, Some(&ch_ids)).await?.iter().map(|c| (c.id, channel_json(&state, c))).collect();
    Ok(Json(json!({ "items": rows.iter().map(|p| {
        let mut v = program_json(&state, p, false);
        v["channel"] = chans.get(&p.channel_id).cloned().unwrap_or(Value::Null);
        v
    }).collect::<Vec<_>>() })))
}

async fn load_channel(state: &Shared, user: &AuthUser, id: i64) -> ApiResult<ChannelRow> {
    visible_channels(state, user, None, false, Some(&[id])).await?.into_iter().next().ok_or_else(|| ApiError::not_found("Canale"))
}

async fn program(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    require_tv(&user)?;
    let pid = decode(&state, &id, "Programma")?;
    let p: ProgramRow = sqlx::query_as(&format!("SELECT {PR_COLS} FROM tv_programs p WHERE p.id = ?"))
        .bind(pid)
        .fetch_optional(&state.db.read)
        .await?
        .ok_or_else(|| ApiError::not_found("Programma"))?;
    let c = load_channel(&state, &user, p.channel_id).await?;
    let mut v = program_json(&state, &p, true);
    v["channel"] = channel_json(&state, &c);
    let rec: Option<(i64, String, String)> = sqlx::query_as(
        "SELECT id, kind, status FROM tv_recordings WHERE channel_id = ? AND ((kind = 'once' AND start_ms = ?) OR (kind = 'series' AND title = ?)) ORDER BY kind LIMIT 1",
    )
    .bind(p.channel_id)
    .bind(p.start_ms)
    .bind(&p.title)
    .fetch_optional(&state.db.read)
    .await?;
    v["recording"] = rec.map(|(id, kind, st)| json!({ "id": state.ids.encode(id), "kind": kind, "status": st })).unwrap_or(Value::Null);
    Ok(Json(v))
}

/// Logo dalla sorgente, tenuto in cache: gli indirizzi di Dispatcharr sono interni, il browser non li raggiunge.
async fn logo(State(state): State<Shared>, Path(id): Path<String>, headers: HeaderMap) -> ApiResult<Response> {
    let cid = decode(&state, &id, "Canale")?;
    logo_response(&state, cid, &headers).await
}

/// Logo del canale `cid` (anche per le app Jellyfin: `/Items/<canale>/Images/Primary`).
pub async fn logo_response(state: &Shared, cid: i64, headers: &HeaderMap) -> ApiResult<Response> {
    let state = state.clone();
    let url: Option<String> = sqlx::query_scalar("SELECT logo_url FROM tv_channels WHERE id = ?").bind(cid).fetch_optional(&state.db.read).await?.flatten();
    let url = url.ok_or_else(|| ApiError::not_found("Logo"))?;
    let key = crate::util::sha256_hex(&url);
    let path = state.boot.cache_dir.join("livetv").join("logos").join(format!("{}.img", &key[..32]));
    let bytes = match tokio::fs::read(&path).await {
        Ok(b) => b,
        Err(_) => {
            let mut req = state.http.get(&url).timeout(Duration::from_secs(15));
            // Logo servito da Dispatcharr: la chiave della sorgente apre anche quelli protetti.
            let src: Option<(String, Option<String>, String)> =
                sqlx::query_as("SELECT s.kind, s.api_key, s.url FROM tv_sources s JOIN tv_channels c ON c.source_id = s.id WHERE c.id = ?")
                    .bind(cid)
                    .fetch_optional(&state.db.read)
                    .await?;
            if let Some((kind, Some(k), base)) = src {
                if kind == "dispatcharr" && url.starts_with(base.trim_end_matches('/')) {
                    req = req.header("X-API-Key", k);
                }
            }
            let r = req.send().await.map_err(|_| ApiError::not_found("Logo"))?;
            if !r.status().is_success() {
                return Err(ApiError::not_found("Logo"));
            }
            let b = r.bytes().await.map_err(|_| ApiError::not_found("Logo"))?;
            if b.len() > 5 * 1024 * 1024 || image::guess_format(&b).is_err() {
                return Err(ApiError::not_found("Logo"));
            }
            if let Some(d) = path.parent() {
                let _ = tokio::fs::create_dir_all(d).await;
            }
            let _ = tokio::fs::write(&path, &b).await;
            b.to_vec()
        }
    };
    let ct = match image::guess_format(&bytes) {
        Ok(image::ImageFormat::Png) => "image/png",
        Ok(image::ImageFormat::WebP) => "image/webp",
        Ok(image::ImageFormat::Gif) => "image/gif",
        _ => "image/jpeg",
    };
    let (resp, _) = crate::stream::serve_bytes(bytes, ct, "public, max-age=86400", headers);
    Ok(resp)
}

async fn fav_add(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    require_tv(&user)?;
    let c = load_channel(&state, &user, decode(&state, &id, "Canale")?).await?;
    sqlx::query("INSERT OR IGNORE INTO user_tv_favorites (user_id, channel_id) VALUES (?, ?)").bind(user.user_id).bind(c.id).execute(&state.db.write).await?;
    Ok(Json(json!({ "favorite": true })))
}

async fn fav_remove(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let cid = decode(&state, &id, "Canale")?;
    sqlx::query("DELETE FROM user_tv_favorites WHERE user_id = ? AND channel_id = ?").bind(user.user_id).bind(cid).execute(&state.db.write).await?;
    Ok(Json(json!({ "favorite": false })))
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct PlayReq {
    pub profile: platea_playback::DeviceProfile,
    pub max_bitrate: Option<u64>,
    /// MPEG-TS così com'è (app che lo leggono da sole).
    pub direct: bool,
    pub force_transcode: bool,
}

/// Avvio della visione: usata dalla web UI e dallo strato Jellyfin.
pub async fn start(state: &Shared, user: &AuthUser, channel_id: i64, req: PlayReq) -> ApiResult<Value> {
    require_tv(user)?;
    user.require_playback()?;
    let c = load_channel(state, user, channel_id).await?;
    let src = livetv::load_source(state, c.source_id).await?.ok_or_else(|| ApiError::not_found("Sorgente"))?;
    // Un dispositivo guarda una cosa alla volta.
    for old in state.live.by_device(user.token_id) {
        live::stop_session(state, &old);
    }
    for old in state.sessions.by_device(user.token_id) {
        crate::sessions::stop_session(state, &old).await;
    }
    if let Some(max) = user.perms.max_streams {
        if (state.sessions.user_count(user.user_id) + state.live.user_count(user.user_id)) as i64 >= max {
            return Err(ApiError::coded(
                StatusCode::TOO_MANY_REQUESTS,
                "TooManyStreams",
                format!("Questo account può avere al massimo {max} riproduzioni contemporanee"),
            ));
        }
    }
    // Limite del fornitore: un canale già acceso non apre connessioni nuove.
    if let Some(max) = src.max_streams.filter(|m| *m > 0) {
        if (req.direct || !state.live.channel_running(c.id)) && state.live.upstreams(src.id) as i64 >= max {
            return Err(ApiError::coded(
                StatusCode::TOO_MANY_REQUESTS,
                "SourceBusy",
                format!("Tutti i {max} flussi della sorgente {} sono in uso: riprova quando qualcuno smette di guardare", src.name),
            ));
        }
    }
    let s = state.settings.get();
    let remote = user.ip.is_some_and(|ip| !crate::net::in_any(&s.network.local_networks, ip));
    let mut max_bitrate = req.max_bitrate.filter(|b| *b > 0).or(user.perms.max_bitrate.map(|b| b as u64));
    let remote_max = s.network.remote_max_bitrate_mbps as u64 * 1_000_000;
    if remote && remote_max > 0 {
        max_bitrate = Some(max_bitrate.map_or(remote_max, |m| m.min(remote_max)));
    }
    let allow_transcode = user.perms.allow_transcode && state.ffmpeg.get().available;
    drop(s);
    let (session_id, token) = live::new_ids();
    let (variant, codecs, stream) = if req.direct {
        (Variant::Direct, json!({}), None)
    } else {
        let codecs =
            live::probe(state, c.id, &c.stream_url).await.map_err(|e| ApiError::coded(StatusCode::BAD_GATEWAY, "ChannelUnavailable", format!("{e:#}")))?;
        let variant = live::choose(&codecs, &req.profile, max_bitrate, allow_transcode, req.force_transcode);
        let height = codecs["height"].as_u64().unwrap_or(720) as u32;
        let max_h = match max_bitrate {
            Some(b) if b < 3_000_000 => 480,
            Some(b) if b < 6_000_000 => 720,
            _ => height.clamp(240, 1080),
        };
        let br = match max_h {
            480 => 1_500_000,
            720 => 3_500_000,
            _ => 6_000_000,
        };
        let interlaced = codecs["interlaced"].as_bool().unwrap_or(false);
        let st = live::ensure_stream(state, c.id, c.source_id, &c.stream_url, variant, max_h, br, interlaced).await.map_err(ApiError::Internal)?;
        if let Err(e) = live::wait_ready(&st, Duration::from_secs(45)).await {
            return Err(ApiError::coded(StatusCode::BAD_GATEWAY, "ChannelUnavailable", format!("{e:#}")));
        }
        (variant, codecs, Some(st))
    };
    let chain = match variant {
        Variant::Direct => "MPEG-TS → client".to_string(),
        Variant::Copy => format!("{} → HLS (copia)", codecs["video"].as_str().unwrap_or("?").to_uppercase()),
        Variant::Audio => {
            format!("{} → HLS · {} → AAC", codecs["video"].as_str().unwrap_or("?").to_uppercase(), codecs["audio"].as_str().unwrap_or("?").to_uppercase())
        }
        Variant::Transcode => format!("{} → H.264 · Software", codecs["video"].as_str().unwrap_or("?").to_uppercase()),
    };
    let sess = Arc::new(LiveSession {
        id: session_id.clone(),
        token: token.clone(),
        user_id: user.user_id,
        token_id: user.token_id,
        username: user.username.clone(),
        device_name: user.device_name.clone(),
        client: user.client.clone(),
        ip: user.ip.map(|i| i.to_string()),
        remote,
        expires_at: if user.is_admin { None } else { user.perms.expires_at },
        channel_id: c.id,
        source_id: c.source_id,
        channel_name: c.name.clone(),
        channel_number: c.number,
        logo: c.logo_url.is_some(),
        variant,
        chain: chain.clone(),
        codecs: codecs.clone(),
        started_at: now_ms(),
        last_seen: AtomicI64::new(now_ms()),
        bytes_sent: Arc::new(AtomicU64::new(0)),
        stream,
        direct_url: req.direct.then(|| c.stream_url.clone()),
    });
    state.live.insert(sess);
    state.events.admin("sessions.changed", json!({ "started": session_id }));
    tracing::info!(user = %user.username, channel = %c.name, method = variant.as_str(), chain = %chain, "Live TV: visione avviata");
    let q = format!("?t={token}");
    Ok(json!({
        "session_id": session_id,
        "stream_token": token,
        "method": variant.as_str(),
        "chain": chain,
        "hls_url": (variant != Variant::Direct).then(|| format!("/api/v1/live/{session_id}/index.m3u8{q}")),
        "stream_url": (variant == Variant::Direct).then(|| format!("/api/v1/live/{session_id}/stream.ts{q}")),
        "channel": channel_json(state, &c),
        "codecs": codecs,
    }))
}

async fn play(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>, Json(req): Json<PlayReq>) -> ApiResult<Json<Value>> {
    let cid = decode(&state, &id, "Canale")?;
    Ok(Json(start(&state, &user, cid, req).await?))
}

fn own(state: &Shared, user: &AuthUser, id: &str) -> ApiResult<Arc<LiveSession>> {
    let s = state.live.get(id).ok_or_else(|| ApiError::not_found("Sessione"))?;
    if s.user_id != user.user_id && !user.is_admin {
        return Err(ApiError::not_found("Sessione"));
    }
    Ok(s)
}

async fn ping(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let s = own(&state, &user, &id)?;
    s.touch();
    let err = s.stream.as_ref().and_then(|x| x.last_error());
    Ok(Json(json!({ "ok": true, "error": err })))
}

async fn stop(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    // Già chiusa (cambio canale, inattività): niente da fare.
    if state.live.get(&id).is_none() {
        return Ok(Json(json!({ "ok": true })));
    }
    own(&state, &user, &id)?;
    live::stop_session(&state, &id);
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct TokenQuery {
    t: Option<String>,
}

async fn serve(State(state): State<Shared>, Path((id, file)): Path<(String, String)>, Query(q): Query<TokenQuery>, headers: HeaderMap) -> ApiResult<Response> {
    let s = state.live.get(&id).ok_or(ApiError::Unauthorized)?;
    if q.t.as_deref() != Some(s.token.as_str()) {
        return Err(ApiError::Unauthorized);
    }
    if s.expires_at.is_some_and(|e| now_ms() >= e) {
        return Err(ApiError::coded(StatusCode::FORBIDDEN, "AccountExpired", "Account scaduto"));
    }
    s.touch();
    if file == "stream.ts" {
        let url = s.direct_url.clone().ok_or_else(|| ApiError::BadRequest("Questa sessione usa HLS".into()))?;
        return direct(&state, &s, &url).await;
    }
    let st = s.stream.clone().ok_or_else(|| ApiError::BadRequest("Questa sessione usa il flusso diretto".into()))?;
    if !(file == "index.m3u8" || (file.starts_with("seg") && file.ends_with(".ts") && file[3..file.len() - 3].chars().all(|c| c.is_ascii_digit()))) {
        return Err(ApiError::not_found("File"));
    }
    if file == "index.m3u8" {
        // FFmpeg caduto (flusso interrotto): si riaccende; il player riprova la playlist da solo.
        if st.exited().await {
            let interlaced = s.codecs["interlaced"].as_bool().unwrap_or(false);
            let c: Option<(String,)> =
                sqlx::query_as("SELECT stream_url FROM tv_channels WHERE id = ?").bind(s.channel_id).fetch_optional(&state.db.read).await?;
            if let Some((url,)) = c {
                let _ = live::ensure_stream(&state, s.channel_id, s.source_id, &url, st.variant, 720, 3_500_000, interlaced).await;
            }
        }
        let text = tokio::fs::read_to_string(st.dir.join("index.m3u8")).await.map_err(|_| ApiError::not_found("Playlist"))?;
        let t = &s.token;
        let body: String = text.lines().map(|l| if !l.is_empty() && !l.starts_with('#') { format!("{l}?t={t}\n") } else { format!("{l}\n") }).collect();
        return Ok(Response::builder()
            .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
            .header(header::CACHE_CONTROL, "no-cache, no-store")
            .body(Body::from(body))
            .unwrap());
    }
    let bytes = tokio::fs::read(st.dir.join(&file)).await.map_err(|_| ApiError::not_found("Segmento"))?;
    s.bytes_sent.fetch_add(bytes.len() as u64, Ordering::Relaxed);
    let (resp, _) = crate::stream::serve_bytes(bytes, "video/mp2t", "private, max-age=60", &headers);
    Ok(resp)
}

/// MPEG-TS dalla sorgente al client, attraverso il server (limite di flussi della sorgente rispettato).
async fn direct(state: &Shared, s: &Arc<LiveSession>, url: &str) -> ApiResult<Response> {
    use futures_util::StreamExt;
    let guard = crate::livetv::stream::LiveManager::open_direct(state, s.source_id);
    let resp = state
        .http
        .get(url)
        .header("User-Agent", format!("Platea/{}", crate::state::VERSION))
        .send()
        .await
        .map_err(|e| ApiError::coded(StatusCode::BAD_GATEWAY, "ChannelUnavailable", format!("Canale non raggiungibile: {e}")))?;
    if !resp.status().is_success() {
        return Err(ApiError::coded(StatusCode::BAD_GATEWAY, "ChannelUnavailable", format!("La sorgente ha risposto {}", resp.status())));
    }
    let sess = s.clone();
    let counter = s.bytes_sent.clone();
    let body = resp.bytes_stream().map(move |chunk| {
        let _keep = &guard;
        if let Ok(c) = &chunk {
            counter.fetch_add(c.len() as u64, Ordering::Relaxed);
            sess.touch();
        }
        chunk.map_err(std::io::Error::other)
    });
    Ok(Response::builder().header(header::CONTENT_TYPE, "video/mp2t").header(header::CACHE_CONTROL, "no-store").body(Body::from_stream(body)).unwrap())
}

// ---------------------------------------------------------------------------------------------
// Registrazioni

#[derive(sqlx::FromRow)]
struct RecRow {
    id: i64,
    user_id: Option<i64>,
    channel_id: Option<i64>,
    source_id: i64,
    remote_id: Option<String>,
    kind: String,
    title: String,
    subtitle: Option<String>,
    description: Option<String>,
    start_ms: i64,
    end_ms: i64,
    status: String,
    file_path: Option<String>,
    shared: i64,
}

const REC_COLS: &str = "id, user_id, channel_id, source_id, remote_id, kind, title, subtitle, description, start_ms, end_ms, status, file_path, shared";

fn require_recording(user: &AuthUser) -> ApiResult<()> {
    require_tv(user)?;
    if !user.perms.allow_recording {
        return Err(ApiError::coded(StatusCode::FORBIDDEN, "RecordingNotAllowed", "Le registrazioni non sono abilitate per questo account"));
    }
    Ok(())
}

/// Allinea lo stato delle registrazioni con Dispatcharr (al massimo ogni 20 s).
async fn sync_recordings(state: &Shared) -> anyhow::Result<()> {
    static LAST: AtomicI64 = AtomicI64::new(0);
    let now = now_ms();
    if now - LAST.load(Ordering::Relaxed) < 20_000 {
        return Ok(());
    }
    LAST.store(now, Ordering::Relaxed);
    let sources: Vec<Source> =
        sqlx::query_as(&format!("SELECT {} FROM tv_sources WHERE kind = 'dispatcharr' AND COALESCE(api_key, '') <> ''", livetv::SOURCE_COLS))
            .fetch_all(&state.db.read)
            .await?;
    for s in sources {
        let remote = match dispatcharr::recordings(state, &s).await {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!(source = %s.name, error = %e, "registrazioni di Dispatcharr non lette");
                continue;
            }
        };
        let ours: Vec<(i64, String)> =
            sqlx::query_as("SELECT id, remote_id FROM tv_recordings WHERE source_id = ? AND kind = 'once' AND remote_id IS NOT NULL")
                .bind(s.id)
                .fetch_all(&state.db.read)
                .await?;
        let known: std::collections::HashSet<String> = ours.iter().map(|x| x.1.clone()).collect();
        let mut tx = state.db.write.begin().await?;
        for (id, rid) in &ours {
            match remote.get(rid) {
                Some((st, file, ..)) => {
                    sqlx::query("UPDATE tv_recordings SET status = ?, file_path = ? WHERE id = ?").bind(st).bind(file).bind(id).execute(&mut *tx).await?;
                }
                None => {
                    sqlx::query("UPDATE tv_recordings SET status = 'deleted' WHERE id = ? AND status <> 'deleted'").bind(id).execute(&mut *tx).await?;
                }
            }
        }
        // Puntate programmate dalle regole di serie create da Platea: diventano righe di chi ha chiesto la serie.
        let series: Vec<(i64, Option<i64>, Option<i64>, String, i64)> =
            sqlx::query_as("SELECT r.user_id, r.channel_id, c.remote_pk, r.title, r.shared FROM tv_recordings r LEFT JOIN tv_channels c ON c.id = r.channel_id WHERE r.source_id = ? AND r.kind = 'series'")
                .bind(s.id)
                .fetch_all(&mut *tx)
                .await?;
        for (rid, (st, file, start, end, ch, title)) in &remote {
            if known.contains(rid) {
                continue;
            }
            let Some((user, channel, _, t, shared)) = series.iter().find(|x| x.2 == *ch && title.as_deref() == Some(x.3.as_str())) else { continue };
            sqlx::query(
                "INSERT INTO tv_recordings (user_id, channel_id, source_id, remote_id, kind, title, start_ms, end_ms, status, file_path, shared, created_at)
                 VALUES (?, ?, ?, ?, 'once', ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(user)
            .bind(channel)
            .bind(s.id)
            .bind(rid)
            .bind(t)
            .bind(start)
            .bind(end)
            .bind(st)
            .bind(file)
            .bind(shared)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
    }
    Ok(())
}

/// Il file registrato visto da Platea: percorso di Dispatcharr tradotto nella cartella della sorgente, poi
/// l'elemento della libreria che lo contiene (se la cartella è una libreria).
async fn recorded_item(state: &Shared, source_id: i64, file: &str) -> Option<String> {
    let map: Option<String> =
        sqlx::query_scalar("SELECT recordings_path FROM tv_sources WHERE id = ?").bind(source_id).fetch_optional(&state.db.read).await.ok()??;
    let map = map?;
    let rel = file.strip_prefix("/data/recordings/")?;
    let abs = format!("{}/{rel}", map.trim_end_matches('/'));
    let rows: Vec<(i64, String)> = sqlx::query_as("SELECT id, path FROM library_paths").fetch_all(&state.db.read).await.ok()?;
    for (pid, root) in rows {
        let root = root.trim_end_matches('/').to_string();
        if let Some(r) = abs.strip_prefix(&format!("{root}/")) {
            let item: Option<i64> = sqlx::query_scalar("SELECT item_id FROM media_files WHERE library_path_id = ? AND relative_path = ?")
                .bind(pid)
                .bind(r)
                .fetch_optional(&state.db.read)
                .await
                .ok()?
                .flatten();
            return item.map(|i| state.ids.encode(i));
        }
    }
    None
}

async fn recordings(State(state): State<Shared>, user: AuthUser) -> ApiResult<Json<Value>> {
    require_tv(&user)?;
    if let Err(e) = sync_recordings(&state).await {
        tracing::debug!(error = %e, "allineamento delle registrazioni");
    }
    let rows: Vec<RecRow> = sqlx::query_as(&format!(
        "SELECT {REC_COLS} FROM tv_recordings WHERE status <> 'deleted' AND (user_id = ?1 OR shared = 1 OR ?2) ORDER BY start_ms DESC LIMIT 500"
    ))
    .bind(user.user_id)
    .bind(user.is_admin)
    .fetch_all(&state.db.read)
    .await?;
    let mut out = Vec::new();
    for r in rows {
        let ch = match r.channel_id {
            Some(c) => visible_channels(&state, &user, None, false, Some(&[c])).await?.into_iter().next().map(|c| channel_json(&state, &c)),
            None => None,
        };
        let item = match &r.file_path {
            Some(f) if matches!(r.status.as_str(), "completed" | "stopped" | "interrupted") => recorded_item(&state, r.source_id, f).await,
            _ => None,
        };
        out.push(json!({
            "id": state.ids.encode(r.id),
            "kind": r.kind,
            "title": r.title,
            "subtitle": r.subtitle,
            "description": r.description,
            "start_ms": r.start_ms,
            "end_ms": r.end_ms,
            "status": r.status,
            "channel": ch,
            "item_id": item,
            "mine": r.user_id == Some(user.user_id),
            "shared": r.shared != 0,
        }));
    }
    Ok(Json(json!({ "items": out })))
}

#[derive(Deserialize)]
struct RecordReq {
    program_id: String,
    #[serde(default)]
    series: bool,
    #[serde(default)]
    only_new: bool,
    #[serde(default)]
    shared: bool,
}

async fn record(State(state): State<Shared>, user: AuthUser, Json(req): Json<RecordReq>) -> ApiResult<Json<Value>> {
    record_inner(&state, &user, req).await
}

/// Per lo strato Jellyfin (timer).
pub async fn record_for(state: &Shared, user: &AuthUser, req: Value) -> ApiResult<Json<Value>> {
    let req: RecordReq = serde_json::from_value(req).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    record_inner(state, user, req).await
}

async fn record_inner(state: &Shared, user: &AuthUser, req: RecordReq) -> ApiResult<Json<Value>> {
    let (state, user) = (state.clone(), user.clone());
    require_recording(&user)?;
    let pid = decode(&state, &req.program_id, "Programma")?;
    let p: ProgramRow = sqlx::query_as(&format!("SELECT {PR_COLS} FROM tv_programs p WHERE p.id = ?"))
        .bind(pid)
        .fetch_optional(&state.db.read)
        .await?
        .ok_or_else(|| ApiError::not_found("Programma"))?;
    let c = load_channel(&state, &user, p.channel_id).await?;
    let src = livetv::load_source(&state, c.source_id).await?.ok_or_else(|| ApiError::not_found("Sorgente"))?;
    if src.kind != "dispatcharr" || src.api_key.as_deref().unwrap_or("").is_empty() {
        return Err(ApiError::BadRequest("Le registrazioni passano da Dispatcharr: questa sorgente non lo è (o manca la chiave API)".into()));
    }
    let pk = c.remote_pk.ok_or_else(|| ApiError::BadRequest("Il canale non è ancora collegato a Dispatcharr: aggiorna la sorgente".into()))?;
    if p.end_ms <= now_ms() {
        return Err(ApiError::BadRequest("Il programma è già finito".into()));
    }
    let shared = req.shared && user.is_admin;
    if req.series {
        dispatcharr::create_series_rule(&state, &src, pk, &p.title, req.only_new).await.map_err(|e| ApiError::BadRequest(format!("{e:#}")))?;
        let id = sqlx::query(
            "INSERT INTO tv_recordings (user_id, channel_id, source_id, kind, title, start_ms, end_ms, status, shared, created_at) VALUES (?, ?, ?, 'series', ?, ?, ?, 'active', ?, ?)",
        )
        .bind(user.user_id)
        .bind(c.id)
        .bind(src.id)
        .bind(&p.title)
        .bind(p.start_ms)
        .bind(p.end_ms)
        .bind(shared as i64)
        .bind(now_ms())
        .execute(&state.db.write)
        .await?
        .last_insert_rowid();
        return Ok(Json(json!({ "id": state.ids.encode(id), "kind": "series" })));
    }
    let info = dispatcharr::ProgramInfo { title: &p.title, subtitle: p.subtitle.as_deref(), description: p.description.as_deref() };
    let rid = dispatcharr::create_recording(&state, &src, pk, p.start_ms, p.end_ms, &info).await.map_err(|e| ApiError::BadRequest(format!("{e:#}")))?;
    let id = sqlx::query(
        "INSERT INTO tv_recordings (user_id, channel_id, source_id, remote_id, kind, title, subtitle, description, start_ms, end_ms, status, shared, created_at)
         VALUES (?, ?, ?, ?, 'once', ?, ?, ?, ?, ?, 'scheduled', ?, ?)",
    )
    .bind(user.user_id)
    .bind(c.id)
    .bind(src.id)
    .bind(&rid)
    .bind(&p.title)
    .bind(&p.subtitle)
    .bind(&p.description)
    .bind(p.start_ms)
    .bind(p.end_ms)
    .bind(shared as i64)
    .bind(now_ms())
    .execute(&state.db.write)
    .await?
    .last_insert_rowid();
    state
        .db
        .activity(Some(user.user_id), "recording_scheduled", &format!("{} ha programmato la registrazione di {} ({})", user.username, p.title, c.name), None)
        .await;
    Ok(Json(json!({ "id": state.ids.encode(id), "kind": "once" })))
}

async fn unrecord(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let rid = decode(&state, &id, "Registrazione")?;
    unrecord_for(&state, &user, rid).await
}

pub async fn unrecord_for(state: &Shared, user: &AuthUser, rid: i64) -> ApiResult<Json<Value>> {
    let (state, user) = (state.clone(), user.clone());
    require_recording(&user)?;
    let r: RecRow = sqlx::query_as(&format!("SELECT {REC_COLS} FROM tv_recordings WHERE id = ?"))
        .bind(rid)
        .fetch_optional(&state.db.read)
        .await?
        .ok_or_else(|| ApiError::not_found("Registrazione"))?;
    if r.user_id != Some(user.user_id) && !user.is_admin {
        return Err(ApiError::not_found("Registrazione"));
    }
    let src = livetv::load_source(&state, r.source_id).await?.ok_or_else(|| ApiError::not_found("Sorgente"))?;
    if r.kind == "series" {
        dispatcharr::delete_series_rule(&state, &src, &r.title).await.map_err(|e| ApiError::BadRequest(format!("{e:#}")))?;
        sqlx::query("DELETE FROM tv_recordings WHERE id = ?").bind(rid).execute(&state.db.write).await?;
    } else if let Some(remote) = &r.remote_id {
        // In corso: si ferma (resta la parte registrata); altrimenti si cancella.
        if r.status == "recording" {
            dispatcharr::stop_recording(&state, &src, remote).await.map_err(|e| ApiError::BadRequest(format!("{e:#}")))?;
            sqlx::query("UPDATE tv_recordings SET status = 'stopped' WHERE id = ?").bind(rid).execute(&state.db.write).await?;
        } else {
            dispatcharr::delete_recording(&state, &src, remote).await.map_err(|e| ApiError::BadRequest(format!("{e:#}")))?;
            sqlx::query("DELETE FROM tv_recordings WHERE id = ?").bind(rid).execute(&state.db.write).await?;
        }
    }
    let _ = (r.subtitle, r.description, r.end_ms, r.file_path);
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------------------------
// Amministrazione

fn source_json(state: &Shared, s: &Source, extra: Option<(Option<String>, Option<String>, i64)>) -> Value {
    let (status, message, channels) = extra.unwrap_or((None, None, 0));
    json!({
        "id": state.ids.encode(s.id),
        "kind": s.kind,
        "name": s.name,
        "url": s.url,
        "epg_url": s.epg_url,
        "has_api_key": s.api_key.as_ref().is_some_and(|k| !k.is_empty()),
        "username": s.username,
        "has_password": s.password.as_ref().is_some_and(|k| !k.is_empty()),
        "channel_profile": s.channel_profile,
        "output_profile": s.output_profile,
        "recordings_path": s.recordings_path,
        "max_streams": s.max_streams,
        "refresh_hours": s.refresh_hours,
        "epg_refresh_hours": s.epg_refresh_hours,
        "epg_days": s.epg_days,
        "enabled": s.enabled != 0,
        "last_refresh_at": s.last_refresh_at,
        "last_epg_at": s.last_epg_at,
        "last_status": status,
        "last_message": message,
        "channels": channels,
        "active_streams": state.live.upstreams(s.id),
    })
}

async fn sources(State(state): State<Shared>, _a: AdminUser) -> ApiResult<Json<Value>> {
    let list: Vec<Source> =
        sqlx::query_as(&format!("SELECT {} FROM tv_sources ORDER BY position, name", livetv::SOURCE_COLS)).fetch_all(&state.db.read).await?;
    let mut out = Vec::new();
    for s in &list {
        let (st, msg): (Option<String>, Option<String>) =
            sqlx::query_as("SELECT last_status, last_message FROM tv_sources WHERE id = ?").bind(s.id).fetch_one(&state.db.read).await?;
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tv_channels WHERE source_id = ? AND present = 1").bind(s.id).fetch_one(&state.db.read).await?;
        out.push(source_json(&state, s, Some((st, msg, n))));
    }
    Ok(Json(json!({ "items": out })))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct SourcePatch {
    kind: Option<String>,
    name: Option<String>,
    url: Option<String>,
    #[serde(deserialize_with = "crate::util::double_option")]
    epg_url: Option<Option<String>>,
    /// Stringa vuota = togli; assente = lascia com'è.
    api_key: Option<String>,
    #[serde(deserialize_with = "crate::util::double_option")]
    username: Option<Option<String>>,
    password: Option<String>,
    #[serde(deserialize_with = "crate::util::double_option")]
    channel_profile: Option<Option<String>>,
    #[serde(deserialize_with = "crate::util::double_option")]
    output_profile: Option<Option<i64>>,
    #[serde(deserialize_with = "crate::util::double_option")]
    recordings_path: Option<Option<String>>,
    #[serde(deserialize_with = "crate::util::double_option")]
    max_streams: Option<Option<i64>>,
    refresh_hours: Option<i64>,
    epg_refresh_hours: Option<i64>,
    epg_days: Option<i64>,
    enabled: Option<bool>,
}

fn validate(p: &SourcePatch, creating: bool) -> ApiResult<()> {
    if let Some(k) = &p.kind {
        if !matches!(k.as_str(), "dispatcharr" | "m3u" | "hdhomerun" | "xtream") {
            return Err(ApiError::BadRequest("Tipo: dispatcharr, m3u, hdhomerun o xtream".into()));
        }
    } else if creating {
        return Err(ApiError::BadRequest("Serve il tipo della sorgente".into()));
    }
    if let Some(u) = &p.url {
        let u = u.trim();
        let ok = u.starts_with("http://") || u.starts_with("https://") || (p.kind.as_deref() == Some("m3u") && u.starts_with('/'));
        if !ok {
            return Err(ApiError::BadRequest("Indirizzo non valido: serve http:// o https:// (per una lista M3U anche un percorso del server)".into()));
        }
    } else if creating {
        return Err(ApiError::BadRequest("Serve l'indirizzo".into()));
    }
    Ok(())
}

fn clean(v: &Option<Option<String>>) -> Option<Option<String>> {
    v.as_ref().map(|x| x.as_ref().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()))
}

async fn write_patch(state: &Shared, id: i64, p: &SourcePatch) -> ApiResult<()> {
    let mut tx = state.db.write.begin().await?;
    macro_rules! set {
        ($col:literal, $v:expr) => {
            sqlx::query(concat!("UPDATE tv_sources SET ", $col, " = ? WHERE id = ?")).bind($v).bind(id).execute(&mut *tx).await?;
        };
    }
    if let Some(v) = &p.name {
        set!("name", v.trim());
    }
    if let Some(v) = &p.url {
        set!("url", v.trim());
    }
    if let Some(v) = clean(&p.epg_url) {
        set!("epg_url", v);
    }
    if let Some(v) = &p.api_key {
        set!("api_key", Some(v.trim()).filter(|s| !s.is_empty()));
    }
    if let Some(v) = clean(&p.username) {
        set!("username", v);
    }
    if let Some(v) = &p.password {
        set!("password", Some(v.as_str()).filter(|s| !s.is_empty()));
    }
    if let Some(v) = clean(&p.channel_profile) {
        set!("channel_profile", v);
    }
    if let Some(v) = p.output_profile {
        set!("output_profile", v);
    }
    if let Some(v) = clean(&p.recordings_path) {
        set!("recordings_path", v);
    }
    if let Some(v) = p.max_streams {
        set!("max_streams", v.filter(|m| *m > 0));
    }
    if let Some(v) = p.refresh_hours {
        set!("refresh_hours", v.clamp(1, 168));
    }
    if let Some(v) = p.epg_refresh_hours {
        set!("epg_refresh_hours", v.clamp(1, 168));
    }
    if let Some(v) = p.epg_days {
        set!("epg_days", v.clamp(1, 14));
    }
    if let Some(v) = p.enabled {
        set!("enabled", v as i64);
    }
    tx.commit().await?;
    Ok(())
}

async fn source_create(State(state): State<Shared>, AdminUser(admin): AdminUser, Json(p): Json<SourcePatch>) -> ApiResult<Json<Value>> {
    validate(&p, true)?;
    let kind = p.kind.clone().unwrap_or_default();
    let name = p.name.clone().map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| match kind.as_str() {
        "dispatcharr" => "Dispatcharr".into(),
        "hdhomerun" => "HDHomeRun".into(),
        "xtream" => "Xtream Codes".into(),
        _ => "IPTV".into(),
    });
    let id = sqlx::query("INSERT INTO tv_sources (kind, name, url, created_at) VALUES (?, ?, ?, ?)")
        .bind(&kind)
        .bind(&name)
        .bind(p.url.clone().unwrap_or_default().trim())
        .bind(now_ms())
        .execute(&state.db.write)
        .await?
        .last_insert_rowid();
    write_patch(&state, id, &p).await?;
    crate::jobs::enqueue(&state, crate::jobs::JobKind::RefreshTvSource { source_id: id, epg: true }, crate::jobs::priority::INTERACTIVE, None).await?;
    state.db.activity(Some(admin.user_id), "livetv_source_created", &format!("{} ha aggiunto la sorgente TV {name}", admin.username), None).await;
    let s = livetv::load_source(&state, id).await?.ok_or_else(|| ApiError::not_found("Sorgente"))?;
    Ok(Json(source_json(&state, &s, None)))
}

async fn source_update(State(state): State<Shared>, _a: AdminUser, Path(id): Path<String>, Json(p): Json<SourcePatch>) -> ApiResult<Json<Value>> {
    let sid = decode(&state, &id, "Sorgente")?;
    validate(&p, false)?;
    livetv::load_source(&state, sid).await?.ok_or_else(|| ApiError::not_found("Sorgente"))?;
    write_patch(&state, sid, &p).await?;
    crate::jobs::enqueue(&state, crate::jobs::JobKind::RefreshTvSource { source_id: sid, epg: true }, crate::jobs::priority::INTERACTIVE, None).await?;
    let s = livetv::load_source(&state, sid).await?.ok_or_else(|| ApiError::not_found("Sorgente"))?;
    Ok(Json(source_json(&state, &s, None)))
}

async fn source_delete(State(state): State<Shared>, AdminUser(admin): AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let sid = decode(&state, &id, "Sorgente")?;
    for s in state.live.sessions().iter().filter(|s| s.source_id == sid) {
        live::stop_session(&state, &s.id);
    }
    sqlx::query("DELETE FROM tv_sources WHERE id = ?").bind(sid).execute(&state.db.write).await?;
    state.db.activity(Some(admin.user_id), "livetv_source_deleted", &format!("{} ha eliminato una sorgente TV", admin.username), None).await;
    Ok(Json(json!({ "ok": true })))
}

async fn source_refresh(State(state): State<Shared>, _a: AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let sid = decode(&state, &id, "Sorgente")?;
    crate::jobs::enqueue(&state, crate::jobs::JobKind::RefreshTvSource { source_id: sid, epg: true }, crate::jobs::priority::INTERACTIVE, None).await?;
    Ok(Json(json!({ "queued": true })))
}

/// Prova senza salvare: canali letti, versione di Dispatcharr e chiave, guida raggiungibile.
async fn source_test(State(state): State<Shared>, _a: AdminUser, Json(p): Json<SourcePatch>) -> ApiResult<Json<Value>> {
    validate(&p, true)?;
    // Chiave o password non riscritte nel dialogo di modifica: quelle salvate.
    let saved: Option<Source> = match p.url.as_deref() {
        Some(u) => {
            sqlx::query_as(&format!("SELECT {} FROM tv_sources WHERE url = ? LIMIT 1", livetv::SOURCE_COLS))
                .bind(u.trim())
                .fetch_optional(&state.db.read)
                .await?
        }
        None => None,
    };
    let s = Source {
        id: 0,
        kind: p.kind.clone().unwrap_or_default(),
        name: p.name.clone().unwrap_or_default(),
        url: p.url.clone().unwrap_or_default().trim().to_string(),
        epg_url: clean(&p.epg_url).flatten(),
        api_key: p.api_key.clone().filter(|k| !k.is_empty()).or(saved.as_ref().and_then(|s| s.api_key.clone())),
        username: clean(&p.username).flatten(),
        password: p.password.clone().filter(|k| !k.is_empty()).or(saved.as_ref().and_then(|s| s.password.clone())),
        channel_profile: clean(&p.channel_profile).flatten(),
        output_profile: p.output_profile.flatten(),
        recordings_path: None,
        max_streams: None,
        refresh_hours: 6,
        epg_refresh_hours: 12,
        epg_days: p.epg_days.unwrap_or(7),
        enabled: 1,
        last_refresh_at: None,
        last_epg_at: None,
    };
    let mut out = json!({ "ok": false });
    if s.kind == "dispatcharr" && s.api_key.as_ref().is_some_and(|k| !k.is_empty()) {
        match dispatcharr::version(&state, &s).await {
            Ok(v) => out["version"] = json!(v),
            Err(e) => {
                out["message"] = json!(format!("{e:#}"));
                return Ok(Json(out));
            }
        }
    }
    match livetv::fetch_channels(&state, &s).await {
        Ok(list) => {
            out["ok"] = json!(!list.is_empty());
            out["channels"] = json!(list.len());
            out["groups"] = json!(list.iter().filter_map(|c| c.group.clone()).collect::<std::collections::BTreeSet<_>>().len());
            out["epg_url"] = json!(livetv::epg_url(&s).map(|u| livetv::redact(&u)));
            out["recordable"] = json!(list.iter().any(|c| c.remote_pk.is_some()));
            if list.is_empty() {
                out["message"] = json!("La sorgente risponde ma non ha canali");
            }
        }
        Err(e) => out["message"] = json!(format!("{e:#}")),
    }
    Ok(Json(out))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct AdminChannelsQuery {
    source_id: Option<String>,
}

async fn admin_channels(State(state): State<Shared>, AdminUser(admin): AdminUser, Query(q): Query<AdminChannelsQuery>) -> ApiResult<Json<Value>> {
    let sid = q.source_id.as_deref().and_then(|s| state.ids.decode(s));
    let rows: Vec<ChannelRow> = sqlx::query_as(&format!(
        "SELECT {CH_COLS} FROM tv_channels c WHERE c.present = 1 AND (?2 IS NULL OR c.source_id = ?2) ORDER BY c.number IS NULL, c.number, c.name LIMIT 20000"
    ))
    .bind(admin.user_id)
    .bind(sid)
    .fetch_all(&state.db.read)
    .await?;
    Ok(Json(json!({ "items": rows.iter().map(|c| {
        let mut v = channel_json(&state, c);
        v["hidden"] = json!(c.hidden != 0);
        v["source_id"] = json!(state.ids.encode(c.source_id));
        let _ = &c.stream_url;
        v
    }).collect::<Vec<_>>() })))
}

#[derive(Deserialize)]
struct ChannelPatch {
    hidden: bool,
}

async fn admin_channel_update(State(state): State<Shared>, _a: AdminUser, Path(id): Path<String>, Json(p): Json<ChannelPatch>) -> ApiResult<Json<Value>> {
    let cid = decode(&state, &id, "Canale")?;
    sqlx::query("UPDATE tv_channels SET hidden = ? WHERE id = ?").bind(p.hidden as i64).bind(cid).execute(&state.db.write).await?;
    Ok(Json(json!({ "hidden": p.hidden })))
}
