use super::*;

fn h264_1080p_mp4() -> MediaInfo {
    MediaInfo {
        container: "mp4".into(),
        bitrate: Some(8_000_000),
        duration_ms: Some(7_200_000),
        video: Some(VideoInfo {
            index: 0,
            codec: "h264".into(),
            profile: Some("high".into()),
            level: Some(41),
            bit_depth: Some(8),
            width: 1920,
            height: 1080,
            hdr: None,
            bitrate: Some(7_500_000),
        }),
        audio: vec![AudioInfo { index: 1, codec: "aac".into(), channels: 2, language: Some("ita".into()), default: true, bitrate: Some(192_000) }],
        subtitles: vec![],
    }
}

fn chrome() -> DeviceProfile {
    DeviceProfile::default()
}

fn req() -> PlaybackRequest {
    PlaybackRequest::default()
}

#[test]
fn direct_play_when_everything_matches() {
    let d = decide(&h264_1080p_mp4(), &chrome(), &req()).unwrap();
    assert_eq!(d.method, Method::DirectPlay);
    assert!(d.reasons.is_empty());
    assert_eq!(d.video, VideoAction::Copy);
    assert_eq!(d.audio, AudioAction::Copy);
}

#[test]
fn mkv_container_means_remux() {
    let mut m = h264_1080p_mp4();
    m.container = "mkv".into();
    let d = decide(&m, &chrome(), &req()).unwrap();
    assert_eq!(d.method, Method::Remux);
    assert_eq!(d.reasons, vec![Reason::ContainerNotSupported]);
    assert_eq!(d.video, VideoAction::Copy);
    assert_eq!(d.audio, AudioAction::Copy);
}

#[test]
fn unsupported_audio_converts_only_audio() {
    let mut m = h264_1080p_mp4();
    m.audio[0].codec = "dts".into();
    m.audio[0].channels = 6;
    let d = decide(&m, &chrome(), &req()).unwrap();
    assert_eq!(d.method, Method::Remux);
    assert_eq!(d.reasons, vec![Reason::AudioCodecNotSupported]);
    assert_eq!(d.video, VideoAction::Copy);
    assert!(matches!(d.audio, AudioAction::Transcode { ref codec, channels: 6, .. } if codec == "aac"));
}

#[test]
fn hevc_on_browser_without_hevc_transcodes() {
    let mut m = h264_1080p_mp4();
    m.container = "mkv".into();
    let v = m.video.as_mut().unwrap();
    v.codec = "hevc".into();
    v.profile = Some("main 10".into());
    v.bit_depth = Some(10);
    let d = decide(&m, &chrome(), &req()).unwrap();
    assert_eq!(d.method, Method::Transcode);
    assert_eq!(d.primary_reason(), Some(Reason::VideoCodecNotSupported));
    assert!(matches!(d.video, VideoAction::Transcode { ref codec, .. } if codec == "h264"));
    // L'audio supportato resta copiato.
    assert_eq!(d.audio, AudioAction::Copy);
}

#[test]
fn h264_10bit_transcodes_with_bit_depth_reason() {
    let mut m = h264_1080p_mp4();
    m.video.as_mut().unwrap().bit_depth = Some(10);
    let d = decide(&m, &chrome(), &req()).unwrap();
    assert_eq!(d.method, Method::Transcode);
    assert_eq!(d.reasons, vec![Reason::VideoBitDepthNotSupported]);
}

#[test]
fn bitrate_limit_forces_transcode_and_caps_bitrate() {
    let m = h264_1080p_mp4();
    let r = PlaybackRequest { user_max_bitrate: Some(3_000_000), ..req() };
    let d = decide(&m, &chrome(), &r).unwrap();
    assert_eq!(d.method, Method::Transcode);
    assert_eq!(d.reasons, vec![Reason::BitrateTooHigh]);
    match d.video {
        VideoAction::Transcode { bitrate, .. } => assert!(bitrate <= 3_000_000, "{bitrate}"),
        _ => panic!(),
    }
}

#[test]
fn hdr_on_sdr_screen_tonemaps() {
    let mut m = h264_1080p_mp4();
    let v = m.video.as_mut().unwrap();
    v.codec = "hevc".into();
    v.hdr = Some("hdr10".into());
    v.bit_depth = Some(10);
    let mut p = chrome();
    p.video_codecs.push(VideoCodecSupport { codec: "hevc".into(), max_bit_depth: Some(10), ..Default::default() });
    let d = decide(&m, &p, &req()).unwrap();
    assert_eq!(d.method, Method::Transcode);
    assert_eq!(d.reasons, vec![Reason::HdrNotSupported]);
    assert!(matches!(d.video, VideoAction::Transcode { tonemap: true, .. }));
}

#[test]
fn text_subtitles_never_touch_video() {
    let mut m = h264_1080p_mp4();
    m.subtitles.push(SubtitleInfo { index: 2, codec: "subrip".into(), language: Some("ita".into()), ..Default::default() });
    let r = PlaybackRequest { subtitle_index: Some(2), ..req() };
    let d = decide(&m, &chrome(), &r).unwrap();
    assert_eq!(d.method, Method::DirectPlay);
    assert_eq!(d.subtitle, SubtitleDelivery::External { index: 2 });
}

