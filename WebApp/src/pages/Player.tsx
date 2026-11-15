import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import type Hls from "hls.js";
import { ArrowLeft, Play, Pause, RotateCcw, RotateCw, Maximize, Minimize, Volume2, VolumeX, Captions, Settings2, Info, SkipForward, ListOrdered, Cast, Airplay } from "lucide-react";
import { api, ApiError, getToken, imageUrl, mediaUrl, serverUrl } from "../api/client";
import { castContext, castLoad, castProfile, loadCast } from "../lib/cast";
import type { ItemDetail, Item, PlaybackInfo, PlaylistDetail, Trickplay } from "../api/types";
import { buildProfile } from "../lib/profile";
import { MethodBadge } from "../components/ui";
import { useI18n } from "../i18n";
import { useSession } from "../session";
import { subscribe } from "../api/events";
import { DEFAULT_PREFS } from "../lib/prefs";

const QUALITIES = [0, 40_000_000, 20_000_000, 8_000_000, 4_000_000, 2_000_000, 1_000_000];
const K_QUALITY = "platea.maxBitrate";

/** Ogni video parte con la qualità predefinita dell'utente (Account, in casa e fuori casa; `null` = automatica):
 *  un limite scelto nel player vale solo per quel video. (Prima la scelta restava nel browser.) */
function readQuality(): number | null {
  try { localStorage.removeItem(K_QUALITY); } catch { /* ignora */ }
  return null;
}

type Panel = null | "subs" | "settings" | "info" | "chapters";
interface Chapter { start_ms: number; title: string | null }

/** Errore del player nel log del server: aiuta a capire perché un browser rifiuta un flusso. */
function reportClientError(sessionId: string, message: string) {
  fetch(`${serverUrl()}/api/v1/playback/${sessionId}/client-error`, {
    method: "POST",
    keepalive: true,
    headers: { Authorization: `Bearer ${getToken() ?? ""}`, "Content-Type": "application/json" },
    body: JSON.stringify({ message: message.slice(0, 500) }),
  }).catch(() => undefined);
}

