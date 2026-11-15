//! Riproduzione. Il client chiama `POST /api/v1/playback/info` con il proprio profilo dispositivo;
//! il server sceglie il metodo meno costoso con dati già nel database e risponde con l'URL.

use super::{Access, Api};
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::sessions::{stop_session, PlaySession, SessionManager};
use crate::state::Shared;
use crate::transcode::command::{plan_segments, JobSpec, SourceVideo};
use crate::transcode::HlsError;
use crate::util::now_ms;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use platea_playback::{self as pb, AudioInfo, DecisionError, DeviceProfile, MediaInfo, PlaybackRequest, SubtitleInfo, VideoAction, VideoInfo};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

pub fn routes(api: Api) -> Api {
    api.post("/api/v1/playback/info", "Riproduzione", Access::User, "Decisione di riproduzione e apertura della sessione", info)
        .post("/api/v1/playback/{session}/progress", "Riproduzione", Access::User, "Posizione corrente (ogni 10 s)", progress)
        .post("/api/v1/playback/{session}/tracks", "Riproduzione", Access::User, "Tracce scelte nel player: ricordate per il film o per tutta la serie", tracks)
        .post("/api/v1/playback/{session}/client-error", "Riproduzione", Access::User, "Errore del player (nel log del server, per la diagnosi)", client_error)
        .post("/api/v1/playback/{session}/stop", "Riproduzione", Access::User, "Fine della riproduzione", stop)
        .get("/api/v1/stream/{session}", "Riproduzione", Access::Stream, "Direct play: file originale con richieste Range", direct)
        .get("/api/v1/hls/{session}/{file}", "Riproduzione", Access::Stream, "Playlist HLS (main.m3u8) e segmenti (N.ts)", hls)
        .get("/api/v1/subtitles/{session}/{file}", "Riproduzione", Access::Stream, "Sottotitolo in WebVTT (INDICE.vtt)", subtitle)
        .get("/api/v1/trickplay/{session}/{file}", "Riproduzione", Access::Stream, "Griglia di miniature per la barra di avanzamento (N.jpg)", trickplay)
}

#[derive(Deserialize)]
pub struct InfoReq {
    pub item_id: String,
    #[serde(default)]
    pub file_id: Option<String>,
    #[serde(default)]
    pub profile: DeviceProfile,
    #[serde(default)]
    pub audio_index: Option<u32>,
    #[serde(default)]
    pub subtitle_index: Option<u32>,
    #[serde(default)]
    pub start_ms: Option<i64>,
    #[serde(default)]
    pub max_bitrate: Option<u64>,
    #[serde(default)]
    pub force_transcode: bool,
    /// Lingua dell'interfaccia del client (es. "it"): per i sottotitoli quando l'utente non ne ha scelta una.
    #[serde(default)]
    pub ui_language: Option<String>,
    /// Qualità scelta a mano nel player (anche "originale"): la qualità predefinita delle preferenze non si applica.
    #[serde(default)]
    pub explicit_quality: bool,
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
    bitrate: Option<i64>,
    language: Option<String>,
    title: Option<String>,
    is_default: i64,
    is_forced: i64,
    is_external: i64,
    external_path: Option<String>,
}

pub struct FileInfo {
    pub id: i64,
    pub abs: PathBuf,
    /// Indirizzo http(s) di un file `.strm`: FFmpeg legge da lì, direct play e download rimandano lì.
    pub url: Option<String>,
    pub root: PathBuf,
    /// Cartella della libreria (`library_paths.id`) e percorso relativo: per trovare le copie sui nodi.
    pub library_path_id: i64,
    pub rel: String,
    pub size: i64,
    pub mtime: i64,
    pub media: MediaInfo,
    pub streams: Vec<(i64, String, Option<String>, Option<String>, bool, Option<String>, bool, bool)>,
}

impl FileInfo {
    /// Ingresso per FFmpeg: il file, oppure l'indirizzo del `.strm`.
    pub fn input(&self) -> PathBuf {
        self.url.as_ref().map(PathBuf::from).unwrap_or_else(|| self.abs.clone())
    }
}

