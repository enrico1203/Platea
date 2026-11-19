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
use crate::state::Shared;
use crate::util::{iso, now_ms};
use axum::body::Body;
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{header, HeaderMap, Method, StatusCode, Uri};
use axum::response::Response;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;

use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Versione di Jellyfin dichiarata: Infuse legge nome e versione al collegamento.
/// Jellyfin è passato da 10.11 a 12.x (settembre 2026); Swiftfin chiede almeno 12.1.0, in tre numeri.
const JELLYFIN_VERSION: &str = "12.1.0";
const TICKS_PER_MS: i64 = 10_000;

pub fn router() -> Router<Shared> {
    Router::new()
        .route("/System/Info/Public", get(public_info))
        .route("/System/Info", get(system_info))
        .route("/System/Ping", get(ping).post(ping))
        .route("/Users/Public", get(users_public))
        .route("/Users/AuthenticateByName", post(authenticate))
        .route("/Users/AuthenticateWithQuickConnect", post(authenticate_quick_connect))
        .route("/QuickConnect/Enabled", get(qc_enabled))
        .route("/QuickConnect/Initiate", post(qc_initiate).get(qc_initiate))
        .route("/QuickConnect/Connect", get(qc_connect))
        .route("/QuickConnect/Authorize", post(qc_authorize))
        .route("/Sessions/Logout", post(logout))
        .route("/Users/Me", get(me))
        .route("/Users/{user}", get(user_by_id))
        .route("/UserViews/GroupingOptions", get(grouping_options))
        .route("/Users/{user}/GroupingOptions", get(grouping_options_u))
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
        .route("/DisplayPreferences/{id}", get(display_prefs).post(ok_empty))
        // Endpoint usati dalle app Jellyfin recenti (Swiftfin, Jellyfin Android/TV, Findroid).
        .route("/System/Configuration", get(system_configuration))
        .route("/socket", get(jf_socket))
        .route("/Playback/BitrateTest", get(bitrate_test))
        .route("/LiveTv/Info", get(lt_info))
        .route("/LiveTv/GuideInfo", get(lt_guide_info))
        .route("/LiveTv/Programs/Recommended", get(lt_recommended))
        .route("/LiveTv/Programs", get(lt_programs_q).post(lt_programs_body))
        .route("/LiveTv/Programs/{id}", get(lt_program))
        .route("/LiveTv/Channels", get(lt_channels))
        .route("/LiveTv/Channels/{id}", get(lt_channel))
        .route("/LiveTv/Timers", get(lt_timers).post(lt_timer_create))
        .route("/LiveTv/Timers/{id}", get(lt_timer).delete(lt_timer_delete))
        .route("/LiveTv/Timers/Defaults", get(lt_timer_defaults))
        .route("/LiveTv/SeriesTimers", get(lt_series_timers).post(lt_series_create))
        .route("/LiveTv/SeriesTimers/{id}", delete(lt_timer_delete))
        .route("/LiveTv/Recordings", get(empty_items))
        .route("/LiveTv/Recordings/Groups", get(empty_items))
        .route("/LiveTv/Recordings/Folders", get(empty_items))
        .route("/UserImage", get(user_image).head(user_image))
        .route("/Users/{user}/Images/{kind}", get(user_image_path).head(user_image_path))
        .route("/Branding/Splashscreen", get(splashscreen))
        .route("/Branding/Css", get(branding_css))
        .route("/Branding/Css.css", get(branding_css))
        .route("/Items/Root", get(items_root))
        .route("/Users/{user}/Items/Root", get(items_root_u))
        .route("/Items/{item}/Similar", get(similar))
        .route("/Movies/{item}/Similar", get(similar))
        .route("/Shows/{item}/Similar", get(similar))
        .route("/Items/{item}/LocalTrailers", get(empty_array))
        .route("/Users/{user}/Items/{item}/LocalTrailers", get(empty_array))
        .route("/Items/{item}/SpecialFeatures", get(empty_array))
        .route("/Users/{user}/Items/{item}/SpecialFeatures", get(empty_array))
        .route("/Items/{item}/Ancestors", get(empty_array))
        .route("/Items/{item}/ThemeMedia", get(theme_media))
        .route("/Items/Suggestions", get(empty_items))
        .route("/Movies/Recommendations", get(empty_array))
        .route("/Playlists", get(empty_items))
        .route("/Devices", get(empty_items))
        .route("/Devices/Options", post(ok_empty))
        .route("/MediaSegments/{item}", get(empty_items))
        .route("/Localization/Options", get(localization_options))
        .route("/Localization/Cultures", get(localization_cultures))
        .route("/Localization/Countries", get(localization_countries))
        .route("/Localization/ParentalRatings", get(empty_array))
        .route("/UserItems/{item}/UserData", get(user_data).post(user_data_update))
        .route("/Users/{user}/Items/{item}/UserData", get(user_data_u).post(user_data_update_u))
        .route("/Users/{user}/Configuration", post(ok_empty))
        .route("/Users/Configuration", post(ok_empty))
        .route("/Users/{user}/Policy", post(ok_empty))
        .route("/Branding/Configuration", get(branding))
        .layer(axum::middleware::map_response(omit_nulls))
}

/// Jellyfin omette i campi nulli e alcuni client (Infuse) non accettano `null` dove si aspettano un numero
/// o un testo: le risposte JSON di questa API escono senza campi nulli. Video e immagini passano intatti.
async fn omit_nulls(resp: Response) -> Response {
    let is_json = resp.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|v| v.starts_with("application/json"));
    if !is_json {
        return resp;
    }
    let (mut parts, body) = resp.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, 64 * 1024 * 1024).await else {
        return Response::from_parts(parts, Body::empty());
    };
    let out = match serde_json::from_slice::<Value>(&bytes) {
        Ok(mut v) => {
            strip_nulls(&mut v);
            serde_json::to_vec(&v).unwrap_or_else(|_| bytes.to_vec())
        }
        Err(_) => bytes.to_vec(),
    };
    parts.headers.remove(header::CONTENT_LENGTH);
    Response::from_parts(parts, Body::from(out))
}

fn strip_nulls(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.retain(|_, x| !x.is_null());
            m.values_mut().for_each(strip_nulls);
        }
        Value::Array(a) => a.iter_mut().for_each(strip_nulls),
        _ => {}
    }
}

// ---------------------------------------------------------------------------------------------
// Identificativi: GUID a 32 cifre esadecimali come Jellyfin. I primi 16 caratteri distinguono il tipo.

const NS_ITEM: &str = "0000000000000000";
const NS_LIBRARY: &str = "1000000000000000";
const NS_USER: &str = "2000000000000000";
const NS_PERSON: &str = "3000000000000000";
/// Solo in uscita (Studios[].Id): Platea non ha pagine degli studi.
const NS_STUDIO: &str = "4000000000000000";
/// Live TV: canali, programmi della guida, registrazioni programmate (timer), vista "Live TV".
const NS_CHANNEL: &str = "5000000000000000";
const NS_PROGRAM: &str = "6000000000000000";
const NS_TIMER: &str = "7000000000000000";
const NS_LIVEVIEW: &str = "8000000000000000";

#[derive(Debug, Clone, Copy, PartialEq)]
enum Ref {
    Item(i64),
    Library(i64),
    User(i64),
    Person(i64),
    Channel(i64),
    Program(i64),
    Timer(i64),
    LiveView,
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
        NS_CHANNEL => Some(Ref::Channel(id)),
        NS_PROGRAM => Some(Ref::Program(id)),
        NS_TIMER => Some(Ref::Timer(id)),
        NS_LIVEVIEW => Some(Ref::LiveView),
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
        "AuthenticateWithQuickConnect",
        "QuickConnect",
        "Enabled",
        "Initiate",
        "Connect",
        "Authorize",
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
        "Root",
        "Similar",
        "Movies",
        "LocalTrailers",
        "SpecialFeatures",
        "Ancestors",
        "ThemeMedia",
        "Suggestions",
        "Recommendations",
        "Playlists",
        "Devices",
        "MediaSegments",
        "Localization",
        "Options",
        "Cultures",
        "Countries",
        "ParentalRatings",
        "UserData",
        "Policy",
        "UserImage",
        "Playback",
        "BitrateTest",
        "LiveTv",
        "Programs",
        "Recommended",
        "Channels",
        "GuideInfo",
        "Timers",
        "SeriesTimers",
        "Defaults",
        "Recordings",
        "Groups",
        "Folders",
        "Splashscreen",
        "Css",
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
    crate::auth::mediabrowser_fields(raw)
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
    #[allow(clippy::type_complexity)]
    let (name, is_admin, last, expires, allow_download, prefs_raw, allow_livetv): (String, i64, Option<i64>, Option<i64>, i64, String, i64) =
        sqlx::query_as("SELECT username, is_admin, last_login_at, expires_at, allow_download, preferences, allow_livetv FROM users WHERE id = ?")
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
    // UserPolicy: l'SDK Kotlin di Jellyfin (Findroid) vuole tutti i campi non nullable.
    let policy = json!({
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
        "EnableCollectionManagement": false,
        "EnableSubtitleManagement": false,
        "EnableLyricManagement": false,
        "EnableRemoteControlOfOtherUsers": false,
        "EnableSharedDeviceControl": false,
        "EnableLiveTvManagement": false,
        "EnableLiveTvAccess": allow_livetv != 0 || is_admin != 0,
        "ForceRemoteSourceTranscoding": false,
        "EnableContentDeletion": false,
        "EnableSyncTranscoding": false,
        "EnableMediaConversion": false,
        "EnableAllDevices": true,
        "EnableAllChannels": true,
        "InvalidLoginAttemptCount": 0,
        "LoginAttemptsBeforeLockout": -1,
        "MaxActiveSessions": 0,
        "EnablePublicSharing": false,
        "RemoteClientBitrateLimit": 0,
        "SyncPlayAccess": "None",
        "AuthenticationProviderId": "Jellyfin.Server.Implementations.Users.DefaultAuthenticationProvider",
        "PasswordResetProviderId": "Jellyfin.Server.Implementations.Users.DefaultPasswordResetProvider",
    });
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
            "DisplayMissingEpisodes": false,
            "GroupedFolders": [],
            "DisplayCollectionsView": false,
            "EnableLocalPassword": false,
            "RememberAudioSelections": true,
            "RememberSubtitleSelections": true,
            "OrderedViews": [],
            "LatestItemsExcludes": [],
            "MyMediaExcludes": [],
        },
        "Policy": policy,
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
    auth_result(
        &state,
        ok.user_id,
        &ok.username,
        &ok.token,
        req.client.as_deref(),
        req.device_name.as_deref(),
        req.device_id.as_deref(),
        req.client_version.as_deref(),
    )
    .await
}

/// Risposta di autenticazione (AuthenticationResult), uguale per password e Quick Connect.
#[allow(clippy::too_many_arguments)]
async fn auth_result(
    state: &Shared,
    user_id: i64,
    username: &str,
    token: &str,
    client: Option<&str>,
    device_name: Option<&str>,
    device_id: Option<&str>,
    version: Option<&str>,
) -> ApiResult<Json<Value>> {
    Ok(Json(json!({
        "User": user_dto(state, user_id).await?,
        "SessionInfo": {
            "Id": crate::util::random_hex(16),
            "UserId": guid(state, NS_USER, user_id),
            "UserName": username,
            "Client": client,
            "DeviceName": device_name,
            "DeviceId": device_id,
            "ApplicationVersion": version,
            "ServerId": state.server_id,
            "SupportsRemoteControl": false,
            "SupportsMediaControl": false,
            "PlayableMediaTypes": ["Video"],
            "SupportedCommands": [],
            "IsActive": true,
            "HasCustomDeviceName": false,
            "LastActivityDate": iso(now_ms()),
            "LastPlaybackCheckIn": iso(now_ms()),
        },
        "AccessToken": token,
        "ServerId": state.server_id,
    })))
}