export default function Player() {
  const { id } = useParams();
  const [search] = useSearchParams();
  const nav = useNavigate();
  const { t, clock, bitrate, locale } = useI18n();
  const { me } = useSession();
  const prefs = { ...DEFAULT_PREFS, ...me?.preferences };
  const back = prefs.skip_back_seconds;
  const fwd = prefs.skip_forward_seconds;
  // Conto alla rovescia verso la voce successiva (episodio o playlist).
  const [upNext, setUpNext] = useState<{ id: string; url: string; title: string; left: number } | null>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const hlsRef = useRef<Hls | null>(null);
  const sessionRef = useRef<string | null>(null);
  const triedFallback = useRef(false);
  const [info, setInfo] = useState<PlaybackInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [errorDetail, setErrorDetail] = useState<string | null>(null);
  // Ripiego sulla transcodifica: impostato da start(), usato dai gestori di errore di hls.js e del video.
  const fallbackRef = useRef<(() => void) | null>(null);
  // Sessione chiusa dal server (410: l'esecutore della transcodifica è caduto senza ricambio): si riapre dalla
  // posizione corrente, al massimo qualche volta per film.
  const reopenRef = useRef<(() => void) | null>(null);
  const reopensRef = useRef(0);
  const adminStopRef = useRef(false);
  // Ultima posizione davvero riprodotta: dopo un errore Safari riporta in currentTime valori senza senso
  // (es. 4008 s a un secondo dall'avvio), e il ripiego ripartirebbe da lì.
  const playedRef = useRef<number | null>(null);
  // HLS nativo di Safari: se rifiuta i segmenti li richiede in loop per minuti prima di dare errore.
  const stallRef = useRef<number | undefined>(undefined);
  const [playing, setPlaying] = useState(false);
  const [time, setTime] = useState(0);
  const [buffered, setBuffered] = useState(0);
  const [muted, setMuted] = useState(false);
  // Casting: AirPlay (Safari) e Google Cast (Chrome). Durante il casting i comandi guidano il televisore.
  const [airplay, setAirplay] = useState(false);
  const [castReady, setCastReady] = useState(false);
  const [casting, setCasting] = useState<string | null>(null);
  const castingRef = useRef(false);
  const castSessionRef = useRef<string | null>(null);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const remoteRef = useRef<{ player: any; controller: any } | null>(null);
  const [visible, setVisible] = useState(true);
  const [panel, setPanel] = useState<Panel>(null);
  const [fullscreen, setFullscreen] = useState(false);
  const [subIndex, setSubIndex] = useState<number | null>(null);
  const [quality, setQuality] = useState<number | null>(readQuality);
  const [seekHint, setSeekHint] = useState<{ side: "left" | "right"; text: string } | null>(null);
  const [dragging, setDragging] = useState<number | null>(null);
  // Messaggio arrivato dall'amministratore (comando remoto), mostrato qualche secondo sopra il video.
  const [notice, setNotice] = useState<string | null>(null);
  // Posizione sotto il mouse sulla barra (anteprima senza trascinare).
  const [hoverPos, setHoverPos] = useState<number | null>(null);
  const hideTimer = useRef<number | undefined>(undefined);

  const fileParam = search.get("file");
  const { data: chapters } = useQuery({
    queryKey: ["chapters", id, fileParam],
    queryFn: () => api<{ items: Chapter[] }>(`/api/v1/items/${id}/chapters${fileParam ? `?file_id=${fileParam}` : ""}`).then((r) => r.items),
    staleTime: Infinity,
  });
  const { data: item } = useQuery({ queryKey: ["item", id], queryFn: () => api<ItemDetail>(`/api/v1/items/${id}`) });
  const duration = (info?.duration_ms ?? 0) / 1000;

  const stopSession = useCallback((positionMs?: number) => {
    const sid = sessionRef.current;
    if (!sid) return;
    sessionRef.current = null;
    fetch(`${serverUrl()}/api/v1/playback/${sid}/stop`, {
      method: "POST",
      keepalive: true,
      headers: { Authorization: `Bearer ${getToken() ?? ""}`, "Content-Type": "application/json" },
      body: JSON.stringify({ position_ms: Math.round(positionMs ?? 0) }),
    }).catch(() => undefined);
  }, []);

  const attach = useCallback(async (p: PlaybackInfo, startSec: number) => {
    const video = videoRef.current;
    if (!video) return;
    hlsRef.current?.destroy();
    hlsRef.current = null;
    window.clearTimeout(stallRef.current);
    video.removeAttribute("src");
    while (video.firstChild) video.removeChild(video.firstChild);
    // Sottotitoli di testo: consegnati a parte in WebVTT, il video non viene toccato.
    for (const s of p.subtitles) {
      if (!s.url) continue;
      const tr = document.createElement("track");
      tr.kind = "subtitles";
      tr.label = [s.language?.toUpperCase(), s.title, s.forced ? t("item.forced") : null].filter(Boolean).join(" · ") || `#${s.index}`;
      if (s.language) tr.srclang = s.language;
      tr.src = mediaUrl(s.url)!;
      tr.dataset.index = String(s.index);
      video.appendChild(tr);
    }
    if (p.stream_url) {
      video.src = mediaUrl(p.stream_url)!;
      video.addEventListener("loadedmetadata", () => { if (startSec > 0) video.currentTime = startSec; }, { once: true });
    } else if (p.hls_url) {
      const url = mediaUrl(p.hls_url)!;
      const { default: HlsLib } = await import("hls.js");
      // Safari (Mac, iPhone, iPad) ha l'HLS nativo: con quello funzionano AirPlay e lo schermo intero di sistema,
      // che con hls.js (MediaSource) non vanno.
      const safariNative = "WebKitPlaybackTargetAvailabilityEvent" in window && video.canPlayType("application/vnd.apple.mpegurl") !== "";
      if (HlsLib.isSupported() && !safariNative) {
        const hls = new HlsLib({ startPosition: startSec, maxBufferLength: 60, backBufferLength: 60, fragLoadingTimeOut: 70000, fragLoadingMaxRetry: 4 });
        let mediaRecoveries = 0;
        hls.on(HlsLib.Events.ERROR, (_e, d) => {
          if (!d.fatal) return;
          if (d.type === HlsLib.ErrorTypes.MEDIA_ERROR && mediaRecoveries < 2) {
            mediaRecoveries++;
            hls.recoverMediaError();
            return;
          }
          const code = (d.response as { code?: number } | undefined)?.code;
          if (code === 410 && !adminStopRef.current && reopenRef.current && reopensRef.current < 5) {
            reopensRef.current++;
            const r = reopenRef.current;
            reopenRef.current = null;
            r();
            return;
          }
          const detail = [d.type, d.details, code ? `HTTP ${code}` : null, d.error?.message].filter(Boolean).join(" · ");
          reportClientError(p.session_id, detail);
          // Remux rifiutato dal browser (codec, contenitore): si riprova una volta con la transcodifica.
          if (p.method !== "transcode" && fallbackRef.current) {
            const f = fallbackRef.current;
            fallbackRef.current = null;
            f();
            return;
          }
          setErrorDetail(detail);
          setError(t("player.errorStream"));
        });
        hls.loadSource(url);
        hls.attachMedia(video);
        hlsRef.current = hls;
      } else if (video.canPlayType("application/vnd.apple.mpegurl")) {
        video.src = url;
        video.addEventListener("loadedmetadata", () => { if (startSec > 0) video.currentTime = startSec; }, { once: true });
        // Nessun fotogramma entro 10 s dalla richiesta di play (non in pausa, nessun errore): copia rifiutata,
        // si passa subito alla transcodifica. Con l'autoplay bloccato da iOS non scatta (niente evento play).
        if (p.method !== "transcode") {
          video.addEventListener("play", () => {
            window.clearTimeout(stallRef.current);
            stallRef.current = window.setTimeout(() => {
              const v = videoRef.current;
              if (!v || v.paused || v.error || playedRef.current != null || !fallbackRef.current) return;
              const f = fallbackRef.current;
              fallbackRef.current = null;
              reportClientError(p.session_id, "HLS nativo fermo all'avvio: nessun fotogramma in 10 s");
              f();
            }, 10_000);
          }, { once: true });
        }
      } else {
        setError(t("player.errorNoHls"));
        return;
      }
    }
    video.play().catch(() => setPlaying(false));
  }, [t]);

  const start = useCallback(async (opts: { startMs: number; audioIndex?: number | null; subtitleIndex?: number | null; force?: boolean }) => {
    setError(null);
    setErrorDetail(null);
    stopSession(videoRef.current ? videoRef.current.currentTime * 1000 : undefined);
    try {
      const p = await api<PlaybackInfo>("/api/v1/playback/info", {
        body: {
          item_id: id,
          file_id: search.get("file") ?? undefined,
          profile: buildProfile(quality || null),
          explicit_quality: quality !== null,
          audio_index: opts.audioIndex ?? undefined,
          subtitle_index: opts.subtitleIndex ?? undefined,
          start_ms: Math.round(opts.startMs),
          force_transcode: opts.force ?? false,
          ui_language: locale,
        },
      });
      sessionRef.current = p.session_id;
      setInfo(p);
      // Un solo ripiego automatico per avvio: dalla posizione corrente, con le stesse tracce.
      playedRef.current = null;
      fallbackRef.current = opts.force || p.method === "transcode" ? null : () => {
        triedFallback.current = true;
        start({ startMs: playedRef.current != null ? playedRef.current * 1000 : opts.startMs, audioIndex: p.audio_index, subtitleIndex: opts.subtitleIndex, force: true });
      };
      reopenRef.current = () => {
        const v = videoRef.current;
        start({ startMs: v && v.currentTime > 0 ? v.currentTime * 1000 : opts.startMs, audioIndex: p.audio_index, subtitleIndex: opts.subtitleIndex, force: opts.force });
      };
      // Primo avvio: sottotitolo scelto dal server secondo le preferenze dell'utente.
      if (opts.subtitleIndex === undefined && opts.audioIndex === undefined && p.subtitle_default != null) setSubIndex(p.subtitle_default);
      await attach(p, opts.startMs / 1000);
    } catch (e) {
      setError(e instanceof ApiError && e.message ? e.message : t("errors.network"));
    }
  }, [attach, id, quality, search, stopSession, t, locale]);

  // AirPlay: Safari dice se c'è un dispositivo nella rete.
  useEffect(() => {
    const v = videoRef.current;
    if (!v || !("WebKitPlaybackTargetAvailabilityEvent" in window)) return;
    const on = (e: Event) => setAirplay((e as Event & { availability?: string }).availability === "available");
    v.addEventListener("webkitplaybacktargetavailabilitychanged", on);
    return () => v.removeEventListener("webkitplaybacktargetavailabilitychanged", on);
  }, []);

  const showAirplay = () => {
    const v = videoRef.current as (HTMLVideoElement & { webkitShowPlaybackTargetPicker?: () => void }) | null;
    v?.webkitShowPlaybackTargetPicker?.();
  };

  // Google Cast: si apre una sessione del server pensata per il Chromecast e la si manda al televisore.
  const beginCast = useCallback(async () => {
    const ctx = castContext();
    const session = ctx?.getCurrentSession();
    if (!session || castingRef.current) return;
    const v = videoRef.current;
    const pos = v?.currentTime ?? 0;
    castingRef.current = true;
    setCasting(session.getCastDevice()?.friendlyName ?? "TV");
    v?.pause();
    stopSession(pos * 1000);
    hlsRef.current?.destroy();
    hlsRef.current = null;
    try {
      const p = await api<PlaybackInfo>("/api/v1/playback/info", {
        body: { item_id: id, file_id: search.get("file") ?? undefined, profile: castProfile(quality || null), explicit_quality: quality !== null, audio_index: info?.audio_index ?? undefined, start_ms: Math.round(pos * 1000), ui_language: locale },
      });
      castSessionRef.current = p.session_id;
      const rel = p.hls_url ?? p.stream_url;
      if (!rel) throw new Error("no url");
      const url = new URL(mediaUrl(rel)!, window.location.href).href;
      let fmp4 = false;
      if (p.hls_url) fmp4 = (await fetch(url).then((r) => r.text()).catch(() => "")).includes("#EXT-X-MAP");
      const poster = item?.images.poster ? new URL(imageUrl(item.images.poster, 480)!, window.location.href).href : null;
      await castLoad({ url, hls: !!p.hls_url, fmp4, container: "mp4", startSec: pos, title: (item?.kind === "episode" ? item.series_title : item?.title) ?? "", subtitle: item?.kind === "episode" ? item.title : item?.year ? String(item.year) : undefined, poster });
    } catch {
      setError(t("player.castFailed"));
      ctx.endCurrentSession(true);
    }
  }, [id, info, item, locale, quality, search, stopSession, t]);

  const endCast = useCallback(() => {
    if (!castingRef.current) return;
    const r = remoteRef.current;
    const pos = r?.player.currentTime ?? 0;
    castingRef.current = false;
    setCasting(null);
    const sid = castSessionRef.current;
    castSessionRef.current = null;
    if (sid) {
      fetch(`${serverUrl()}/api/v1/playback/${sid}/stop`, {
        method: "POST", keepalive: true,
        headers: { Authorization: `Bearer ${getToken() ?? ""}`, "Content-Type": "application/json" },
        body: JSON.stringify({ position_ms: Math.round(pos * 1000) }),
      }).catch(() => undefined);
    }
    // Di nuovo qui, dal punto in cui era il televisore.
    start({ startMs: pos * 1000, audioIndex: info?.audio_index });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [info]);

  const castFns = useRef({ beginCast, endCast });
  castFns.current = { beginCast, endCast };

  useEffect(() => {
    let off = () => {};
    let cancelled = false;
    loadCast().then((ok) => {
      if (!ok || cancelled) return;
      const cf = window.cast.framework;
      const ctx = castContext();
      const player = new cf.RemotePlayer();
      const controller = new cf.RemotePlayerController(player);
      remoteRef.current = { player, controller };
      const onState = () => setCastReady(ctx.getCastState() !== cf.CastState.NO_DEVICES_AVAILABLE);
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const onSession = (e: any) => {
        if (e.sessionState === cf.SessionState.SESSION_STARTED || e.sessionState === cf.SessionState.SESSION_RESUMED) castFns.current.beginCast();
        if (e.sessionState === cf.SessionState.SESSION_ENDED) castFns.current.endCast();
      };
      const onTime = () => { if (castingRef.current) setTime(player.currentTime); };
      const onPaused = () => { if (castingRef.current) setPlaying(!player.isPaused); };
      ctx.addEventListener(cf.CastContextEventType.CAST_STATE_CHANGED, onState);
      ctx.addEventListener(cf.CastContextEventType.SESSION_STATE_CHANGED, onSession);
      controller.addEventListener(cf.RemotePlayerEventType.CURRENT_TIME_CHANGED, onTime);
      controller.addEventListener(cf.RemotePlayerEventType.IS_PAUSED_CHANGED, onPaused);
      onState();
      // Una sessione già aperta (altra scheda, pagina ricaricata): si continua sul televisore.
      if (ctx.getCurrentSession()) castFns.current.beginCast();
      off = () => {
        ctx.removeEventListener(cf.CastContextEventType.CAST_STATE_CHANGED, onState);
        ctx.removeEventListener(cf.CastContextEventType.SESSION_STATE_CHANGED, onSession);
        controller.removeEventListener(cf.RemotePlayerEventType.CURRENT_TIME_CHANGED, onTime);
        controller.removeEventListener(cf.RemotePlayerEventType.IS_PAUSED_CHANGED, onPaused);
      };
    });
    return () => { cancelled = true; off(); };
  }, []);

  // Avvio: dalla posizione salvata.
  useEffect(() => {
    if (!item) return;
    triedFallback.current = false;
    const resume = item.user_data.position_ms > 0 && !item.user_data.played ? item.user_data.position_ms : 0;
    start({ startMs: resume });
    return () => {
      stopSession(videoRef.current ? videoRef.current.currentTime * 1000 : undefined);
      window.clearTimeout(stallRef.current);
      hlsRef.current?.destroy();
      hlsRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [item?.id]);

  // Chiusura della scheda o navigazione completa: React non smonta, quindi lo stop parte da qui.
  useEffect(() => {
    const onHide = () => stopSession(videoRef.current ? videoRef.current.currentTime * 1000 : undefined);
    window.addEventListener("pagehide", onHide);
    return () => window.removeEventListener("pagehide", onHide);
  }, [stopSession]);

  // Posizione al server ogni 10 secondi.
  useEffect(() => {
    const h = window.setInterval(() => {
      const r = remoteRef.current;
      if (castingRef.current && r && castSessionRef.current) {
        api(`/api/v1/playback/${castSessionRef.current}/progress`, { body: { position_ms: Math.round(r.player.currentTime * 1000), paused: r.player.isPaused } }).catch(() => undefined);
        return;
      }
      const v = videoRef.current;
      const sid = sessionRef.current;
      if (!v || !sid) return;
      api(`/api/v1/playback/${sid}/progress`, { body: { position_ms: Math.round(v.currentTime * 1000), paused: v.paused } }).catch(() => undefined);
    }, 10_000);
    return () => window.clearInterval(h);
  }, []);

  // Sottotitolo attivo.
  useEffect(() => {
    const v = videoRef.current;
    if (!v) return;
    for (const tr of Array.from(v.textTracks)) tr.mode = "disabled";
    const els = Array.from(v.querySelectorAll("track"));
    const idx = els.findIndex((e) => e.dataset.index === String(subIndex));
    if (idx >= 0) v.textTracks[idx].mode = "showing";
  }, [subIndex, info]);

  const poke = useCallback(() => {
    setVisible(true);
    window.clearTimeout(hideTimer.current);
    hideTimer.current = window.setTimeout(() => { if (videoRef.current && !videoRef.current.paused) { setVisible(false); setPanel(null); } }, 3000);
  }, []);

  const seekTo = useCallback((sec: number) => {
    const r = remoteRef.current;
    if (castingRef.current && r) {
      r.player.currentTime = Math.max(0, Math.min(duration || Infinity, sec));
      r.controller.seek();
      setTime(r.player.currentTime);
      return;
    }
    const v = videoRef.current;
    if (!v) return;
    v.currentTime = Math.max(0, Math.min(duration || v.duration || 0, sec));
    setTime(v.currentTime);
  }, [duration]);

  const skip = useCallback((d: number) => {
    const v = videoRef.current;
    const r = remoteRef.current;
    if (castingRef.current && r) seekTo(r.player.currentTime + d);
    else if (!v) return;
    else seekTo(v.currentTime + d);
    setSeekHint({ side: d < 0 ? "left" : "right", text: `${d > 0 ? "+" : "−"}${Math.abs(d)} s` });
    window.setTimeout(() => setSeekHint(null), 600);
    poke();
  }, [poke, seekTo]);

  // Comandi remoti per questa sessione (dalla dashboard): pausa, ripresa, messaggio, interruzione.
  useEffect(() => subscribe((e) => {
    const d = e.data as { session_id?: string; command?: string; text?: string | null; from?: string; by?: string } | undefined;
    if (!d || !info || d.session_id !== info.session_id) return;
    const v = videoRef.current;
    if (e.type === "session.command") {
      if (d.command === "pause") v?.pause();
      else if (d.command === "resume") v?.play().catch(() => {});
      else if (d.command === "message" && d.text) {
        setNotice(`${d.from ?? ""}: ${d.text}`);
        window.setTimeout(() => setNotice(null), 10000);
      }
    } else if (e.type === "playback.stopped" && d.by === "admin") {
      adminStopRef.current = true;
      v?.pause();
      setError(t("player.stoppedByAdmin"));
    }
  }), [info, t]);

  const togglePlay = useCallback(() => {
    if (castingRef.current && remoteRef.current) {
      remoteRef.current.controller.playOrPause();
      poke();
      return;
    }
    const v = videoRef.current;
    if (!v) return;
    if (v.paused) v.play().catch(() => undefined); else v.pause();
    poke();
  }, [poke]);

  // Schermo intero in tre modi: standard; iPad e Safari vecchi con il prefisso webkit; iPhone, dove una pagina
  // non può andare a schermo intero e si usa il player nativo del video (con i suoi comandi e sottotitoli).
  const toggleFs = useCallback(() => {
    const d = document as Document & { webkitFullscreenElement?: Element; webkitExitFullscreen?: () => void };
    const el = document.documentElement as HTMLElement & { webkitRequestFullscreen?: () => void };
    const video = videoRef.current as (HTMLVideoElement & { webkitEnterFullscreen?: () => void; webkitDisplayingFullscreen?: boolean }) | null;
    const nativeVideo = () => {
      try { video?.webkitEnterFullscreen?.(); } catch { /* video senza metadati: si riprova al prossimo tocco */ }
    };
    if (d.fullscreenElement || d.webkitFullscreenElement) {
      if (d.exitFullscreen) d.exitFullscreen().catch(() => undefined);
      else d.webkitExitFullscreen?.();
      return;
    }
    if (typeof el.requestFullscreen === "function") {
      el.requestFullscreen().catch(nativeVideo);
    } else if (typeof el.webkitRequestFullscreen === "function") {
      el.webkitRequestFullscreen();
    } else {
      nativeVideo();
    }
  }, []);

  useEffect(() => {
    const onFs = () => setFullscreen(!!(document.fullscreenElement || (document as Document & { webkitFullscreenElement?: Element }).webkitFullscreenElement));
    document.addEventListener("fullscreenchange", onFs);
    document.addEventListener("webkitfullscreenchange", onFs);
    // iPhone: schermo intero nativo del video.
    const v = videoRef.current;
    const onBegin = () => setFullscreen(true);
    const onEnd = () => setFullscreen(false);
    v?.addEventListener("webkitbeginfullscreen", onBegin);
    v?.addEventListener("webkitendfullscreen", onEnd);
    const onKey = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement)?.tagName === "INPUT") return;
      if (e.key === " " || e.key === "k") { e.preventDefault(); togglePlay(); }
      else if (e.key === "ArrowLeft" || e.key === "j") skip(-back);
      else if (e.key === "ArrowRight" || e.key === "l") skip(fwd);
      else if (e.key === "f") toggleFs();
      else if (e.key === "m") { const v = videoRef.current; if (v) { v.muted = !v.muted; setMuted(v.muted); } }
      else if (e.key === "Escape" && !document.fullscreenElement) nav(-1);
      poke();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("fullscreenchange", onFs);
      document.removeEventListener("webkitfullscreenchange", onFs);
      v?.removeEventListener("webkitbeginfullscreen", onBegin);
      v?.removeEventListener("webkitendfullscreen", onEnd);
      window.removeEventListener("keydown", onKey);
    };
  }, [nav, poke, skip, toggleFs, togglePlay]);

  // Doppio tocco a sinistra o a destra per saltare; tocco singolo mostra o nasconde i controlli.
  const lastTap = useRef<{ t: number; x: number } | null>(null);
  const onSurfacePointer = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.target !== e.currentTarget) return;
    const now = Date.now();
    const w = e.currentTarget.clientWidth;
    if (e.pointerType === "mouse") { togglePlay(); return; }
    if (lastTap.current && now - lastTap.current.t < 300) {
      const x = e.clientX;
      if (x < w / 3) skip(-back); else if (x > (2 * w) / 3) skip(fwd); else togglePlay();
      lastTap.current = null;
      return;
    }
    lastTap.current = { t: now, x: e.clientX };
    window.setTimeout(() => {
      if (lastTap.current && lastTap.current.t === now) {
        if (visible) { setVisible(false); setPanel(null); } else poke();
        lastTap.current = null;
      }
    }, 300);
  };

  const barRef = useRef<HTMLDivElement>(null);
  const posFromEvent = (clientX: number) => {
    const r = barRef.current!.getBoundingClientRect();
    return Math.max(0, Math.min(1, (clientX - r.left) / r.width)) * duration;
  };

  // Dentro una playlist (?pl=…&i=…) la voce successiva viene dalla playlist; altrimenti l'episodio successivo.
  const playlistId = search.get("pl");
  const playlistIndex = Number(search.get("i") ?? "0");
  const findNext = async (): Promise<{ id: string; url: string; title: string } | null> => {
    if (playlistId) {
      const pl = await api<PlaylistDetail>(`/api/v1/playlists/${playlistId}`);
      const n = pl.items[playlistIndex + 1];
      return n ? { id: n.id, url: `/play/${n.id}?pl=${playlistId}&i=${playlistIndex + 1}`, title: n.kind === "episode" ? `${n.series_title ?? ""} · ${n.title}` : n.title } : null;
    }
    if (item?.kind !== "episode") return null;
    const r = await api<{ item: Item | null }>(`/api/v1/items/${item.id}/next`);
    return r.item ? { id: r.item.id, url: `/play/${r.item.id}`, title: `S${r.item.parent_index_number} · E${r.item.index_number} · ${r.item.title}` } : null;
  };
  const goNext = async () => {
    const n = await findNext();
    if (n) nav(n.url, { replace: true });
  };
  const onEnded = async () => {
    stopSession(info?.duration_ms);
    if (!prefs.autoplay_next) return;
    const n = await findNext();
    if (!n) return;
    if (prefs.autoplay_countdown_seconds <= 0) nav(n.url, { replace: true });
    else setUpNext({ ...n, left: prefs.autoplay_countdown_seconds });
  };
  useEffect(() => {
    if (!upNext) return;
    if (upNext.left <= 0) { nav(upNext.url, { replace: true }); setUpNext(null); return; }
    const h = window.setTimeout(() => setUpNext((u) => (u ? { ...u, left: u.left - 1 } : u)), 1000);
    return () => window.clearTimeout(h);
  }, [upNext, nav]);
  useEffect(() => setUpNext(null), [id]);

  // Le tracce scelte valgono anche per la prossima volta (per il film, o per tutta la serie).
  const rememberTracks = (audio: number | null, sub: number | null) => {
    const sid = sessionRef.current;
    if (!sid || !prefs.remember_tracks) return;
    api(`/api/v1/playback/${sid}/tracks`, { body: { audio_index: audio ?? undefined, subtitle_index: sub ?? undefined, subtitle_off: sub === null } }).catch(() => undefined);
  };

  const onVideoError = () => {
    // Il browser rifiuta il file nonostante il profilo: si riprova una volta con la transcodifica.
    // Direct play o HLS nativo di Safari rifiutati: stesso ripiego sulla transcodifica di hls.js.
    if (info && info.method !== "transcode" && !hlsRef.current && fallbackRef.current) {
      const f = fallbackRef.current;
      fallbackRef.current = null;
      reportClientError(info.session_id, `video element error ${videoRef.current?.error?.code ?? ""} ${videoRef.current?.error?.message ?? ""}`.trim());
      f();
    } else if (!hlsRef.current) {
      setError(t("player.errorDecode"));
    }
  };

  const shown = dragging ?? time;
  const reason = info?.reasons[0];
  const title = item?.kind === "episode" ? item.series_title : item?.title;
  const subtitleLine = item?.kind === "episode" ? `S${item.parent_index_number} · E${item.index_number} · ${item.title}` : item?.year ? String(item.year) : "";

  return (
    <div className={`player player--sub-${prefs.subtitle_size}${prefs.subtitle_background ? " player--sub-bg" : ""}`} onPointerMove={(e) => e.pointerType === "mouse" && poke()}>
      <video
        ref={videoRef}
        playsInline
        x-webkit-airplay="allow"
        style={casting ? { visibility: "hidden" } : undefined}
        crossOrigin={serverUrl() ? "anonymous" : undefined}
        onPlay={() => { setPlaying(true); poke(); }}
        onPause={() => { setPlaying(false); setVisible(true); }}
        onTimeUpdate={(e) => {
          const v = e.currentTarget;
          setTime(v.currentTime);
          if (!v.error && !v.paused && v.readyState >= 3) playedRef.current = v.currentTime;
        }}
        onProgress={(e) => { const b = e.currentTarget.buffered; if (b.length) setBuffered(b.end(b.length - 1)); }}
        onEnded={onEnded}
        onError={onVideoError}
      />
      <div className={`player__ui${visible ? "" : " hidden"}`} onPointerUp={onSurfacePointer}>
        <div className="player__top">
          <button className="player__btn" onClick={() => nav(-1)} aria-label={t("common.back")}><ArrowLeft /></button>
          <div className="player__title spacer">
            <div className="t-h3">{title}</div>
            {subtitleLine && <div className="t-small" style={{ color: "#a2a9b4" }}>{subtitleLine}</div>}
          </div>
          {info && <MethodBadge method={info.method} reason={reason} />}
        </div>

        {!error && <div className="player__center" onPointerUp={onSurfacePointer}>
          <button className="player__btn player__btn--big player__hide-sm" onClick={() => skip(-back)} aria-label={t("player.backN", { s: back })}><RotateCcw /></button>
          <button className="pl-btn pl-btn--play" style={{ width: 72, height: 72 }} onClick={togglePlay} aria-label={playing ? t("player.pause") : t("player.play")}>
            {playing ? <Pause fill="currentColor" /> : <Play fill="currentColor" />}
          </button>
          <button className="player__btn player__btn--big player__hide-sm" onClick={() => skip(fwd)} aria-label={t("player.forwardN", { s: fwd })}><RotateCw /></button>
        </div>}

        <div className="player__bottom">
          <div
            className="player__bar"
            ref={barRef}
            onPointerDown={(e) => { e.currentTarget.setPointerCapture(e.pointerId); setDragging(posFromEvent(e.clientX)); poke(); }}
            onPointerMove={(e) => { if (dragging !== null) setDragging(posFromEvent(e.clientX)); else if (e.pointerType === "mouse") setHoverPos(posFromEvent(e.clientX)); }}
            onPointerLeave={() => setHoverPos(null)}
            onPointerUp={(e) => { if (dragging !== null) { seekTo(posFromEvent(e.clientX)); setDragging(null); } }}
            role="slider"
            aria-label={t("player.position")}
            aria-valuemin={0}
            aria-valuemax={Math.round(duration)}
            aria-valuenow={Math.round(shown)}
            tabIndex={0}
          >
            <div className="player__track">
              <div className="player__buffer" style={{ width: `${duration ? (buffered / duration) * 100 : 0}%` }} />
              <div className="player__progress" style={{ width: `${duration ? (shown / duration) * 100 : 0}%` }} />
              {duration > 0 && chapters && chapters.length > 1 && chapters.slice(1).map((c) => (
                <span key={c.start_ms} className="player__chapter" style={{ left: `${(c.start_ms / 1000 / duration) * 100}%` }} />
              ))}
            </div>
            <div className="player__thumb" style={{ left: `${duration ? (shown / duration) * 100 : 0}%` }} />
            {info?.trickplay && duration > 0 && (dragging ?? hoverPos) !== null && (
              <TrickplayPreview tp={info.trickplay} sec={(dragging ?? hoverPos)!} duration={duration} />
            )}
          </div>
          <div className="player__controls">
            <span className="player__time">{clock(shown * 1000)} / {clock(duration * 1000)}</span>
            <span className="spacer" />
            <button className="player__btn" onClick={() => { const v = videoRef.current; if (v) { v.muted = !v.muted; setMuted(v.muted); } }} aria-label={muted ? t("player.unmute") : t("player.mute")}>{muted ? <VolumeX /> : <Volume2 />}</button>
            {(item?.kind === "episode" || playlistId) && <button className="player__btn" onClick={goNext} aria-label={t("player.next")}><SkipForward /></button>}
            {chapters && chapters.length > 1 && <button className="player__btn" onClick={() => setPanel(panel === "chapters" ? null : "chapters")} aria-label={t("player.chapters")}><ListOrdered /></button>}
            <button className="player__btn" onClick={() => setPanel(panel === "subs" ? null : "subs")} aria-label={t("player.subtitlesAudio")}><Captions /></button>
            <button className="player__btn" onClick={() => setPanel(panel === "settings" ? null : "settings")} aria-label={t("player.quality")}><Settings2 /></button>
            <button className="player__btn" onClick={() => setPanel(panel === "info" ? null : "info")} aria-label={t("player.info")}><Info /></button>
            {airplay && <button className="player__btn" onClick={showAirplay} aria-label={t("player.airplay")} title={t("player.airplay")}><Airplay /></button>}
            {(castReady || casting) && (
              <button className={`player__btn${casting ? " is-active" : ""}`} onClick={() => castContext()?.requestSession().catch(() => undefined)} aria-label={t("player.cast")} title={t("player.cast")}><Cast /></button>
            )}
            <button className="player__btn" onClick={toggleFs} aria-label={t("player.fullscreen")}>{fullscreen ? <Minimize /> : <Maximize />}</button>
          </div>
        </div>
      </div>

      {upNext && (
        <div className="player__upnext" role="status">
          <div className="t-small muted">{t("player.upNextIn", { s: upNext.left })}</div>
          <div className="t-label">{upNext.title}</div>
          <div className="row" style={{ marginTop: 8 }}>
            <button className="pl-btn pl-btn--primary pl-btn--sm" onClick={() => { const u = upNext.url; setUpNext(null); nav(u, { replace: true }); }}><Play fill="currentColor" />{t("player.playNow")}</button>
            <button className="pl-btn pl-btn--ghost pl-btn--sm" onClick={() => setUpNext(null)}>{t("common.cancel")}</button>
          </div>
        </div>
      )}
      {notice && <div className="player__notice" role="status">{notice}</div>}
      {seekHint && <div className="player__seek-hint" style={seekHint.side === "left" ? { left: "15%" } : { right: "15%" }}>{seekHint.text}</div>}

      {panel === "chapters" && chapters && (
        <div className="player__panel">
          <h4>{t("player.chapters")}</h4>
          {chapters.map((c, i) => {
            const next = chapters[i + 1]?.start_ms ?? (duration * 1000);
            const current = shown * 1000 >= c.start_ms && shown * 1000 < next;
            return (
              <button key={c.start_ms} className={current ? "active" : ""} onClick={() => { seekTo(c.start_ms / 1000); setPanel(null); }}>
                <span>{c.title ?? t("player.chapterN", { n: i + 1 })}</span>
                <span className="t-mono-sm" style={{ color: "#808893" }}>{clock(c.start_ms)}</span>
              </button>
            );
          })}
        </div>
      )}

      {panel === "subs" && info && (
        <div className="player__panel">
          <h4>{t("item.subtitles")}</h4>
          <button className={subIndex === null ? "active" : ""} onClick={() => { setSubIndex(null); rememberTracks(info.audio_index, null); }}>{t("player.off")}</button>
          {info.subtitles.map((s) => (
            <button
              key={s.index}
              className={subIndex === s.index ? "active" : ""}
              onClick={() => {
                setSubIndex(s.index);
                rememberTracks(info.audio_index, s.index);
                // Un sottotitolo a immagine va impresso nel video: serve una nuova decisione.
                if (s.image) start({ startMs: (videoRef.current?.currentTime ?? 0) * 1000, audioIndex: info.audio_index, subtitleIndex: s.index });
              }}
            >
              <span>{[s.language?.toUpperCase() ?? "?", s.title].filter(Boolean).join(" · ")}</span>
              <span className="t-mono-sm" style={{ color: "#808893" }}>{s.codec.replace("hdmv_pgs_subtitle", "pgs").toUpperCase()}{s.forced ? ` · ${t("item.forced")}` : ""}</span>
            </button>
          ))}
          {info.audio_tracks.length > 1 && (
            <>
              <h4>{t("item.audio")}</h4>
              {info.audio_tracks.map((a) => (
                <button key={a.index} className={info.audio_index === a.index ? "active" : ""} onClick={() => { rememberTracks(a.index, subIndex); start({ startMs: (videoRef.current?.currentTime ?? 0) * 1000, audioIndex: a.index }); }}>
                  <span>{a.language?.toUpperCase() ?? `#${a.index}`}</span>
                  <span className="t-mono-sm" style={{ color: "#808893" }}>{a.codec.toUpperCase()} {a.channels > 2 ? `${a.channels - 1}.1` : `${a.channels}.0`}</span>
                </button>
              ))}
            </>
          )}
        </div>
      )}

      {panel === "settings" && (
        <div className="player__panel">
          <h4>{t("player.quality")}</h4>
          <button className={quality === null ? "active" : ""} onClick={() => {
            setQuality(null);
            setPanel(null);
            window.setTimeout(() => start({ startMs: (videoRef.current?.currentTime ?? 0) * 1000, audioIndex: info?.audio_index }), 0);
          }}>
            <span>{t("player.qualityAuto")}{quality === null && info ? ` · ${info.max_bitrate ? bitrate(info.max_bitrate) : t("player.original")}` : ""}</span>
          </button>
          {QUALITIES.map((q) => (
            <button key={q} className={quality === q ? "active" : ""} onClick={() => {
              setQuality(q);
              setPanel(null);
              // La nuova qualità richiede una nuova decisione dal punto corrente.
              window.setTimeout(() => start({ startMs: (videoRef.current?.currentTime ?? 0) * 1000, audioIndex: info?.audio_index }), 0);
            }}>
              <span>{q === 0 ? t("player.original") : bitrate(q)}</span>
            </button>
          ))}
        </div>
      )}

      {panel === "info" && info && (
        <div className="player__panel" style={{ padding: 16 }}>
          <div className="stack" style={{ gap: 8 }}>
            <MethodBadge method={info.method} reason={reason} />
            <div className="t-mono" style={{ color: "#a2a9b4" }}>{info.chain}</div>
            {info.reasons.length > 0 && <div className="t-mono-sm" style={{ color: "#808893" }}>{info.reasons.join(", ")}</div>}
            <div className="t-small" style={{ color: "#a2a9b4" }}>{t(`player.why.${info.method}` as "player.why.remux")}</div>
          </div>
        </div>
      )}

      {casting && !error && (
        <div className="player__casting" role="status">
          <Cast />
          <div className="t-h3">{t("player.castingOn", { device: casting })}</div>
          <button className="pl-btn pl-btn--secondary" onClick={() => castContext()?.endCurrentSession(true)}>{t("player.castStop")}</button>
        </div>
      )}

      {error && (
        <div className="player__msg">
          <div className="t-h3">{t("player.cannotPlay")}</div>
          <div className="t-small" style={{ color: "#a2a9b4" }}>{error}</div>
          {errorDetail && <div className="t-mono-sm" style={{ color: "#7d8590" }}>{errorDetail}</div>}
          <div className="row" style={{ justifyContent: "center" }}>
            <button className="pl-btn pl-btn--secondary" onClick={() => nav(-1)}>{t("common.back")}</button>
            <button className="pl-btn pl-btn--primary" onClick={() => start({ startMs: time * 1000, force: true })}>{t("player.retry")}</button>
          </div>
        </div>
      )}
    </div>
  );
}

