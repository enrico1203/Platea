//! Traduzione del `DeviceProfile` di Jellyfin (inviato dalle app in `PlaybackInfo`) nel profilo di Platea, così la
//! decisione direct play / remux / transcodifica è la stessa della web UI.

use platea_playback::{AudioCodecSupport, DeviceProfile, VideoCodecSupport};
use serde_json::Value;

fn list(v: &Value) -> Vec<String> {
    v.as_str().unwrap_or("").split([',', '|']).map(|x| x.trim().to_ascii_lowercase()).filter(|x| !x.is_empty()).collect()
}

fn video_codec(c: &str) -> String {
    match c {
        "h265" | "hevc" | "hvc1" | "hev1" => "hevc",
        "avc" | "h264" | "avc1" => "h264",
        "mpeg2" | "mpeg2video" => "mpeg2video",
        other => other,
    }
    .to_string()
}

fn audio_codec(c: &str) -> String {
    match c {
        "dca" | "dts" => "dts",
        "e-ac-3" | "eac3" => "eac3",
        "ac-3" | "ac3" => "ac3",
        "mpeg" | "mp3" => "mp3",
        other => other,
    }
    .to_string()
}

fn container(c: &str) -> String {
    match c {
        "matroska" | "mkv" => "mkv",
        "m4v" | "mp4" => "mp4",
        "mpegts" | "ts" | "m2ts" => "ts",
        other => other,
    }
    .to_string()
}