// Quick Connect come in Jellyfin: Initiate → Connect (stato) → Authorize dall'altro dispositivo → AuthenticateWithQuickConnect.

fn qc_dto(p: &crate::quickconnect::Pending, secret: &str) -> Value {
    json!({
        "Authenticated": p.user_id.is_some(),
        "Secret": secret,
        "Code": p.code,
        "DeviceId": p.device_id,
        "DeviceName": p.device_name,
        "AppName": p.client,
        "AppVersion": p.client_version,
        "DateAdded": iso(p.created_at),
    })
}

/// Parametri di query senza distinzione tra maiuscole e minuscole (`secret` o `Secret`).
fn query_ci(q: &HashMap<String, String>, key: &str) -> Option<String> {
    q.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.clone())
}

async fn qc_enabled(State(state): State<Shared>) -> Json<bool> {
    Json(state.settings.get().general.quick_connect)
}

async fn qc_initiate(State(state): State<Shared>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let f = auth_fields(&headers);
    let ip = crate::net::client_ip(Some(peer), &headers, &state.settings.get().network.trusted_proxies);
    let req = crate::api::quickconnect::InitiateReq {
        device_id: f.get("deviceid").cloned(),
        device_name: f.get("device").cloned(),
        client: f.get("client").cloned(),
        client_version: f.get("version").cloned(),
    };
    let (_, secret, _, _) = crate::api::quickconnect::start(&state, &req, ip)?;
    let p = state.quick_connect.status(&secret).ok_or_else(|| ApiError::not_found("Quick Connect"))?;
    Ok(Json(qc_dto(&p, &secret)))
}

async fn qc_connect(State(state): State<Shared>, Query(q): Query<HashMap<String, String>>) -> ApiResult<Json<Value>> {
    crate::api::quickconnect::enabled(&state)?;
    let secret = query_ci(&q, "secret").unwrap_or_default();
    let p = state.quick_connect.status(&secret).ok_or_else(|| ApiError::not_found("Quick Connect"))?;
    Ok(Json(qc_dto(&p, &secret)))
}

async fn qc_authorize(State(state): State<Shared>, user: AuthUser, Query(q): Query<HashMap<String, String>>) -> ApiResult<Json<bool>> {
    let code = query_ci(&q, "code").unwrap_or_default();
    crate::api::quickconnect::confirm(&state, &user, &code).await?;
    Ok(Json(true))
}

#[derive(Deserialize)]
struct QcAuthBody {
    #[serde(alias = "secret")]
    #[serde(rename = "Secret")]
    secret: String,
}

async fn authenticate_quick_connect(
    State(state): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<QcAuthBody>,
) -> ApiResult<Json<Value>> {
    let ip = crate::net::client_ip(Some(peer), &headers, &state.settings.get().network.trusted_proxies);
    let (user_id, username, _, token, p) = crate::api::quickconnect::finish(&state, &body.secret, ip).await?;
    auth_result(&state, user_id, &username, &token, Some(&p.client), Some(&p.device_name), Some(&p.device_id), Some(&p.client_version)).await
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
        "MediaType": "Unknown",
        "UserData": { "PlaybackPositionTicks": 0, "PlayCount": 0, "IsFavorite": false, "Played": false, "Key": format!("lib{id}"), "ItemId": guid(state, NS_LIBRARY, id) },
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
    let items: Vec<Value> =
        names.iter().map(|n| json!({ "Name": n, "Id": format!("{:x}", md5_like(n)), "Type": "Genre", "ServerId": state.server_id, "ImageTags": {} })).collect();
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
/// Librerie che si possono raggruppare nelle viste (Infuse le chiede subito dopo l'accesso).
async fn grouping_options(State(state): State<Shared>, user: AuthUser) -> ApiResult<Json<Value>> {
    let libs: Vec<(i64, String)> =
        sqlx::query_as(&format!("SELECT id, name FROM libraries i WHERE i.kind <> 'audiobooks' {} ORDER BY name", user.library_filter("i.id")))
            .fetch_all(&state.db.read)
            .await?;
    Ok(Json(json!(libs.into_iter().map(|(id, name)| json!({ "Name": name, "Id": guid(&state, NS_LIBRARY, id) })).collect::<Vec<_>>())))
}

async fn grouping_options_u(state: State<Shared>, user: AuthUser, Path(_u): Path<String>) -> ApiResult<Json<Value>> {
    grouping_options(state, user).await
}

async fn virtual_folders(State(state): State<Shared>, user: AuthUser) -> ApiResult<Json<Value>> {
    let libs: Vec<(i64, String, String)> =
        sqlx::query_as(&format!("SELECT id, name, kind FROM libraries i WHERE i.kind <> 'audiobooks' {} ORDER BY name", user.library_filter("i.id")))
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
        sqlx::query_as(&format!("SELECT id, name, kind FROM libraries i WHERE i.kind <> 'audiobooks' {} ORDER BY name", user.library_filter("i.id")))
            .fetch_all(&state.db.read)
            .await?;
    let mut items = Vec::new();
    for (id, name, kind) in libs {
        items.push(library_dto(state, id, &name, &kind).await);
    }
    if lt_enabled(state, user).await {
        items.push(live_view_dto(state));
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
                    "IsInterlaced": false,
                    "IsHearingImpaired": false,
                    "IsOriginal": false,
                    "AudioSpatialFormat": "None",
                    "VideoRange": "Unknown",
                    "VideoRangeType": "Unknown",
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
            "ReadAtNativeFramerate": false,
            "IgnoreDts": false,
            "IgnoreIndex": false,
            "GenPtsInput": false,
            "UseMostCompatibleTranscodingProfile": false,
            "HasSegments": false,
            "TranscodingSubProtocol": "http",
            "MediaStreams": ms,
            "DirectStreamUrl": format!("/Videos/{}/stream.{}?static=true&MediaSourceId={}", item_guid(state, item), container.unwrap_or(ext), state.ids.encode(fid)),
        }));
    }
    Ok(out)
}

/// DTO di un elenco con i campi chiesti in `Fields` (stagioni, episodi, prossimi, ultimi, da riprendere).
async fn dtos(state: &Shared, rows: &[Row], uri: &Uri) -> ApiResult<Vec<Value>> {
    let mut out: Vec<Value> = rows.iter().map(|r| dto(state, r)).collect();
    if let Some(f) = normalize_query(uri).fields {
        for (r, v) in rows.iter().zip(out.iter_mut()) {
            list_fields(state, r, v, &f).await?;
        }
    }
    Ok(out)
}

/// Campi chiesti con `Fields` negli elenchi. Infuse sincronizza la libreria a pagine di 50 e vuole le
/// sorgenti (percorso, contenitore, flussi) e gli identificativi esterni già nell'elenco.
async fn list_fields(state: &Shared, r: &Row, v: &mut Value, fields: &str) -> ApiResult<()> {
    let has = |name: &str| fields.split(',').any(|f| f.trim().eq_ignore_ascii_case(name));
    let Some(o) = v.as_object_mut() else { return Ok(()) };
    let meta_id = if r.kind == "season" || r.kind == "episode" { r.series_id.unwrap_or(r.id) } else { r.id };
    if has("Genres") {
        let genres: Vec<String> = sqlx::query_scalar("SELECT g.name FROM item_genres ig JOIN genres g ON g.id = ig.genre_id WHERE ig.item_id = ?")
            .bind(meta_id)
            .fetch_all(&state.db.read)
            .await?;
        o.insert("Genres".into(), json!(genres));
    }
    if has("ProviderIds") {
        let ext: Vec<(String, String)> =
            sqlx::query_as("SELECT provider, value FROM external_ids WHERE item_id = ?").bind(r.id).fetch_all(&state.db.read).await?;
        o.insert("ProviderIds".into(), provider_ids(ext));
    }
    if (has("MediaSources") || has("Path") || has("MediaStreams")) && (r.kind == "movie" || r.kind == "episode") {
        let sources = media_sources(state, r.id).await?;
        if let Some(first) = sources.first() {
            o.insert("Container".into(), first["Container"].clone());
            o.insert("Path".into(), first["Path"].clone());
            if has("MediaStreams") || has("MediaSources") {
                o.insert("MediaStreams".into(), first["MediaStreams"].clone());
            }
        }
        if has("MediaSources") {
            o.insert("MediaSources".into(), json!(sources));
        }
    }
    Ok(())
}

fn provider_ids(ext: Vec<(String, String)>) -> Value {
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
    Value::Object(provider)
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
    let studios: Vec<(i64, String)> = sqlx::query_as("SELECT s.id, s.name FROM item_studios x JOIN studios s ON s.id = x.studio_id WHERE x.item_id = ?")
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
    let (tagline, trailers): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT tagline, trailers FROM items WHERE id = ?").bind(id).fetch_one(&state.db.read).await?;
    let trailers: Vec<Value> = trailers.and_then(|t| serde_json::from_str::<Vec<Value>>(&t).ok()).unwrap_or_default();
    let o = v.as_object_mut().unwrap();
    o.insert("Genres".into(), json!(genres));
    o.insert("Studios".into(), json!(studios.iter().map(|(sid, s)| json!({ "Name": s, "Id": guid(state, NS_STUDIO, *sid) })).collect::<Vec<_>>()));
    o.insert("Taglines".into(), json!(tagline.into_iter().collect::<Vec<_>>()));
    // Trailer YouTube: Infuse e le app Jellyfin li mostrano come "Trailer".
    o.insert(
        "RemoteTrailers".into(),
        json!(trailers
            .iter()
            .filter_map(|t| t["key"].as_str().map(|k| json!({ "Url": format!("https://www.youtube.com/watch?v={k}"), "Name": t["name"] })))
            .collect::<Vec<_>>()),
    );
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
        Some(Ref::Channel(c)) => Ok(Json(lt_channel_dto(state, user, c, true).await?)),
        Some(Ref::Program(p)) => Ok(Json(lt_program_dto_by_id(state, user, p).await?)),
        Some(Ref::LiveView) => Ok(Json(live_view_dto(state))),
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
    /// Campi in più richiesti dal client (MediaSources, Path, ProviderIds, Genres…).
    fields: Option<String>,
}