/// Dati tecnici di un file dal database: lo storage non viene toccato.
pub async fn load_file(state: &Shared, file_id: i64) -> ApiResult<FileInfo> {
    let row: Option<(String, String, i64, i64, Option<String>, Option<i64>, Option<i64>, String, Option<String>, i64)> = sqlx::query_as(
        "SELECT p.path, f.relative_path, f.size, f.mtime, f.container, f.bitrate, f.duration_ms, f.state, f.stream_url, p.id FROM media_files f
         JOIN library_paths p ON p.id = f.library_path_id WHERE f.id = ?",
    )
    .bind(file_id)
    .fetch_optional(&state.db.read)
    .await?;
    let Some((root, rel, size, mtime, container, bitrate, duration, st, url, library_path_id)) = row else { return Err(ApiError::not_found("File")) };
    if st != "present" {
        return Err(ApiError::coded(StatusCode::CONFLICT, "FileMissing", "Il file non è disponibile: il mount potrebbe essere assente"));
    }
    if rel.split('/').any(|p| p == "..") {
        return Err(ApiError::Forbidden("Percorso non valido".into()));
    }
    let streams: Vec<StreamRow> = sqlx::query_as(
        "SELECT stream_index, kind, codec, profile, level, bit_depth, width, height, hdr, channels, bitrate, language, title, is_default, is_forced, is_external, external_path
         FROM media_streams WHERE file_id = ? ORDER BY stream_index",
    )
    .bind(file_id)
    .fetch_all(&state.db.read)
    .await?;
    let mut media = MediaInfo {
        container: container.unwrap_or_default(),
        bitrate: bitrate.map(|b| b as u64),
        duration_ms: duration.map(|d| d as u64),
        ..Default::default()
    };
    let mut subs_meta = Vec::new();
    for s in &streams {
        match s.kind.as_str() {
            "video" if media.video.is_none() => {
                media.video = Some(VideoInfo {
                    index: s.stream_index as u32,
                    codec: s.codec.clone(),
                    profile: s.profile.clone(),
                    level: s.level.map(|l| l as u32),
                    bit_depth: s.bit_depth.map(|b| b as u8),
                    width: s.width.unwrap_or(0) as u32,
                    height: s.height.unwrap_or(0) as u32,
                    hdr: s.hdr.clone(),
                    bitrate: s.bitrate.map(|b| b as u64),
                })
            }
            "audio" => media.audio.push(AudioInfo {
                index: s.stream_index as u32,
                codec: s.codec.clone(),
                channels: s.channels.unwrap_or(2) as u32,
                language: s.language.clone(),
                default: s.is_default != 0,
                bitrate: s.bitrate.map(|b| b as u64),
            }),
            "subtitle" => {
                media.subtitles.push(SubtitleInfo {
                    index: s.stream_index as u32,
                    codec: s.codec.clone(),
                    language: s.language.clone(),
                    forced: s.is_forced != 0,
                    default: s.is_default != 0,
                    external: s.is_external != 0,
                });
                subs_meta.push((
                    s.stream_index,
                    s.codec.clone(),
                    s.language.clone(),
                    s.title.clone(),
                    s.is_external != 0,
                    s.external_path.clone(),
                    s.is_forced != 0,
                    s.is_default != 0,
                ));
            }
            _ => {}
        }
    }
    let root = PathBuf::from(root);
    Ok(FileInfo { id: file_id, abs: root.join(&rel), url, root, library_path_id, rel, size, mtime, media, streams: subs_meta })
}

fn codec_label(c: &str) -> String {
    match c {
        "h264" => "H.264".into(),
        "hevc" => "HEVC".into(),
        "av1" => "AV1".into(),
        "vp9" => "VP9".into(),
        "mpeg4" => "MPEG-4".into(),
        "mpeg2video" => "MPEG-2".into(),
        "aac" => "AAC".into(),
        "ac3" => "AC3".into(),
        "eac3" => "E-AC3".into(),
        "dts" => "DTS".into(),
        "truehd" => "TrueHD".into(),
        "flac" => "FLAC".into(),
        "opus" => "Opus".into(),
        other => other.to_uppercase(),
    }
}

/// Riga tecnica: descrive la catena e cosa il server ha cambiato (`→`).
pub fn chain_text(media: &MediaInfo, d: &pb::Decision, backend: &str) -> String {
    let v = media.video.as_ref();
    let src_v = v.map(|v| {
        let mut s = codec_label(&v.codec);
        if v.bit_depth.unwrap_or(8) > 8 {
            s.push_str(&format!(" {} bit", v.bit_depth.unwrap()));
        }
        if let Some(h) = &v.hdr {
            s.push_str(&format!(" {}", h.to_uppercase()));
        }
        s
    });
    let audio = d.audio_index.and_then(|i| media.audio.iter().find(|a| a.index == i));
    let a_src =
        audio.map(|a| format!("{} {}.{}", codec_label(&a.codec), if a.channels > 2 { a.channels - 1 } else { a.channels }, if a.channels > 2 { 1 } else { 0 }));
    match d.method {
        pb::Method::DirectPlay => {
            let mut parts = vec![media.container.to_uppercase()];
            parts.extend(src_v);
            parts.extend(a_src);
            parts.join(" · ")
        }
        pb::Method::Remux => {
            let mut s = format!("{} → HLS", media.container.to_uppercase());
            if let Some(v) = src_v {
                s.push_str(&format!(" · {v} copiato"));
            }
            match &d.audio {
                pb::AudioAction::Transcode { channels, .. } => s.push_str(&format!(" · {} → AAC {}ch", a_src.unwrap_or_default(), channels)),
                pb::AudioAction::Copy => s.push_str(&format!(" · {}", a_src.unwrap_or_default())),
                _ => {}
            }
            s
        }
        pb::Method::Transcode => {
            let (out, h) = match &d.video {
                // Altezza reale d'uscita, non il limite: 3840×1606 entro 1920×1080 esce a 802p.
                VideoAction::Transcode { codec, max_width, max_height, .. } => {
                    let h = media
                        .video
                        .as_ref()
                        .map(|v| crate::transcode::command::output_size(v.width, v.height, *max_width, *max_height).1)
                        .unwrap_or(*max_height);
                    (codec_label(codec), h)
                }
                _ => ("?".into(), 0),
            };
            let mut s = format!("{} → {out} {h}p · {backend}", src_v.unwrap_or_default());
            if let pb::AudioAction::Transcode { channels, .. } = &d.audio {
                s.push_str(&format!(" · {} → AAC {}ch", a_src.unwrap_or_default(), channels));
            }
            s
        }
    }
}