/// Profilo di Platea da quello di Jellyfin. `max_bitrate` viene da `MaxStreamingBitrate`.
pub fn convert(p: &Value, max_bitrate: Option<u64>) -> DeviceProfile {
    let mut out = DeviceProfile { name: p["Name"].as_str().unwrap_or("jellyfin").to_string(), ..DeviceProfile::default() };
    out.containers.clear();
    out.video_codecs.clear();
    out.audio_codecs.clear();
    out.subtitle_text.clear();
    out.subtitle_image.clear();
    out.hdr.clear();
    out.protocols = vec!["file".into()];
    out.hls_video_codecs.clear();
    out.hls_audio_codecs.clear();
    out.max_bitrate = max_bitrate.or_else(|| p["MaxStreamingBitrate"].as_u64()).filter(|b| *b > 0);

    let push_v = |out: &mut DeviceProfile, c: String| {
        if !out.video_codecs.iter().any(|v| v.codec == c) {
            out.video_codecs.push(VideoCodecSupport { codec: c, ..Default::default() });
        }
    };
    let push_a = |out: &mut DeviceProfile, c: String| {
        if !out.audio_codecs.iter().any(|a| a.codec == c) {
            out.audio_codecs.push(AudioCodecSupport { codec: c, max_channels: None });
        }
    };
    for d in p["DirectPlayProfiles"].as_array().into_iter().flatten() {
        if d["Type"].as_str().is_some_and(|t| !t.eq_ignore_ascii_case("video")) {
            continue;
        }
        for c in list(&d["Container"]) {
            let c = container(&c);
            if !out.containers.contains(&c) {
                out.containers.push(c);
            }
        }
        for c in list(&d["VideoCodec"]) {
            push_v(&mut out, video_codec(&c));
        }
        for c in list(&d["AudioCodec"]) {
            push_a(&mut out, audio_codec(&c));
        }
    }
    // Vincoli per codec: profondità di bit, livello, risoluzione, profili, canali, intervallo dinamico.
    let mut unsupported: Vec<String> = Vec::new();
    // HDR dichiarato per esclusione (Jellyfin per Android TV): `VideoRangeType NotEquals "DOVI|HDR10Plus|…"`.
    let mut hdr_excluded: Option<Vec<String>> = None;
    for cp in p["CodecProfiles"].as_array().into_iter().flatten() {
        let kind = cp["Type"].as_str().unwrap_or("").to_ascii_lowercase();
        let codecs = list(&cp["Codec"]);
        let conditional = cp["ApplyConditions"].as_array().is_some_and(|a| !a.is_empty());
        for cond in cp["Conditions"].as_array().into_iter().flatten() {
            let prop = cond["Property"].as_str().unwrap_or("");
            let op = cond["Condition"].as_str().unwrap_or("");
            let val = cond["Value"].as_str().unwrap_or("");
            let num = val.parse::<u32>().ok();
            if kind == "video" {
                // Codec che il dispositivo non decodifica: l'app lo elenca nel direct play e lo esclude così.
                if (prop, op) == ("VideoProfile", "Equals") && val.eq_ignore_ascii_case("none") && !conditional {
                    unsupported.extend(codecs.iter().map(|c| video_codec(c)));
                    continue;
                }
                if prop == "VideoRangeType" && op == "NotEquals" && (codecs.is_empty() || codecs.iter().any(|c| video_codec(c) == "hevc")) {
                    hdr_excluded = Some(list(&cond["Value"]));
                    continue;
                }
                for v in
                    out.video_codecs.iter_mut().filter(|v| codecs.is_empty() || codecs.contains(&v.codec) || codecs.iter().any(|c| video_codec(c) == v.codec))
                {
                    match (prop, op) {
                        ("VideoBitDepth", "LessThanEqual") => v.max_bit_depth = num.map(|n| n as u8),
                        ("VideoLevel", "LessThanEqual") => v.max_level = num,
                        ("Width", "LessThanEqual") => v.max_width = num,
                        ("Height", "LessThanEqual") => v.max_height = num,
                        ("VideoProfile", "EqualsAny") => v.profiles = list(&cond["Value"]),
                        // Swiftfin con AVPlayer: HEVC solo `hvc1|dvh1`.
                        ("VideoCodecTag", "EqualsAny") => v.codec_tags = list(&cond["Value"]),
                        ("VideoProfile", "Equals") if !conditional => v.profiles = vec![val.to_ascii_lowercase()],
                        // AV1 senza Main 10: solo 8 bit.
                        ("VideoProfile", "NotEquals") if val.eq_ignore_ascii_case("main 10") => v.max_bit_depth = Some(8),
                        _ => {}
                    }
                }
                if prop == "VideoRangeType" && op == "EqualsAny" {
                    for r in list(&cond["Value"]) {
                        let h = match r.as_str() {
                            "hdr10" => Some("hdr10"),
                            "hdr10plus" => Some("hdr10plus"),
                            "hlg" => Some("hlg"),
                            // Solo il Dolby Vision vero: DOVIWithSDR/HLG/HDR10 sono lo strato base (AVPlayer elenca
                            // sempre DOVIWithSDR), e per quelli basta l'HDR10 o l'SDR.
                            "dovi" => Some("dovi"),
                            _ => None,
                        };
                        if let Some(h) = h.map(String::from) {
                            if !out.hdr.contains(&h) {
                                out.hdr.push(h);
                            }
                        }
                    }
                }
            } else if kind == "videoaudio" || kind == "audio" {
                if let ("AudioChannels", "LessThanEqual") = (prop, op) {
                    for a in out.audio_codecs.iter_mut().filter(|a| codecs.is_empty() || codecs.iter().any(|c| audio_codec(c) == a.codec)) {
                        a.max_channels = num;
                    }
                }
            }
        }
    }
    out.video_codecs.retain(|v| !unsupported.contains(&v.codec));
    if let Some(excluded) = hdr_excluded {
        // Supportato ciò che non è escluso, come fa Jellyfin. HLG non è mai elencato dall'app: segue l'HDR10.
        let has = |n: &str| excluded.iter().any(|x| x == n);
        for (name, ours) in [("hdr10", "hdr10"), ("hdr10plus", "hdr10plus"), ("dovi", "dovi")] {
            if !has(name) && !out.hdr.iter().any(|h| h == ours) {
                out.hdr.push(ours.into());
            }
        }
        if !has("hlg") && !has("hdr10") && !out.hdr.iter().any(|h| h == "hlg") {
            out.hdr.push("hlg".into());
        }
    }
    // Transcodifica in HLS: i codec accettati nei segmenti. L'audio solo fra quelli che stanno sia in MPEG-TS sia in
    // fMP4: Platea non sa ancora quali segmenti farà (H.264 → TS) e le app dichiarano anche FLAC, ALAC, TrueHD, DTS.
    let hls = |t: &&Value| {
        t["Type"].as_str().is_none_or(|x| x.eq_ignore_ascii_case("video")) && t["Protocol"].as_str().is_some_and(|x| x.eq_ignore_ascii_case("hls"))
    };
    for t in p["TranscodingProfiles"].as_array().into_iter().flatten().filter(hls) {
        if !out.protocols.iter().any(|x| x == "hls") {
            out.protocols.push("hls".into());
        }
        for c in list(&t["VideoCodec"]) {
            let c = video_codec(&c);
            if !out.hls_video_codecs.contains(&c) {
                out.hls_video_codecs.push(c);
            }
        }
        for c in list(&t["AudioCodec"]) {
            let c = audio_codec(&c);
            if matches!(c.as_str(), "aac" | "ac3" | "eac3" | "mp3") && !out.hls_audio_codecs.contains(&c) {
                out.hls_audio_codecs.push(c);
            }
        }
    }
    if out.hls_video_codecs.is_empty() {
        out.hls_video_codecs.push("h264".into());
    }
    if out.hls_audio_codecs.is_empty() {
        out.hls_audio_codecs.push("aac".into());
    }
    // Sottotitoli che l'app disegna da sé: testo consegnato a parte, immagini incorporate.
    for sp in p["SubtitleProfiles"].as_array().into_iter().flatten() {
        let f = sp["Format"].as_str().unwrap_or("").to_ascii_lowercase();
        let method = sp["Method"].as_str().unwrap_or("");
        let (text, image) = match f.as_str() {
            "srt" | "subrip" => (Some("srt"), None),
            "vtt" | "webvtt" => (Some("vtt"), None),
            "ass" => (Some("ass"), None),
            "ssa" => (Some("ass"), None),
            "pgssub" | "pgs" => (None, Some("pgs")),
            "dvdsub" | "vobsub" => (None, Some("dvdsub")),
            _ => (None, None),
        };
        if let (Some(t), "External" | "Embed") = (text, method) {
            if !out.subtitle_text.iter().any(|x| x == t) {
                out.subtitle_text.push(t.into());
            }
        }
        if let (Some(i), "Embed") = (image, method) {
            if !out.subtitle_image.iter().any(|x| x == i) {
                out.subtitle_image.push(i.into());
            }
        }
    }
    out
}