/// Parametri della query: Jellyfin accetta sia PascalCase sia camelCase.
fn normalize_query(uri: &Uri) -> ItemsQuery {
    let mut map: HashMap<String, String> = HashMap::new();
    if let Some(q) = uri.query() {
        for pair in q.split('&') {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            let v = urlencoding::decode(&v.replace('+', " ")).map(|s| s.into_owned()).unwrap_or_default();
            // Le app recenti (Swiftfin, SDK di Jellyfin 12) ripetono il parametro per ogni valore
            // (`includeItemTypes=Movie&includeItemTypes=Series`): i valori si uniscono con la virgola.
            map.entry(k.to_lowercase())
                .and_modify(|e: &mut String| {
                    if !v.is_empty() {
                        if !e.is_empty() {
                            e.push(',');
                        }
                        e.push_str(&v);
                    }
                })
                .or_insert(v);
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
        fields: s("fields"),
    }
}

async fn query_items(state: &Shared, user: &AuthUser, q: ItemsQuery) -> ApiResult<Json<Value>> {
    if let Some(v) = live_items(state, user, &q).await? {
        return Ok(Json(v));
    }
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
    // Gli audiolibri (e le loro serie) non passano dallo strato Jellyfin: i client video non saprebbero cosa farne.
    sql.push_str(" AND i.library_id NOT IN (SELECT id FROM libraries WHERE kind = 'audiobooks')");
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
    let mut items: Vec<Value> = rows.iter().map(|r| dto(state, r)).collect();
    if let Some(f) = q.fields.as_deref() {
        for (r, v) in rows.iter().zip(items.iter_mut()) {
            list_fields(state, r, v, f).await?;
        }
    }
    Ok(Json(json!({ "Items": items, "TotalRecordCount": total, "StartIndex": start })))
}

/// Parametri delle richieste di elenco dei client Jellyfin, al livello debug, senza chiavi o token.
fn log_query(uri: &Uri) {
    if !tracing::enabled!(tracing::Level::DEBUG) {
        return;
    }
    let q: Vec<&str> = uri
        .query()
        .unwrap_or("")
        .split('&')
        .filter(|p| {
            let k = p.split('=').next().unwrap_or("").to_ascii_lowercase();
            !(k.contains("key") || k.contains("token"))
        })
        .collect();
    tracing::debug!(path = uri.path(), query = %q.join("&"), "richiesta di elenco Jellyfin");
}

async fn items_u(State(state): State<Shared>, user: AuthUser, Path(_u): Path<String>, uri: Uri) -> ApiResult<Json<Value>> {
    log_query(&uri);
    query_items(&state, &user, normalize_query(&uri)).await
}

async fn items_q(State(state): State<Shared>, user: AuthUser, uri: Uri) -> ApiResult<Json<Value>> {
    log_query(&uri);
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
    Ok(Json(json!(dtos(state, &rows, uri).await?)))
}

async fn latest_u(State(state): State<Shared>, user: AuthUser, Path(_u): Path<String>, uri: Uri) -> ApiResult<Json<Value>> {
    latest_list(&state, &user, &uri).await
}

async fn latest_q(State(state): State<Shared>, user: AuthUser, uri: Uri) -> ApiResult<Json<Value>> {
    latest_list(&state, &user, &uri).await
}

async fn next_up(State(state): State<Shared>, user: AuthUser, uri: Uri) -> ApiResult<Json<Value>> {
    let api_user = user.clone();
    let series: Vec<i64> = sqlx::query_scalar(&format!(
        "SELECT i.id FROM items i JOIN user_item_data u ON u.item_id = i.id AND u.user_id = ?1 WHERE i.kind = 'series' AND u.last_played_at IS NOT NULL {} ORDER BY u.last_played_at DESC LIMIT 20",
        user.library_filter("i.library_id")
    ))
    .bind(user.user_id)
    .fetch_all(&state.db.read)
    .await?;
    let mut found: Vec<Row> = Vec::new();
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
                        found.push(r);
                    }
                }
            }
        }
    }
    let out = dtos(&state, &found, &uri).await?;
    let n = out.len();
    Ok(Json(json!({ "Items": out, "TotalRecordCount": n, "StartIndex": 0 })))
}

async fn seasons(State(state): State<Shared>, user: AuthUser, Path(series): Path<String>, uri: Uri) -> ApiResult<Json<Value>> {
    let sid = item_id(&state, &series)?;
    let sql = format!("{SELECT} WHERE i.parent_id = ?2 AND i.kind = 'season' {} ORDER BY COALESCE(i.index_number, 0)", user.library_filter("i.library_id"));
    let rows: Vec<Row> = sqlx::query_as(&sql).bind(user.user_id).bind(sid).fetch_all(&state.db.read).await?;
    let n = rows.len();
    Ok(Json(json!({ "Items": dtos(&state, &rows, &uri).await?, "TotalRecordCount": n, "StartIndex": 0 })))
}

async fn episodes(State(state): State<Shared>, user: AuthUser, Path(series): Path<String>, uri: Uri) -> ApiResult<Json<Value>> {
    log_query(&uri);
    let sid = item_id(&state, &series)?;
    let q = normalize_query(&uri);
    let mut sql = format!("{SELECT} WHERE i.series_id = ?2 AND i.kind = 'episode' {}", user.library_filter("i.library_id"));
    if let Some(Ref::Item(season)) = q.season_id.as_deref().and_then(|s| parse_ref(&state, s)) {
        sql.push_str(&format!(" AND i.parent_id = {season}"));
    }
    sql.push_str(" ORDER BY COALESCE(i.parent_index_number, 0), COALESCE(i.index_number, 0)");
    let rows: Vec<Row> = sqlx::query_as(&sql).bind(user.user_id).bind(sid).fetch_all(&state.db.read).await?;
    let n = rows.len();
    Ok(Json(json!({ "Items": dtos(&state, &rows, &uri).await?, "TotalRecordCount": n, "StartIndex": 0 })))
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

async fn serve_image(state: &Shared, item: &str, kind: &str, w: Option<u32>, headers: &HeaderMap) -> ApiResult<Response> {
    // Canali (e i loro programmi): il logo dalla sorgente, come nella web app.
    match parse_ref(state, item) {
        Some(Ref::Channel(c)) => return crate::api::livetv::logo_response(state, c, headers).await,
        Some(Ref::Program(p)) if p >> 32 != 0 => return crate::api::livetv::logo_response(state, p >> 32, headers).await,
        _ => {}
    }
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

/// PlaybackInfo: con il `DeviceProfile` dell'app si decide come in Platea. Se basta il direct play, il file com'è
/// (come prima); altrimenti si apre una sessione HLS di Platea (remux o transcodifica) e la si passa come
/// `TranscodingUrl`. Infuse apre tutto da sé: per lui sempre direct play.
async fn playback_info(
    State(state): State<Shared>,
    user: AuthUser,
    Path(item): Path<String>,
    parts: axum::http::request::Parts,
    body: axum::body::Bytes,
) -> ApiResult<Json<Value>> {
    let headers = &parts.headers;
    let uri = &parts.uri;
    user.require_playback()?;
    if let Some(Ref::Channel(c)) = parse_ref(&state, &item) {
        let play_session = crate::util::random_hex(16);
        grant_stream(&play_session, &user, -c, crate::auth::peer_ip(&state, &parts));
        return Ok(Json(json!({ "MediaSources": [lt_media_source(&state, &user, c, &play_session).await?], "PlaySessionId": play_session })));
    }
    let id = item_id(&state, &item)?;
    let row = crate::api::items::load_item(&state, &user, id).await?;
    let sources = media_sources(&state, id).await?;
    let b: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let mut q: HashMap<String, String> = HashMap::new();
    if let Some(qs) = uri.query() {
        for pair in qs.split('&') {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            q.insert(k.to_lowercase(), urlencoding::decode(v).map(|s| s.into_owned()).unwrap_or_default());
        }
    }
    let num = |k: &str, body_key: &str| b[body_key].as_i64().or_else(|| q.get(k).and_then(|v| v.parse().ok()));
    let client = auth_fields(headers).get("client").cloned().unwrap_or_default();
    let infuse = client.to_ascii_lowercase().contains("infuse");
    let wants_transcoding = b["EnableTranscoding"].as_bool().unwrap_or(true);
    let ip = crate::auth::peer_ip(&state, &parts);
    let direct = || {
        let play_session = crate::util::random_hex(16);
        grant_stream(&play_session, &user, id, ip);
        Json(json!({ "MediaSources": sources.clone(), "PlaySessionId": play_session }))
    };
    let Some(profile_json) = b.get("DeviceProfile").filter(|p| p.is_object()) else { return Ok(direct()) };
    if infuse || !wants_transcoding || !matches!(row.kind.as_str(), "movie" | "episode") {
        return Ok(direct());
    }
    let max_bitrate = num("maxstreamingbitrate", "MaxStreamingBitrate").filter(|b| *b > 0).map(|b| b as u64);
    let profile = super::jellyfin_profile::convert(profile_json, max_bitrate);
    let source = b["MediaSourceId"].as_str().map(String::from).or_else(|| q.get("mediasourceid").cloned());
    let fid = choose_file(&state, id, source.as_deref()).await?;
    let info = crate::api::playback::load_file(&state, fid).await?;
    let audio = num("audiostreamindex", "AudioStreamIndex").map(|i| i as u32);
    let subtitle = num("subtitlestreamindex", "SubtitleStreamIndex").filter(|i| *i >= 0).map(|i| i as u32);
    let preq = platea_playback::PlaybackRequest {
        audio_index: audio,
        subtitle_index: subtitle,
        allow_remux: user.perms.allow_remux,
        allow_transcode: user.perms.allow_transcode && state.ffmpeg.get().available,
        user_max_bitrate: max_bitrate,
        server_video_encoders: crate::api::playback::allowed_encoders(&state),
        force_transcode: false,
    };
    match platea_playback::decide(&info.media, &profile, &preq) {
        Ok(d) if d.method == platea_playback::Method::DirectPlay => return Ok(direct()),
        Err(_) => return Ok(direct()),
        Ok(_) => {}
    }
    // Serve una conversione: la stessa sessione HLS della web UI.
    let start_ms = num("starttimeticks", "StartTimeTicks").map(|t| t / TICKS_PER_MS);
    let req = crate::api::playback::InfoReq {
        item_id: state.ids.encode(id),
        file_id: Some(state.ids.encode(fid)),
        profile,
        audio_index: audio,
        subtitle_index: subtitle,
        start_ms,
        max_bitrate,
        force_transcode: false,
        ui_language: None,
        explicit_quality: true,
    };
    let r = crate::api::playback::start(state.clone(), user.clone(), req).await?;
    let Some(hls) = r["hls_url"].as_str() else { return Ok(direct()) };
    let session = r["session_id"].as_str().unwrap_or_default().to_string();
    let mut src = sources.iter().find(|s| s["Id"].as_str() == Some(state.ids.encode(fid).as_str())).cloned().unwrap_or_else(|| sources[0].clone());
    let fmp4 = r["chain"].as_str().is_some_and(|c| c.contains("fMP4"));
    if let Some(o) = src.as_object_mut() {
        o.insert("SupportsDirectPlay".into(), json!(false));
        o.insert("SupportsDirectStream".into(), json!(false));
        o.insert("SupportsTranscoding".into(), json!(true));
        o.insert("TranscodingUrl".into(), json!(hls));
        o.insert("TranscodingSubProtocol".into(), json!("hls"));
        o.insert("TranscodingContainer".into(), json!(if fmp4 { "mp4" } else { "ts" }));
        o.remove("DirectStreamUrl");
    }
    grant_stream(&session, &user, id, ip);
    tracing::info!(client = %client, method = r["method"].as_str().unwrap_or(""), "app Jellyfin: riproduzione convertita in HLS");
    Ok(Json(json!({ "MediaSources": [src], "PlaySessionId": session })))
}

// ---------------------------------------------------------------------------------------------
// Stream senza token. Jellyfin non chiede l'autenticazione su `/Videos/{id}/stream` e alcune app (Findroid, con
// ExoPlayer) aprono l'URL così com'è, senza intestazioni né `api_key`. Platea non apre gli stream a chiunque: vale
// solo per l'elemento di un PlaybackInfo fatto poco prima da un utente autenticato, riconosciuto dal `PlaySessionId`
// che quella risposta ha dato oppure, se l'app non lo rimanda, dallo stesso indirizzo IP.

const GRANT_TTL: Duration = Duration::from_secs(12 * 3600);

#[derive(Clone)]
struct StreamGrant {
    user: AuthUser,
    item: i64,
    ip: Option<std::net::IpAddr>,
    at: std::time::Instant,
}

static STREAM_GRANTS: Lazy<Mutex<HashMap<String, StreamGrant>>> = Lazy::new(|| Mutex::new(HashMap::new()));

fn grant_stream(play_session: &str, user: &AuthUser, item: i64, ip: Option<std::net::IpAddr>) {
    if play_session.is_empty() {
        return;
    }
    let mut map = STREAM_GRANTS.lock().unwrap();
    map.retain(|_, g| g.at.elapsed() < GRANT_TTL);
    if map.len() >= 10_000 {
        if let Some(oldest) = map.iter().min_by_key(|(_, g)| g.at).map(|(k, _)| k.clone()) {
            map.remove(&oldest);
        }
    }
    map.insert(play_session.to_string(), StreamGrant { user: user.clone(), item, ip, at: std::time::Instant::now() });
}

/// Il permesso valido per questa richiesta, se c'è: prima per `PlaySessionId`, poi per IP (il più recente).
fn find_grant(play_session: Option<&str>, item: i64, ip: Option<std::net::IpAddr>) -> Option<StreamGrant> {
    let mut map = STREAM_GRANTS.lock().unwrap();
    let key = match play_session.and_then(|p| map.get(p).map(|g| (p, g))) {
        Some((p, g)) if g.item == item && g.at.elapsed() < GRANT_TTL => Some(p.to_string()),
        Some(_) => None,
        None => ip.and_then(|ip| {
            map.iter().filter(|(_, g)| g.item == item && g.ip == Some(ip) && g.at.elapsed() < GRANT_TTL).max_by_key(|(_, g)| g.at).map(|(k, _)| k.clone())
        }),
    }?;
    let g = map.get_mut(&key)?;
    g.at = std::time::Instant::now();
    Some(g.clone())
}

/// Utente di una richiesta di stream: col token come sempre, altrimenti dal permesso del PlaybackInfo.
struct StreamUser(AuthUser);

impl axum::extract::FromRequestParts<Shared> for StreamUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut axum::http::request::Parts, state: &Shared) -> Result<Self, Self::Rejection> {
        let err = match AuthUser::from_request_parts(parts, state).await {
            Ok(u) => return Ok(StreamUser(u)),
            Err(e @ ApiError::Unauthorized) => e,
            Err(e) => return Err(e),
        };
        let mut segs = parts.uri.path().split('/');
        segs.find(|s| s.eq_ignore_ascii_case("videos"));
        let item = segs
            .next()
            .and_then(|i| match parse_ref(state, i) {
                Some(Ref::Item(x)) => Some(x),
                // Canali: il permesso di PlaybackInfo è registrato con l'id negativo.
                Some(Ref::Channel(c)) => Some(-c),
                _ => None,
            })
            .ok_or(err)?;
        let play_session = crate::auth::query_param(parts, "playsessionid");
        let ip = crate::auth::peer_ip(state, parts);
        let Some(g) = find_grant(play_session.as_deref(), item, ip) else {
            let keys: Vec<&str> = parts.uri.query().unwrap_or("").split('&').filter_map(|p| p.split('=').next()).collect();
            tracing::info!(path = %parts.uri.path(), query = ?keys, "stream senza token e senza PlaybackInfo valido");
            return Err(ApiError::Unauthorized);
        };
        // Il token di quel PlaybackInfo deve esistere ancora (dispositivo non rimosso, utente non uscito).
        let alive: Option<i64> = sqlx::query_scalar("SELECT id FROM tokens WHERE id = ?").bind(g.user.token_id).fetch_optional(&state.db.read).await?;
        if alive.is_none() {
            return Err(ApiError::Unauthorized);
        }
        let mut user = g.user;
        user.ip = ip;
        Ok(StreamUser(user))
    }
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
    if let Some(Ref::Channel(c)) = parse_ref(state, item) {
        return lt_stream(state, user, c).await;
    }
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
    let info = crate::api::playback::load_file(state, fid).await?;
    // Byte inviati alla sessione di questo dispositivo, se Infuse l'ha già aperta con un rapporto.
    let session = COMPAT_SESSIONS.lock().unwrap().get(&(user.token_id, id)).and_then(|sid| state.sessions.get(sid));
    let sent = session.as_ref().map(|s| s.bytes_sent.clone());
    // Stessa copia per tutte le richieste Range di questo dispositivo su questo file.
    let opts = crate::nodes::cdn::Options { sticky: state.nodes.sticky_get((user.token_id, fid)), allow_redirect: false, download: false };
    let served = crate::nodes::cdn::serve(state, &info, method, headers, opts, sent).await?;
    state.nodes.sticky_set((user.token_id, fid), served.node);
    if let Some(s) = session {
        if s.copy_node.swap(served.node, std::sync::atomic::Ordering::Relaxed) != served.node {
            *s.copy_name.lock().unwrap() = state.nodes.row(served.node).filter(|r| !r.is_local).map(|r| r.name);
        }
    }
    Ok(served.response)
}

