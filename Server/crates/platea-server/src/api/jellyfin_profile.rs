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
            out.video_codecs.push(VideoCodecSupport { codec: c, profiles: vec![], max_level: None, max_bit_depth: None, max_width: None, max_height: None });
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
    for cp in p["CodecProfiles"].as_array().into_iter().flatten() {
        let kind = cp["Type"].as_str().unwrap_or("").to_ascii_lowercase();
        let codecs = list(&cp["Codec"]);
        for cond in cp["Conditions"].as_array().into_iter().flatten() {
            let prop = cond["Property"].as_str().unwrap_or("");
            let op = cond["Condition"].as_str().unwrap_or("");
            let val = cond["Value"].as_str().unwrap_or("");
            let num = val.parse::<u32>().ok();
            if kind == "video" {
                for v in
                    out.video_codecs.iter_mut().filter(|v| codecs.is_empty() || codecs.contains(&v.codec) || codecs.iter().any(|c| video_codec(c) == v.codec))
                {
                    match (prop, op) {
                        ("VideoBitDepth", "LessThanEqual") => v.max_bit_depth = num.map(|n| n as u8),
                        ("VideoLevel", "LessThanEqual") => v.max_level = num,
                        ("Width", "LessThanEqual") => v.max_width = num,
                        ("Height", "LessThanEqual") => v.max_height = num,
                        ("VideoProfile", "EqualsAny") => v.profiles = list(&cond["Value"]),
                        _ => {}
                    }
                }
                if prop == "VideoRangeType" && op == "EqualsAny" {
                    for r in list(&cond["Value"]) {
                        let h = match r.as_str() {
                            "hdr10" => Some("hdr10"),
                            "hdr10plus" => Some("hdr10plus"),
                            "hlg" => Some("hlg"),
                            "dovi" | "dovihdr10" | "dovihlg" | "dovisdr" | "dovihdr10plus" => Some("dovi"),
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
    // Transcodifica in HLS: i codec accettati nei segmenti.
    for t in p["TranscodingProfiles"].as_array().into_iter().flatten() {
        if t["Type"].as_str().is_some_and(|x| !x.eq_ignore_ascii_case("video")) || !t["Protocol"].as_str().is_some_and(|x| x.eq_ignore_ascii_case("hls")) {
            continue;
        }
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
            if !out.hls_audio_codecs.contains(&c) {
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
}
