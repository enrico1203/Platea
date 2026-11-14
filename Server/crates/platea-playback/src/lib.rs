//! Decisione di riproduzione.
//!
//! Funzione pura: confronta i dati tecnici di un file (già nel database) con il profilo
//! dichiarato dal client e sceglie il metodo meno costoso: Direct play, poi Remux, poi Transcodifica.
//! Nessun I/O: risponde in microsecondi e si verifica con tabelle di casi.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct VideoCodecSupport {
    /// Nome del codec come in ffprobe: `h264`, `hevc`, `av1`, `vp9`...
    pub codec: String,
    /// Profili accettati, minuscoli (`main`, `high`, `main 10`). Vuoto = tutti.
    pub profiles: Vec<String>,
    /// Livello massimo come in ffprobe (es. 51 per H.264 5.1, 153 per HEVC 5.1). `None` = nessun limite.
    pub max_level: Option<u32>,
    /// Profondità di bit massima. `None` = nessun limite.
    pub max_bit_depth: Option<u8>,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AudioCodecSupport {
    pub codec: String,
    pub max_channels: Option<u32>,
}

/// Ciò che il client dichiara di saper riprodurre. Il server non indovina mai.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct DeviceProfile {
    pub name: String,
    /// Contenitori riproducibili come file diretto: `mp4`, `mkv`, `webm`, `mov`...
    pub containers: Vec<String>,
    pub video_codecs: Vec<VideoCodecSupport>,
    pub audio_codecs: Vec<AudioCodecSupport>,
    /// Sottotitoli di testo che il client disegna da sé (consegnati a parte): `vtt`, `ass`, `srt`.
    pub subtitle_text: Vec<String>,
    /// Sottotitoli a immagine che il client disegna da sé: `pgs`, `dvdsub`.
    pub subtitle_image: Vec<String>,
    /// Formati HDR supportati: `hdr10`, `hlg`, `dovi`, `hdr10plus`.
    pub hdr: Vec<String>,
    /// Bitrate massimo in bit/s.
    pub max_bitrate: Option<u64>,
    /// `file` e/o `hls`.
    pub protocols: Vec<String>,
    /// Codec video accettati dentro i segmenti HLS.
    pub hls_video_codecs: Vec<String>,
    /// Codec audio accettati dentro i segmenti HLS.
    pub hls_audio_codecs: Vec<String>,
}