async fn download(State(state): State<Shared>, user: AuthUser, Path(item): Path<String>, method: Method, headers: HeaderMap) -> ApiResult<Response> {
    let id = item_id(&state, &item)?;
    crate::api::download::serve_for_user(&state, &user, id, &method, &headers).await
}

async fn stream(
    State(state): State<Shared>,
    StreamUser(user): StreamUser,
    Path(item): Path<String>,
    uri: Uri,
    method: Method,
    headers: HeaderMap,
) -> ApiResult<Response> {
    do_stream(&state, &user, &item, &uri, &method, &headers).await
}

async fn stream_named(
    State(state): State<Shared>,
    StreamUser(user): StreamUser,
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
    let path = crate::api::playback::external_subtitle_path(state, &info.root, &ext)?;
    let bytes = tokio::fs::read(path).await.map_err(|_| ApiError::not_found("Sottotitolo"))?;
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
    /// Sessione HLS aperta da PlaybackInfo (transcodifica): i rapporti vanno a lei.
    play_session_id: Option<String>,
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
    crate::webhooks::playback_started(state, &s);
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
    let hls_session = r.play_session_id.as_deref().and_then(|p| state.sessions.get(p)).filter(|s| s.user_id == user.user_id);
    let s = match hls_session {
        Some(s) => s,
        None => ensure_session(state, user, item, r.media_source_id.as_deref()).await?,
    };
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
    Ok(Json(user_data_dto(state, user, item).await?))
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
    fn strip_nulls_recursive() {
        let mut v = json!({ "a": null, "b": 1, "c": { "d": null, "e": [ { "f": null, "g": "x" } ] } });
        strip_nulls(&mut v);
        assert_eq!(v, json!({ "b": 1, "c": { "e": [ { "g": "x" } ] } }));
    }
    #[test]
    fn repeated_query_params_are_merged() {
        let uri: Uri = "/Items?includeItemTypes=Movie&includeItemTypes=Series&fields=Genres&fields=Studios&limit=20".parse().unwrap();
        let q = normalize_query(&uri);
        assert_eq!(q.include_item_types.as_deref(), Some("Movie,Series"));
        assert_eq!(q.fields.as_deref(), Some("Genres,Studios"));
        assert_eq!(q.limit, Some(20));
        let uri: Uri = "/Items?IncludeItemTypes=Movie,Series".parse().unwrap();
        assert_eq!(normalize_query(&uri).include_item_types.as_deref(), Some("Movie,Series"));
    }
    #[test]
    fn iso_dates() {
        assert_eq!(iso(0), "1970-01-01T00:00:00.0000000Z");
        assert_eq!(iso(1_700_000_000_000), "2023-11-14T22:13:20.0000000Z");
    }
}

// ---------------------------------------------------------------------------------------------
// Endpoint delle app Jellyfin recenti: risposte vere dove Platea ha l'equivalente, vuote ma valide altrove.

async fn system_configuration(State(state): State<Shared>, _u: AuthUser) -> Json<Value> {
    let s = state.settings.get();
    let lang = s.metadata.language.clone();
    Json(json!({
        "ServerName": s.general.server_name,
        "UICulture": lang.split('-').next().unwrap_or("it"),
        "PreferredMetadataLanguage": lang.split('-').next().unwrap_or("it"),
        "MetadataCountryCode": s.metadata.country,
        "IsStartupWizardCompleted": true,
        "EnableMetrics": false,
        "QuickConnectAvailable": s.general.quick_connect,
        "EnableRemoteAccess": true,
        "EnableCaseSensitiveItemIds": true,
        "LibraryMonitorDelay": 60,
        "MinResumePct": 5,
        "MaxResumePct": s.playback.played_threshold_percent,
        "MinResumeDurationSeconds": s.playback.resume_min_seconds,
    }))
}

fn root_dto(state: &Shared) -> Value {
    json!({
        "Name": "Media Folders",
        "ServerId": state.server_id,
        "Id": "e9d5075a555c1cbc394eec4cef295274",
        "Type": "UserRootFolder",
        "IsFolder": true,
        "CollectionType": "folders",
        "LocationType": "Remote",
        "ImageTags": {},
        "BackdropImageTags": [],
    })
}

async fn items_root(State(state): State<Shared>, _u: AuthUser) -> Json<Value> {
    Json(root_dto(&state))
}

async fn items_root_u(State(state): State<Shared>, _u: AuthUser, Path(_x): Path<String>) -> Json<Value> {
    Json(root_dto(&state))
}

/// Titoli simili: stessi generi, la saga prima; come la pagina del titolo nella web UI.
async fn similar(State(state): State<Shared>, user: AuthUser, Path(item): Path<String>, uri: Uri) -> ApiResult<Json<Value>> {
    let id = item_id(&state, &item)?;
    let row = crate::api::items::load_item(&state, &user, id).await?;
    let base = match row.kind.as_str() {
        "movie" | "series" => id,
        "season" | "episode" => row.series_id.unwrap_or(id),
        _ => return Ok(Json(json!({ "Items": [], "TotalRecordCount": 0, "StartIndex": 0 }))),
    };
    let kind = if row.kind == "movie" { "movie" } else { "series" };
    let sql = format!(
        "SELECT i.id FROM items i JOIN (
            SELECT ig.item_id AS sid, COUNT(*) AS shared FROM item_genres ig
            WHERE ig.genre_id IN (SELECT genre_id FROM item_genres WHERE item_id = ?2) AND ig.item_id <> ?2
            GROUP BY ig.item_id
         ) g ON g.sid = i.id
         WHERE i.kind = '{kind}' AND i.available = 1 AND ?1 = ?1 {}
         ORDER BY EXISTS (SELECT 1 FROM collection_members a JOIN collection_members b ON a.collection_id = b.collection_id WHERE a.item_id = ?2 AND b.item_id = i.id) DESC,
                  g.shared DESC, COALESCE(i.community_rating, 0) DESC
         LIMIT 12",
        user.library_filter("i.library_id")
    );
    let ids: Vec<i64> = sqlx::query_scalar(&sql).bind(user.user_id).bind(base).fetch_all(&state.db.read).await?;
    if ids.is_empty() {
        return Ok(Json(json!({ "Items": [], "TotalRecordCount": 0, "StartIndex": 0 })));
    }
    let mut q = normalize_query(&uri);
    q.ids = Some(ids.iter().map(|i| item_guid(&state, *i)).collect::<Vec<_>>().join(","));
    q.parent_id = None;
    q.limit = None;
    q.start_index = None;
    q.recursive = Some(true);
    query_items(&state, &user, q).await
}