/** Anteprima sopra la barra: la miniatura del punto indicato, ritagliata dalla sua griglia, con l'orario sotto. */
function TrickplayPreview({ tp, sec, duration }: { tp: Trickplay; sec: number; duration: number }) {
  const { clock } = useI18n();
  const idx = Math.max(0, Math.min(tp.count - 1, Math.floor((sec * 1000) / tp.interval_ms)));
  const perSheet = tp.cols * tp.rows;
  const sheet = Math.floor(idx / perSheet);
  const cell = idx % perSheet;
  // Su schermi stretti la miniatura si rimpicciolisce; la griglia scala con lei.
  const w = Math.min(tp.width, typeof window !== "undefined" && window.innerWidth < 600 ? 160 : tp.width);
  const scale = w / tp.width;
  const h = Math.round(tp.height * scale);
  return (
    <div className="player__preview" style={{ left: `clamp(${w / 2}px, ${(sec / duration) * 100}%, calc(100% - ${w / 2}px))`, width: w }} aria-hidden>
      <div
        className="player__preview-img"
        style={{
          height: h,
          backgroundImage: `url(${mediaUrl(tp.url.replace("{n}", String(sheet)))})`,
          backgroundSize: `${tp.cols * w}px ${tp.rows * h}px`,
          backgroundPosition: `-${(cell % tp.cols) * w}px -${Math.floor(cell / tp.cols) * h}px`,
        }}
      />
      <span className="player__preview-time">{clock(sec * 1000)}</span>
    </div>
  );
}