fn decision_error(e: DecisionError) -> ApiError {
    match e {
        DecisionError::NotProbed => ApiError::coded(StatusCode::CONFLICT, "NotProbed", "Il file non è ancora stato analizzato"),
        DecisionError::RemuxNotAllowed(r) => ApiError::coded(
            StatusCode::FORBIDDEN,
            "RemuxNotAllowed",
            format!("Serve un remux ({}) ma l'amministratore non lo consente per questo account", r.iter().map(|x| x.as_str()).collect::<Vec<_>>().join(", ")),
        ),
        DecisionError::TranscodeNotAllowed(r) => ApiError::coded(
            StatusCode::FORBIDDEN,
            "TranscodeNotAllowed",
            format!("Serve una transcodifica ({}) ma non è consentita per questo account", r.iter().map(|x| x.as_str()).collect::<Vec<_>>().join(", ")),
        ),
        DecisionError::NoProtocol => ApiError::BadRequest("Il profilo non dichiara nessun protocollo (file o hls)".into()),
    }
}

async fn info(State(state): State<Shared>, user: AuthUser, Json(req): Json<InfoReq>) -> ApiResult<Json<Value>> {
    Ok(Json(start(state, user, req).await?))
}

/// Decisione e apertura della sessione: usata dalla web UI e dallo strato compatibile Jellyfin (transcodifica).
pub async fn start(state: Shared, user: AuthUser, req: InfoReq) -> ApiResult<Value> {
    user.require_playback()?;
    let item_id = state.ids.decode(&req.item_id).ok_or_else(|| ApiError::not_found("Elemento"))?;
    let item = super::items::load_item(&state, &user, item_id).await?;
    if item.kind != "movie" && item.kind != "episode" {
        return Err(ApiError::BadRequest("Si riproducono film ed episodi".into()));
    }
    // Un dispositivo riproduce una cosa alla volta: le sessioni precedenti liberano subito FFmpeg e i posti.
    for old in state.sessions.by_device(user.token_id) {
        stop_session(&state, &old).await;
    }
    if let Some(max) = user.perms.max_streams {
        if state.sessions.user_count(user.user_id) as i64 >= max {
            return Err(ApiError::coded(
                StatusCode::TOO_MANY_REQUESTS,
                "TooManyStreams",
                format!("Questo account può avere al massimo {max} riproduzioni contemporanee"),
            ));
        }
    }
    let files: Vec<i64> = if let Some(f) = &req.file_id {
        let fid = state.ids.decode(f).ok_or_else(|| ApiError::not_found("File"))?;
        let ok: Option<i64> =
            // Una versione del titolo o uno dei suoi extra.
            sqlx::query_scalar("SELECT id FROM media_files WHERE id = ?1 AND (item_id = ?2 OR extra_of = ?2)").bind(fid).bind(item_id).fetch_optional(&state.db.read).await?;
        vec![ok.ok_or_else(|| ApiError::not_found("File"))?]
    } else {
        sqlx::query_scalar("SELECT id FROM media_files WHERE item_id = ? AND state = 'present' ORDER BY size DESC")
            .bind(item_id)
            .fetch_all(&state.db.read)
            .await?
    };
    if files.is_empty() {
        return Err(ApiError::coded(StatusCode::CONFLICT, "FileMissing", "Nessun file disponibile per questo elemento"));
    }

    let s = state.settings.get();
    let mut prefs = crate::prefs::load(&state, user.user_id).await;
    // Limite di bitrate: utente, client, eventualmente da remoto, qualità predefinita dell'utente.
    let mut user_max = user.perms.max_bitrate.map(|b| b as u64);
    let remote = user.ip.is_some_and(|ip| !crate::net::in_any(&s.network.local_networks, ip));
    let remote_max = s.network.remote_max_bitrate_mbps as u64 * 1_000_000;
    if remote && remote_max > 0 {
        user_max = Some(user_max.map_or(remote_max, |u| u.min(remote_max)));
    }
    let pref_max = if remote { prefs.max_bitrate_remote } else { prefs.max_bitrate_local };
    if !req.explicit_quality && pref_max > 0 {
        user_max = Some(user_max.map_or(pref_max, |u| u.min(pref_max)));
    }
    if let Some(m) = req.max_bitrate.filter(|m| *m > 0) {
        user_max = Some(user_max.map_or(m, |u| u.min(m)));
    }
    let preq = PlaybackRequest {
        audio_index: req.audio_index,
        subtitle_index: req.subtitle_index,
        allow_remux: user.perms.allow_remux,
        allow_transcode: user.perms.allow_transcode && (state.ffmpeg.get().available || crate::nodes::any_transcoder_online(&state)),
        user_max_bitrate: user_max,
        server_video_encoders: decision_encoders(&state),
        force_transcode: req.force_transcode,
    };

    // Preferenze dell'utente: lingua audio (per file, gli indici cambiano tra versioni) e sottotitoli suggeriti.
    // Le tracce scelte l'ultima volta per questo film o per questa serie vincono sulle preferenze generali.
    let mut subtitles_off = false;
    if prefs.remember_tracks {
        let key = if item.kind == "episode" { item.series_id.unwrap_or(item_id) } else { item_id };
        let choice: Option<(Option<String>, Option<String>, i64)> =
            sqlx::query_as("SELECT audio_language, subtitle_language, subtitle_off FROM track_choices WHERE user_id = ? AND item_id = ?")
                .bind(user.user_id)
                .bind(key)
                .fetch_optional(&state.db.read)
                .await?;
        if let Some((audio, sub, off)) = choice {
            if let Some(a) = audio.filter(|a| !a.is_empty()) {
                prefs.audio_language = a;
            }
            if off != 0 {
                subtitles_off = true;
            } else if let Some(s) = sub.filter(|s| !s.is_empty()) {
                prefs.subtitle_language = s;
                prefs.subtitle_mode = crate::prefs::SubtitleMode::Always;
            }
        }
    }
    // Più versioni: si preferisce quella riproducibile in direct play, poi in remux.
    let mut best: Option<(FileInfo, pb::Decision)> = None;
    let mut last_err = None;
    for fid in files {
        if let Err(e) = crate::probe::ensure_probed(&state, fid).await {
            last_err = Some(ApiError::coded(StatusCode::CONFLICT, "ProbeFailed", format!("Analisi del file non riuscita: {e:#}")));
            continue;
        }
        let f = load_file(&state, fid).await?;
        let preq = PlaybackRequest { audio_index: req.audio_index.or_else(|| crate::prefs::pick_audio(&f.media.audio, &prefs)), ..preq.clone() };
        match pb::decide(&f.media, &req.profile, &preq) {
            Ok(d) => {
                let better = best.as_ref().is_none_or(|(_, b)| crate::sessions::method_rank(d.method) > crate::sessions::method_rank(b.method));
                let direct = d.method == pb::Method::DirectPlay;
                if better {
                    best = Some((f, d));
                }
                if direct {
                    break;
                }
            }
            Err(e) => last_err = Some(decision_error(e)),
        }
    }
    let Some((file, mut decision)) = best else { return Err(last_err.unwrap_or_else(|| ApiError::not_found("File"))) };

    let (session_id, token) = SessionManager::new_ids();
    let duration_ms = file.media.duration_ms.map(|d| d as i64).or(item.runtime_ms).unwrap_or(0);
    let mut hls: Option<Arc<crate::nodes::hls::HlsHandle>> = None;
    let mut backend_label = String::from("Software");
    let mut executor = String::new();

    if decision.method != pb::Method::DirectPlay {
        let seg_ms = s.playback.segment_seconds as i64 * 1000;
        let mut cuts = None;
        if decision.method == pb::Method::Remux {
            // I confini dei segmenti cadono sui keyframe: l'indice viene dall'analisi o si estrae ora.
            let kf = {
                let _permit = state.extractions.acquire().await.map_err(|e| ApiError::Internal(e.into()))?;
                tokio::time::timeout(Duration::from_secs(180), crate::probe::extract_keyframes_job(&state, file.id)).await
            };
            match kf {
                Ok(Ok(k)) => cuts = Some(plan_segments(duration_ms, seg_ms, Some(&k))),
                other => {
                    tracing::warn!(file = file.id, result = ?other.map(|r| r.err().map(|e| e.to_string())), "keyframe non disponibili: si passa alla transcodifica");
                    let r = PlaybackRequest { force_transcode: true, audio_index: decision.audio_index, ..preq.clone() };
                    decision = pb::decide(&file.media, &req.profile, &r).map_err(decision_error)?;
                }
            }
        }
        let cuts = cuts.unwrap_or_else(|| plan_segments(duration_ms, seg_ms, None));
        let ts = s.transcode.clone();
        let spec = JobSpec {
            input: file.input(),
            video: file.media.video.as_ref().map(|v| SourceVideo {
                index: v.index,
                codec: v.codec.clone(),
                bit_depth: v.bit_depth.unwrap_or(8),
                width: v.width,
                height: v.height,
                hdr: v.hdr.is_some() && ts.enable_tonemap,
            }),
            audio_index: decision.audio_index,
            video_action: match &decision.video {
                VideoAction::Transcode { codec, max_width, max_height, bitrate, tonemap, burn_subtitle } => VideoAction::Transcode {
                    codec: codec.clone(),
                    max_width: *max_width,
                    max_height: *max_height,
                    bitrate: *bitrate,
                    tonemap: *tonemap && ts.enable_tonemap,
                    burn_subtitle: *burn_subtitle,
                },
                VideoAction::Copy => VideoAction::Copy,
            },
            audio_action: decision.audio.clone(),
            start_ms: 0,
            start_segment: 0,
            cut_points_ms: vec![],
            out_dir: state.boot.transcode_dir.join(&session_id),
            threads: ts.threads,
            sw_preset: ts.sw_preset.clone(),
            sw_crf: ts.sw_crf,
            tonemapx: state.ffmpeg.get().is_jellyfin,
            // HEVC e AV1 copiati viaggiano solo in fMP4; H.264 e la transcodifica restano in MPEG-TS.
            fmp4: match &decision.video {
                VideoAction::Copy => s.playback.fmp4_always || file.media.video.as_ref().is_some_and(|v| v.codec == "hevc" || v.codec == "av1"),
                VideoAction::Transcode { codec, .. } => codec == "hevc" || codec == "av1",
            },
            run: 0,
        };
        let library_id: i64 =
            sqlx::query_scalar("SELECT library_id FROM library_paths WHERE id = ?").bind(file.library_path_id).fetch_one(&state.db.read).await?;
        let v = file.media.video.as_ref();
        let job = crate::nodes::route::Job {
            height: v.map(|v| v.height).unwrap_or(0),
            hdr: v.is_some_and(|v| v.hdr.is_some()),
            codec: v.map(|v| v.codec.clone()).unwrap_or_default(),
            bit_depth: v.and_then(|v| v.bit_depth).unwrap_or(8),
            library_id,
            transcode: matches!(decision.video, VideoAction::Transcode { .. }),
            out_codec: match &decision.video {
                VideoAction::Transcode { codec, .. } => Some(codec.clone()),
                VideoAction::Copy => None,
            },
        };
        let seg = s.playback.segment_seconds.max(2) as usize;
        let plan = crate::nodes::hls::Plan {
            session_id: session_id.clone(),
            spec,
            job,
            file_id: file.id,
            file_size: file.size,
            library_path_id: file.library_path_id,
            rel: file.rel.clone(),
            cuts,
            duration_ms,
            ahead: (s.playback.throttle_ahead_seconds as usize / seg).max(5),
            keep: s.playback.keep_segments_behind as usize,
        };
        let started = crate::nodes::hls::start(&state, plan).await?;
        // Dimensioni effettive (ridotte se la transcodifica finisce in software), per la riga tecnica.
        decision.video = match (&decision.video, started.video) {
            (VideoAction::Transcode { tonemap, .. }, VideoAction::Transcode { codec, max_width, max_height, bitrate, burn_subtitle, .. }) => {
                VideoAction::Transcode { codec, max_width, max_height, bitrate, tonemap: *tonemap, burn_subtitle }
            }
            (d, _) => d.clone(),
        };
        backend_label = if matches!(decision.video, VideoAction::Copy) { "Software".into() } else { started.backend.clone() };
        executor = started.executor;
        hls = Some(started.handle);
    }

    let mut chain = chain_text(&file.media, &decision, &backend_label);
    if !executor.is_empty() {
        chain.push_str(&format!(" · {executor}"));
    }
    let bitrate = match &decision.video {
        VideoAction::Transcode { bitrate, .. } => Some(*bitrate as i64),
        VideoAction::Copy => file.media.bitrate.map(|b| b as i64),
    };
    let reason = decision.reasons.iter().map(|r| r.as_str()).collect::<Vec<_>>().join(",");
    let history_id = sqlx::query(
        "INSERT INTO playback_history (user_id, item_id, file_id, client, device_name, method, reason, started_at, node) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(user.user_id)
    .bind(item_id)
    .bind(file.id)
    .bind(&user.client)
    .bind(&user.device_name)
    .bind(decision.method.as_str())
    .bind(if reason.is_empty() { None } else { Some(reason.clone()) })
    .bind(now_ms())
    .bind(if executor.is_empty() { None } else { Some(executor.clone()) })
    .execute(&state.db.write)
    .await?
    .last_insert_rowid();

    let (title, subtitle_line) = if item.kind == "episode" {
        (
            item.series_title.clone().unwrap_or_default(),
            Some(format!("S{:02}E{:02} · {}", item.parent_index_number.unwrap_or(0), item.index_number.unwrap_or(0), item.title)),
        )
    } else {
        (item.title.clone(), item.year.map(|y| y.to_string()))
    };
    let start = req.start_ms.unwrap_or(0).max(0);
    let client_ip = user.ip;
    let extra = crate::sessions::SessionExtra {
        ip: client_ip.map(|i| i.to_string()),
        remote: client_ip.is_some_and(|i| !crate::net::in_any(&s.network.local_networks, i)),
        details: crate::sessions::details(&file, &decision, hls.as_ref().map(|h| h.is_fmp4())),
        position_ms: start,
    };
    let mut sess = PlaySession::new(
        session_id.clone(),
        token.clone(),
        &user,
        item_id,
        file.id,
        title,
        subtitle_line,
        item.poster_id.or(item.series_poster_id),
        decision.clone(),
        chain.clone(),
        bitrate,
        duration_ms,
        history_id,
        hls,
        extra,
    );
    sess.extra = sqlx::query_scalar::<_, Option<i64>>("SELECT extra_of FROM media_files WHERE id = ?").bind(file.id).fetch_one(&state.db.read).await?.is_some();
    let sess = Arc::new(sess);
    state.sessions.insert(sess.clone());
    crate::webhooks::playback_started(&state, &sess);
    state.events.admin("sessions.changed", json!({ "started": session_id }));
    tracing::info!(user = %user.username, method = decision.method.as_str(), reason = %reason, chain = %chain, "riproduzione avviata");

    let q = format!("?t={token}");
    let (stream_url, hls_url) = match decision.method {
        pb::Method::DirectPlay => (Some(format!("/api/v1/stream/{session_id}{q}")), None),
        _ => (None, Some(format!("/api/v1/hls/{session_id}/main.m3u8{q}"))),
    };
    let subtitles: Vec<Value> = file
        .streams
        .iter()
        .map(|(idx, codec, lang, title, external, _, forced, default)| {
            let image = pb::is_image_subtitle(codec);
            json!({
                "index": idx,
                "codec": codec,
                "language": lang,
                "title": title,
                "external": external,
                "forced": forced,
                "default": default,
                "image": image,
                // I sottotitoli di testo arrivano a parte in WebVTT; quelli a immagine richiedono una nuova decisione.
                "url": if image { None } else { Some(format!("/api/v1/subtitles/{session_id}/{idx}.vtt{q}")) },
            })
        })
        .collect();
    // Miniature per la barra di avanzamento, se generate: griglie di `cols`×`rows`, una miniatura ogni `interval_ms`.
    let trickplay = crate::trickplay::manifest(&state, file.id).await.map(|m| {
        json!({
            "interval_ms": m.interval_ms,
            "width": m.width,
            "height": m.height,
            "cols": m.cols,
            "rows": m.rows,
            "count": m.count,
            "url": format!("/api/v1/trickplay/{session_id}/{{n}}.jpg{q}"),
        })
    });
    Ok(json!({
        "session_id": session_id,
        "stream_token": token,
        "method": decision.method.as_str(),
        "reasons": decision.reasons.iter().map(|r| r.as_str()).collect::<Vec<_>>(),
        "chain": chain,
        "stream_url": stream_url,
        "hls_url": hls_url,
        "file_id": state.ids.encode(file.id),
        "duration_ms": duration_ms,
        "start_ms": start,
        "audio_index": decision.audio_index,
        "subtitle": decision.subtitle,
        // Sottotitolo di testo da attivare all'avvio secondo le preferenze (solo alla prima richiesta: dopo sceglie l'utente).
        "subtitle_default": if req.subtitle_index.is_none() && req.audio_index.is_none() && !subtitles_off {
            let audio_lang = decision.audio_index.and_then(|i| file.media.audio.iter().find(|a| a.index == i)).and_then(|a| a.language.clone());
            crate::prefs::pick_subtitle(&file.media.subtitles, audio_lang.as_deref(), &prefs, req.ui_language.as_deref().unwrap_or("it"))
        } else {
            None
        },
        "audio_tracks": file.media.audio.iter().map(|a| json!({ "index": a.index, "codec": a.codec, "channels": a.channels, "language": a.language, "default": a.default })).collect::<Vec<_>>(),
        "subtitles": subtitles,
        "trickplay": trickplay,
        // Limite di bitrate applicato (permessi, rete, qualità predefinita o scelta nel player); null = originale.
        "max_bitrate": user_max,
        "container": file.media.container,
    }))
}

/// Encoder video per la decisione: quelli del server più quelli dei nodi di transcodifica raggiungibili (l'esecutore
/// si sceglie poi tra chi sa codificare il codec deciso).
fn decision_encoders(state: &Shared) -> Vec<String> {
    let s = state.settings.get();
    let allowed = |c: &str| c == "h264" || (c == "hevc" && s.transcode.allow_hevc_output) || (c == "av1" && s.transcode.allow_av1_output);
    let mut v = if state.ffmpeg.get().available { allowed_encoders(state) } else { vec![] };
    for r in state.nodes.rows().iter().filter(|r| !r.is_local && r.enabled && r.role_transcode && !r.draining) {
        if !state.nodes.online(r.id) {
            continue;
        }
        for c in state.nodes.live(r.id).status.map(|st| st.encoders).unwrap_or_default() {
            if allowed(&c) && !v.contains(&c) {
                v.push(c);
            }
        }
    }
    if v.is_empty() {
        v.push("h264".into());
    }
    v
}

/// Encoder video consentiti dalle impostazioni e disponibili sulla macchina.
pub fn allowed_encoders(state: &Shared) -> Vec<String> {
    let s = state.settings.get();
    state
        .transcoder
        .server_encoders()
        .into_iter()
        .filter(|c| c == "h264" || (c == "hevc" && s.transcode.allow_hevc_output) || (c == "av1" && s.transcode.allow_av1_output))
        .collect()
}

#[derive(Deserialize)]
struct ProgressReq {
    position_ms: i64,
    #[serde(default)]
    paused: bool,
}

fn own_session(state: &Shared, user: &AuthUser, id: &str) -> ApiResult<Arc<PlaySession>> {
    let s = state.sessions.get(id).ok_or_else(|| ApiError::not_found("Sessione"))?;
    if s.user_id != user.user_id && !user.is_admin {
        return Err(ApiError::not_found("Sessione"));
    }
    Ok(s)
}

async fn progress(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>, Json(req): Json<ProgressReq>) -> ApiResult<Json<Value>> {
    let s = own_session(&state, &user, &id)?;
    let pos = req.position_ms.clamp(0, s.duration_ms.max(req.position_ms));
    s.report(Some(pos), req.paused);
    if !s.extra {
        state.sessions.record_progress(s.user_id, s.item_id, pos, s.duration_ms);
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct TracksReq {
    audio_index: Option<u32>,
    subtitle_index: Option<u32>,
    #[serde(default)]
    subtitle_off: bool,
}

/// Tracce scelte nel player: si salvano come lingue sul film o sulla serie, per la prossima riproduzione.
async fn tracks(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>, Json(req): Json<TracksReq>) -> ApiResult<Json<Value>> {
    let s = own_session(&state, &user, &id)?;
    let info = load_file(&state, s.file_id).await?;
    let audio = req.audio_index.and_then(|i| info.media.audio.iter().find(|a| a.index == i)).and_then(|a| a.language.clone());
    let sub = req.subtitle_index.and_then(|i| info.media.subtitles.iter().find(|x| x.index == i)).and_then(|x| x.language.clone());
    let (kind, series): (String, Option<i64>) =
        sqlx::query_as("SELECT kind, series_id FROM items WHERE id = ?").bind(s.item_id).fetch_one(&state.db.read).await?;
    let key = if kind == "episode" { series.unwrap_or(s.item_id) } else { s.item_id };
    sqlx::query(
        "INSERT INTO track_choices (user_id, item_id, audio_language, subtitle_language, subtitle_off, updated_at) VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(user_id, item_id) DO UPDATE SET
           audio_language = COALESCE(excluded.audio_language, track_choices.audio_language),
           subtitle_language = CASE WHEN excluded.subtitle_off = 1 THEN track_choices.subtitle_language ELSE COALESCE(excluded.subtitle_language, track_choices.subtitle_language) END,
           subtitle_off = excluded.subtitle_off, updated_at = excluded.updated_at",
    )
    .bind(user.user_id)
    .bind(key)
    .bind(&audio)
    .bind(&sub)
    .bind(req.subtitle_off as i64)
    .bind(crate::util::now_ms())
    .execute(&state.db.write)
    .await?;
    Ok(Json(json!({ "audio_language": audio, "subtitle_language": sub, "subtitle_off": req.subtitle_off })))
}

#[derive(Deserialize)]
struct ClientErrorReq {
    message: String,
}

async fn client_error(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>, Json(req): Json<ClientErrorReq>) -> ApiResult<Json<Value>> {
    let s = own_session(&state, &user, &id)?;
    let msg: String = req.message.chars().filter(|c| !c.is_control()).take(500).collect();
    tracing::warn!(user = %s.username, client = %s.client, method = s.decision.method.as_str(), chain = %s.chain, error = %msg, "il player non riesce a riprodurre il flusso");
    Ok(Json(json!({ "ok": true })))
}

async fn stop(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>, body: Option<Json<ProgressReq>>) -> ApiResult<Json<Value>> {
    let s = own_session(&state, &user, &id)?;
    if let Some(Json(p)) = body {
        s.position_ms.store(p.position_ms.max(0), Ordering::Relaxed);
    }
    stop_session(&state, &id).await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct TokenQuery {
    t: Option<String>,
}

fn session_by_token(state: &Shared, id: &str, t: Option<&str>) -> ApiResult<Arc<PlaySession>> {
    let t = t.ok_or(ApiError::Unauthorized)?;
    let s = state.sessions.by_token(t).ok_or(ApiError::Unauthorized)?;
    if s.id != id {
        return Err(ApiError::Unauthorized);
    }
    // Account scaduto durante la riproduzione: lo stream si ferma.
    if s.expires_at.is_some_and(|e| now_ms() >= e) {
        return Err(ApiError::coded(StatusCode::FORBIDDEN, "AccountExpired", "Account scaduto"));
    }
    s.touch();
    Ok(s)
}

async fn direct(State(state): State<Shared>, Path(id): Path<String>, Query(q): Query<TokenQuery>, method: Method, headers: HeaderMap) -> ApiResult<Response> {
    let s = session_by_token(&state, &id, q.t.as_deref())?;
    if s.decision.method != pb::Method::DirectPlay {
        return Err(ApiError::BadRequest("Questa sessione usa HLS".into()));
    }
    let info = load_file(&state, s.file_id).await?;
    // Questo server o un nodo con la cartella in copia; la sessione resta sulla copia scelta.
    let sticky = Some(s.copy_node.load(Ordering::Relaxed)).filter(|n| *n != 0);
    let opts = crate::nodes::cdn::Options { sticky, allow_redirect: true, download: false };
    let served = crate::nodes::cdn::serve(&state, &info, &method, &headers, opts, Some(s.bytes_sent.clone())).await?;
    if state.nodes.has_remote() {
        let prev = s.copy_node.swap(served.node, Ordering::Relaxed);
        if prev != served.node {
            let name = state.nodes.row(served.node).filter(|r| !r.is_local).map(|r| r.name);
            *s.copy_name.lock().unwrap() = name;
            state.events.admin("sessions.changed", json!({ "updated": s.id }));
        }
    }
    Ok(served.response)
}

async fn hls(State(state): State<Shared>, Path((id, name)): Path<(String, String)>, Query(q): Query<TokenQuery>, headers: HeaderMap) -> ApiResult<Response> {
    let s = session_by_token(&state, &id, q.t.as_deref())?;
    let rt = s.hls.clone().ok_or_else(|| ApiError::BadRequest("Questa sessione è in direct play".into()))?;
    if name == "main.m3u8" {
        let t = s.token.clone();
        let ext = if rt.is_fmp4() { "m4s" } else { "ts" };
        let body = rt.playlist(|i| if i == usize::MAX { format!("init.mp4?t={t}") } else { format!("{i}.{ext}?t={t}") });
        return Ok(Response::builder()
            .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from(body))
            .unwrap());
    }
    if name == "init.mp4" && rt.is_fmp4() {
        let bytes = rt.init(&state).await.map_err(hls_error)?;
        let (resp, sent) = crate::stream::serve_bytes(bytes, "video/mp4", "private, max-age=3600", &headers);
        s.bytes_sent.fetch_add(sent, Ordering::Relaxed);
        return Ok(resp);
    }
    let ext = if rt.is_fmp4() { ".m4s" } else { ".ts" };
    let n: usize = name.strip_suffix(ext).and_then(|n| n.parse().ok()).ok_or_else(|| ApiError::not_found("Segmento"))?;
    // La posizione stimata dal segmento richiesto tiene viva la sessione anche senza rapporti dal client.
    if let Some(start) = rt.cuts.get(n) {
        if s.position_ms.load(Ordering::Relaxed) == 0 {
            s.position_ms.store(*start, Ordering::Relaxed);
        }
    }
    let bytes = match rt.segment(&state, n).await {
        Ok(b) => b,
        Err(HlsError::Busy) => {
            let mut r = ApiError::coded(StatusCode::SERVICE_UNAVAILABLE, "TranscodeBusy", HlsError::Busy.to_string()).into_response();
            r.headers_mut().insert(header::RETRY_AFTER, axum::http::HeaderValue::from_static("2"));
            return Ok(r);
        }
        Err(e) => return Err(hls_error(e)),
    };
    let ct = if rt.is_fmp4() { "video/mp4" } else { "video/mp2t" };
    let (resp, sent) = crate::stream::serve_bytes(bytes, ct, "private, max-age=3600", &headers);
    s.bytes_sent.fetch_add(sent, Ordering::Relaxed);
    Ok(resp)
}

/// Errori della sessione HLS. Una sessione chiusa (nuova riproduzione, stop, esecutore perso senza ricambio) è 410:
/// il player web riapre alla posizione corrente.
fn hls_error(e: HlsError) -> ApiError {
    match e {
        HlsError::OutOfRange => ApiError::not_found("Segmento"),
        HlsError::Stopped => ApiError::coded(StatusCode::GONE, "SessionEnded", "La sessione di riproduzione è terminata"),
        HlsError::Busy => ApiError::coded(StatusCode::SERVICE_UNAVAILABLE, "TranscodeBusy", HlsError::Busy.to_string()),
        HlsError::Failed(m) => ApiError::coded(StatusCode::INTERNAL_SERVER_ERROR, "TranscodeFailed", m),
    }
}

/// Sottotitoli di testo in WebVTT: estratti al primo utilizzo, poi in cache.
/// Prefisso dei sottotitoli aggiunti da Platea (caricati o scaricati): stanno in `cache/subtitles/added/`, mai nei media.
pub const ADDED_PREFIX: &str = "@platea/";

/// Percorso di un sottotitolo esterno: accanto al video, oppure nella cache se aggiunto da Platea.
pub fn external_subtitle_path(state: &Shared, root: &std::path::Path, rel: &str) -> ApiResult<PathBuf> {
    if rel.split('/').any(|p| p == "..") {
        return Err(ApiError::Forbidden("Percorso non valido".into()));
    }
    Ok(match rel.strip_prefix(ADDED_PREFIX) {
        Some(name) => state.boot.subtitles_dir().join("added").join(name),
        None => root.join(rel),
    })
}

async fn subtitle(State(state): State<Shared>, Path((id, name)): Path<(String, String)>, Query(q): Query<TokenQuery>) -> ApiResult<Response> {
    let s = session_by_token(&state, &id, q.t.as_deref())?;
    let idx: i64 = name.strip_suffix(".vtt").and_then(|n| n.parse().ok()).ok_or_else(|| ApiError::not_found("Sottotitolo"))?;
    let info = load_file(&state, s.file_id).await?;
    let Some((_, codec, _, _, external, ext_path, ..)) = info.streams.iter().find(|x| x.0 == idx).cloned() else {
        return Err(ApiError::not_found("Sottotitolo"));
    };
    if pb::is_image_subtitle(&codec) {
        return Err(ApiError::BadRequest("Sottotitolo a immagine: non convertibile in testo".into()));
    }
    let cache = state.boot.subtitles_dir().join(format!("{}_{}_{}_{}.vtt", info.id, idx, info.size, info.mtime));
    if !cache.exists() {
        let _permit = state.extractions.acquire().await.map_err(|e| ApiError::Internal(e.into()))?;
        if !cache.exists() {
            let input = if external {
                let rel = ext_path.ok_or_else(|| ApiError::not_found("Sottotitolo"))?;
                external_subtitle_path(&state, &info.root, &rel)?
            } else {
                info.input()
            };
            // Nome temporaneo unico: due richieste dello stesso sottotitolo (due sessioni, un ripiego) non si pestano.
            let tmp = cache.with_extension(format!("{}.tmp.vtt", crate::util::random_hex(4)));
            let mut cmd = tokio::process::Command::new(&state.ffmpeg.get().ffmpeg);
            cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"]).arg(&input);
            if external {
                cmd.args(["-map", "0:s:0"]);
            } else {
                cmd.args(["-map", &format!("0:{idx}")]);
            }
            cmd.args(["-c:s", "webvtt", "-f", "webvtt"]).arg(&tmp).kill_on_drop(true).stdin(std::process::Stdio::null());
            let out = tokio::time::timeout(Duration::from_secs(600), cmd.output())
                .await
                .map_err(|_| ApiError::coded(StatusCode::GATEWAY_TIMEOUT, "SubtitleTimeout", "Estrazione del sottotitolo oltre il tempo limite"))?
                .map_err(|e| ApiError::Internal(e.into()))?;
            if !out.status.success() {
                let _ = tokio::fs::remove_file(&tmp).await;
                return Err(ApiError::coded(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "SubtitleFailed",
                    format!("Conversione non riuscita: {}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("")),
                ));
            }
            if let Err(e) = tokio::fs::rename(&tmp, &cache).await {
                let _ = tokio::fs::remove_file(&tmp).await;
                if !cache.exists() {
                    return Err(ApiError::Internal(e.into()));
                }
            }
        }
    }
    let body = tokio::fs::read(&cache).await.map_err(|e| ApiError::Internal(e.into()))?;
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "text/vtt; charset=utf-8")
        .header(header::CACHE_CONTROL, "private, max-age=86400")
        .body(Body::from(body))
        .unwrap())
}

async fn trickplay(State(state): State<Shared>, Path((id, name)): Path<(String, String)>, Query(q): Query<TokenQuery>) -> ApiResult<Response> {
    let s = session_by_token(&state, &id, q.t.as_deref())?;
    let n: u32 = name.strip_suffix(".jpg").and_then(|n| n.parse().ok()).ok_or_else(|| ApiError::not_found("Miniature"))?;
    let path = crate::trickplay::dir(&state, s.file_id).join(format!("{n}.jpg"));
    let body = tokio::fs::read(&path).await.map_err(|_| ApiError::not_found("Miniature"))?;
    Ok(Response::builder().header(header::CONTENT_TYPE, "image/jpeg").header(header::CACHE_CONTROL, "private, max-age=86400").body(Body::from(body)).unwrap())
}