async fn theme_media(State(state): State<Shared>, _u: AuthUser, Path(item): Path<String>) -> Json<Value> {
    let empty = json!({ "Items": [], "TotalRecordCount": 0, "StartIndex": 0, "OwnerId": item });
    let _ = &state;
    Json(json!({ "ThemeVideosResult": empty, "ThemeSongsResult": empty, "SoundtrackSongsResult": empty }))
}

async fn localization_options() -> Json<Value> {
    Json(json!([{ "Name": "Italiano", "Value": "it" }, { "Name": "English", "Value": "en-US" }]))
}

async fn localization_cultures() -> Json<Value> {
    let c = |name: &str, two: &str, three: &str| json!({ "Name": name, "DisplayName": name, "TwoLetterISOLanguageName": two, "ThreeLetterISOLanguageName": three, "ThreeLetterISOLanguageNames": [three] });
    Json(json!([
        c("Italiano", "it", "ita"),
        c("English", "en", "eng"),
        c("Français", "fr", "fra"),
        c("Deutsch", "de", "deu"),
        c("Español", "es", "spa"),
        c("Português", "pt", "por"),
        c("日本語", "ja", "jpn")
    ]))
}

async fn localization_countries() -> Json<Value> {
    let c = |name: &str, two: &str, three: &str| json!({ "Name": two, "DisplayName": name, "TwoLetterISORegionName": two, "ThreeLetterISORegionName": three });
    Json(json!([
        c("Italia", "IT", "ITA"),
        c("United States", "US", "USA"),
        c("United Kingdom", "GB", "GBR"),
        c("France", "FR", "FRA"),
        c("Deutschland", "DE", "DEU"),
        c("España", "ES", "ESP")
    ]))
}

/// Dati di visione di un elemento (UserItemDataDto).
async fn user_data_dto(state: &Shared, user: &AuthUser, item: &str) -> ApiResult<Value> {
    let id = item_id(state, item)?;
    let d = detail_dto(state, user, id).await?;
    Ok(d.get("UserData")
        .cloned()
        .unwrap_or_else(|| json!({ "PlaybackPositionTicks": 0, "PlayCount": 0, "IsFavorite": false, "Played": false, "Key": item, "ItemId": item })))
}

async fn user_data(State(state): State<Shared>, user: AuthUser, Path(item): Path<String>) -> ApiResult<Json<Value>> {
    Ok(Json(user_data_dto(&state, &user, &item).await?))
}

async fn user_data_u(State(state): State<Shared>, user: AuthUser, Path((_u, item)): Path<(String, String)>) -> ApiResult<Json<Value>> {
    Ok(Json(user_data_dto(&state, &user, &item).await?))
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "PascalCase")]
struct UserDataUpdate {
    played: Option<bool>,
    is_favorite: Option<bool>,
}

async fn apply_user_data(state: &Shared, user: &AuthUser, item: &str, u: &UserDataUpdate) -> ApiResult<Json<Value>> {
    if let Some(p) = u.played {
        let _ = set_flag(state, user, item, "played", p).await?;
    }
    if let Some(f) = u.is_favorite {
        let _ = set_flag(state, user, item, "favorite", f).await?;
    }
    Ok(Json(user_data_dto(state, user, item).await?))
}

async fn user_data_update(State(state): State<Shared>, user: AuthUser, Path(item): Path<String>, Json(u): Json<UserDataUpdate>) -> ApiResult<Json<Value>> {
    apply_user_data(&state, &user, &item, &u).await
}

async fn user_data_update_u(
    State(state): State<Shared>,
    user: AuthUser,
    Path((_x, item)): Path<(String, String)>,
    Json(u): Json<UserDataUpdate>,
) -> ApiResult<Json<Value>> {
    apply_user_data(&state, &user, &item, &u).await
}

