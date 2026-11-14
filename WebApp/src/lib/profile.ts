/**
 * Profilo dispositivo del browser: il client dichiara ciò che sa riprodurre, il server non indovina mai.
 * - Direct play: `canPlayType` dell'elemento video.
 * - HLS: i segmenti sono MPEG-TS; hls.js li trasforma in MP4 per Media Source (H.264, AAC, MP3).
 */
export interface DeviceProfile {
  name: string; containers: string[];
  video_codecs: { codec: string; profiles?: string[]; max_level?: number | null; max_bit_depth?: number | null; max_width?: number | null; max_height?: number | null }[];
  audio_codecs: { codec: string; max_channels?: number | null }[];
  subtitle_text: string[]; subtitle_image: string[]; hdr: string[]; max_bitrate: number | null; protocols: string[];
  hls_video_codecs: string[]; hls_audio_codecs: string[];
}

let cached: DeviceProfile | null = null;

export function buildProfile(maxBitrate: number | null): DeviceProfile {
  if (!cached) cached = detect();
  return { ...cached, max_bitrate: maxBitrate };
}

function detect(): DeviceProfile {
  const v = document.createElement("video");
  const can = (t: string) => v.canPlayType(t) !== "";
  const containers = ["mp4"];
  if (can('video/webm; codecs="vp9, opus"') || can("video/webm")) containers.push("webm");
  if (can('video/x-matroska; codecs="avc1.64001F, mp4a.40.2"') || can("video/x-matroska")) containers.push("mkv");
  if (can("video/quicktime")) containers.push("mov");
  containers.push("m4v");

  const video: DeviceProfile["video_codecs"] = [{ codec: "h264", profiles: ["baseline", "constrained baseline", "main", "high"], max_level: 52, max_bit_depth: 8 }];
  const hevc = can('video/mp4; codecs="hvc1.1.6.L153.B0"') || can('video/mp4; codecs="hev1.1.6.L153.B0"');
  const hevc10 = can('video/mp4; codecs="hvc1.2.4.L153.B0"');
  if (hevc) video.push({ codec: "hevc", profiles: hevc10 ? ["main", "main 10"] : ["main"], max_bit_depth: hevc10 ? 10 : 8 });
  if (can('video/mp4; codecs="av01.0.08M.08"') || can('video/webm; codecs="av01.0.08M.08"')) {
    video.push({ codec: "av1", max_bit_depth: can('video/mp4; codecs="av01.0.08M.10"') ? 10 : 8 });
  }
  if (can('video/webm; codecs="vp9"') || can('video/mp4; codecs="vp09.00.40.08"')) video.push({ codec: "vp9", max_bit_depth: 8 });
  if (can('video/webm; codecs="vp8"')) video.push({ codec: "vp8" });

  const audio: DeviceProfile["audio_codecs"] = [{ codec: "aac", max_channels: 6 }, { codec: "mp3", max_channels: 2 }];
  if (can('audio/webm; codecs="opus"') || can('audio/mp4; codecs="opus"')) audio.push({ codec: "opus", max_channels: 8 });
  if (can('audio/flac') || can('audio/mp4; codecs="flac"')) audio.push({ codec: "flac", max_channels: 8 });
  if (can('audio/webm; codecs="vorbis"')) audio.push({ codec: "vorbis", max_channels: 8 });
  if (can('audio/mp4; codecs="ac-3"')) audio.push({ codec: "ac3", max_channels: 6 });
  if (can('audio/mp4; codecs="ec-3"')) audio.push({ codec: "eac3", max_channels: 8 });

  const hdr: string[] = [];
  if (hevc10 && window.matchMedia?.("(dynamic-range: high)").matches) hdr.push("hdr10", "hlg");

  const hlsAudio = ["aac", "mp3"];
  // Nei segmenti HLS conta ciò che accetta MediaSource (hls.js); senza MSE (Safari su iPhone) decide il player nativo.
  const mse = (t: string) => (typeof MediaSource !== "undefined" && MediaSource.isTypeSupported ? MediaSource.isTypeSupported(t) : false);
  const nativeHls = typeof MediaSource === "undefined";
  const hlsHevc = hevc && (nativeHls || mse('video/mp4; codecs="hvc1.1.6.L120.90"'));
  const hlsAv1 = video.some((v) => v.codec === "av1") && (nativeHls || mse('video/mp4; codecs="av01.0.08M.08"'));
  return {
    name: "Platea Web",
    containers,
    video_codecs: video,
    audio_codecs: audio,
    subtitle_text: ["vtt"],
    subtitle_image: [],
    hdr,
    max_bitrate: null,
    protocols: ["file", "hls"],
    // HEVC e AV1 nei segmenti HLS arrivano in fMP4 (copiati dal file, senza transcodifica).
    hls_video_codecs: ["h264", ...(hlsHevc ? ["hevc"] : []), ...(hlsAv1 ? ["av1"] : [])],
    hls_audio_codecs: hlsAudio,
  };
}