#[test]
fn image_subtitles_are_burned_in() {
    let mut m = h264_1080p_mp4();
    m.subtitles.push(SubtitleInfo { index: 3, codec: "hdmv_pgs_subtitle".into(), ..Default::default() });
    let r = PlaybackRequest { subtitle_index: Some(3), ..req() };
    let d = decide(&m, &chrome(), &r).unwrap();
    assert_eq!(d.method, Method::Transcode);
    assert_eq!(d.reasons, vec![Reason::SubtitleBurnIn]);
    assert!(matches!(d.video, VideoAction::Transcode { burn_subtitle: Some(3), .. }));
}

#[test]
fn image_subtitles_passed_when_client_draws_them() {
    let mut m = h264_1080p_mp4();
    m.container = "mkv".into();
    m.subtitles.push(SubtitleInfo { index: 3, codec: "hdmv_pgs_subtitle".into(), ..Default::default() });
    let mut p = chrome();
    p.containers.push("mkv".into());
    p.subtitle_image.push("pgs".into());
    let r = PlaybackRequest { subtitle_index: Some(3), ..req() };
    let d = decide(&m, &p, &r).unwrap();
    assert_eq!(d.method, Method::DirectPlay);
    assert_eq!(d.subtitle, SubtitleDelivery::Embedded { index: 3 });
}

#[test]
fn admin_can_forbid_transcode_and_remux() {
    let mut m = h264_1080p_mp4();
    m.container = "mkv".into();
    let r = PlaybackRequest { allow_remux: false, ..req() };
    assert_eq!(decide(&m, &chrome(), &r), Err(DecisionError::RemuxNotAllowed(vec![Reason::ContainerNotSupported])));
    m.video.as_mut().unwrap().codec = "hevc".into();
    let r = PlaybackRequest { allow_transcode: false, ..req() };
    assert!(matches!(decide(&m, &chrome(), &r), Err(DecisionError::TranscodeNotAllowed(_))));
}

#[test]
fn hevc_remux_needs_hls_support() {
    // Il browser decodifica HEVC (Safari) ma i segmenti HLS dichiarati accettano solo H.264.
    let mut m = h264_1080p_mp4();
    m.container = "mkv".into();
    m.video.as_mut().unwrap().codec = "hevc".into();
    let mut p = chrome();
    p.video_codecs.push(VideoCodecSupport { codec: "hevc".into(), ..Default::default() });
    let d = decide(&m, &p, &req()).unwrap();
    assert_eq!(d.method, Method::Transcode);
    assert_eq!(d.reasons, vec![Reason::HlsVideoCodecNotSupported]);
    p.hls_video_codecs.push("hevc".into());
    let d = decide(&m, &p, &req()).unwrap();
    assert_eq!(d.method, Method::Remux);
}

#[test]
fn multichannel_aac_over_limit_is_downmixed() {
    let mut m = h264_1080p_mp4();
    m.audio[0].channels = 8;
    let d = decide(&m, &chrome(), &req()).unwrap();
    assert_eq!(d.method, Method::Remux);
    assert_eq!(d.reasons, vec![Reason::AudioChannelsNotSupported]);
    assert!(matches!(d.audio, AudioAction::Transcode { channels: 6, .. }));
}

#[test]
fn picks_requested_audio_track() {
    let mut m = h264_1080p_mp4();
    m.audio.push(AudioInfo { index: 2, codec: "ac3".into(), channels: 6, language: Some("eng".into()), default: false, bitrate: None });
    let r = PlaybackRequest { audio_index: Some(2), ..req() };
    let d = decide(&m, &chrome(), &r).unwrap();
    assert_eq!(d.audio_index, Some(2));
    assert_eq!(d.method, Method::Remux);
}

#[test]
fn not_probed_is_an_error() {
    let m = MediaInfo::default();
    assert_eq!(decide(&m, &chrome(), &req()), Err(DecisionError::NotProbed));
}

#[test]
fn profile_json_roundtrip_with_defaults() {
    let p: DeviceProfile = serde_json::from_str(r#"{"name":"x","containers":["mp4"]}"#).unwrap();
    assert_eq!(p.containers, vec!["mp4"]);
    assert!(!p.protocols.is_empty());
}

#[test]
fn forced_transcode_of_audio_only_reencodes_audio() {
    // Audiolibro .m4b: niente video (la copertina non conta), AAC che il browser non è riuscito a leggere in diretta.
    let mut m = h264_1080p_mp4();
    m.video = None;
    m.bitrate = Some(128_000);
    let d = decide(&m, &chrome(), &PlaybackRequest { force_transcode: true, ..req() }).unwrap();
    assert_eq!(d.method, Method::Transcode);
    assert_eq!(d.video, VideoAction::Copy);
    assert!(matches!(d.audio, AudioAction::Transcode { ref codec, channels: 2, .. } if codec == "aac"));
}