/// WebSocket di Jellyfin (`/socket?api_key=…&deviceId=…`): le app lo aprono dopo il login. Platea non manda comandi
/// alle app Jellyfin, ma tiene viva la connessione come Jellyfin (ForceKeepAlive / KeepAlive).
async fn jf_socket(
    State(state): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    uri: Uri,
    upgrade: axum::extract::ws::WebSocketUpgrade,
) -> ApiResult<Response> {
    let mut token = auth_fields(&headers).get("token").cloned();
    for h in ["x-emby-token", "x-mediabrowser-token"] {
        if token.is_none() {
            token = headers.get(h).and_then(|v| v.to_str().ok()).map(String::from);
        }
    }
    if token.is_none() {
        token = uri.query().and_then(|q| {
            q.split('&').find_map(|p| {
                let (k, v) = p.split_once('=')?;
                (k.eq_ignore_ascii_case("api_key") || k.eq_ignore_ascii_case("apikey")).then(|| v.to_string())
            })
        });
    }
    let token = token.ok_or(ApiError::Unauthorized)?;
    crate::auth::resolve_token(&state, &token, Some(peer.ip())).await?;
    Ok(upgrade.on_upgrade(|mut socket| async move {
        use axum::extract::ws::Message;
        let keep = json!({ "MessageType": "ForceKeepAlive", "Data": 60, "MessageId": crate::util::random_hex(16) }).to_string();
        if socket.send(Message::Text(keep.into())).await.is_err() {
            return;
        }
        let mut ping = tokio::time::interval(Duration::from_secs(30));
        loop {
            tokio::select! {
                msg = socket.recv() => match msg {
                    Some(Ok(Message::Text(t))) => {
                        let v: Value = serde_json::from_str(&t).unwrap_or(Value::Null);
                        if v["MessageType"].as_str() == Some("KeepAlive") {
                            let r = json!({ "MessageType": "KeepAlive", "MessageId": crate::util::random_hex(16) }).to_string();
                            if socket.send(Message::Text(r.into())).await.is_err() { break; }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                },
                _ = ping.tick() => {
                    if socket.send(Message::Ping(Vec::new().into())).await.is_err() { break; }
                }
            }
        }
    }))
}

/// Immagine del profilo di un utente (`/UserImage?userId=…`, Jellyfin 10.9+). Senza immagine: 404, come Jellyfin.
async fn user_image(State(state): State<Shared>, Query(q): Query<HashMap<String, String>>) -> ApiResult<Response> {
    let id = query_ci(&q, "userid").unwrap_or_default();
    serve_user_image(&state, &id).await
}

async fn user_image_path(State(state): State<Shared>, Path((user, _kind)): Path<(String, String)>) -> ApiResult<Response> {
    serve_user_image(&state, &user).await
}

async fn serve_user_image(state: &Shared, id: &str) -> ApiResult<Response> {
    let enc = if id.len() == 32 { &id[16..] } else { id };
    let uid = state.ids.decode(enc).ok_or_else(|| ApiError::not_found("Immagine"))?;
    let bytes = tokio::fs::read(crate::api::auth::avatar_path(state, uid)).await.map_err(|_| ApiError::not_found("Immagine"))?;
    Ok(Response::builder().header(header::CONTENT_TYPE, "image/png").header(header::CACHE_CONTROL, "public, max-age=3600").body(Body::from(bytes)).unwrap())
}

/// Sfondo della schermata di accesso: Platea non ne ha uno, si manda un PNG trasparente di 1×1.
async fn splashscreen() -> Response {
    const PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08,
        0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00,
        0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    Response::builder().header(header::CONTENT_TYPE, "image/png").body(Body::from(PNG)).unwrap()
}

async fn branding_css() -> Response {
    Response::builder().header(header::CONTENT_TYPE, "text/css").body(Body::from("")).unwrap()
}

/// Prova di velocità di Swiftfin prima della riproduzione: `size` byte da scaricare (al massimo 20 MB), senza compressione.
async fn bitrate_test(_u: AuthUser, Query(q): Query<HashMap<String, String>>) -> Response {
    let size = query_ci(&q, "size").and_then(|v| v.parse::<usize>().ok()).unwrap_or(102_400).clamp(1, 20 * 1024 * 1024);
    // Byte pseudo-casuali: niente di comprimibile lungo il percorso (Cloudflare), la misura resta vera.
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15 ^ size as u64;
    let body: Vec<u8> = (0..size)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect();
    Response::builder().header(header::CONTENT_TYPE, "application/octet-stream").header(header::CACHE_CONTROL, "no-store").body(Body::from(body)).unwrap()
}

// ---------------------------------------------------------------------------------------------
// Live TV (canali, guida, timer) per Infuse, Swiftfin e le app Jellyfin. I canali si guardano in MPEG-TS
// attraverso il server, come fanno queste app con Jellyfin.

async fn lt_enabled(state: &Shared, user: &AuthUser) -> bool {
    if !user.perms.allow_livetv {
        return false;
    }
    let (filter, binds) = crate::livetv::channel_filter(user, "c");
    let sql = format!("SELECT COUNT(*) FROM tv_channels c WHERE 1 {filter}");
    let mut q = sqlx::query_scalar::<_, i64>(&sql);
    for b in &binds {
        q = q.bind(b);
    }
    q.fetch_one(&state.db.read).await.unwrap_or(0) > 0
}

fn live_view_dto(state: &Shared) -> Value {
    json!({
        "Name": "Live TV",
        "ServerId": state.server_id,
        "Id": format!("{NS_LIVEVIEW}{}", state.ids.encode(1)),
        "Etag": "livetv",
        "Type": "UserView",
        "CollectionType": "livetv",
        "IsFolder": true,
        "ChildCount": 0,
        "LocationType": "Virtual",
        "ImageTags": {},
        "BackdropImageTags": [],
        "UserData": { "PlaybackPositionTicks": 0, "PlayCount": 0, "IsFavorite": false, "Played": false, "Key": "livetv", "ItemId": format!("{NS_LIVEVIEW}{}", state.ids.encode(1)) },
        "MediaType": "Unknown",
    })
}

fn jf_date(ms: i64) -> String {
    let s = crate::livetv::dispatcharr::iso(ms);
    format!("{}.0000000Z", s.trim_end_matches('Z'))
}

#[derive(sqlx::FromRow)]
struct LtChannel {
    id: i64,
    number: Option<f64>,
    name: String,
    logo_url: Option<String>,
    radio: i64,
    favorite: i64,
}

async fn lt_channels_rows(state: &Shared, user: &AuthUser, only: Option<i64>, favorites: bool) -> ApiResult<Vec<LtChannel>> {
    if !user.perms.allow_livetv {
        return Ok(vec![]);
    }
    let (filter, binds) = crate::livetv::channel_filter(user, "c");
    let mut sql = format!(
        "SELECT c.id, c.number, c.name, c.logo_url, c.radio, EXISTS (SELECT 1 FROM user_tv_favorites f WHERE f.channel_id = c.id AND f.user_id = ?1) AS favorite
         FROM tv_channels c WHERE 1 {filter}"
    );
    if only.is_some() {
        sql.push_str(" AND c.id = ?");
    }
    if favorites {
        sql.push_str(" AND EXISTS (SELECT 1 FROM user_tv_favorites f WHERE f.channel_id = c.id AND f.user_id = ?1)");
    }
    sql.push_str(" ORDER BY c.number IS NULL, c.number, c.name");
    let sql = crate::livetv::number_params(&sql, 2);
    let mut q = sqlx::query_as::<_, LtChannel>(&sql).bind(user.user_id);
    for b in &binds {
        q = q.bind(b);
    }
    if let Some(o) = only {
        q = q.bind(o);
    }
    Ok(q.fetch_all(&state.db.read).await?)
}

fn number_text(n: Option<f64>) -> Option<String> {
    n.map(|n| if n.fract() == 0.0 { format!("{}", n as i64) } else { n.to_string() })
}

fn channel_value(state: &Shared, c: &LtChannel, current: Option<Value>) -> Value {
    let id = guid(state, NS_CHANNEL, c.id);
    let mut v = json!({
        "Name": c.name,
        "ServerId": state.server_id,
        "Id": id,
        "Type": "TvChannel",
        "ChannelType": if c.radio != 0 { "Radio" } else { "TV" },
        "Number": number_text(c.number),
        "ChannelNumber": number_text(c.number),
        "MediaType": if c.radio != 0 { "Audio" } else { "Video" },
        "IsFolder": false,
        "LocationType": "Remote",
        "ImageTags": if c.logo_url.is_some() { json!({ "Primary": format!("logo{}", c.id) }) } else { json!({}) },
        "BackdropImageTags": [],
        "UserData": { "PlaybackPositionTicks": 0, "PlayCount": 0, "IsFavorite": c.favorite != 0, "Played": false, "Key": id, "ItemId": id },
    });
    if let Some(p) = current {
        v["CurrentProgram"] = p;
    }
    v
}

#[derive(sqlx::FromRow)]
struct LtProgram {
    channel_id: i64,
    start_ms: i64,
    end_ms: i64,
    title: String,
    subtitle: Option<String>,
    description: Option<String>,
    category: Option<String>,
    season: Option<i64>,
    episode: Option<i64>,
    is_new: i64,
    rating: Option<String>,
    channel_name: String,
    channel_number: Option<f64>,
    logo_url: Option<String>,
}

const LT_PR: &str = "p.id, p.channel_id, p.start_ms, p.end_ms, p.title, p.subtitle, p.description, p.category, p.season, p.episode, p.is_new, p.rating,
  c.name AS channel_name, c.number AS channel_number, c.logo_url";

/// Id del programma per le app: canale e ora d'inizio, non la riga del database. L'aggiornamento della guida
/// riscrive le righe (id nuovi), e Swiftfin chiedeva ancora quelle vecchie (404).
fn program_key(channel_id: i64, start_ms: i64) -> i64 {
    (channel_id << 32) | (start_ms.div_euclid(1000) & 0xffff_ffff)
}

/// Riga del database di un id di programma (anche quelli vecchi, che erano la riga stessa).
async fn program_row(state: &Shared, key: i64) -> ApiResult<i64> {
    if key >> 32 == 0 {
        return Ok(key);
    }
    sqlx::query_scalar("SELECT id FROM tv_programs WHERE channel_id = ? AND start_ms >= ? AND start_ms < ?")
        .bind(key >> 32)
        .bind((key & 0xffff_ffff) * 1000)
        .bind((key & 0xffff_ffff) * 1000 + 1000)
        .fetch_optional(&state.db.read)
        .await?
        .ok_or_else(|| ApiError::not_found("Programma"))
}

struct ProgramKind {
    series: bool,
    news: bool,
    sports: bool,
    kids: bool,
    movie: bool,
}

/// Categorie di Jellyfin dalla categoria XMLTV.
fn program_kind(p: &LtProgram) -> ProgramKind {
    let cat = p.category.clone().unwrap_or_default().to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| cat.contains(w));
    ProgramKind {
        series: p.season.is_some() || p.episode.is_some() || has(&["serie", "series", "telefilm", "fiction", "soap"]),
        news: has(&["news", "notizi", "telegiornal", "tg"]),
        sports: has(&["sport", "calcio", "football", "soccer", "motori", "tennis", "basket"]),
        kids: has(&["kids", "bambin", "ragazz", "children", "animazion", "cartoni"]),
        movie: has(&["film", "movie", "cinema"]),
    }
}

fn program_value(state: &Shared, p: &LtProgram) -> Value {
    let id = guid(state, NS_PROGRAM, program_key(p.channel_id, p.start_ms));
    let k = program_kind(p);
    let now = crate::util::now_ms();
    json!({
        "Name": p.title,
        "EpisodeTitle": p.subtitle,
        "Overview": p.description,
        "ServerId": state.server_id,
        "Id": id,
        "Type": "Program",
        "ChannelId": guid(state, NS_CHANNEL, p.channel_id),
        "ChannelName": p.channel_name,
        "ChannelNumber": number_text(p.channel_number),
        "ChannelPrimaryImageTag": p.logo_url.as_ref().map(|_| format!("logo{}", p.channel_id)),
        "StartDate": jf_date(p.start_ms),
        "EndDate": jf_date(p.end_ms),
        "PremiereDate": jf_date(p.start_ms),
        "RunTimeTicks": (p.end_ms - p.start_ms) * TICKS_PER_MS,
        "ParentIndexNumber": p.season,
        "IndexNumber": p.episode,
        "Genres": p.category.iter().collect::<Vec<_>>(),
        "OfficialRating": p.rating,
        "IsSeries": k.series,
        "IsNews": k.news,
        "IsSports": k.sports,
        "IsKids": k.kids,
        "IsMovie": k.movie,
        "IsPremiere": p.is_new != 0,
        "IsLive": false,
        "IsNew": p.is_new != 0,
        "IsFolder": false,
        "MediaType": "Video",
        "LocationType": "Remote",
        "ImageTags": {},
        "BackdropImageTags": [],
        "UserData": { "PlaybackPositionTicks": 0, "PlayCount": 0, "IsFavorite": false, "Played": p.end_ms < now, "Key": id, "ItemId": id },
    })
}

async fn current_programs(state: &Shared, ids: &[i64]) -> ApiResult<HashMap<i64, Value>> {
    let mut out = HashMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    let now = crate::util::now_ms();
    for chunk in ids.chunks(900) {
        let l = chunk.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
        let rows: Vec<LtProgram> = sqlx::query_as(&format!(
            "SELECT {LT_PR} FROM tv_programs p JOIN tv_channels c ON c.id = p.channel_id WHERE p.channel_id IN ({l}) AND p.start_ms <= ?1 AND p.end_ms > ?1"
        ))
        .bind(now)
        .fetch_all(&state.db.read)
        .await?;
        for p in rows {
            out.insert(p.channel_id, program_value(state, &p));
        }
    }
    Ok(out)
}

/// Scheda completa del canale: con la sorgente, come Jellyfin. Swiftfin la legge prima di chiedere PlaybackInfo
/// e senza `MediaSources` non avvia la riproduzione ("Unable to find media source").
async fn lt_channel_dto(state: &Shared, user: &AuthUser, id: i64, current: bool) -> ApiResult<Value> {
    let c = lt_channels_rows(state, user, Some(id), false).await?.into_iter().next().ok_or_else(|| ApiError::not_found("Canale"))?;
    let cur = if current { current_programs(state, &[c.id]).await?.remove(&c.id) } else { None };
    let mut v = channel_value(state, &c, cur);
    if current {
        v["MediaSources"] = json!([lt_media_source(state, user, c.id, "").await?]);
    }
    Ok(v)
}

/// `/Items` per la Live TV: i canali (tipo `TvChannel`/`LiveTvChannel` o dentro la vista Live TV) e canali o
/// programmi chiesti per id. Swiftfin apre "Canali" e trova il canale di un programma così, non con `/LiveTv/Channels`.
async fn live_items(state: &Shared, user: &AuthUser, q: &ItemsQuery) -> ApiResult<Option<Value>> {
    let page = |items: Vec<Value>| {
        let total = items.len();
        let start = q.start_index.unwrap_or(0).max(0) as usize;
        let limit = q.limit.map(|l| l.max(0) as usize).unwrap_or(total);
        json!({ "Items": items.into_iter().skip(start).take(limit).collect::<Vec<_>>(), "TotalRecordCount": total, "StartIndex": start })
    };
    if let Some(ids) = &q.ids {
        let refs: Vec<Option<Ref>> = ids.split(',').map(|x| parse_ref(state, x.trim())).collect();
        let live = refs.iter().any(|r| matches!(r, Some(Ref::Channel(_)) | Some(Ref::Program(_))));
        let other = refs.iter().any(|r| matches!(r, Some(Ref::Item(_))));
        if !live || other {
            return Ok(None);
        }
        let mut items = Vec::new();
        for r in refs.into_iter().flatten() {
            let v = match r {
                Ref::Channel(c) => lt_channel_dto(state, user, c, true).await.ok(),
                Ref::Program(p) => lt_program_dto_by_id(state, user, p).await.ok(),
                _ => None,
            };
            items.extend(v);
        }
        return Ok(Some(page(items)));
    }
    let types: Vec<String> = q.include_item_types.as_deref().unwrap_or("").split(',').map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty()).collect();
    let wants_channels = types.iter().any(|t| t == "tvchannel" || t == "livetvchannel");
    let in_view = q.parent_id.as_deref().and_then(|p| parse_ref(state, p)).is_some_and(|r| matches!(r, Ref::LiveView));
    if !(wants_channels || in_view && types.is_empty()) {
        return Ok(None);
    }
    let mut rows = lt_channels_rows(state, user, None, q.is_favorite == Some(true)).await?;
    if let Some(term) = q.search_term.as_deref().map(str::to_lowercase) {
        rows.retain(|c| c.name.to_lowercase().contains(&term));
    }
    let cur = current_programs(state, &rows.iter().map(|c| c.id).collect::<Vec<_>>()).await?;
    Ok(Some(page(rows.iter().map(|c| channel_value(state, c, cur.get(&c.id).cloned())).collect())))
}

const PLACEHOLDER_MS: i64 = 3_600_000;

/// Programma segnaposto (canale senza guida): un'ora, col nome del canale. L'id è canale + inizio come gli altri.
fn placeholder_program(c: &LtChannel, start_ms: i64) -> LtProgram {
    LtProgram {
        channel_id: c.id,
        start_ms,
        end_ms: start_ms + PLACEHOLDER_MS,
        title: c.name.clone(),
        subtitle: None,
        description: None,
        category: None,
        season: None,
        episode: None,
        is_new: 0,
        rating: None,
        channel_name: c.name.clone(),
        channel_number: c.number,
        logo_url: c.logo_url.clone(),
    }
}

async fn lt_program_dto_by_id(state: &Shared, user: &AuthUser, key: i64) -> ApiResult<Value> {
    let id = match program_row(state, key).await {
        Ok(id) => id,
        // Segnaposto: si ricostruisce dall'id, se il canale è visibile e l'ora è allineata.
        Err(e) => {
            let start = (key & 0xffff_ffff) * 1000;
            let c = lt_channels_rows(state, user, Some(key >> 32), false).await?.into_iter().next();
            return match c {
                Some(c) if key >> 32 != 0 && start % PLACEHOLDER_MS == 0 => Ok(program_value(state, &placeholder_program(&c, start))),
                _ => Err(e),
            };
        }
    };
    let p: LtProgram = sqlx::query_as(&format!("SELECT {LT_PR} FROM tv_programs p JOIN tv_channels c ON c.id = p.channel_id WHERE p.id = ?"))
        .bind(id)
        .fetch_optional(&state.db.read)
        .await?
        .ok_or_else(|| ApiError::not_found("Programma"))?;
    lt_channels_rows(state, user, Some(p.channel_id), false).await?.into_iter().next().ok_or_else(|| ApiError::not_found("Programma"))?;
    Ok(program_value(state, &p))
}

async fn lt_info(State(state): State<Shared>, user: AuthUser) -> ApiResult<Json<Value>> {
    let on = lt_enabled(&state, &user).await;
    Ok(Json(json!({
        "Services": [{ "Name": "Platea", "HomePageUrl": "", "Status": "Ok", "StatusMessage": "", "Version": crate::state::VERSION, "HasUpdateAvailable": false, "IsVisible": true, "Tuners": [] }],
        "IsEnabled": on,
        "EnabledUsers": if on { vec![guid(&state, NS_USER, user.user_id)] } else { vec![] },
    })))
}

