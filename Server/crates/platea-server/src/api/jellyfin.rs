//! Strato compatibile Jellyfin, solo per Infuse.
//!
//! Platea espone la piccola parte dell'API di Jellyfin che Infuse usa in modalità diretta: login,
//! librerie, elenchi e dettagli, immagini, ricerca, stato di visione, streaming del file originale
//! con Range e rapporti di riproduzione. Scritto in modo indipendente, senza codice di Jellyfin.
//! L'API nativa (`/api/v1`) resta il contratto principale.

use crate::api::auth::{do_login, LoginReq};
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::sessions::{stop_session, PlaySession, SessionManager};
use crate::state::{Shared, VERSION};
use crate::util::now_ms;
use axum::body::Body;
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{header, HeaderMap, Method, StatusCode, Uri};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;

use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Versione di Jellyfin dichiarata: Infuse legge nome e versione al collegamento.
const JELLYFIN_VERSION: &str = "10.11.0";
const TICKS_PER_MS: i64 = 10_000;

pub fn router() -> Router<Shared> {
    Router::new()
        .route("/System/Info/Public", get(public_info))
        .route("/System/Info", get(system_info))
        .route("/System/Ping", get(ping).post(ping))
        .route("/Users/Public", get(users_public))
        .route("/Users/AuthenticateByName", post(authenticate))
        .route("/Sessions/Logout", post(logout))
        .route("/Users/Me", get(me))
        .route("/Users/{user}", get(user_by_id))
        .route("/Genres", get(genres_list))
        .route("/Studios", get(empty_items))
        .route("/Persons", get(empty_items))
        .route("/Artists", get(empty_items))
        .route("/MusicGenres", get(empty_items))
        .route("/Items/Filters", get(filters))
        .route("/Items/Filters2", get(filters))
        .route("/Items/Counts", get(counts))
        .route("/Library/VirtualFolders", get(virtual_folders))
        .route("/Sessions", get(empty_array))
        .route("/Plugins", get(empty_array))
        .route("/Users/{user}/Views", get(views))
        .route("/UserViews", get(views_q))
        .route("/Library/MediaFolders", get(views_q))
        .route("/Users/{user}/Items", get(items_u))
        .route("/Items", get(items_q))
        .route("/Users/{user}/Items/Resume", get(resume_u))
        .route("/UserItems/Resume", get(resume_q))
        .route("/Users/{user}/Items/Latest", get(latest_u))
        .route("/Items/Latest", get(latest_q))
        .route("/Users/{user}/Items/{item}", get(item_u))
        .route("/Items/{item}", get(item_q))
        .route("/Shows/NextUp", get(next_up))
        .route("/Shows/{series}/Seasons", get(seasons))
        .route("/Shows/{series}/Episodes", get(episodes))
        .route("/Search/Hints", get(search_hints))
        .route("/Items/{item}/Images/{kind}", get(image).head(image))
        .route("/Items/{item}/Images/{kind}/{index}", get(image_idx).head(image_idx))
        .route("/Items/{item}/PlaybackInfo", get(playback_info).post(playback_info))
        .route("/Videos/{item}/stream", get(stream).head(stream))
        .route("/Items/{item}/Download", get(download).head(download))
        .route("/Videos/{item}/{file}", get(stream_named).head(stream_named))
        .route("/Videos/{item}/{source}/Subtitles/{index}/{name}", get(subtitle))
        .route("/Videos/{item}/{source}/Subtitles/{index}/{offset}/{name}", get(subtitle_offset))
        .route("/Sessions/Playing", post(playing))
        .route("/Sessions/Playing/Progress", post(progress))
        .route("/Sessions/Playing/Stopped", post(stopped))
        .route("/Sessions/Playing/Ping", post(ping))
        .route("/Sessions/Capabilities", post(ok_empty))
        .route("/Sessions/Capabilities/Full", post(ok_empty))
        .route("/Users/{user}/PlayedItems/{item}", post(played_add_u).delete(played_del_u))
        .route("/UserPlayedItems/{item}", post(played_add).delete(played_del))
        .route("/Users/{user}/FavoriteItems/{item}", post(fav_add_u).delete(fav_del_u))
        .route("/UserFavoriteItems/{item}", post(fav_add).delete(fav_del))
        .route("/DisplayPreferences/{id}", get(display_prefs))
        .route("/Branding/Configuration", get(branding))
}

// ---------------------------------------------------------------------------------------------
// Identificativi: GUID a 32 cifre esadecimali come Jellyfin. I primi 16 caratteri distinguono il tipo.

const NS_ITEM: &str = "0000000000000000";
const NS_LIBRARY: &str = "1000000000000000";
const NS_USER: &str = "2000000000000000";
const NS_PERSON: &str = "3000000000000000";

#[derive(Debug, Clone, Copy, PartialEq)]
enum Ref {
    Item(i64),
    Library(i64),
    User(i64),
    Person(i64),
}

fn guid(state: &Shared, ns: &str, id: i64) -> String {
    format!("{ns}{}", state.ids.encode(id))
}

fn item_guid(state: &Shared, id: i64) -> String {
    guid(state, NS_ITEM, id)
}

fn parse_ref(state: &Shared, s: &str) -> Option<Ref> {
    let s: String = s.chars().filter(|c| *c != '-').collect::<String>().to_lowercase();
    if s.len() != 32 {
        return None;
    }
    let (ns, rest) = s.split_at(16);
    let id = state.ids.decode(rest)?;
    match ns {
        NS_ITEM => Some(Ref::Item(id)),
        NS_LIBRARY => Some(Ref::Library(id)),
        NS_USER => Some(Ref::User(id)),
        NS_PERSON => Some(Ref::Person(id)),
        _ => None,
    }
}

fn item_id(state: &Shared, s: &str) -> ApiResult<i64> {
    match parse_ref(state, s) {
        Some(Ref::Item(i)) => Ok(i),
        _ => Err(ApiError::not_found("Elemento")),
    }
}

fn ticks(ms: Option<i64>) -> Option<i64> {
    ms.map(|m| m * TICKS_PER_MS)
}

fn iso(ms: i64) -> String {
    // Data ISO 8601 in UTC senza dipendenze esterne.
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Algoritmo civile di Howard Hinnant.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}.0000000Z")
}

// ---------------------------------------------------------------------------------------------
// Instradamento senza distinzione tra maiuscole e minuscole, come Jellyfin.

static CANONICAL: Lazy<HashMap<String, &'static str>> = Lazy::new(|| {
    let words = [
        "System",
        "Info",
        "Public",
        "Ping",
        "Users",
        "AuthenticateByName",
        "Sessions",
        "Logout",
        "Me",
        "Views",
        "UserViews",
        "Library",
        "MediaFolders",
        "Items",
        "Resume",
        "UserItems",
        "Latest",
        "Shows",
        "NextUp",
        "Seasons",
        "Episodes",
        "Search",
        "Hints",
        "Images",
        "PlaybackInfo",
        "Videos",
        "stream",
        "Subtitles",
        "Playing",
        "Progress",
        "Stopped",
        "Capabilities",
        "Full",
        "PlayedItems",
        "UserPlayedItems",
        "FavoriteItems",
        "UserFavoriteItems",
        "DisplayPreferences",
        "Branding",
        "Configuration",
    ];
    words.iter().map(|w| (w.to_lowercase(), *w)).collect()
});

/// Riscrive i segmenti noti nella forma canonica (`/users/me` → `/Users/Me`) e toglie il prefisso `/emby`.
/// Va applicata prima dell'instradamento. Non tocca `/api/v1`.
pub fn normalize_uri<B>(mut req: axum::http::Request<B>) -> axum::http::Request<B> {
    let path = req.uri().path();
    if !path.starts_with("/api/") && path.len() > 1 {
        let fixed: Vec<String> =
            path.split('/').map(|seg| CANONICAL.get(&seg.to_lowercase()).map(|c| c.to_string()).unwrap_or_else(|| seg.to_string())).collect();
        let mut new_path = fixed.join("/");
        if let Some(rest) = new_path.strip_prefix("/emby").or_else(|| new_path.strip_prefix("/Emby")) {
            new_path = rest.to_string();
        }
        if new_path != path {
            let pq = match req.uri().query() {
                Some(q) => format!("{new_path}?{q}"),
                None => new_path,
            };
            if let Ok(uri) = pq.parse::<Uri>() {
                *req.uri_mut() = uri;
            }
        }
    }
    req
}

// ---------------------------------------------------------------------------------------------
// Sistema e accesso

fn server_name(state: &Shared) -> String {
    state.settings.get().general.server_name.clone()
}

async fn public_info(State(state): State<Shared>) -> Json<Value> {
    Json(json!({
        "LocalAddress": state.settings.get().general.public_url,
        "ServerName": server_name(&state),
        "Version": JELLYFIN_VERSION,
        "ProductName": "Jellyfin Server",
        "OperatingSystem": "Linux",
        "Id": state.server_id,
        "StartupWizardCompleted": state.setup_code.lock().unwrap().is_none(),
    }))
}

async fn system_info(State(state): State<Shared>, _u: AuthUser) -> Json<Value> {
    Json(json!({
        "LocalAddress": state.settings.get().general.public_url,
        "ServerName": server_name(&state),
        "Version": JELLYFIN_VERSION,
        "ProductName": "Jellyfin Server",
        "OperatingSystem": "Linux",
        "OperatingSystemDisplayName": "Linux",
        "Id": state.server_id,
        "StartupWizardCompleted": true,
        "SupportsLibraryMonitor": false,
        "CanSelfRestart": false,
        "CanLaunchWebBrowser": false,
        "HasUpdateAvailable": false,
        "HasPendingRestart": false,
        "IsShuttingDown": false,
        "EncoderLocation": "System",
        "SystemArchitecture": std::env::consts::ARCH,
        "PlateaVersion": VERSION,
    }))
}