/// Profilo per un'app che riproduce da sé qualsiasi file (Infuse): accetta il file così com'è, tracce e sottotitoli
/// compresi, e lascia come unico limite il bitrate chiesto. Sopra quel bitrate si converte in HLS con i codec che
/// l'app dichiara nei TranscodingProfiles (H.264 e AAC se non ne dichiara).
pub fn plays_everything(p: &Value, media: &platea_playback::MediaInfo, max_bitrate: Option<u64>) -> DeviceProfile {
    let mut out = convert(p, max_bitrate);
    out.containers = vec![media.container.clone()];
    out.video_codecs = media.video.iter().map(|v| VideoCodecSupport { codec: v.codec.clone(), ..Default::default() }).collect();
    out.audio_codecs = media.audio.iter().map(|a| AudioCodecSupport { codec: a.codec.clone(), max_channels: None }).collect();
    out.hdr = ["hdr10", "hdr10plus", "hlg", "dovi"].map(String::from).to_vec();
    out.subtitle_text = ["srt", "ass", "vtt"].map(String::from).to_vec();
    out.subtitle_image = ["pgs", "dvdsub"].map(String::from).to_vec();
    out.protocols = vec!["file".into(), "hls".into()];
    out
}

/// Il file va in direct play solo se contenitore, video e audio stanno nello stesso `DirectPlayProfile`, come in
/// Jellyfin. `convert` li unisce (il profilo di Platea è uno solo): Swiftfin con AVPlayer ne ha uno per contenitore
/// (AVI solo con MJPEG, MP3 solo in MOV) e senza questo controllo un AVI MPEG-4 + MP3 risulterebbe riproducibile.
pub fn direct_play_matches(p: &Value, media: &platea_playback::MediaInfo, audio_index: Option<u32>) -> bool {
    let video = media.video.as_ref().map(|v| video_codec(&v.codec.to_ascii_lowercase()));
    let audio = audio_index
        .and_then(|i| media.audio.iter().find(|a| a.index == i))
        .or_else(|| media.audio.iter().find(|a| a.default))
        .or_else(|| media.audio.first())
        .map(|a| audio_codec(&a.codec.to_ascii_lowercase()));
    let file_container = container(&media.container.to_ascii_lowercase());
    p["DirectPlayProfiles"].as_array().into_iter().flatten().filter(|d| d["Type"].as_str().is_none_or(|t| t.eq_ignore_ascii_case("video"))).any(|d| {
        let ok = |field: &str, value: &Option<String>, norm: fn(&str) -> String| {
            let l = list(&d[field]);
            l.is_empty() || value.as_ref().is_none_or(|v| l.iter().any(|x| norm(x) == *v))
        };
        ok("Container", &Some(file_container.clone()), container) && ok("VideoCodec", &video, video_codec) && ok("AudioCodec", &audio, audio_codec)
    })
}