async fn lt_guide_info(State(state): State<Shared>, _user: AuthUser) -> ApiResult<Json<Value>> {
    let (min, max): (Option<i64>, Option<i64>) = sqlx::query_as("SELECT MIN(start_ms), MAX(end_ms) FROM tv_programs").fetch_one(&state.db.read).await?;
    let now = crate::util::now_ms();
    Ok(Json(json!({ "StartDate": jf_date(min.unwrap_or(now)), "EndDate": jf_date(max.unwrap_or(now)) })))
}

fn qmap(uri: &Uri) -> HashMap<String, String> {
    let mut q = HashMap::new();
    if let Some(qs) = uri.query() {
        for pair in qs.split('&') {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            let v = urlencoding::decode(v).map(|s| s.into_owned()).unwrap_or_default();
            // Swiftfin ripete il parametro per ogni valore (`channelIds=a&channelIds=b`): si uniscono con la virgola.
            q.entry(k.to_lowercase())
                .and_modify(|e: &mut String| {
                    if !v.is_empty() {
                        if !e.is_empty() {
                            e.push(',');
                        }
                        e.push_str(&v);
                    }
                })
                .or_insert(v);
        }
    }
    q
}

async fn lt_channels(State(state): State<Shared>, user: AuthUser, uri: Uri) -> ApiResult<Json<Value>> {
    let q = qmap(&uri);
    let fav = q.get("isfavorite").is_some_and(|v| v.eq_ignore_ascii_case("true"));
    let rows = lt_channels_rows(&state, &user, None, fav).await?;
    let total = rows.len();
    let start = q.get("startindex").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
    let limit = q.get("limit").and_then(|v| v.parse::<usize>().ok()).unwrap_or(total);
    let page: Vec<&LtChannel> = rows.iter().skip(start).take(limit).collect();
    let cur = if q.get("addcurrentprogram").is_none_or(|v| !v.eq_ignore_ascii_case("false")) {
        current_programs(&state, &page.iter().map(|c| c.id).collect::<Vec<_>>()).await?
    } else {
        HashMap::new()
    };
    let items: Vec<Value> = page.iter().map(|c| channel_value(&state, c, cur.get(&c.id).cloned())).collect();
    Ok(Json(json!({ "Items": items, "TotalRecordCount": total, "StartIndex": start })))
}

async fn lt_channel(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    match parse_ref(&state, &id) {
        Some(Ref::Channel(c)) => Ok(Json(lt_channel_dto(&state, &user, c, true).await?)),
        _ => Err(ApiError::not_found("Canale")),
    }
}

async fn lt_program(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    match parse_ref(&state, &id) {
        Some(Ref::Program(p)) => Ok(Json(lt_program_dto_by_id(&state, &user, p).await?)),
        _ => Err(ApiError::not_found("Programma")),
    }
}

/// Filtri dei programmi, dalla query (GET) o dal corpo (POST): stessi nomi di Jellyfin.
#[derive(Default)]
struct ProgramFilter {
    channels: Vec<i64>,
    min_start: Option<i64>,
    max_start: Option<i64>,
    min_end: Option<i64>,
    max_end: Option<i64>,
    is_airing: Option<bool>,
    has_aired: Option<bool>,
    /// IsMovie, IsSeries, IsNews, IsKids, IsSports: true li vuole, false li esclude (Swiftfin manda tutti e cinque).
    is_movie: Option<bool>,
    is_series: Option<bool>,
    is_news: Option<bool>,
    is_kids: Option<bool>,
    is_sports: Option<bool>,
    start: usize,
    limit: Option<usize>,
}

fn parse_jf_date(s: &str) -> Option<i64> {
    crate::livetv::dispatcharr::parse_iso(s)
}

async fn lt_programs(state: &Shared, user: &AuthUser, f: ProgramFilter) -> ApiResult<Json<Value>> {
    let visible = lt_channels_rows(state, user, None, false).await?;
    let allowed: Vec<i64> = visible.iter().map(|c| c.id).collect();
    let chans: Vec<i64> = if f.channels.is_empty() { allowed.clone() } else { f.channels.iter().copied().filter(|c| allowed.contains(c)).collect() };
    if chans.is_empty() {
        return Ok(Json(json!({ "Items": [], "TotalRecordCount": 0, "StartIndex": 0 })));
    }
    let now = crate::util::now_ms();
    let mut sql = format!(
        "SELECT {LT_PR} FROM tv_programs p JOIN tv_channels c ON c.id = p.channel_id WHERE p.channel_id IN ({})",
        chans.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",")
    );
    let mut binds: Vec<i64> = Vec::new();
    for (cond, v) in [("p.start_ms >= ?", f.min_start), ("p.start_ms <= ?", f.max_start), ("p.end_ms >= ?", f.min_end), ("p.end_ms <= ?", f.max_end)] {
        if let Some(v) = v {
            sql.push_str(&format!(" AND {cond}"));
            binds.push(v);
        }
    }
    match f.is_airing {
        Some(true) => {
            sql.push_str(" AND p.start_ms <= ? AND p.end_ms > ?");
            binds.extend([now, now]);
        }
        Some(false) => {
            sql.push_str(" AND NOT (p.start_ms <= ? AND p.end_ms > ?)");
            binds.extend([now, now]);
        }
        None => {}
    }
    match f.has_aired {
        Some(true) => {
            sql.push_str(" AND p.end_ms <= ?");
            binds.push(now);
        }
        Some(false) => {
            sql.push_str(" AND p.end_ms > ?");
            binds.push(now);
        }
        None => {}
    }
    // Senza limiti di tempo: da adesso a 24 ore (le guide intere sarebbero enormi).
    if f.min_start.is_none() && f.max_start.is_none() && f.min_end.is_none() && f.max_end.is_none() && f.is_airing.is_none() && f.has_aired.is_none() {
        sql.push_str(" AND p.end_ms > ? AND p.start_ms < ?");
        binds.extend([now, now + 86_400_000]);
    }
    sql.push_str(" ORDER BY p.start_ms, c.number LIMIT 20000");
    let mut q = sqlx::query_as::<_, LtProgram>(&sql);
    for b in &binds {
        q = q.bind(b);
    }
    let mut rows = q.fetch_all(&state.db.read).await?;
    // Canali senza guida in questo intervallo: programmi segnaposto di un'ora col nome del canale. Le app (Swiftfin)
    // mostrano solo programmi: senza, il canale non compare in "In onda" né nella guida.
    if f.has_aired != Some(true) && f.is_airing != Some(false) {
        let with_guide: std::collections::HashSet<i64> = rows.iter().map(|p| p.channel_id).collect();
        let (lo, hi) = if f.is_airing == Some(true) {
            (now, now + 1)
        } else {
            let lo = f.min_end.or(f.min_start).unwrap_or(now);
            let hi = f.max_start.or(f.max_end).unwrap_or(lo + 86_400_000);
            (lo, hi.min(lo + 7 * 86_400_000))
        };
        for c in visible.iter().filter(|c| chans.contains(&c.id) && !with_guide.contains(&c.id)) {
            let mut start = lo.div_euclid(PLACEHOLDER_MS) * PLACEHOLDER_MS;
            while start < hi {
                if f.has_aired != Some(false) || start + PLACEHOLDER_MS > now {
                    rows.push(placeholder_program(c, start));
                }
                start += PLACEHOLDER_MS;
            }
        }
        rows.sort_by(|a, b| a.start_ms.cmp(&b.start_ms).then(a.channel_number.partial_cmp(&b.channel_number).unwrap_or(std::cmp::Ordering::Equal)));
    }
    rows.retain(|p| {
        let k = program_kind(p);
        [(f.is_movie, k.movie), (f.is_series, k.series), (f.is_news, k.news), (f.is_kids, k.kids), (f.is_sports, k.sports)]
            .iter()
            .all(|(want, is)| want.is_none_or(|w| w == *is))
    });
    let total = rows.len();
    let items: Vec<Value> = rows.iter().skip(f.start).take(f.limit.unwrap_or(total)).map(|p| program_value(state, p)).collect();
    Ok(Json(json!({ "Items": items, "TotalRecordCount": total, "StartIndex": f.start })))
}

fn channel_ids(state: &Shared, list: impl Iterator<Item = String>) -> Vec<i64> {
    list.filter_map(|x| match parse_ref(state, x.trim()) {
        Some(Ref::Channel(c)) => Some(c),
        _ => None,
    })
    .collect()
}

async fn lt_programs_q(State(state): State<Shared>, user: AuthUser, uri: Uri) -> ApiResult<Json<Value>> {
    let q = qmap(&uri);
    let b = |k: &str| q.get(k).map(|v| v.eq_ignore_ascii_case("true"));
    let f = ProgramFilter {
        channels: channel_ids(&state, q.get("channelids").map(|v| v.split(',').map(String::from).collect::<Vec<_>>()).unwrap_or_default().into_iter()),
        min_start: q.get("minstartdate").and_then(|v| parse_jf_date(v)),
        max_start: q.get("maxstartdate").and_then(|v| parse_jf_date(v)),
        min_end: q.get("minenddate").and_then(|v| parse_jf_date(v)),
        max_end: q.get("maxenddate").and_then(|v| parse_jf_date(v)),
        is_airing: b("isairing"),
        has_aired: b("hasaired"),
        is_movie: b("ismovie"),
        is_series: b("isseries"),
        is_news: b("isnews"),
        is_kids: b("iskids"),
        is_sports: b("issports"),
        start: q.get("startindex").and_then(|v| v.parse().ok()).unwrap_or(0),
        limit: q.get("limit").and_then(|v| v.parse().ok()),
    };
    lt_programs(&state, &user, f).await
}

async fn lt_programs_body(State(state): State<Shared>, user: AuthUser, body: axum::body::Bytes) -> ApiResult<Json<Value>> {
    let v: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let d = |k: &str| v[k].as_str().and_then(parse_jf_date);
    let f = ProgramFilter {
        channels: channel_ids(&state, v["ChannelIds"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from))),
        min_start: d("MinStartDate"),
        max_start: d("MaxStartDate"),
        min_end: d("MinEndDate"),
        max_end: d("MaxEndDate"),
        is_airing: v["IsAiring"].as_bool(),
        has_aired: v["HasAired"].as_bool(),
        is_movie: v["IsMovie"].as_bool(),
        is_series: v["IsSeries"].as_bool(),
        is_news: v["IsNews"].as_bool(),
        is_kids: v["IsKids"].as_bool(),
        is_sports: v["IsSports"].as_bool(),
        start: v["StartIndex"].as_u64().unwrap_or(0) as usize,
        limit: v["Limit"].as_u64().map(|l| l as usize),
    };
    lt_programs(&state, &user, f).await
}

async fn lt_recommended(State(state): State<Shared>, user: AuthUser, uri: Uri) -> ApiResult<Json<Value>> {
    let q = qmap(&uri);
    let f = ProgramFilter {
        is_airing: Some(q.get("isairing").is_none_or(|v| v.eq_ignore_ascii_case("true"))),
        limit: q.get("limit").and_then(|v| v.parse().ok()),
        ..Default::default()
    };
    lt_programs(&state, &user, f).await
}