async fn ping() -> &'static str {
    "Jellyfin Server"
}

async fn ok_empty() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn users_public() -> Json<Value> {
    // Nessun elenco pubblico degli utenti: si entra con nome e password.
    Json(json!([]))
}

/// `MediaBrowser Client="Infuse", Device="iPhone", DeviceId="…", Version="8.0"`
fn auth_fields(headers: &HeaderMap) -> HashMap<String, String> {
    let raw = headers.get("x-emby-authorization").or_else(|| headers.get(header::AUTHORIZATION)).and_then(|v| v.to_str().ok()).unwrap_or("");
    let mut out = HashMap::new();
    let body = raw.trim().trim_start_matches("MediaBrowser").trim_start_matches("Emby").trim();
    for part in body.split(',') {
        if let Some((k, v)) = part.trim().split_once('=') {
            out.insert(k.trim().to_lowercase(), v.trim().trim_matches('"').to_string());
        }
    }
    out
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AuthBody {
    username: String,
    #[serde(default)]
    pw: Option<String>,
    #[serde(default)]
    password: Option<String>,
}

async fn user_dto(state: &Shared, user_id: i64) -> ApiResult<Value> {
    let (name, is_admin, last, expires, allow_download, prefs_raw): (String, i64, Option<i64>, Option<i64>, i64, String) =
        sqlx::query_as("SELECT username, is_admin, last_login_at, expires_at, allow_download, preferences FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_one(&state.db.read)
            .await?;
    let prefs = crate::prefs::UserPrefs::parse(&prefs_raw);
    let sub_mode = match prefs.subtitle_mode {
        crate::prefs::SubtitleMode::Default => "Default",
        crate::prefs::SubtitleMode::Always => "Always",
        crate::prefs::SubtitleMode::OnlyForced => "OnlyForced",
        crate::prefs::SubtitleMode::None => "None",
        crate::prefs::SubtitleMode::Smart => "Smart",
    };
    Ok(json!({
        "Name": name,
        "ServerId": state.server_id,
        "Id": guid(state, NS_USER, user_id),
        "HasPassword": true,
        "HasConfiguredPassword": true,
        "HasConfiguredEasyPassword": false,
        "EnableAutoLogin": false,
        "LastLoginDate": last.map(iso),
        "LastActivityDate": last.map(iso),
        "Configuration": {
            "PlayDefaultAudioTrack": prefs.audio_language.is_empty(),
            "AudioLanguagePreference": prefs.audio_language,
            "SubtitleLanguagePreference": prefs.subtitle_language,
            "SubtitleMode": sub_mode,
            "HidePlayedInLatest": prefs.hide_played_in_recent,
            "EnableNextEpisodeAutoPlay": prefs.autoplay_next,
            "OrderedViews": [],
            "LatestItemsExcludes": [],
            "MyMediaExcludes": [],
        },
        "Policy": {
            "IsAdministrator": is_admin != 0,
            "IsHidden": true,
            "IsDisabled": false,
            "EnableAllFolders": true,
            "EnableMediaPlayback": is_admin != 0 || expires.is_none_or(|e| now_ms() < e),
            "EnableAudioPlaybackTranscoding": true,
            "EnableVideoPlaybackTranscoding": true,
            "EnablePlaybackRemuxing": true,
            "EnableContentDownloading": is_admin != 0 || allow_download != 0,
            "EnableRemoteAccess": true,
            "EnableUserPreferenceAccess": true,
            "AuthenticationProviderId": "Platea",
            "PasswordResetProviderId": "Platea",
        },
    }))
}

async fn authenticate(
    State(state): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<AuthBody>,
) -> ApiResult<Json<Value>> {
    let f = auth_fields(&headers);
    let ip = crate::net::client_ip(Some(peer), &headers, &state.settings.get().network.trusted_proxies);
    let req = LoginReq {
        username: body.username,
        password: body.pw.or(body.password).unwrap_or_default(),
        device_id: f.get("deviceid").cloned(),
        device_name: f.get("device").cloned(),
        client: f.get("client").cloned(),
        client_version: f.get("version").cloned(),
    };
    let ok = do_login(&state, &req, ip).await?;
    Ok(Json(json!({
        "User": user_dto(&state, ok.user_id).await?,
        "SessionInfo": {
            "Id": crate::util::random_hex(16),
            "UserId": guid(&state, NS_USER, ok.user_id),
            "UserName": ok.username,
            "Client": req.client,
            "DeviceName": req.device_name,
            "DeviceId": req.device_id,
            "ApplicationVersion": req.client_version,
            "ServerId": state.server_id,
            "SupportsRemoteControl": false,
            "PlayableMediaTypes": ["Video"],
        },
        "AccessToken": ok.token,
        "ServerId": state.server_id,
    })))
}

async fn logout(State(state): State<Shared>, user: AuthUser) -> ApiResult<StatusCode> {
    sqlx::query("DELETE FROM tokens WHERE id = ?").bind(user.token_id).execute(&state.db.write).await?;
    state.token_cache.clear();
    Ok(StatusCode::NO_CONTENT)
}

async fn me(State(state): State<Shared>, user: AuthUser) -> ApiResult<Json<Value>> {
    Ok(Json(user_dto(&state, user.user_id).await?))
}

async fn user_by_id(State(state): State<Shared>, user: AuthUser, Path(_id): Path<String>) -> ApiResult<Json<Value>> {
    Ok(Json(user_dto(&state, user.user_id).await?))
}

async fn display_prefs(Path(id): Path<String>) -> Json<Value> {
    Json(
        json!({ "Id": id, "SortBy": "SortName", "SortOrder": "Ascending", "RememberIndexing": false, "PrimaryImageHeight": 250, "PrimaryImageWidth": 250, "CustomPrefs": {}, "ScrollDirection": "Horizontal", "ShowBackdrop": true, "RememberSorting": false, "ShowSidebar": false, "Client": "emby" }),
    )
}

async fn branding() -> Json<Value> {
    Json(json!({ "LoginDisclaimer": "", "CustomCss": "", "SplashscreenEnabled": false }))
}

// ---------------------------------------------------------------------------------------------
// Librerie e elementi

async fn library_dto(state: &Shared, id: i64, name: &str, kind: &str) -> Value {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM items WHERE library_id = ? AND kind IN ('movie','series')")
        .bind(id)
        .fetch_one(&state.db.read)
        .await
        .unwrap_or(0);
    json!({
        "Name": name,
        "ServerId": state.server_id,
        "Id": guid(state, NS_LIBRARY, id),
        "Etag": format!("{id}"),
        "Type": "CollectionFolder",
        "CollectionType": if kind == "movies" { "movies" } else { "tvshows" },
        "IsFolder": true,
        "ChildCount": count,
        "LocationType": "FileSystem",
        "ImageTags": {},
        "BackdropImageTags": [],
        "UserData": { "PlaybackPositionTicks": 0, "PlayCount": 0, "IsFavorite": false, "Played": false, "Key": format!("lib{id}") },
    })
}

/// Elenchi che Platea non gestisce (studi, persone, musica): vuoti ma validi, così i client non si fermano.
async fn empty_items(_user: AuthUser) -> Json<Value> {
    Json(json!({ "Items": [], "TotalRecordCount": 0, "StartIndex": 0 }))
}

async fn empty_array(_user: AuthUser) -> Json<Value> {
    Json(json!([]))
}

async fn genre_names(state: &Shared, user: &AuthUser) -> ApiResult<Vec<String>> {
    Ok(sqlx::query_scalar(&format!(
        "SELECT DISTINCT g.name FROM genres g JOIN item_genres ig ON ig.genre_id = g.id JOIN items i ON i.id = ig.item_id WHERE 1 {} ORDER BY g.name",
        user.library_filter("i.library_id")
    ))
    .fetch_all(&state.db.read)
    .await?)
}

async fn genres_list(State(state): State<Shared>, user: AuthUser) -> ApiResult<Json<Value>> {
    let names = genre_names(&state, &user).await?;
    let items: Vec<Value> = names
        .iter()
        .map(|n| json!({ "Name": n, "Id": format!("{:x}", md5_like(n)), "Type": "Genre", "ServerId": state.server_id, "ImageTags": {} }))
        .collect();
    let n = items.len();
    Ok(Json(json!({ "Items": items, "TotalRecordCount": n, "StartIndex": 0 })))
}

/// Identificativo stabile di un genere dal nome (i generi non hanno un ID pubblico in Platea).
fn md5_like(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

async fn filters(State(state): State<Shared>, user: AuthUser) -> ApiResult<Json<Value>> {
    let genres = genre_names(&state, &user).await?;
    let years: Vec<i64> = sqlx::query_scalar(&format!(
        "SELECT DISTINCT i.year FROM items i WHERE i.year IS NOT NULL AND i.kind IN ('movie','series') {} ORDER BY i.year DESC",
        user.library_filter("i.library_id")
    ))
    .fetch_all(&state.db.read)
    .await?;
    let ratings: Vec<String> = sqlx::query_scalar(&format!(
        "SELECT DISTINCT i.official_rating FROM items i WHERE i.official_rating IS NOT NULL {} ORDER BY 1",
        user.library_filter("i.library_id")
    ))
    .fetch_all(&state.db.read)
    .await?;
    let genre_items: Vec<Value> = genres.iter().map(|n| json!({ "Name": n, "Id": format!("{:x}", md5_like(n)) })).collect();
    Ok(Json(json!({ "Genres": genre_items, "Tags": [], "OfficialRatings": ratings, "Years": years })))
}

async fn counts(State(state): State<Shared>, user: AuthUser) -> ApiResult<Json<Value>> {
    let rows: Vec<(String, i64)> =
        sqlx::query_as(&format!("SELECT i.kind, COUNT(*) FROM items i WHERE 1 {} GROUP BY i.kind", user.library_filter("i.library_id")))
                    .fetch_all(&state.db.read)
            .await?;
    let get = |k: &str| rows.iter().find(|(x, _)| x == k).map(|(_, n)| *n).unwrap_or(0);
    Ok(Json(json!({
        "MovieCount": get("movie"), "SeriesCount": get("series"), "EpisodeCount": get("episode"), "BoxSetCount": get("collection"),
        "ArtistCount": 0, "ProgramCount": 0, "TrailerCount": 0, "SongCount": 0, "AlbumCount": 0, "MusicVideoCount": 0, "BookCount": 0, "ItemCount": get("movie") + get("series") + get("episode"),
    })))
}

/// Cartelle delle librerie, nella forma di Jellyfin (alcuni client le leggono anche senza essere amministratori).
async fn virtual_folders(State(state): State<Shared>, user: AuthUser) -> ApiResult<Json<Value>> {
    let libs: Vec<(i64, String, String)> =
        sqlx::query_as(&format!("SELECT id, name, kind FROM libraries i WHERE 1 {} ORDER BY name", user.library_filter("i.id")))
                    .fetch_all(&state.db.read)
            .await?;
    let out: Vec<Value> = libs
        .into_iter()
        .map(|(id, name, kind)| {
            json!({
                "Name": name,
                "Locations": [],
                "CollectionType": if kind == "movies" { "movies" } else { "tvshows" },
                "ItemId": guid(&state, NS_LIBRARY, id),
                "RefreshStatus": "Idle",
            })
        })
        .collect();
    Ok(Json(json!(out)))
}

async fn views_list(state: &Shared, user: &AuthUser) -> ApiResult<Json<Value>> {
    let libs: Vec<(i64, String, String)> =
        sqlx::query_as(&format!("SELECT id, name, kind FROM libraries i WHERE 1 {} ORDER BY name", user.library_filter("i.id")))
            .fetch_all(&state.db.read)
            .await?;
    let mut items = Vec::new();
    for (id, name, kind) in libs {
        items.push(library_dto(state, id, &name, &kind).await);
    }
    let n = items.len();
    Ok(Json(json!({ "Items": items, "TotalRecordCount": n, "StartIndex": 0 })))
}

async fn views(State(state): State<Shared>, user: AuthUser, Path(_u): Path<String>) -> ApiResult<Json<Value>> {
    views_list(&state, &user).await
}

async fn views_q(State(state): State<Shared>, user: AuthUser) -> ApiResult<Json<Value>> {
    views_list(&state, &user).await
}

#[derive(sqlx::FromRow)]
struct Row {
    id: i64,
    library_id: i64,
    kind: String,
    parent_id: Option<i64>,
    series_id: Option<i64>,
    title: String,
    sort_title: String,
    original_title: Option<String>,
    year: Option<i64>,
    index_number: Option<i64>,
    index_end: Option<i64>,
    parent_index_number: Option<i64>,
    overview: Option<String>,
    runtime_ms: Option<i64>,
    community_rating: Option<f64>,
    official_rating: Option<String>,
    premiere_date: Option<String>,
    end_date: Option<String>,
    status: Option<String>,
    added_at: i64,
    series_title: Option<String>,
    season_title: Option<String>,
    poster_id: Option<i64>,
    backdrop_id: Option<i64>,
    still_id: Option<i64>,
    logo_id: Option<i64>,
    series_poster_id: Option<i64>,
    series_backdrop_id: Option<i64>,
    position_ms: Option<i64>,
    played: Option<i64>,
    play_count: Option<i64>,
    favorite: Option<i64>,
    last_played_at: Option<i64>,
    child_count: Option<i64>,
    unplayed_count: Option<i64>,
}

const SELECT: &str =
    "SELECT i.id, i.library_id, i.kind, i.parent_id, i.series_id, i.title, i.sort_title, i.original_title, i.year, i.index_number, i.index_end,
  i.parent_index_number, i.overview, i.runtime_ms, i.community_rating, i.official_rating, i.premiere_date, i.end_date, i.status, i.added_at,
  (SELECT title FROM items s WHERE s.id = i.series_id) AS series_title,
  (SELECT title FROM items s WHERE s.id = i.parent_id AND s.kind = 'season') AS season_title,
  COALESCE((SELECT id FROM images im WHERE im.item_id = i.id AND im.kind = 'poster'),
    CASE WHEN i.kind = 'collection' THEN (SELECT im.id FROM collection_members cm JOIN items m ON m.id = cm.item_id
      JOIN images im ON im.item_id = m.id AND im.kind = 'poster' WHERE cm.collection_id = i.id ORDER BY m.year, m.id LIMIT 1) END) AS poster_id,
  (SELECT id FROM images im WHERE im.item_id = i.id AND im.kind = 'backdrop') AS backdrop_id,
  (SELECT id FROM images im WHERE im.item_id = i.id AND im.kind = 'still') AS still_id,
  (SELECT id FROM images im WHERE im.item_id = COALESCE(i.series_id, i.id) AND im.kind = 'logo') AS logo_id,
  (SELECT id FROM images im WHERE im.item_id = i.series_id AND im.kind = 'poster') AS series_poster_id,
  (SELECT id FROM images im WHERE im.item_id = i.series_id AND im.kind = 'backdrop') AS series_backdrop_id,
  u.position_ms, u.played, u.play_count, u.favorite, u.last_played_at,
  CASE WHEN i.kind IN ('series','season') THEN (SELECT COUNT(*) FROM items c WHERE c.parent_id = i.id)
       WHEN i.kind = 'collection' THEN (SELECT COUNT(*) FROM collection_members cm WHERE cm.collection_id = i.id) END AS child_count,
  CASE WHEN i.kind IN ('series','season') THEN (SELECT COUNT(*) FROM items e LEFT JOIN user_item_data ue ON ue.item_id = e.id AND ue.user_id = ?1
     WHERE e.kind = 'episode' AND (e.series_id = i.id OR e.parent_id = i.id) AND COALESCE(ue.played, 0) = 0) END AS unplayed_count
FROM items i LEFT JOIN user_item_data u ON u.item_id = i.id AND u.user_id = ?1";

/// Collezioni esposte: almeno due film presenti, come nella web UI.
const BOXSET_MIN: &str = "(SELECT COUNT(*) FROM collection_members cm WHERE cm.collection_id = i.id) >= 2";

fn jf_type(kind: &str) -> &'static str {
    match kind {
        "movie" => "Movie",
        "series" => "Series",
        "season" => "Season",
        "episode" => "Episode",
        "collection" => "BoxSet",
        _ => "Folder",
    }
}

fn date_iso(d: &Option<String>) -> Option<String> {
    d.as_ref().filter(|x| x.len() >= 10).map(|x| format!("{}T00:00:00.0000000Z", &x[..10]))
}

fn dto(state: &Shared, r: &Row) -> Value {
    let id = item_guid(state, r.id);
    let mut tags = serde_json::Map::new();
    let primary = match r.kind.as_str() {
        "episode" => r.still_id.or(r.poster_id),
        _ => r.poster_id,
    };
    if let Some(p) = primary {
        tags.insert("Primary".into(), json!(state.ids.encode(p)));
    }
    if let Some(l) = r.logo_id {
        tags.insert("Logo".into(), json!(state.ids.encode(l)));
    }
    if let Some(t) = r.backdrop_id.or(r.still_id) {
        tags.insert("Thumb".into(), json!(state.ids.encode(t)));
    }
    let backdrops: Vec<String> = r.backdrop_id.map(|b| vec![state.ids.encode(b)]).unwrap_or_default();
    let played = r.played.unwrap_or(0) != 0;
    let mut v = json!({
        "Name": r.title,
        "OriginalTitle": r.original_title,
        "ServerId": state.server_id,
        "Id": id,
        "Etag": format!("{}", r.added_at),
        "SortName": r.sort_title,
        "DateCreated": iso(r.added_at),
        "PremiereDate": date_iso(&r.premiere_date),
        "EndDate": date_iso(&r.end_date),
        "ProductionYear": r.year,
        "Overview": r.overview,
        "CommunityRating": r.community_rating,
        "OfficialRating": r.official_rating,
        "RunTimeTicks": ticks(r.runtime_ms),
        "Type": jf_type(&r.kind),
        "IsFolder": r.kind == "series" || r.kind == "season" || r.kind == "collection",
        "ParentId": r.parent_id.map(|p| item_guid(state, p)).unwrap_or_else(|| guid(state, NS_LIBRARY, r.library_id)),
        "MediaType": if r.kind == "movie" || r.kind == "episode" { "Video" } else { "Unknown" },
        "LocationType": "FileSystem",
        "ImageTags": tags,
        "BackdropImageTags": backdrops,
        "Status": r.status,
        "UserData": {
            "PlaybackPositionTicks": r.position_ms.unwrap_or(0) * TICKS_PER_MS,
            "PlayCount": r.play_count.unwrap_or(0),
            "IsFavorite": r.favorite.unwrap_or(0) != 0,
            "Played": played,
            "LastPlayedDate": r.last_played_at.map(iso),
            "UnplayedItemCount": r.unplayed_count,
            "PlayedPercentage": match (r.position_ms, r.runtime_ms) { (Some(p), Some(t)) if t > 0 && p > 0 => Some((p as f64 / t as f64) * 100.0), _ => None },
            "Key": id,
            "ItemId": id,
        },
    });
    let o = v.as_object_mut().unwrap();
    if r.kind == "series" || r.kind == "season" || r.kind == "collection" {
        o.insert("ChildCount".into(), json!(r.child_count));
    }
    if r.kind == "episode" || r.kind == "season" {
        o.insert("IndexNumber".into(), json!(r.index_number));
        o.insert("SeriesName".into(), json!(r.series_title));
        o.insert("SeriesId".into(), json!(r.series_id.map(|s| item_guid(state, s))));
        if let Some(p) = r.series_poster_id {
            o.insert("SeriesPrimaryImageTag".into(), json!(state.ids.encode(p)));
        }
        if let Some(b) = r.series_backdrop_id {
            o.insert("ParentBackdropItemId".into(), json!(r.series_id.map(|s| item_guid(state, s))));
            o.insert("ParentBackdropImageTags".into(), json!([state.ids.encode(b)]));
        }
    }
    if r.kind == "episode" {
        o.insert("ParentIndexNumber".into(), json!(r.parent_index_number));
        o.insert("IndexNumberEnd".into(), json!(r.index_end));
        o.insert("SeasonId".into(), json!(r.parent_id.map(|s| item_guid(state, s))));
        o.insert("SeasonName".into(), json!(r.season_title));
    }
    if r.kind == "movie" || r.kind == "episode" {
        o.insert("VideoType".into(), json!("VideoFile"));
    }
    v
}

#[derive(sqlx::FromRow)]
struct StreamRow {
    stream_index: i64,
    kind: String,
    codec: String,
    profile: Option<String>,
    level: Option<i64>,
    bit_depth: Option<i64>,
    width: Option<i64>,
    height: Option<i64>,
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

/// Sorgenti multimediali per film ed episodi: Infuse usa la prima in direct play.
async fn media_sources(state: &Shared, item: i64) -> ApiResult<Vec<Value>> {
    let files: Vec<(i64, String, Option<String>, i64, Option<i64>, Option<i64>, Option<String>)> = sqlx::query_as(
        "SELECT id, relative_path, container, size, duration_ms, bitrate, version_label FROM media_files WHERE item_id = ? AND state = 'present' ORDER BY size DESC",
    )
    .bind(item)
    .fetch_all(&state.db.read)
    .await?;
    let mut out = Vec::new();
    for (fid, rel, container, size, dur, br, label) in files {
        let streams: Vec<StreamRow> = sqlx::query_as(
            "SELECT stream_index, kind, codec, profile, level, bit_depth, width, height, hdr, channels, channel_layout, bitrate, language, title, is_default, is_forced, is_external
             FROM media_streams WHERE file_id = ? AND kind IN ('video','audio','subtitle') ORDER BY stream_index",
        )
        .bind(fid)
        .fetch_all(&state.db.read)
        .await?;
        let name = rel.rsplit('/').next().unwrap_or(&rel).to_string();
        let ext = platea_naming::extension(&rel).unwrap_or_default();
        let ms: Vec<Value> = streams
            .into_iter()
            .map(|st| {
                let StreamRow {
                    stream_index: idx,
                    kind,
                    codec,
                    profile,
                    level,
                    bit_depth: bd,
                    width: w,
                    height: h,
                    hdr,
                    channels: ch,
                    channel_layout: layout,
                    bitrate: sbr,
                    language: lang,
                    title,
                    is_default: def,
                    is_forced: forced,
                    is_external: ext_flag,
                } = st;
                let typ = match kind.as_str() {
                    "video" => "Video",
                    "audio" => "Audio",
                    _ => "Subtitle",
                };
                let display =
                    [lang.clone().map(|l| l.to_uppercase()), Some(codec.to_uppercase()), title.clone()].into_iter().flatten().collect::<Vec<_>>().join(" - ");
                let mut s = json!({
                    "Type": typ,
                    "Index": idx,
                    "Codec": codec,
                    "Language": lang,
                    "Title": title,
                    "DisplayTitle": display,
                    "IsDefault": def != 0,
                    "IsForced": forced != 0,
                    "IsExternal": ext_flag != 0,
                    "BitRate": sbr,
                    "IsTextSubtitleStream": kind == "subtitle" && !platea_playback::is_image_subtitle(&codec),
                    "SupportsExternalStream": kind == "subtitle",
                });
                let o = s.as_object_mut().unwrap();
                if kind == "video" {
                    o.insert("Profile".into(), json!(profile));
                    o.insert("Level".into(), json!(level));
                    o.insert("BitDepth".into(), json!(bd));
                    o.insert("Width".into(), json!(w));
                    o.insert("Height".into(), json!(h));
                    o.insert("VideoRange".into(), json!(if hdr.is_some() { "HDR" } else { "SDR" }));
                    o.insert(
                        "VideoRangeType".into(),
                        json!(match hdr.as_deref() {
                            Some("dovi") => "DOVI",
                            Some("hlg") => "HLG",
                            Some("hdr10plus") => "HDR10Plus",
                            Some(_) => "HDR10",
                            None => "SDR",
                        }),
                    );
                }
                if kind == "audio" {
                    o.insert("Channels".into(), json!(ch));
                    o.insert("ChannelLayout".into(), json!(layout));
                }
                if kind == "subtitle" && ext_flag != 0 {
                    let fmt = if codec == "subrip" {
                        "srt"
                    } else if codec == "webvtt" {
                        "vtt"
                    } else {
                        codec.as_str()
                    };
                    o.insert("DeliveryMethod".into(), json!("External"));
                    o.insert(
                        "DeliveryUrl".into(),
                        json!(format!("/Videos/{}/{}/Subtitles/{}/Stream.{}", item_guid(state, item), state.ids.encode(fid), idx, fmt)),
                    );
                }
                s
            })
            .collect();
        out.push(json!({
            "Protocol": "File",
            "Id": state.ids.encode(fid),
            "Path": name,
            "Type": "Default",
            "Container": container.clone().unwrap_or(ext.clone()),
            "Size": size,
            "Name": label.unwrap_or_else(|| name.clone()),
            "IsRemote": false,
            "ETag": format!("{size:x}"),
            "RunTimeTicks": ticks(dur),
            "Bitrate": br,
            "SupportsTranscoding": false,
            "SupportsDirectStream": true,
            "SupportsDirectPlay": true,
            "IsInfiniteStream": false,
            "RequiresOpening": false,
            "RequiresClosing": false,
            "RequiresLooping": false,
            "SupportsProbing": false,
            "MediaStreams": ms,
            "DirectStreamUrl": format!("/Videos/{}/stream.{}?static=true&MediaSourceId={}", item_guid(state, item), container.unwrap_or(ext), state.ids.encode(fid)),
        }));
    }
    Ok(out)
}

async fn detail_dto(state: &Shared, user: &AuthUser, id: i64) -> ApiResult<Value> {
    let sql = format!("{SELECT} WHERE i.id = ?2");
    let r: Row = sqlx::query_as(&sql).bind(user.user_id).bind(id).fetch_optional(&state.db.read).await?.ok_or_else(|| ApiError::not_found("Elemento"))?;
    if !user.can_see_item(state, id, r.library_id).await? {
        return Err(ApiError::not_found("Elemento"));
    }
    let mut v = dto(state, &r);
    let meta_id = if r.kind == "season" || r.kind == "episode" { r.series_id.unwrap_or(id) } else { id };
    let genres: Vec<String> = sqlx::query_scalar("SELECT g.name FROM item_genres ig JOIN genres g ON g.id = ig.genre_id WHERE ig.item_id = ?")
        .bind(meta_id)
        .fetch_all(&state.db.read)
        .await?;
    let studios: Vec<String> = sqlx::query_scalar("SELECT s.name FROM item_studios x JOIN studios s ON s.id = x.studio_id WHERE x.item_id = ?")
        .bind(meta_id)
        .fetch_all(&state.db.read)
        .await?;
    let people: Vec<(i64, String, String, Option<String>, Option<i64>)> = sqlx::query_as(
        "SELECT p.id, p.name, ip.role, ip.character, (SELECT id FROM images im WHERE im.person_id = p.id AND im.kind = 'profile')
         FROM item_people ip JOIN people p ON p.id = ip.person_id WHERE ip.item_id = ? ORDER BY ip.sort_order LIMIT 30",
    )
    .bind(meta_id)
    .fetch_all(&state.db.read)
    .await?;
    let ext: Vec<(String, String)> = sqlx::query_as("SELECT provider, value FROM external_ids WHERE item_id = ?").bind(id).fetch_all(&state.db.read).await?;
    let tagline: Option<String> = sqlx::query_scalar("SELECT tagline FROM items WHERE id = ?").bind(id).fetch_one(&state.db.read).await?;
    let o = v.as_object_mut().unwrap();
    o.insert("Genres".into(), json!(genres));
    o.insert("Studios".into(), json!(studios.iter().map(|s| json!({ "Name": s })).collect::<Vec<_>>()));
    o.insert("Taglines".into(), json!(tagline.into_iter().collect::<Vec<_>>()));
    o.insert(
        "People".into(),
        json!(people
            .into_iter()
            .map(|(pid, name, role, character, img)| json!({
                "Name": name,
                "Id": guid(state, NS_PERSON, pid),
                "Role": character.unwrap_or_default(),
                "Type": match role.as_str() { "actor" => "Actor", "director" => "Director", "writer" => "Writer", _ => "Producer" },
                "PrimaryImageTag": img.map(|i| state.ids.encode(i)),
            }))
            .collect::<Vec<_>>()),
    );
    let mut provider = serde_json::Map::new();
    for (p, val) in ext {
        let key = match p.as_str() {
            "tmdb" => "Tmdb",
            "imdb" => "Imdb",
            "tvdb" => "Tvdb",
            other => other,
        };
        provider.insert(key.to_string(), json!(val));
    }
    o.insert("ProviderIds".into(), Value::Object(provider));
    if r.kind == "movie" || r.kind == "episode" {
        let sources = media_sources(state, id).await?;
        if let Some(first) = sources.first() {
            o.insert("Container".into(), first["Container"].clone());
            o.insert("Path".into(), first["Path"].clone());
            o.insert("MediaStreams".into(), first["MediaStreams"].clone());
        }
        o.insert("MediaSources".into(), json!(sources));
    }
    Ok(v)
}

async fn item_u(State(state): State<Shared>, user: AuthUser, Path((_u, item)): Path<(String, String)>) -> ApiResult<Json<Value>> {
    item_detail(&state, &user, &item).await
}

async fn item_q(State(state): State<Shared>, user: AuthUser, Path(item): Path<String>) -> ApiResult<Json<Value>> {
    item_detail(&state, &user, &item).await
}

async fn item_detail(state: &Shared, user: &AuthUser, item: &str) -> ApiResult<Json<Value>> {
    match parse_ref(state, item) {
        Some(Ref::Item(i)) => Ok(Json(detail_dto(state, user, i).await?)),
        Some(Ref::Library(l)) => {
            let row: (String, String) = sqlx::query_as("SELECT name, kind FROM libraries WHERE id = ?")
                .bind(l)
                .fetch_optional(&state.db.read)
                .await?
                .ok_or_else(|| ApiError::not_found("Libreria"))?;
            Ok(Json(library_dto(state, l, &row.0, &row.1).await))
        }
        _ => Err(ApiError::not_found("Elemento")),
    }
}

#[derive(Deserialize, Default, Debug)]
#[serde(default, rename_all = "PascalCase")]
struct ItemsQuery {
    parent_id: Option<String>,
    include_item_types: Option<String>,
    exclude_item_types: Option<String>,
    recursive: Option<bool>,
    sort_by: Option<String>,
    sort_order: Option<String>,
    start_index: Option<i64>,
    limit: Option<i64>,
    filters: Option<String>,
    search_term: Option<String>,
    ids: Option<String>,
    is_favorite: Option<bool>,
    is_played: Option<bool>,
    genres: Option<String>,
    years: Option<String>,
    name_starts_with: Option<String>,
    #[serde(alias = "userId", alias = "UserId")]
    user_id: Option<String>,
    #[serde(alias = "seasonId")]
    season_id: Option<String>,
    #[serde(alias = "searchTerm")]
    search_term_lc: Option<String>,
}

/// Parametri della query: Jellyfin accetta sia PascalCase sia camelCase.
fn normalize_query(uri: &Uri) -> ItemsQuery {
    let mut map: HashMap<String, String> = HashMap::new();
    if let Some(q) = uri.query() {
        for pair in q.split('&') {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            let v = urlencoding::decode(&v.replace('+', " ")).map(|s| s.into_owned()).unwrap_or_default();
            map.insert(k.to_lowercase(), v);
        }
    }
    let s = |k: &str| map.get(k).cloned().filter(|v| !v.is_empty());
    let b = |k: &str| map.get(k).map(|v| v.eq_ignore_ascii_case("true"));
    let n = |k: &str| map.get(k).and_then(|v| v.parse::<i64>().ok());
    ItemsQuery {
        parent_id: s("parentid"),
        include_item_types: s("includeitemtypes"),
        exclude_item_types: s("excludeitemtypes"),
        recursive: b("recursive"),
        sort_by: s("sortby"),
        sort_order: s("sortorder"),
        start_index: n("startindex"),
        limit: n("limit"),
        filters: s("filters"),
        search_term: s("searchterm"),
        ids: s("ids"),
        is_favorite: b("isfavorite"),
        is_played: b("isplayed"),
        genres: s("genres"),
        years: s("years"),
        name_starts_with: s("namestartswith"),
        user_id: s("userid"),
        season_id: s("seasonid"),
        search_term_lc: None,
    }
}

async fn query_items(state: &Shared, user: &AuthUser, q: ItemsQuery) -> ApiResult<Json<Value>> {
    let mut sql = format!("{SELECT} WHERE 1 {}", user.library_filter("i.library_id"));
    let mut binds: Vec<Value> = vec![json!(user.user_id)];
    // Tutti i "?" di un frammento usano lo stesso valore.
    let mut push = |sql: &mut String, frag: &str, v: Value| {
        let n = binds.len() + 1;
        sql.push_str(&frag.replace('?', &format!("?{n}")));
        binds.push(v);
    };
    // Tipi richiesti.
    let types: Vec<&str> = q
        .include_item_types
        .as_deref()
        .map(|t| {
            t.split(',')
                .filter_map(|x| match x.trim().to_lowercase().as_str() {
                    "movie" => Some("movie"),
                    "series" => Some("series"),
                    "season" => Some("season"),
                    "episode" => Some("episode"),
                    "boxset" => Some("collection"),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    // Solo tipi che Platea non ha (Playlist, Audio, Photo…): nessun risultato, non "tutto".
    if q.include_item_types.as_deref().is_some_and(|t| !t.trim().is_empty()) && types.is_empty() {
        return Ok(Json(json!({ "Items": [], "TotalRecordCount": 0, "StartIndex": 0 })));
    }
    let excluded: Vec<String> = q.exclude_item_types.as_deref().map(|t| t.split(',').map(|x| x.trim().to_lowercase()).collect()).unwrap_or_default();
    // Ids espliciti.
    if let Some(ids) = &q.ids {
        let list: Vec<String> = ids
            .split(',')
            .filter_map(|x| match parse_ref(state, x.trim()) {
                Some(Ref::Item(i)) => Some(i.to_string()),
                _ => None,
            })
            .collect();
        if list.is_empty() {
            return Ok(Json(json!({ "Items": [], "TotalRecordCount": 0, "StartIndex": 0 })));
        }
        sql.push_str(&format!(" AND i.id IN ({})", list.join(",")));
    } else {
        let parent = q.parent_id.as_deref().and_then(|p| parse_ref(state, p));
        let recursive = q.recursive.unwrap_or(false) || q.search_term.is_some();
        match parent {
            Some(Ref::Library(l)) => {
                push(&mut sql, " AND i.library_id = ?", json!(l));
                if !recursive && types.is_empty() {
                    sql.push_str(" AND i.kind IN ('movie','series')");
                }
            }
            Some(Ref::Item(p))
                if sqlx::query_scalar::<_, String>("SELECT kind FROM items WHERE id = ?").bind(p).fetch_optional(&state.db.read).await?.as_deref()
                    == Some("collection") =>
            {
                // I film di una collezione (BoxSet), in ordine di uscita se il client non chiede altro.
                push(&mut sql, " AND i.id IN (SELECT item_id FROM collection_members WHERE collection_id = ?)", json!(p));
            }
            Some(Ref::Item(p)) => {
                if recursive {
                    push(&mut sql, " AND (i.parent_id = ? OR i.series_id = ?)", json!(p));
                } else {
                    push(&mut sql, " AND i.parent_id = ?", json!(p));
                }
            }
            _ => {
                if !recursive && types.is_empty() {
                    sql.push_str(" AND i.kind IN ('movie','series')");
                }
            }
        }
    }
    if !types.is_empty() {
        sql.push_str(&format!(" AND i.kind IN ({})", types.iter().map(|t| format!("'{t}'")).collect::<Vec<_>>().join(",")));
        if types.contains(&"collection") {
            sql.push_str(&format!(" AND (i.kind <> 'collection' OR {BOXSET_MIN})"));
        }
    } else if q.ids.is_none() {
        sql.push_str(" AND i.kind IN ('movie','series','season','episode')");
    }
    for ex in &excluded {
        match ex.as_str() {
            "movie" | "series" | "season" | "episode" => sql.push_str(&format!(" AND i.kind <> '{ex}'")),
            _ => {}
        }
    }
    if let Some(term) = q.search_term.as_deref() {
        if let Some(m) = crate::api::items::fts_query(term) {
            push(&mut sql, " AND i.id IN (SELECT rowid FROM items_fts WHERE items_fts MATCH ?)", json!(m));
        }
    }
    let filters = q.filters.clone().unwrap_or_default().to_lowercase();
    if filters.contains("isplayed") || q.is_played == Some(true) {
        sql.push_str(" AND COALESCE(u.played,0) = 1");
    }
    if filters.contains("isunplayed") || q.is_played == Some(false) {
        sql.push_str(" AND COALESCE(u.played,0) = 0");
    }
    if filters.contains("isfavorite") || q.is_favorite == Some(true) {
        sql.push_str(" AND COALESCE(u.favorite,0) = 1");
    }
    if filters.contains("isresumable") {
        sql.push_str(" AND COALESCE(u.position_ms,0) > 0 AND COALESCE(u.played,0) = 0");
    }
    if let Some(g) = &q.genres {
        let first = g.split('|').next().unwrap_or(g).to_string();
        push(&mut sql, " AND EXISTS (SELECT 1 FROM item_genres ig JOIN genres g ON g.id = ig.genre_id WHERE ig.item_id = i.id AND g.name = ?)", json!(first));
    }
    if let Some(y) = &q.years {
        let years: Vec<String> = y.split(',').filter_map(|x| x.trim().parse::<i64>().ok()).map(|x| x.to_string()).collect();
        if !years.is_empty() {
            sql.push_str(&format!(" AND i.year IN ({})", years.join(",")));
        }
    }
    if let Some(l) = &q.name_starts_with {
        push(&mut sql, " AND i.sort_title LIKE ?", json!(format!("{}%", l.to_lowercase())));
    }
    let count_sql = format!("SELECT COUNT(*) FROM ({sql})");
    // Un elenco di espressioni: dividerle sulle virgole spezzerebbe COALESCE(x, '').
    let order: &[&str] = match q.sort_by.as_deref().unwrap_or("SortName").split(',').next().unwrap_or("SortName").trim().to_lowercase().as_str() {
        "datecreated" => &["i.added_at"],
        "premieredate" => &["COALESCE(i.premiere_date, '')"],
        "productionyear" => &["COALESCE(i.year, 0)"],
        "communityrating" | "criticrating" => &["COALESCE(i.community_rating, 0)"],
        "dateplayed" | "datelastcontentadded" => &["COALESCE(u.last_played_at, i.added_at)"],
        "random" => &["RANDOM()"],
        "runtime" => &["COALESCE(i.runtime_ms, 0)"],
        "indexnumber" | "parentindexnumber" | "airedepisodeorder" => &["COALESCE(i.parent_index_number, 0)", "COALESCE(i.index_number, 0)"],
        _ => {
            if q.parent_id.as_deref().and_then(|p| parse_ref(state, p)).is_some_and(|r| matches!(r, Ref::Item(_))) {
                &["COALESCE(i.parent_index_number, 0)", "COALESCE(i.index_number, 0)", "i.sort_title"]
            } else {
                &["i.sort_title"]
            }
        }
    };
    let dir = if q.sort_order.as_deref().is_some_and(|o| o.eq_ignore_ascii_case("descending")) { "DESC" } else { "ASC" };
    let start = q.start_index.unwrap_or(0).max(0);
    let limit = q.limit.unwrap_or(200).clamp(1, 500);
    let order_sql = order.iter().map(|o| format!("{o} {dir}")).collect::<Vec<_>>().join(", ");
    sql.push_str(&format!(" ORDER BY {order_sql}, i.id LIMIT {limit} OFFSET {start}"));

    let mut qr = sqlx::query_as::<_, Row>(&sql);
    for b in &binds {
        qr = match b {
            Value::Number(n) => qr.bind(n.as_i64()),
            Value::String(s) => qr.bind(s.clone()),
            _ => qr.bind(Option::<i64>::None),
        };
    }
    let rows = qr.fetch_all(&state.db.read).await?;
    let mut cq = sqlx::query_scalar::<_, i64>(&count_sql);
    for b in &binds {
        cq = match b {
            Value::Number(n) => cq.bind(n.as_i64()),
            Value::String(s) => cq.bind(s.clone()),
            _ => cq.bind(Option::<i64>::None),
        };
    }
    let total = cq.fetch_one(&state.db.read).await?;
    let items: Vec<Value> = rows.iter().map(|r| dto(state, r)).collect();
    Ok(Json(json!({ "Items": items, "TotalRecordCount": total, "StartIndex": start })))
}

async fn items_u(State(state): State<Shared>, user: AuthUser, Path(_u): Path<String>, uri: Uri) -> ApiResult<Json<Value>> {
    query_items(&state, &user, normalize_query(&uri)).await
}

async fn items_q(State(state): State<Shared>, user: AuthUser, uri: Uri) -> ApiResult<Json<Value>> {
    query_items(&state, &user, normalize_query(&uri)).await
}

async fn resume_list(state: &Shared, user: &AuthUser, uri: &Uri) -> ApiResult<Json<Value>> {
    let q = normalize_query(uri);
    let limit = q.limit.unwrap_or(20).clamp(1, 100);
    let sql = format!(
        "{SELECT} WHERE i.kind IN ('movie','episode') AND COALESCE(u.position_ms,0) > 0 AND COALESCE(u.played,0) = 0 {} ORDER BY u.last_played_at DESC LIMIT {limit}",
        user.library_filter("i.library_id")
    );
    let rows: Vec<Row> = sqlx::query_as(&sql).bind(user.user_id).fetch_all(&state.db.read).await?;
    let n = rows.len();
    Ok(Json(json!({ "Items": rows.iter().map(|r| dto(state, r)).collect::<Vec<_>>(), "TotalRecordCount": n, "StartIndex": 0 })))
}

async fn resume_u(State(state): State<Shared>, user: AuthUser, Path(_u): Path<String>, uri: Uri) -> ApiResult<Json<Value>> {
    resume_list(&state, &user, &uri).await
}

async fn resume_q(State(state): State<Shared>, user: AuthUser, uri: Uri) -> ApiResult<Json<Value>> {
    resume_list(&state, &user, &uri).await
}

async fn latest_list(state: &Shared, user: &AuthUser, uri: &Uri) -> ApiResult<Json<Value>> {
    let q = normalize_query(uri);
    let limit = q.limit.unwrap_or(16).clamp(1, 100);
    let mut sql = format!("{SELECT} WHERE i.kind IN ('movie','episode') AND i.available = 1 {}", user.library_filter("i.library_id"));
    if let Some(Ref::Library(l)) = q.parent_id.as_deref().and_then(|p| parse_ref(state, p)) {
        sql.push_str(&format!(" AND i.library_id = {l}"));
    }
    sql.push_str(&format!(" ORDER BY i.added_at DESC LIMIT {limit}"));
    let rows: Vec<Row> = sqlx::query_as(&sql).bind(user.user_id).fetch_all(&state.db.read).await?;
    // Latest restituisce un array semplice.
    Ok(Json(json!(rows.iter().map(|r| dto(state, r)).collect::<Vec<_>>())))
}

async fn latest_u(State(state): State<Shared>, user: AuthUser, Path(_u): Path<String>, uri: Uri) -> ApiResult<Json<Value>> {
    latest_list(&state, &user, &uri).await
}

async fn latest_q(State(state): State<Shared>, user: AuthUser, uri: Uri) -> ApiResult<Json<Value>> {
    latest_list(&state, &user, &uri).await
}

async fn next_up(State(state): State<Shared>, user: AuthUser) -> ApiResult<Json<Value>> {
    let api_user = user.clone();
    let series: Vec<i64> = sqlx::query_scalar(&format!(
        "SELECT i.id FROM items i JOIN user_item_data u ON u.item_id = i.id AND u.user_id = ?1 WHERE i.kind = 'series' AND u.last_played_at IS NOT NULL {} ORDER BY u.last_played_at DESC LIMIT 20",
        user.library_filter("i.library_id")
    ))
    .bind(user.user_id)
    .fetch_all(&state.db.read)
    .await?;
    let mut out = Vec::new();
    for sid in series {
        let last: Option<i64> = sqlx::query_scalar(
            "SELECT e.id FROM items e JOIN user_item_data u ON u.item_id = e.id AND u.user_id = ?1 WHERE e.series_id = ?2 AND e.kind = 'episode' AND u.played = 1 AND COALESCE(e.parent_index_number,0) > 0
             ORDER BY e.parent_index_number DESC, e.index_number DESC LIMIT 1",
        )
        .bind(user.user_id)
        .bind(sid)
        .fetch_optional(&state.db.read)
        .await?;
        if let Some(l) = last {
            if let Some(n) = crate::api::items::next_after(&state, &api_user, l).await? {
                if n.played.unwrap_or(0) == 0 {
                    let sql = format!("{SELECT} WHERE i.id = ?2");
                    if let Some(r) = sqlx::query_as::<_, Row>(&sql).bind(user.user_id).bind(n.id).fetch_optional(&state.db.read).await? {
                        out.push(dto(&state, &r));
                    }
                }
            }
        }
    }
    let n = out.len();
    Ok(Json(json!({ "Items": out, "TotalRecordCount": n, "StartIndex": 0 })))
}

async fn seasons(State(state): State<Shared>, user: AuthUser, Path(series): Path<String>) -> ApiResult<Json<Value>> {
    let sid = item_id(&state, &series)?;
    let sql = format!("{SELECT} WHERE i.parent_id = ?2 AND i.kind = 'season' {} ORDER BY COALESCE(i.index_number, 0)", user.library_filter("i.library_id"));
    let rows: Vec<Row> = sqlx::query_as(&sql).bind(user.user_id).bind(sid).fetch_all(&state.db.read).await?;
    let n = rows.len();
    Ok(Json(json!({ "Items": rows.iter().map(|r| dto(&state, r)).collect::<Vec<_>>(), "TotalRecordCount": n, "StartIndex": 0 })))
}

async fn episodes(State(state): State<Shared>, user: AuthUser, Path(series): Path<String>, uri: Uri) -> ApiResult<Json<Value>> {
    let sid = item_id(&state, &series)?;
    let q = normalize_query(&uri);
    let mut sql = format!("{SELECT} WHERE i.series_id = ?2 AND i.kind = 'episode' {}", user.library_filter("i.library_id"));
    if let Some(Ref::Item(season)) = q.season_id.as_deref().and_then(|s| parse_ref(&state, s)) {
        sql.push_str(&format!(" AND i.parent_id = {season}"));
    }
    sql.push_str(" ORDER BY COALESCE(i.parent_index_number, 0), COALESCE(i.index_number, 0)");
    let rows: Vec<Row> = sqlx::query_as(&sql).bind(user.user_id).bind(sid).fetch_all(&state.db.read).await?;
    let n = rows.len();
    Ok(Json(json!({ "Items": rows.iter().map(|r| dto(&state, r)).collect::<Vec<_>>(), "TotalRecordCount": n, "StartIndex": 0 })))
}

async fn search_hints(State(state): State<Shared>, user: AuthUser, uri: Uri) -> ApiResult<Json<Value>> {
    let q = normalize_query(&uri);
    let Some(m) = q.search_term.as_deref().and_then(crate::api::items::fts_query) else {
        return Ok(Json(json!({ "SearchHints": [], "TotalRecordCount": 0 })));
    };
    let limit = q.limit.unwrap_or(30).clamp(1, 100);
    let sql = format!(
        "{SELECT} WHERE i.id IN (SELECT rowid FROM items_fts WHERE items_fts MATCH ?2) AND i.kind IN ('movie','series','episode') {} LIMIT {limit}",
        user.library_filter("i.library_id")
    );
    let rows: Vec<Row> = sqlx::query_as(&sql).bind(user.user_id).bind(m).fetch_all(&state.db.read).await?;
    let hints: Vec<Value> = rows
        .iter()
        .map(|r| {
            let d = dto(&state, r);
            json!({
                "ItemId": d["Id"], "Id": d["Id"], "Name": r.title, "Type": jf_type(&r.kind), "ProductionYear": r.year,
                "PrimaryImageTag": d["ImageTags"]["Primary"], "RunTimeTicks": d["RunTimeTicks"], "MediaType": d["MediaType"],
                "Series": r.series_title, "IndexNumber": r.index_number, "ParentIndexNumber": r.parent_index_number,
            })
        })
        .collect();
    let n = hints.len();
    Ok(Json(json!({ "SearchHints": hints, "TotalRecordCount": n })))
}

// ---------------------------------------------------------------------------------------------
// Immagini

#[derive(Deserialize, Default)]
#[serde(default)]
struct ImgQuery {
    #[serde(alias = "maxWidth", alias = "MaxWidth", alias = "fillWidth", alias = "FillWidth", alias = "width", alias = "Width")]
    w: Option<u32>,
}

async fn image_idx(
    State(state): State<Shared>,
    Path((item, kind, _idx)): Path<(String, String, String)>,
    Query(q): Query<ImgQuery>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    serve_image(&state, &item, &kind, q.w, &headers).await
}

async fn image(State(state): State<Shared>, Path((item, kind)): Path<(String, String)>, Query(q): Query<ImgQuery>, headers: HeaderMap) -> ApiResult<Response> {
    serve_image(&state, &item, &kind, q.w, &headers).await
}

async fn serve_image(state: &Shared, item: &str, kind: &str, w: Option<u32>, _headers: &HeaderMap) -> ApiResult<Response> {
    let image_id: Option<i64> = match parse_ref(state, item) {
        Some(Ref::Item(i)) => {
            let kinds: &[&str] = match kind.to_lowercase().as_str() {
                "primary" => &["poster", "still"],
                "backdrop" | "art" => &["backdrop"],
                "thumb" => &["still", "backdrop"],
                "logo" => &["logo"],
                _ => &["poster"],
            };
            let mut found = None;
            for k in kinds {
                let id: Option<i64> = sqlx::query_scalar(
                    "SELECT im.id FROM images im JOIN items it ON it.id = ?1 WHERE im.kind = ?2 AND (im.item_id = it.id OR (im.item_id = it.series_id AND ?2 IN ('backdrop','logo'))) ORDER BY im.item_id = it.id DESC LIMIT 1",
                )
                .bind(i)
                .bind(k)
                .fetch_optional(&state.db.read)
                .await?;
                if id.is_some() {
                    found = id;
                    break;
                }
            }
            found
        }
        Some(Ref::Person(p)) => {
            sqlx::query_scalar("SELECT id FROM images WHERE person_id = ? AND kind = 'profile'").bind(p).fetch_optional(&state.db.read).await?
        }
        _ => None,
    };
    let image_id = image_id.ok_or_else(|| ApiError::not_found("Immagine"))?;
    let path = crate::metadata::images::serve_path(state, image_id, w).await.map_err(|e| ApiError::NotFound(format!("Immagine non disponibile: {e}")))?;
    let bytes = tokio::fs::read(&path).await.map_err(|e| ApiError::Internal(e.into()))?;
    let ct = image::guess_format(&bytes).ok().map(|f| f.to_mime_type()).unwrap_or("image/jpeg");
    Ok(Response::builder().header(header::CONTENT_TYPE, ct).header(header::CACHE_CONTROL, "public, max-age=31536000").body(Body::from(bytes)).unwrap())
}

// ---------------------------------------------------------------------------------------------
// Riproduzione: solo direct play. Infuse decodifica da sé quasi tutti i formati.

async fn playback_info(State(state): State<Shared>, user: AuthUser, Path(item): Path<String>) -> ApiResult<Json<Value>> {
    user.require_playback()?;
    let id = item_id(&state, &item)?;
    crate::api::items::load_item(&state, &user, id).await?;
    let sources = media_sources(&state, id).await?;
    Ok(Json(json!({ "MediaSources": sources, "PlaySessionId": crate::util::random_hex(16) })))
}

async fn open_cached(state: &Shared, file_id: i64) -> ApiResult<(Arc<std::fs::File>, crate::api::playback::FileInfo)> {
    let info = crate::api::playback::load_file(state, file_id).await?;
    let timeout = Duration::from_secs(state.settings.get().scanner.fs_timeout_seconds as u64);
    let f = crate::stream::open_library_file(file_id, &info.abs, &info.root, timeout).await?;
    Ok((f, info))
}

async fn choose_file(state: &Shared, item: i64, source: Option<&str>) -> ApiResult<i64> {
    if let Some(fid) = source.and_then(|s| state.ids.decode(s)) {
        let ok: Option<i64> =
            sqlx::query_scalar("SELECT id FROM media_files WHERE id = ? AND item_id = ?").bind(fid).bind(item).fetch_optional(&state.db.read).await?;
        if let Some(f) = ok {
            return Ok(f);
        }
    }
    sqlx::query_scalar("SELECT id FROM media_files WHERE item_id = ? AND state = 'present' ORDER BY size DESC LIMIT 1")
        .bind(item)
        .fetch_optional(&state.db.read)
        .await?
        .ok_or_else(|| ApiError::not_found("File"))
}

async fn do_stream(state: &Shared, user: &AuthUser, item: &str, uri: &Uri, method: &Method, headers: &HeaderMap) -> ApiResult<Response> {
    user.require_playback()?;
    let id = item_id(state, item)?;
    crate::api::items::load_item(state, user, id).await?;
    let source = uri.query().and_then(|q| {
        q.split('&').find_map(|p| {
            let (k, v) = p.split_once('=')?;
            k.eq_ignore_ascii_case("mediasourceid").then(|| v.to_string())
        })
    });
    let fid = choose_file(state, id, source.as_deref()).await?;
    crate::probe::ensure_probed(state, fid).await.ok();
    let (file, info) = open_cached(state, fid).await?;
    let ct = crate::stream::content_type_for(&info.media.container, &info.abs);
    // Byte inviati alla sessione di questo dispositivo, se Infuse l'ha già aperta con un rapporto.
    let sent = COMPAT_SESSIONS.lock().unwrap().get(&(user.token_id, id)).and_then(|sid| state.sessions.get(sid)).map(|s| s.bytes_sent.clone());
    crate::stream::serve_file(file, info.size as u64, info.mtime, ct, method, headers, sent)
}

async fn download(State(state): State<Shared>, user: AuthUser, Path(item): Path<String>, method: Method, headers: HeaderMap) -> ApiResult<Response> {
    let id = item_id(&state, &item)?;
    crate::api::download::serve_for_user(&state, &user, id, &method, &headers).await
}

async fn stream(State(state): State<Shared>, user: AuthUser, Path(item): Path<String>, uri: Uri, method: Method, headers: HeaderMap) -> ApiResult<Response> {
    do_stream(&state, &user, &item, &uri, &method, &headers).await
}

async fn stream_named(
    State(state): State<Shared>,
    user: AuthUser,
    Path((item, name)): Path<(String, String)>,
    uri: Uri,
    method: Method,
    headers: HeaderMap,
) -> ApiResult<Response> {
    if !name.to_lowercase().starts_with("stream") {
        return Err(ApiError::not_found("Risorsa"));
    }
    do_stream(&state, &user, &item, &uri, &method, &headers).await
}

async fn subtitle_offset(
    State(state): State<Shared>,
    user: AuthUser,
    Path((item, source, index, _offset, name)): Path<(String, String, i64, String, String)>,
) -> ApiResult<Response> {
    serve_subtitle(&state, &user, &item, &source, index, &name).await
}

async fn subtitle(State(state): State<Shared>, user: AuthUser, Path((item, source, index, name)): Path<(String, String, i64, String)>) -> ApiResult<Response> {
    serve_subtitle(&state, &user, &item, &source, index, &name).await
}

/// Sottotitoli esterni serviti così come sono (srt, ass, vtt): Infuse li disegna da sé.
async fn serve_subtitle(state: &Shared, user: &AuthUser, item: &str, source: &str, index: i64, _name: &str) -> ApiResult<Response> {
    user.require_playback()?;
    let id = item_id(state, item)?;
    crate::api::items::load_item(state, user, id).await?;
    let fid = choose_file(state, id, Some(source)).await?;
    let info = crate::api::playback::load_file(state, fid).await?;
    let ext = info.streams.iter().find(|s| s.0 == index && s.4).and_then(|s| s.5.clone()).ok_or_else(|| ApiError::not_found("Sottotitolo"))?;
    if ext.split('/').any(|p| p == "..") {
        return Err(ApiError::Forbidden("Percorso non valido".into()));
    }
    let bytes = tokio::fs::read(info.root.join(&ext)).await.map_err(|_| ApiError::not_found("Sottotitolo"))?;
    let ct = match platea_naming::extension(&ext).as_deref() {
        Some("vtt") => "text/vtt",
        Some("ass") | Some("ssa") => "text/x-ssa",
        _ => "application/x-subrip",
    };
    Ok(Response::builder().header(header::CONTENT_TYPE, ct).body(Body::from(bytes)).unwrap())
}

// ---------------------------------------------------------------------------------------------
// Rapporti di riproduzione: compaiono nella Panoramica come direct play di Infuse.

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "PascalCase")]
struct PlayReport {
    item_id: Option<String>,
    media_source_id: Option<String>,
    position_ticks: Option<i64>,
    is_paused: Option<bool>,
}

static COMPAT_SESSIONS: Lazy<Mutex<HashMap<(i64, i64), String>>> = Lazy::new(|| Mutex::new(HashMap::new()));

async fn ensure_session(state: &Shared, user: &AuthUser, item: i64, source: Option<&str>) -> ApiResult<Arc<PlaySession>> {
    let key = (user.token_id, item);
    if let Some(id) = COMPAT_SESSIONS.lock().unwrap().get(&key).cloned() {
        if let Some(s) = state.sessions.get(&id) {
            return Ok(s);
        }
    }
    for old in state.sessions.by_device(user.token_id) {
        stop_session(state, &old).await;
    }
    let row = crate::api::items::load_item(state, user, item).await?;
    let fid = choose_file(state, item, source).await?;
    let info = crate::api::playback::load_file(state, fid).await?;
    let decision = platea_playback::Decision {
        method: platea_playback::Method::DirectPlay,
        reasons: vec![],
        video: platea_playback::VideoAction::Copy,
        audio: platea_playback::AudioAction::Copy,
        audio_index: None,
        subtitle: platea_playback::SubtitleDelivery::None,
    };
    let chain = crate::api::playback::chain_text(&info.media, &decision, "");
    let history_id = sqlx::query(
        "INSERT INTO playback_history (user_id, item_id, file_id, client, device_name, method, started_at) VALUES (?, ?, ?, ?, ?, 'direct_play', ?)",
    )
    .bind(user.user_id)
    .bind(item)
    .bind(fid)
    .bind(&user.client)
    .bind(&user.device_name)
    .bind(now_ms())
    .execute(&state.db.write)
    .await?
    .last_insert_rowid();
    let (sid, token) = SessionManager::new_ids();
    let (title, subtitle) = if row.kind == "episode" {
        (
            row.series_title.clone().unwrap_or_default(),
            Some(format!("S{:02}E{:02} · {}", row.parent_index_number.unwrap_or(0), row.index_number.unwrap_or(0), row.title)),
        )
    } else {
        (row.title.clone(), row.year.map(|y| y.to_string()))
    };
    let local = state.settings.get().network.local_networks.clone();
    let extra = crate::sessions::SessionExtra {
        ip: user.ip.map(|i| i.to_string()),
        remote: user.ip.is_some_and(|i| !crate::net::in_any(&local, i)),
        details: crate::sessions::details(&info, &decision, None),
        position_ms: 0,
    };
    let bitrate = info.media.bitrate.map(|b| b as i64);
    let duration_ms = info.media.duration_ms.map(|d| d as i64).or(row.runtime_ms).unwrap_or(0);
    let s = Arc::new(PlaySession::new(
        sid.clone(),
        token,
        user,
        item,
        fid,
        title,
        subtitle,
        row.poster_id.or(row.series_poster_id),
        decision,
        chain,
        bitrate,
        duration_ms,
        history_id,
        None,
        extra,
    ));
    state.sessions.insert(s.clone());
    COMPAT_SESSIONS.lock().unwrap().insert(key, sid);
    state.events.admin("sessions.changed", json!({}));
    Ok(s)
}

async fn report(state: &Shared, user: &AuthUser, r: &PlayReport) -> ApiResult<Option<Arc<PlaySession>>> {
    if user.require_playback().is_err() {
        return Ok(None);
    }
    let Some(item) = r.item_id.as_deref().and_then(|i| match parse_ref(state, i) {
        Some(Ref::Item(x)) => Some(x),
        _ => None,
    }) else {
        return Ok(None);
    };
    let s = ensure_session(state, user, item, r.media_source_id.as_deref()).await?;
    let pos = r.position_ticks.map(|t| (t / TICKS_PER_MS).max(0));
    if let Some(pos) = pos {
        state.sessions.record_progress(user.user_id, item, pos, s.duration_ms);
    }
    s.report(pos, r.is_paused.unwrap_or(false));
    Ok(Some(s))
}

async fn playing(State(state): State<Shared>, user: AuthUser, Json(r): Json<PlayReport>) -> ApiResult<StatusCode> {
    report(&state, &user, &r).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn progress(State(state): State<Shared>, user: AuthUser, Json(r): Json<PlayReport>) -> ApiResult<StatusCode> {
    report(&state, &user, &r).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn stopped(State(state): State<Shared>, user: AuthUser, Json(r): Json<PlayReport>) -> ApiResult<StatusCode> {
    if let Some(s) = report(&state, &user, &r).await? {
        COMPAT_SESSIONS.lock().unwrap().retain(|_, v| *v != s.id);
        stop_session(&state, &s.id).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------------------------
// Stato di visione e preferiti

async fn set_flag(state: &Shared, user: &AuthUser, item: &str, field: &str, on: bool) -> ApiResult<Json<Value>> {
    let id = item_id(state, item)?;
    let row = crate::api::items::load_item(state, user, id).await?;
    let targets: Vec<i64> = if field == "played" && (row.kind == "series" || row.kind == "season") {
        sqlx::query_scalar("SELECT id FROM items WHERE kind = 'episode' AND (series_id = ?1 OR parent_id = ?1)").bind(id).fetch_all(&state.db.read).await?
    } else {
        vec![id]
    };
    let now = now_ms();
    let mut tx = state.db.write.begin().await?;
    for t in targets {
        let sql = if field == "played" {
            "INSERT INTO user_item_data (user_id, item_id, played, position_ms, play_count, last_played_at) VALUES (?1, ?2, ?3, 0, ?3, ?4)
             ON CONFLICT(user_id, item_id) DO UPDATE SET played = ?3, position_ms = 0, play_count = play_count + CASE WHEN ?3 = 1 AND played = 0 THEN 1 ELSE 0 END,
             last_played_at = CASE WHEN ?3 = 1 THEN ?4 ELSE last_played_at END"
        } else {
            "INSERT INTO user_item_data (user_id, item_id, favorite) VALUES (?1, ?2, ?3) ON CONFLICT(user_id, item_id) DO UPDATE SET favorite = ?3"
        };
        sqlx::query(sql).bind(user.user_id).bind(t).bind(on as i64).bind(now).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    state.events.to_user(user.user_id, "user_data.changed", json!({}));
    Ok(Json(json!({ "Played": field == "played" && on, "IsFavorite": field == "favorite" && on, "Key": item, "ItemId": item })))
}

async fn played_add_u(State(s): State<Shared>, u: AuthUser, Path((_x, i)): Path<(String, String)>) -> ApiResult<Json<Value>> {
    set_flag(&s, &u, &i, "played", true).await
}
async fn played_del_u(State(s): State<Shared>, u: AuthUser, Path((_x, i)): Path<(String, String)>) -> ApiResult<Json<Value>> {
    set_flag(&s, &u, &i, "played", false).await
}
async fn played_add(State(s): State<Shared>, u: AuthUser, Path(i): Path<String>) -> ApiResult<Json<Value>> {
    set_flag(&s, &u, &i, "played", true).await
}
async fn played_del(State(s): State<Shared>, u: AuthUser, Path(i): Path<String>) -> ApiResult<Json<Value>> {
    set_flag(&s, &u, &i, "played", false).await
}
async fn fav_add_u(State(s): State<Shared>, u: AuthUser, Path((_x, i)): Path<(String, String)>) -> ApiResult<Json<Value>> {
    set_flag(&s, &u, &i, "favorite", true).await
}
async fn fav_del_u(State(s): State<Shared>, u: AuthUser, Path((_x, i)): Path<(String, String)>) -> ApiResult<Json<Value>> {
    set_flag(&s, &u, &i, "favorite", false).await
}
async fn fav_add(State(s): State<Shared>, u: AuthUser, Path(i): Path<String>) -> ApiResult<Json<Value>> {
    set_flag(&s, &u, &i, "favorite", true).await
}
async fn fav_del(State(s): State<Shared>, u: AuthUser, Path(i): Path<String>) -> ApiResult<Json<Value>> {
    set_flag(&s, &u, &i, "favorite", false).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn iso_dates() {
        assert_eq!(iso(0), "1970-01-01T00:00:00.0000000Z");
        assert_eq!(iso(1_700_000_000_000), "2023-11-14T22:13:20.0000000Z");
    }
}
