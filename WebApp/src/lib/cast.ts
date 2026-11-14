/**
 * Google Cast (Chromecast, TV con Google TV) dal player web.
 *
 * - L'SDK del mittente si carica solo quando serve (pagina del player) e solo nei browser che lo supportano
 *   (Chrome, Edge su desktop e Android); altrove il pulsante non compare.
 * - Si usa il ricevitore predefinito di Google: legge HLS e MP4 da un indirizzo pubblico. Il server crea per
 *   il Chromecast una sessione propria, con un profilo adatto (H.264 + AAC), e il ricevitore scarica i
 *   segmenti direttamente dal server (indirizzi con il token della sessione).
 */
import type { DeviceProfile } from "./profile";

/* eslint-disable @typescript-eslint/no-explicit-any */
declare global {
  interface Window {
    __onGCastApiAvailable?: (available: boolean) => void;
    cast?: any;
    chrome?: any;
  }
}

let loading: Promise<boolean> | null = null;

/** Carica l'SDK una volta; `false` se il browser non supporta Cast. */
export function loadCast(): Promise<boolean> {
  if (loading) return loading;
  loading = new Promise((resolve) => {
    // Cast funziona solo in Chrome/Chromium e solo in un contesto sicuro (https o localhost).
    const chromium = /Chrome\/|CriOS\/|Edg\//.test(navigator.userAgent) && !/CriOS\//.test(navigator.userAgent);
    if (!chromium || !window.isSecureContext) return resolve(false);
    window.__onGCastApiAvailable = (ok) => {
      if (!ok || !window.cast?.framework) return resolve(false);
      try {
        window.cast.framework.CastContext.getInstance().setOptions({
          receiverApplicationId: window.chrome.cast.media.DEFAULT_MEDIA_RECEIVER_APP_ID,
          autoJoinPolicy: window.chrome.cast.AutoJoinPolicy.ORIGIN_SCOPED,
        });
        resolve(true);
      } catch {
        resolve(false);
      }
    };
    const s = document.createElement("script");
    s.src = "https://www.gstatic.com/cv/js/sender/v1/cast_sender.js?loadCastFramework=1";
    s.async = true;
    s.onerror = () => resolve(false);
    document.head.appendChild(s);
    // Rete che blocca gstatic: niente pulsante, nessuna attesa infinita.
    window.setTimeout(() => resolve(false), 15_000);
  });
  return loading;
}

export const castContext = () => window.cast?.framework?.CastContext.getInstance();

/** Profilo del ricevitore predefinito: H.264 fino a 1080p, AAC; MP4 e WebM letti direttamente. */
export function castProfile(maxBitrate: number | null): DeviceProfile {
  return {
    name: "Google Cast",
    containers: ["mp4", "m4v", "webm"],
    video_codecs: [
      { codec: "h264", profiles: ["baseline", "constrained baseline", "main", "high"], max_level: 42, max_bit_depth: 8, max_width: 1920, max_height: 1080 },
      { codec: "vp9", max_bit_depth: 8, max_width: 1920, max_height: 1080 },
    ],
    audio_codecs: [{ codec: "aac", max_channels: 6 }, { codec: "mp3", max_channels: 2 }, { codec: "opus", max_channels: 2 }],
    subtitle_text: ["vtt"],
    subtitle_image: [],
    hdr: [],
    max_bitrate: maxBitrate ?? 20_000_000,
    protocols: ["file", "hls"],
    hls_video_codecs: ["h264"],
    hls_audio_codecs: ["aac", "mp3"],
  } as DeviceProfile;
}

export interface CastLoad {
  url: string;
  hls: boolean;
  fmp4: boolean;
  container: string;
  startSec: number;
  title: string;
  subtitle?: string;
  poster?: string | null;
}

/** Manda il video al dispositivo della sessione Cast attiva. */
export async function castLoad(m: CastLoad): Promise<void> {
  const ctx = castContext();
  const session = ctx?.getCurrentSession();
  if (!session) throw new Error("no cast session");
  const { media } = window.chrome.cast;
  const contentType = m.hls ? "application/x-mpegURL" : m.container === "webm" ? "video/webm" : "video/mp4";
  const info = new media.MediaInfo(m.url, contentType);
  info.streamType = media.StreamType.BUFFERED;
  if (m.hls) {
    info.hlsSegmentFormat = m.fmp4 ? media.HlsSegmentFormat.FMP4 : media.HlsSegmentFormat.TS;
    info.hlsVideoSegmentFormat = m.fmp4 ? media.HlsVideoSegmentFormat.FMP4 : media.HlsVideoSegmentFormat.MPEG2_TS;
  }
  const meta = new media.MovieMediaMetadata();
  meta.title = m.title;
  if (m.subtitle) meta.subtitle = m.subtitle;
  if (m.poster) meta.images = [new window.chrome.cast.Image(m.poster)];
  info.metadata = meta;
  const req = new media.LoadRequest(info);
  req.currentTime = m.startSec;
  req.autoplay = true;
  await session.loadMedia(req);
}