/// Flusso MPEG-TS del canale: una sessione dal vivo diretta (visibile nel pannello), poi il flusso attraverso il server.
async fn lt_stream(state: &Shared, user: &AuthUser, channel: i64) -> ApiResult<Response> {
    let existing = state
        .live
        .sessions()
        .into_iter()
        .find(|s| s.user_id == user.user_id && s.token_id == user.token_id && s.channel_id == channel && s.direct_url.is_some());
    let (sid, token) = match existing {
        Some(s) => (s.id.clone(), s.token.clone()),
        None => {
            let v = crate::api::livetv::start(state, user, channel, crate::api::livetv::PlayReq { direct: true, ..Default::default() }).await?;
            (v["session_id"].as_str().unwrap_or_default().to_string(), v["stream_token"].as_str().unwrap_or_default().to_string())
        }
    };
    let s = state.live.get(&sid).ok_or_else(|| ApiError::not_found("Sessione"))?;
    let _ = token;
    let url = s.direct_url.clone().ok_or_else(|| ApiError::not_found("Canale"))?;
    let guard = crate::livetv::stream::LiveManager::open_direct(state, s.source_id);
    let resp = state
        .http
        .get(&url)
        .header("User-Agent", format!("Platea/{}", crate::state::VERSION))
        .send()
        .await
        .map_err(|e| ApiError::coded(StatusCode::BAD_GATEWAY, "ChannelUnavailable", format!("Canale non raggiungibile: {e}")))?;
    if !resp.status().is_success() {
        return Err(ApiError::coded(StatusCode::BAD_GATEWAY, "ChannelUnavailable", format!("La sorgente ha risposto {}", resp.status())));
    }
    use futures_util::StreamExt;
    let counter = s.bytes_sent.clone();
    let sess = s.clone();
    let body = resp.bytes_stream().map(move |chunk| {
        let _keep = &guard;
        if let Ok(c) = &chunk {
            counter.fetch_add(c.len() as u64, std::sync::atomic::Ordering::Relaxed);
            sess.touch();
        }
        chunk.map_err(std::io::Error::other)
    });
    Ok(Response::builder().header(header::CONTENT_TYPE, "video/mp2t").header(header::CACHE_CONTROL, "no-store").body(Body::from_stream(body)).unwrap())
}

/// Sorgente di un canale per PlaybackInfo: MPEG-TS infinito, letto dal client così com'è.
async fn lt_media_source(state: &Shared, user: &AuthUser, channel: i64, play_session: &str) -> ApiResult<Value> {
    if !user.perms.allow_livetv {
        return Err(ApiError::Forbidden("Live TV non abilitata".into()));
    }
    let c = lt_channels_rows(state, user, Some(channel), false).await?.into_iter().next().ok_or_else(|| ApiError::not_found("Canale"))?;
    let codecs: Option<String> = sqlx::query_scalar("SELECT codecs FROM tv_channels WHERE id = ?").bind(c.id).fetch_optional(&state.db.read).await?.flatten();
    let codecs: Value = codecs.as_deref().and_then(|x| serde_json::from_str(x).ok()).unwrap_or(json!({}));
    let id = guid(state, NS_CHANNEL, c.id);
    let url = format!("/Videos/{id}/stream.ts?static=true&MediaSourceId={id}&PlaySessionId={play_session}");
    let mut streams = Vec::new();
    if let Some(v) = codecs["video"].as_str() {
        streams.push(json!({ "Type": "Video", "Codec": v, "Index": 0, "Width": codecs["width"], "Height": codecs["height"], "IsInterlaced": codecs["interlaced"].as_bool().unwrap_or(false),
            "IsDefault": true, "IsForced": false, "IsExternal": false, "IsTextSubtitleStream": false, "SupportsExternalStream": false, "VideoRange": "SDR", "VideoRangeType": "SDR",
            "AudioSpatialFormat": "None", "IsHearingImpaired": false, "IsOriginal": true, "IsAVC": v == "h264", "IsAnamorphic": false, "Level": 0.0 }));
    }
    if let Some(a) = codecs["audio"].as_str() {
        streams.push(
            json!({ "Type": "Audio", "Codec": a, "Index": 1, "Channels": codecs["channels"], "IsDefault": true, "IsForced": false, "IsExternal": false,
            "IsTextSubtitleStream": false, "SupportsExternalStream": false, "IsInterlaced": false, "VideoRange": "Unknown", "VideoRangeType": "Unknown",
            "AudioSpatialFormat": "None", "IsHearingImpaired": false, "IsOriginal": true, "Level": 0.0 }),
        );
    }
    Ok(json!({
        "Protocol": "Http",
        "Id": id,
        "Path": url,
        "Type": "Default",
        "Container": "ts",
        "Name": c.name,
        "IsRemote": true,
        "ETag": id,
        "RunTimeTicks": null,
        "ReadAtNativeFramerate": false,
        "IgnoreDts": true,
        "IgnoreIndex": true,
        "GenPtsInput": true,
        "SupportsTranscoding": false,
        "SupportsDirectStream": true,
        "SupportsDirectPlay": true,
        "IsInfiniteStream": true,
        "UseMostCompatibleTranscodingProfile": false,
        "RequiresOpening": false,
        "RequiresClosing": false,
        "RequiresLooping": false,
        "SupportsProbing": true,
        "MediaStreams": streams,
        "MediaAttachments": [],
        "Formats": [],
        "RequiredHttpHeaders": {},
        "DirectStreamUrl": url,
        "TranscodingSubProtocol": "http",
        "HasSegments": false,
        "AnalyzeDurationMs": 3000,
        "DefaultAudioStreamIndex": 1,
    }))
}

// Timer: le registrazioni programmate con Dispatcharr.

#[derive(sqlx::FromRow)]
struct LtTimer {
    id: i64,
    kind: String,
    title: String,
    start_ms: i64,
    end_ms: i64,
    status: String,
    channel_id: Option<i64>,
    channel_name: Option<String>,
}

fn timer_value(state: &Shared, t: &LtTimer) -> Value {
    let id = guid(state, NS_TIMER, t.id);
    json!({
        "Id": id,
        "Type": if t.kind == "series" { "SeriesTimer" } else { "Timer" },
        "ServerId": state.server_id,
        "ExternalId": id,
        "ChannelId": t.channel_id.map(|c| guid(state, NS_CHANNEL, c)),
        "ChannelName": t.channel_name,
        "Name": t.title,
        "StartDate": jf_date(t.start_ms),
        "EndDate": jf_date(t.end_ms),
        "Status": match t.status.as_str() { "recording" => "InProgress", "completed" => "Completed", "stopped" | "interrupted" => "Cancelled", "failed" => "Error", _ => "New" },
        "PrePaddingSeconds": 0,
        "PostPaddingSeconds": 0,
        "IsPrePaddingRequired": false,
        "IsPostPaddingRequired": false,
        "KeepUntil": "UntilDeleted",
        "Priority": 0,
        "RecordAnyTime": false,
        "RecordAnyChannel": false,
        "SkipEpisodesInLibrary": false,
        "RecordNewOnly": false,
        "Days": [],
        "DayPattern": "Daily",
        "KeepUpTo": 0,
        "ParentPrimaryImageItemId": t.channel_id.map(|c| guid(state, NS_CHANNEL, c)),
    })
}

async fn timers_of(state: &Shared, user: &AuthUser, kind: &str) -> ApiResult<Vec<LtTimer>> {
    Ok(sqlx::query_as(
        "SELECT r.id, r.kind, r.title, r.start_ms, r.end_ms, r.status, r.channel_id, c.name AS channel_name FROM tv_recordings r LEFT JOIN tv_channels c ON c.id = r.channel_id
         WHERE r.kind = ? AND r.status NOT IN ('deleted') AND (r.user_id = ? OR r.shared = 1 OR ?) AND (? = 'series' OR r.end_ms > ?) ORDER BY r.start_ms",
    )
    .bind(kind)
    .bind(user.user_id)
    .bind(user.is_admin)
    .bind(kind)
    .bind(crate::util::now_ms())
    .fetch_all(&state.db.read)
    .await?)
}

async fn lt_timers(State(state): State<Shared>, user: AuthUser) -> ApiResult<Json<Value>> {
    let list = timers_of(&state, &user, "once").await?;
    let n = list.len();
    Ok(Json(json!({ "Items": list.iter().map(|t| timer_value(&state, t)).collect::<Vec<_>>(), "TotalRecordCount": n, "StartIndex": 0 })))
}

async fn lt_series_timers(State(state): State<Shared>, user: AuthUser) -> ApiResult<Json<Value>> {
    let list = timers_of(&state, &user, "series").await?;
    let n = list.len();
    Ok(Json(json!({ "Items": list.iter().map(|t| timer_value(&state, t)).collect::<Vec<_>>(), "TotalRecordCount": n, "StartIndex": 0 })))
}

async fn lt_timer(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let Some(Ref::Timer(t)) = parse_ref(&state, &id) else { return Err(ApiError::not_found("Timer")) };
    let all: Vec<LtTimer> = [timers_of(&state, &user, "once").await?, timers_of(&state, &user, "series").await?].into_iter().flatten().collect();
    let t = all.iter().find(|x| x.id == t).ok_or_else(|| ApiError::not_found("Timer"))?;
    Ok(Json(timer_value(&state, t)))
}

async fn lt_timer_defaults(State(state): State<Shared>, user: AuthUser, uri: Uri) -> ApiResult<Json<Value>> {
    let q = qmap(&uri);
    let program = q.get("programid").and_then(|p| match parse_ref(&state, p) {
        Some(Ref::Program(x)) => Some(x),
        _ => None,
    });
    let mut v = json!({ "PrePaddingSeconds": 0, "PostPaddingSeconds": 0, "IsPrePaddingRequired": false, "IsPostPaddingRequired": false, "KeepUntil": "UntilDeleted",
        "RecordAnyTime": false, "RecordAnyChannel": false, "RecordNewOnly": false, "SkipEpisodesInLibrary": false, "Days": [], "DayPattern": "Daily", "KeepUpTo": 0, "Priority": 0 });
    if let Some(p) = program {
        let dto = lt_program_dto_by_id(&state, &user, p).await?;
        v["ProgramId"] = json!(guid(&state, NS_PROGRAM, p));
        v["ChannelId"] = dto["ChannelId"].clone();
        v["Name"] = dto["Name"].clone();
        v["StartDate"] = dto["StartDate"].clone();
        v["EndDate"] = dto["EndDate"].clone();
    }
    Ok(Json(v))
}

async fn create_from_body(state: &Shared, user: &AuthUser, _parts: &axum::http::request::Parts, body: &[u8], series: bool) -> ApiResult<StatusCode> {
    let v: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    let Some(Ref::Program(p)) = v["ProgramId"].as_str().and_then(|p| parse_ref(state, p)) else {
        return Err(ApiError::BadRequest("ProgramId mancante".into()));
    };
    let p = program_row(state, p).await?;
    let req = json!({ "program_id": state.ids.encode(p), "series": series, "only_new": v["RecordNewOnly"].as_bool().unwrap_or(false) });
    let _ = crate::api::livetv::record_for(state, user, req).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn lt_timer_create(State(state): State<Shared>, user: AuthUser, parts: axum::http::request::Parts, body: axum::body::Bytes) -> ApiResult<StatusCode> {
    create_from_body(&state, &user, &parts, &body, false).await
}

async fn lt_series_create(State(state): State<Shared>, user: AuthUser, parts: axum::http::request::Parts, body: axum::body::Bytes) -> ApiResult<StatusCode> {
    create_from_body(&state, &user, &parts, &body, true).await
}

async fn lt_timer_delete(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>) -> ApiResult<StatusCode> {
    let Some(Ref::Timer(t)) = parse_ref(&state, &id) else { return Err(ApiError::not_found("Timer")) };
    let _ = crate::api::livetv::unrecord_for(&state, &user, t).await?;
    Ok(StatusCode::NO_CONTENT)
}