impl Default for DeviceProfile {
    fn default() -> Self {
        // Un browser moderno qualunque: il minimo comune denominatore.
        DeviceProfile {
            name: "generic".into(),
            containers: vec!["mp4".into(), "webm".into()],
            video_codecs: vec![VideoCodecSupport {
                codec: "h264".into(),
                profiles: vec![],
                max_level: None,
                max_bit_depth: Some(8),
                max_width: None,
                max_height: None,
            }],
            audio_codecs: vec![
                AudioCodecSupport { codec: "aac".into(), max_channels: Some(6) },
                AudioCodecSupport { codec: "mp3".into(), max_channels: Some(2) },
                AudioCodecSupport { codec: "opus".into(), max_channels: None },
                AudioCodecSupport { codec: "flac".into(), max_channels: None },
            ],
            subtitle_text: vec!["vtt".into()],
            subtitle_image: vec![],
            hdr: vec![],
            max_bitrate: None,
            protocols: vec!["file".into(), "hls".into()],
            hls_video_codecs: vec!["h264".into()],
            hls_audio_codecs: vec!["aac".into(), "mp3".into()],
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct VideoInfo {
    pub index: u32,
    pub codec: String,
    pub profile: Option<String>,
    pub level: Option<u32>,
    pub bit_depth: Option<u8>,
    pub width: u32,
    pub height: u32,
    /// `hdr10`, `hlg`, `dovi`, `hdr10plus` oppure `None` per SDR.
    pub hdr: Option<String>,
    pub bitrate: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AudioInfo {
    pub index: u32,
    pub codec: String,
    pub channels: u32,
    pub language: Option<String>,
    pub default: bool,
    pub bitrate: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SubtitleInfo {
    /// Indice della traccia nel file, oppure identificativo del file esterno.
    pub index: u32,
    pub codec: String,
    pub language: Option<String>,
    pub forced: bool,
    pub default: bool,
    pub external: bool,
}

impl SubtitleInfo {
    pub fn is_image(&self) -> bool {
        is_image_subtitle(&self.codec)
    }
}

pub fn is_image_subtitle(codec: &str) -> bool {
    matches!(
        codec,
        "hdmv_pgs_subtitle" | "pgs" | "pgssub" | "dvd_subtitle" | "dvdsub" | "dvb_subtitle" | "dvbsub" | "xsub" | "vobsub"
    )
}

/// Nome breve per il confronto con il profilo: `hdmv_pgs_subtitle` → `pgs`.
pub fn short_subtitle_codec(codec: &str) -> &str {
    match codec {
        "hdmv_pgs_subtitle" | "pgssub" => "pgs",
        "dvd_subtitle" | "vobsub" => "dvdsub",
        "dvb_subtitle" => "dvbsub",
        "subrip" => "srt",
        "webvtt" => "vtt",
        "mov_text" => "tx3g",
        "ssa" => "ass",
        other => other,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MediaInfo {
    /// Contenitore normalizzato: `mkv`, `mp4`, `webm`, `avi`, `ts`, `mov`...
    pub container: String,
    pub bitrate: Option<u64>,
    pub duration_ms: Option<u64>,
    pub video: Option<VideoInfo>,
    pub audio: Vec<AudioInfo>,
    pub subtitles: Vec<SubtitleInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PlaybackRequest {
    /// Traccia audio scelta; `None` = predefinita.
    pub audio_index: Option<u32>,
    /// Sottotitolo scelto; `None` = nessuno.
    pub subtitle_index: Option<u32>,
    pub allow_remux: bool,
    pub allow_transcode: bool,
    /// Limite di bitrate imposto dall'amministratore per l'utente (bit/s).
    pub user_max_bitrate: Option<u64>,
    /// Codec video che il server sa codificare (dalla prova all'avvio).
    pub server_video_encoders: Vec<String>,
    /// Forza la transcodifica (per esempio dopo un fallimento del direct play nel client).
    pub force_transcode: bool,
}

impl Default for PlaybackRequest {
    fn default() -> Self {
        PlaybackRequest {
            audio_index: None,
            subtitle_index: None,
            allow_remux: true,
            allow_transcode: true,
            user_max_bitrate: None,
            server_video_encoders: vec!["h264".into()],
            force_transcode: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    DirectPlay,
    Remux,
    Transcode,
}

impl Method {
    pub fn as_str(&self) -> &'static str {
        match self {
            Method::DirectPlay => "direct_play",
            Method::Remux => "remux",
            Method::Transcode => "transcode",
        }
    }
}

/// Codici motivo: restano in inglese, come nell'API e nella dashboard.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Reason {
    ContainerNotSupported,
    VideoCodecNotSupported,
    VideoProfileNotSupported,
    VideoLevelNotSupported,
    VideoBitDepthNotSupported,
    VideoResolutionNotSupported,
    HdrNotSupported,
    BitrateTooHigh,
    AudioCodecNotSupported,
    AudioChannelsNotSupported,
    SubtitleBurnIn,
    DirectFileNotSupported,
    HlsVideoCodecNotSupported,
    HlsAudioCodecNotSupported,
    TranscodeForced,
}

impl Reason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Reason::ContainerNotSupported => "ContainerNotSupported",
            Reason::VideoCodecNotSupported => "VideoCodecNotSupported",
            Reason::VideoProfileNotSupported => "VideoProfileNotSupported",
            Reason::VideoLevelNotSupported => "VideoLevelNotSupported",
            Reason::VideoBitDepthNotSupported => "VideoBitDepthNotSupported",
            Reason::VideoResolutionNotSupported => "VideoResolutionNotSupported",
            Reason::HdrNotSupported => "HdrNotSupported",
            Reason::BitrateTooHigh => "BitrateTooHigh",
            Reason::AudioCodecNotSupported => "AudioCodecNotSupported",
            Reason::AudioChannelsNotSupported => "AudioChannelsNotSupported",
            Reason::SubtitleBurnIn => "SubtitleBurnIn",
            Reason::DirectFileNotSupported => "DirectFileNotSupported",
            Reason::HlsVideoCodecNotSupported => "HlsVideoCodecNotSupported",
            Reason::HlsAudioCodecNotSupported => "HlsAudioCodecNotSupported",
            Reason::TranscodeForced => "TranscodeForced",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum VideoAction {
    Copy,
    Transcode {
        codec: String,
        max_width: u32,
        max_height: u32,
        bitrate: u64,
        /// Tone mapping da HDR a SDR.
        tonemap: bool,
        /// Sottotitolo a immagine da imprimere nel video.
        burn_subtitle: Option<u32>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AudioAction {
    Copy,
    Transcode { codec: String, channels: u32, bitrate: u64 },
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "delivery", rename_all = "snake_case")]
pub enum SubtitleDelivery {
    None,
    /// Consegnato a parte in WebVTT, senza toccare il video.
    External { index: u32 },
    /// Il client lo legge dal file originale (direct play di un contenitore che lo contiene).
    Embedded { index: u32 },
    /// Impresso nel video.
    BurnIn { index: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Decision {
    pub method: Method,
    pub reasons: Vec<Reason>,
    pub video: VideoAction,
    pub audio: AudioAction,
    pub audio_index: Option<u32>,
    pub subtitle: SubtitleDelivery,
}

impl Decision {
    /// Primo motivo, quello mostrato nel badge.
    pub fn primary_reason(&self) -> Option<Reason> {
        self.reasons.first().copied()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DecisionError {
    /// Il file non è ancora stato analizzato: niente dati tecnici.
    NotProbed,
    /// Serve un remux ma l'amministratore l'ha vietato per l'utente.
    RemuxNotAllowed(Vec<Reason>),
    /// Serve una transcodifica ma l'amministratore l'ha vietata per l'utente.
    TranscodeNotAllowed(Vec<Reason>),
    /// Il client non dichiara nessun protocollo utilizzabile.
    NoProtocol,
}

fn has(list: &[String], v: &str) -> bool {
    list.iter().any(|x| x.eq_ignore_ascii_case(v))
}

/// Alias dei contenitori: un profilo che dice `mp4` accetta anche `m4v`.
fn container_supported(profile: &DeviceProfile, container: &str) -> bool {
    let c = container.to_ascii_lowercase();
    if has(&profile.containers, &c) {
        return true;
    }
    match c.as_str() {
        "m4v" | "mov" => has(&profile.containers, "mp4") && c == "m4v",
        "mkv" => has(&profile.containers, "matroska"),
        "webm" => has(&profile.containers, "webm"),
        _ => false,
    }
}

/// Verifica del video. Restituisce i motivi per cui serve una ricodifica.
fn check_video(profile: &DeviceProfile, v: &VideoInfo) -> Vec<Reason> {
    let mut reasons = Vec::new();
    let Some(support) = profile.video_codecs.iter().find(|s| s.codec.eq_ignore_ascii_case(&v.codec)) else {
        return vec![Reason::VideoCodecNotSupported];
    };
    if let Some(p) = &v.profile {
        if !support.profiles.is_empty() && !has(&support.profiles, p) {
            reasons.push(Reason::VideoProfileNotSupported);
        }
    }
    if let (Some(max), Some(level)) = (support.max_level, v.level) {
        if level > max {
            reasons.push(Reason::VideoLevelNotSupported);
        }
    }
    if let (Some(max), Some(bd)) = (support.max_bit_depth, v.bit_depth) {
        if bd > max {
            reasons.push(Reason::VideoBitDepthNotSupported);
        }
    }
    let too_wide = support.max_width.is_some_and(|w| v.width > w);
    let too_tall = support.max_height.is_some_and(|h| v.height > h);
    if too_wide || too_tall {
        reasons.push(Reason::VideoResolutionNotSupported);
    }
    if let Some(hdr) = &v.hdr {
        let ok = has(&profile.hdr, hdr)
            // Dolby Vision con base HDR10 si riproduce come HDR10.
            || (hdr == "dovi" && has(&profile.hdr, "hdr10"))
            || (hdr == "hdr10plus" && has(&profile.hdr, "hdr10"));
        if !ok {
            reasons.push(Reason::HdrNotSupported);
        }
    }
    reasons
}

fn check_audio(profile: &DeviceProfile, a: &AudioInfo, codecs: Option<&[String]>) -> Option<Reason> {
    let support = profile.audio_codecs.iter().find(|s| s.codec.eq_ignore_ascii_case(&a.codec));
    match support {
        None => Some(Reason::AudioCodecNotSupported),
        Some(s) => {
            if let Some(list) = codecs {
                if !has(list, &a.codec) {
                    return Some(Reason::HlsAudioCodecNotSupported);
                }
            }
            if s.max_channels.is_some_and(|m| a.channels > m) {
                Some(Reason::AudioChannelsNotSupported)
            } else {
                None
            }
        }
    }
}

/// Bitrate predefinito per la transcodifica a una certa altezza (bit/s).
pub fn default_bitrate_for_height(height: u32) -> u64 {
    match height {
        0..=480 => 2_000_000,
        481..=720 => 4_000_000,
        721..=1080 => 8_000_000,
        1081..=1440 => 14_000_000,
        _ => 20_000_000,
    }
}

/// Il cuore del "direct play prima di tutto".
pub fn decide(media: &MediaInfo, profile: &DeviceProfile, req: &PlaybackRequest) -> Result<Decision, DecisionError> {
    let can_file = has(&profile.protocols, "file");
    let can_hls = has(&profile.protocols, "hls");
    if !can_file && !can_hls {
        return Err(DecisionError::NoProtocol);
    }
    if media.container.is_empty() {
        return Err(DecisionError::NotProbed);
    }

    let max_bitrate = match (profile.max_bitrate, req.user_max_bitrate) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };

    // Traccia audio: quella chiesta, altrimenti la predefinita, altrimenti la prima.
    let audio = req
        .audio_index
        .and_then(|i| media.audio.iter().find(|a| a.index == i))
        .or_else(|| media.audio.iter().find(|a| a.default))
        .or_else(|| media.audio.first());

    let subtitle = req.subtitle_index.and_then(|i| media.subtitles.iter().find(|s| s.index == i));

    let mut video_reasons: Vec<Reason> = Vec::new();
    let mut reasons: Vec<Reason> = Vec::new();

    if req.force_transcode {
        video_reasons.push(Reason::TranscodeForced);
    }

    // 2. Video
    if let Some(v) = &media.video {
        video_reasons.extend(check_video(profile, v));
    }

    // 3. Bitrate
    let total_bitrate = media.bitrate.or_else(|| media.video.as_ref().and_then(|v| v.bitrate));
    if let (Some(max), Some(br)) = (max_bitrate, total_bitrate) {
        if br > max {
            video_reasons.push(Reason::BitrateTooHigh);
        }
    }

    // 5. Sottotitoli
    let mut burn_in: Option<u32> = None;
    let subtitle_delivery = match subtitle {
        None => SubtitleDelivery::None,
        Some(s) if s.is_image() => {
            if has(&profile.subtitle_image, short_subtitle_codec(&s.codec)) && !s.external {
                SubtitleDelivery::Embedded { index: s.index }
            } else {
                burn_in = Some(s.index);
                video_reasons.push(Reason::SubtitleBurnIn);
                SubtitleDelivery::BurnIn { index: s.index }
            }
        }
        Some(s) => SubtitleDelivery::External { index: s.index },
    };

    let need_video_transcode = !video_reasons.is_empty();

    // 4. Audio (con il video copiato basta convertire il solo audio)
    let audio_reason_direct = audio.and_then(|a| check_audio(profile, a, None));

    // 6. Contenitore
    let container_ok = container_supported(profile, &media.container);

    if !need_video_transcode && audio_reason_direct.is_none() && container_ok && can_file {
        return Ok(Decision {
            method: Method::DirectPlay,
            reasons: vec![],
            video: VideoAction::Copy,
            audio: if audio.is_some() { AudioAction::Copy } else { AudioAction::None },
            audio_index: audio.map(|a| a.index),
            subtitle: subtitle_delivery,
        });
    }

    if !can_hls {
        // Il client vuole solo file diretti: l'unica alternativa sarebbe un file ricodificato.
        let mut r = video_reasons.clone();
        if let Some(a) = audio_reason_direct {
            r.push(a);
        }
        if !container_ok {
            r.push(Reason::ContainerNotSupported);
        }
        return Err(DecisionError::TranscodeNotAllowed(r));
    }

    // Da qui in poi l'uscita è HLS: un sottotitolo a immagine che il client leggeva dal file va impresso.
    let mut subtitle_delivery = subtitle_delivery;
    if let SubtitleDelivery::Embedded { index } = subtitle_delivery {
        video_reasons.push(Reason::SubtitleBurnIn);
        burn_in = Some(index);
        subtitle_delivery = SubtitleDelivery::BurnIn { index };
    }
    let need_video_transcode = !video_reasons.is_empty();
    let hls_audio_reason = audio.and_then(|a| check_audio(profile, a, Some(&profile.hls_audio_codecs)));
    let audio_action = match (audio, hls_audio_reason) {
        (None, _) => AudioAction::None,
        (Some(_), None) => AudioAction::Copy,
        (Some(a), Some(_)) => {
            let max_ch = profile
                .audio_codecs
                .iter()
                .find(|s| s.codec == "aac")
                .and_then(|s| s.max_channels)
                .unwrap_or(2);
            let channels = a.channels.clamp(1, max_ch.max(1));
            AudioAction::Transcode { codec: "aac".into(), channels, bitrate: if channels > 2 { 384_000 } else { 192_000 } }
        }
    };
    if let Some(r) = audio_reason_direct {
        reasons.push(r);
    } else if let Some(r) = hls_audio_reason {
        reasons.push(r);
    }

    if !need_video_transcode {
        let v_codec = media.video.as_ref().map(|v| v.codec.as_str());
        let hls_video_ok = v_codec.is_none_or(|c| has(&profile.hls_video_codecs, c));
        if hls_video_ok {
            // Remux: video copiato.
            let mut r = Vec::new();
            if !container_ok {
                r.push(Reason::ContainerNotSupported);
            } else if !can_file {
                r.push(Reason::DirectFileNotSupported);
            }
            r.extend(reasons);
            if !req.allow_remux {
                return Err(DecisionError::RemuxNotAllowed(r));
            }
            return Ok(Decision {
                method: Method::Remux,
                reasons: r,
                video: VideoAction::Copy,
                audio: audio_action,
                audio_index: audio.map(|a| a.index),
                subtitle: subtitle_delivery,
            });
        }
        video_reasons.push(Reason::HlsVideoCodecNotSupported);
    }

    // Transcodifica video.
    let mut all = video_reasons;
    all.extend(reasons);
    if !req.allow_transcode {
        return Err(DecisionError::TranscodeNotAllowed(all));
    }

    let (src_w, src_h) = media.video.as_ref().map(|v| (v.width, v.height)).unwrap_or((1920, 1080));
    // Codec di uscita: H.264 se il client lo accetta (il più economico da codificare e il più compatibile),
    // altrimenti HEVC o AV1 quando client e server li condividono.
    let out_codec = ["h264", "hevc", "av1"]
        .into_iter()
        .find(|c| has(&profile.hls_video_codecs, c) && has(&req.server_video_encoders, c))
        .unwrap_or("h264");
    let support = profile.video_codecs.iter().find(|s| s.codec == out_codec);
    let max_w = support.and_then(|s| s.max_width).unwrap_or(u32::MAX).min(src_w.max(1));
    let max_h = support.and_then(|s| s.max_height).unwrap_or(u32::MAX).min(src_h.max(1));
    let audio_br = match &audio_action {
        AudioAction::Transcode { bitrate, .. } => *bitrate,
        AudioAction::Copy => audio.and_then(|a| a.bitrate).unwrap_or(640_000),
        AudioAction::None => 0,
    };
    let mut bitrate = default_bitrate_for_height(max_h);
    if let Some(src) = media.video.as_ref().and_then(|v| v.bitrate).or(total_bitrate) {
        // Ricodificare a un bitrate più alto dell'originale non migliora nulla.
        bitrate = bitrate.min(src.max(1_000_000));
    }
    if let Some(max) = max_bitrate {
        bitrate = bitrate.min(max.saturating_sub(audio_br).max(500_000) * 9 / 10);
    }
    let tonemap = media.video.as_ref().is_some_and(|v| v.hdr.is_some()) && all.contains(&Reason::HdrNotSupported);
    Ok(Decision {
        method: Method::Transcode,
        reasons: all,
        video: VideoAction::Transcode {
            codec: out_codec.into(),
            max_width: max_w,
            max_height: max_h,
            bitrate,
            tonemap,
            burn_subtitle: burn_in,
        },
        audio: audio_action,
        audio_index: audio.map(|a| a.index),
        subtitle: subtitle_delivery,
    })
}

#[cfg(test)]
mod tests;