/// Codec audio dichiarati dal profilo, per il log: `aac 6ch, ac3, eac3`.
pub fn audio_summary(p: &DeviceProfile) -> String {
    let v: Vec<String> = p
        .audio_codecs
        .iter()
        .map(|a| match a.max_channels {
            Some(ch) => format!("{} {ch}ch", a.codec),
            None => a.codec.clone(),
        })
        .collect();
    if v.is_empty() {
        "nessuno".into()
    } else {
        v.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn android_like_profile() {
        let p = json!({
            "Name": "ExoPlayer",
            "MaxStreamingBitrate": 20000000,
            "DirectPlayProfiles": [
                { "Type": "Video", "Container": "mp4,mkv,webm", "VideoCodec": "h264,vp9", "AudioCodec": "aac,mp3,opus,ac3" },
                { "Type": "Audio", "Container": "flac" }
            ],
            "CodecProfiles": [
                { "Type": "Video", "Codec": "h264", "Conditions": [
                    { "Condition": "LessThanEqual", "Property": "VideoBitDepth", "Value": "8" },
                    { "Condition": "LessThanEqual", "Property": "VideoLevel", "Value": "51" },
                    { "Condition": "EqualsAny", "Property": "VideoProfile", "Value": "high|main|baseline" }
                ]},
                { "Type": "VideoAudio", "Codec": "aac", "Conditions": [{ "Condition": "LessThanEqual", "Property": "AudioChannels", "Value": "6" }] }
            ],
            "TranscodingProfiles": [{ "Type": "Video", "Container": "ts", "Protocol": "hls", "VideoCodec": "h264", "AudioCodec": "aac,ac3" }],
            "SubtitleProfiles": [{ "Format": "srt", "Method": "External" }, { "Format": "pgssub", "Method": "Embed" }, { "Format": "ass", "Method": "Encode" }]
        });
        let d = convert(&p, None);
        assert_eq!(d.containers, vec!["mp4", "mkv", "webm"]);
        let h = d.video_codecs.iter().find(|v| v.codec == "h264").unwrap();
        assert_eq!((h.max_bit_depth, h.max_level, h.profiles.len()), (Some(8), Some(51), 3));
        assert!(!d.video_codecs.iter().any(|v| v.codec == "hevc"));
        assert_eq!(d.audio_codecs.iter().find(|a| a.codec == "aac").unwrap().max_channels, Some(6));
        assert_eq!(d.max_bitrate, Some(20_000_000));
        assert!(d.protocols.contains(&"hls".to_string()));
        assert_eq!(d.hls_audio_codecs, vec!["aac", "ac3"]);
        assert_eq!(audio_summary(&d), "aac 6ch, mp3, opus, ac3");
        assert_eq!(d.subtitle_text, vec!["srt"]);
        assert_eq!(d.subtitle_image, vec!["pgs"]);
    }

    #[test]
    fn decision_with_converted_profile() {
        // HEVC 10 bit in MKV su un profilo solo H.264 8 bit: non può essere direct play.
        let p = json!({
            "DirectPlayProfiles": [{ "Type": "Video", "Container": "mkv,mp4", "VideoCodec": "h264", "AudioCodec": "aac,ac3" }],
            "TranscodingProfiles": [{ "Type": "Video", "Container": "ts", "Protocol": "hls", "VideoCodec": "h264", "AudioCodec": "aac" }]
        });
        let d = convert(&p, None);
        let media = platea_playback::MediaInfo {
            container: "mkv".into(),
            video: Some(platea_playback::VideoInfo { index: 0, codec: "hevc".into(), bit_depth: Some(10), width: 1920, height: 1080, ..Default::default() }),
            audio: vec![platea_playback::AudioInfo { index: 1, codec: "ac3".into(), channels: 6, language: None, default: true, bitrate: None }],
            ..Default::default()
        };
        let req =
            platea_playback::PlaybackRequest { allow_remux: true, allow_transcode: true, server_video_encoders: vec!["h264".into()], ..Default::default() };
        let dec = platea_playback::decide(&media, &d, &req).unwrap();
        assert_ne!(dec.method, platea_playback::Method::DirectPlay);
    }

    /// Profilo come lo costruisce Jellyfin per Android TV 0.20 (`util/profile/deviceProfile.kt`) su un dispositivo
    /// con HEVC Main 10, HDR10 e Dolby Vision, senza AV1 né VC1, con E-AC3 in bitstream spento.
    #[test]
    fn android_tv_020_profile() {
        let p = json!({
            "Name": "AndroidTV-Default",
            "MaxStreamingBitrate": 120000000,
            "TranscodingProfiles": [
                { "Type": "Video", "Container": "ts", "Protocol": "hls", "VideoCodec": "hevc,h264", "AudioCodec": "aac,ac3,mp3" },
                { "Type": "Video", "Container": "mp4", "Protocol": "hls", "VideoCodec": "hevc,h264", "AudioCodec": "aac,ac3,mp3,alac,flac,opus,dts" },
                { "Type": "Audio", "Container": "ts", "Protocol": "hls", "AudioCodec": "aac" }
            ],
            "DirectPlayProfiles": [
                { "Type": "Video", "Container": "asf,hls,m4v,mkv,mov,mp4,ogm,ogv,ts,vob,webm,wmv,xvid",
                  "VideoCodec": "av1,h264,hevc,mpeg,mpeg2video,vc1,vp8,vp9",
                  "AudioCodec": "aac,aac_latm,ac3,alac,dca,dts,flac,mlp,mp2,mp3,opus,pcm_alaw,pcm_mulaw,pcm_s16le,pcm_s20le,pcm_s24le,truehd,vorbis" },
                { "Type": "Audio", "AudioCodec": "aac,ac3" }
            ],
            "CodecProfiles": [
                { "Type": "Video", "Codec": "hevc", "Conditions": [{ "Condition": "EqualsAny", "Property": "VideoProfile", "Value": "main|main 10", "IsRequired": false }] },
                { "Type": "Video", "Codec": "hevc", "ApplyConditions": [{ "Condition": "Equals", "Property": "VideoProfile", "Value": "main" }],
                  "Conditions": [{ "Condition": "LessThanEqual", "Property": "VideoLevel", "Value": "153" }] },
                { "Type": "Video", "Codec": "av1", "Conditions": [{ "Condition": "Equals", "Property": "VideoProfile", "Value": "none" }] },
                { "Type": "Video", "Codec": "vc1", "Conditions": [{ "Condition": "Equals", "Property": "VideoProfile", "Value": "none" }] },
                { "Type": "Video", "Codec": "hevc",
                  "ApplyConditions": [{ "Condition": "EqualsAny", "Property": "VideoRangeType", "Value": "DOVIInvalid|DOVIWithEL|HDR10Plus" }],
                  "Conditions": [{ "Condition": "NotEquals", "Property": "VideoRangeType", "Value": "DOVIInvalid|DOVIWithEL|HDR10Plus" }] },
                { "Type": "VideoAudio", "Conditions": [{ "Condition": "LessThanEqual", "Property": "AudioChannels", "Value": "8" }] }
            ]
        });
        let d = convert(&p, None);
        let codecs: Vec<&str> = d.video_codecs.iter().map(|v| v.codec.as_str()).collect();
        assert_eq!(codecs, vec!["h264", "hevc", "mpeg", "mpeg2video", "vp8", "vp9"]);
        assert_eq!(d.hdr, vec!["hdr10", "dovi", "hlg"]);
        // E-AC3 non dichiarato: va convertito, mai mandato diretto.
        assert!(!d.audio_codecs.iter().any(|a| a.codec == "eac3"));
        assert!(d.audio_codecs.iter().all(|a| a.max_channels == Some(8)));
        assert_eq!(d.hls_audio_codecs, vec!["aac", "ac3", "mp3"]);
        assert_eq!(d.max_bitrate, Some(120_000_000));

        let req =
            platea_playback::PlaybackRequest { allow_remux: true, allow_transcode: true, server_video_encoders: vec!["h264".into()], ..Default::default() };
        let hevc = |hdr: Option<&str>, depth: u8| platea_playback::VideoInfo {
            codec: "hevc".into(),
            profile: Some(if depth == 10 { "main 10" } else { "main" }.into()),
            bit_depth: Some(depth),
            width: 3840,
            height: 2160,
            hdr: hdr.map(String::from),
            ..Default::default()
        };
        let audio =
            |codec: &str, ch: u32| platea_playback::AudioInfo { index: 1, codec: codec.into(), channels: ch, language: None, default: true, bitrate: None };
        // Tulsa King S01E01: HEVC + E-AC3 2.0 → video copiato, audio convertito.
        let m = platea_playback::MediaInfo { container: "mkv".into(), video: Some(hevc(None, 8)), audio: vec![audio("eac3", 2)], ..Default::default() };
        let dec = platea_playback::decide(&m, &d, &req).unwrap();
        assert_eq!(dec.method, platea_playback::Method::Remux);
        assert!(matches!(dec.audio, platea_playback::AudioAction::Transcode { .. }));
        // HEVC HDR10 + AC3: direct play, niente tonemapping.
        let m =
            platea_playback::MediaInfo { container: "mkv".into(), video: Some(hevc(Some("hdr10"), 10)), audio: vec![audio("ac3", 6)], ..Default::default() };
        assert_eq!(platea_playback::decide(&m, &d, &req).unwrap().method, platea_playback::Method::DirectPlay);
    }

    fn media(container: &str, video: &str, audio: &str) -> platea_playback::MediaInfo {
        platea_playback::MediaInfo {
            container: container.into(),
            video: Some(platea_playback::VideoInfo {
                codec: video.into(),
                profile: Some("main".into()),
                bit_depth: Some(8),
                width: 1920,
                height: 1080,
                ..Default::default()
            }),
            audio: vec![platea_playback::AudioInfo { index: 1, codec: audio.into(), channels: 2, language: None, default: true, bitrate: None }],
            ..Default::default()
        }
    }

    fn req() -> platea_playback::PlaybackRequest {
        platea_playback::PlaybackRequest { allow_remux: true, allow_transcode: true, server_video_encoders: vec!["h264".into()], ..Default::default() }
    }

    /// Swiftfin 1.7.1 con AVPlayer (`MediaPlayerType+AVPlayer.swift`) su un iPhone con HEVC e senza Dolby Vision.
    #[test]
    fn swiftfin_avplayer_profile() {
        let p = json!({
            "DirectPlayProfiles": [
                { "Type": "Audio", "Container": "m4a,m4b,mp4,mov,aac,ac3,eac3", "AudioCodec": "aac,alac,ac3,eac3" },
                { "Type": "Video", "Container": "mp4,m4v", "VideoCodec": "h264,mpeg4,hevc", "AudioCodec": "aac,ac3,alac,eac3,flac" },
                { "Type": "Video", "Container": "mov", "VideoCodec": "h264,mjpeg,mpeg4,hevc", "AudioCodec": "aac,ac3,alac,eac3,mp3,pcm_s16be,pcm_s16le,pcm_s24be,pcm_s24le" },
                { "Type": "Video", "Container": "3g2,3gp", "VideoCodec": "h264,mpeg4", "AudioCodec": "aac,amr_nb" },
                { "Type": "Video", "Container": "avi", "VideoCodec": "mjpeg", "AudioCodec": "pcm_mulaw,pcm_s16le" }
            ],
            "TranscodingProfiles": [
                { "Type": "Audio", "Container": "mp3", "Protocol": "http", "AudioCodec": "mp3" },
                { "Type": "Video", "Container": "mp4", "Protocol": "hls", "VideoCodec": "hevc,h264,mpeg4", "AudioCodec": "aac,ac3,alac,eac3,flac", "MaxAudioChannels": "8" }
            ],
            "CodecProfiles": [
                { "Type": "Video", "Codec": "hevc", "Conditions": [
                    { "Condition": "EqualsAny", "Property": "VideoProfile", "Value": "main|main 10", "IsRequired": false },
                    { "Condition": "LessThanEqual", "Property": "VideoLevel", "Value": "175", "IsRequired": false },
                    { "Condition": "EqualsAny", "Property": "VideoRangeType", "Value": "SDR|DOVIWithSDR|HLG|DOVIWithHLG|HDR10|HDR10Plus|DOVIWithHDR10|DOVIWithHDR10Plus|DOVIWithEL|DOVIWithELHDR10Plus", "IsRequired": false },
                    { "Condition": "EqualsAny", "Property": "VideoCodecTag", "Value": "hvc1|dvh1", "IsRequired": true }
                ] }
            ]
        });
        let d = convert(&p, None);
        // Niente Dolby Vision vero: DOVIWithSDR/HDR10 sono solo lo strato base.
        assert_eq!(d.hdr, vec!["hlg", "hdr10", "hdr10plus"]);
        assert_eq!(d.hls_audio_codecs, vec!["aac", "ac3", "eac3"]);
        assert_eq!(d.video_codecs.iter().find(|v| v.codec == "hevc").unwrap().codec_tags, vec!["hvc1", "dvh1"]);
        // MP4 HEVC etichettato hev1: AVPlayer non lo apre, va in HLS (che etichetta hvc1).
        let mut m = media("mp4", "hevc", "aac");
        m.video.as_mut().unwrap().codec_tag = Some("hev1".into());
        let dec = platea_playback::decide(&m, &d, &req()).unwrap();
        assert_eq!((dec.method, dec.reasons), (platea_playback::Method::Remux, vec![platea_playback::Reason::VideoCodecTagNotSupported]));
        m.video.as_mut().unwrap().codec_tag = Some("hvc1".into());
        assert_eq!(platea_playback::decide(&m, &d, &req()).unwrap().method, platea_playback::Method::DirectPlay);
        // MKV: non è fra i contenitori, va in HLS.
        let m = media("mkv", "hevc", "eac3");
        assert_eq!(platea_playback::decide(&m, &d, &req()).unwrap().method, platea_playback::Method::Remux);
        // MP4 H.264 + AAC: direct play, nello stesso profilo.
        let m = media("mp4", "h264", "aac");
        assert_eq!(platea_playback::decide(&m, &d, &req()).unwrap().method, platea_playback::Method::DirectPlay);
        assert!(direct_play_matches(&p, &m, None));
        // AVI MPEG-4 + MP3: ammessi uno per uno, ma da profili diversi.
        assert!(!direct_play_matches(&p, &media("avi", "mpeg4", "mp3"), None));
        // MP3 nel MOV sì, nell'MP4 no.
        assert!(direct_play_matches(&p, &media("mov", "h264", "mp3"), None));
        assert!(!direct_play_matches(&p, &media("mp4", "h264", "mp3"), None));
    }

    /// Swiftfin 1.7.1 con MPV (`MediaPlayerType+MPV.swift`): MKV, DTS e TrueHD diretti; nell'HLS solo l'audio che sta in TS.
    #[test]
    fn swiftfin_mpv_profile() {
        let p = json!({
            "DirectPlayProfiles": [
                { "Type": "Video", "Container": "avi,flv,m4v,mkv,mov,mp4,mpegts,ts,3g2,3gp,webm",
                  "VideoCodec": "ffv1,flv1,h263,h264,hevc,mjpeg,mpeg1video,mpeg2video,mpeg4,prores,vc1,vp8,vp9,wmv1,wmv2,wmv3",
                  "AudioCodec": "aac,ac3,alac,amr_nb,amr_wb,dts,dts_hd,eac3,flac,mp1,mp2,mp3,opus,truehd,vorbis" }
            ],
            "TranscodingProfiles": [
                { "Type": "Video", "Container": "mp4", "Protocol": "hls", "VideoCodec": "h264,hevc,vp9", "AudioCodec": "aac,ac3,alac,dts,eac3,flac,mp3,opus,truehd" }
            ]
        });
        let d = convert(&p, None);
        assert_eq!(d.hls_audio_codecs, vec!["aac", "ac3", "eac3", "mp3"]);
        let m = media("mkv", "hevc", "truehd");
        assert_eq!(platea_playback::decide(&m, &d, &req()).unwrap().method, platea_playback::Method::DirectPlay);
        assert!(direct_play_matches(&p, &m, None));
    }

    /// Findroid 1.2.0 (`JellyfinRepositoryImpl.getMediaSources`): profilo vuoto, l'app apre sempre il file diretto.
    #[test]
    fn findroid_empty_profile() {
        let p = json!({ "Name": "Direct play all", "MaxStaticBitrate": 1000000000, "MaxStreamingBitrate": 1000000000,
            "CodecProfiles": [], "ContainerProfiles": [], "DirectPlayProfiles": [], "TranscodingProfiles": [],
            "SubtitleProfiles": [{ "Format": "srt", "Method": "External" }, { "Format": "ass", "Method": "External" }] });
        let d = convert(&p, None);
        assert!(d.containers.is_empty() && !d.protocols.contains(&"hls".to_string()));
        assert_eq!(d.subtitle_text, vec!["srt", "ass"]);
    }

    /// Infuse 8.5, "Transcodifica" a 3 Mbit/s: profilo senza TranscodingProfiles, file HEVC 4K DV con E-AC3.
    #[test]
    fn infuse_quality_choice() {
        let p = json!({});
        let mut m = media("mkv", "hevc", "eac3");
        m.bitrate = Some(25_000_000);
        m.video.as_mut().unwrap().hdr = Some("dovi".into());
        // Senza limite: tutto diretto, anche DV e E-AC3.
        let d = plays_everything(&p, &m, None);
        assert_eq!(platea_playback::decide(&m, &d, &req()).unwrap().method, platea_playback::Method::DirectPlay);
        // 3 Mbit/s: conversione in HLS H.264 + AAC, con tonemapping.
        let d = plays_everything(&p, &m, Some(3_000_000));
        let dec = platea_playback::decide(&m, &d, &req()).unwrap();
        assert_eq!(dec.method, platea_playback::Method::Transcode);
        assert!(matches!(dec.video, platea_playback::VideoAction::Transcode { ref codec, bitrate, .. } if codec == "h264" && bitrate <= 3_000_000));
        assert_eq!(d.hls_audio_codecs, vec!["aac"]);
        // File già sotto il limite: resta diretto.
        m.bitrate = Some(2_000_000);
        assert_eq!(platea_playback::decide(&m, &d, &req()).unwrap().method, platea_playback::Method::DirectPlay);
    }
}
