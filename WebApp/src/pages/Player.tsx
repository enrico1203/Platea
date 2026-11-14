import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import type Hls from "hls.js";
import { ArrowLeft, Play, Pause, RotateCcw, RotateCw, Maximize, Minimize, Volume2, VolumeX, Captions, Settings2, Info, SkipForward } from "lucide-react";
import { api, ApiError, getToken, mediaUrl, serverUrl } from "../api/client";
import type { ItemDetail, Item, PlaybackInfo } from "../api/types";
import { buildProfile } from "../lib/profile";
import { MethodBadge } from "../components/ui";
import { useI18n } from "../i18n";

const QUALITIES = [0, 40_000_000, 20_000_000, 8_000_000, 4_000_000, 2_000_000, 1_000_000];
const K_QUALITY = "platea.maxBitrate";

function readQuality(): number {
  try { return Number(localStorage.getItem(K_QUALITY) ?? 0) || 0; } catch { return 0; }
}

type Panel = null | "subs" | "settings" | "info";

export default function Player() {
  const { id } = useParams();
  const [search] = useSearchParams();
  const nav = useNavigate();
  const { t, clock, bitrate } = useI18n();
  const videoRef = useRef<HTMLVideoElement>(null);
  const hlsRef = useRef<Hls | null>(null);
  const sessionRef = useRef<string | null>(null);
  const triedFallback = useRef(false);
  const [info, setInfo] = useState<PlaybackInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [playing, setPlaying] = useState(false);
  const [time, setTime] = useState(0);
  const [buffered, setBuffered] = useState(0);
  const [muted, setMuted] = useState(false);
  const [visible, setVisible] = useState(true);
  const [panel, setPanel] = useState<Panel>(null);
  const [fullscreen, setFullscreen] = useState(false);
  const [subIndex, setSubIndex] = useState<number | null>(null);
  const [quality, setQuality] = useState<number>(readQuality);
  const [seekHint, setSeekHint] = useState<{ side: "left" | "right"; text: string } | null>(null);
  const [dragging, setDragging] = useState<number | null>(null);
  const hideTimer = useRef<number | undefined>(undefined);

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
      if (HlsLib.isSupported()) {
        const hls = new HlsLib({ startPosition: startSec, maxBufferLength: 60, backBufferLength: 60, fragLoadingTimeOut: 70000, fragLoadingMaxRetry: 4 });
        hls.on(HlsLib.Events.ERROR, (_e, d) => {
          if (d.fatal) {
            if (d.type === HlsLib.ErrorTypes.MEDIA_ERROR) hls.recoverMediaError();
            else setError(t("player.errorStream"));
          }
        });
        hls.loadSource(url);
        hls.attachMedia(video);
        hlsRef.current = hls;
      } else if (video.canPlayType("application/vnd.apple.mpegurl")) {
        video.src = url;
        video.addEventListener("loadedmetadata", () => { if (startSec > 0) video.currentTime = startSec; }, { once: true });
      } else {
        setError(t("player.errorNoHls"));
        return;
      }
    }
    video.play().catch(() => setPlaying(false));
  }, [t]);

  const start = useCallback(async (opts: { startMs: number; audioIndex?: number | null; subtitleIndex?: number | null; force?: boolean }) => {
    setError(null);
    stopSession(videoRef.current ? videoRef.current.currentTime * 1000 : undefined);
    try {
      const p = await api<PlaybackInfo>("/api/v1/playback/info", {
        body: {
          item_id: id,
          file_id: search.get("file") ?? undefined,
          profile: buildProfile(quality || null),
          audio_index: opts.audioIndex ?? undefined,
          subtitle_index: opts.subtitleIndex ?? undefined,
          start_ms: Math.round(opts.startMs),
          force_transcode: opts.force ?? false,
        },
      });
      sessionRef.current = p.session_id;
      setInfo(p);
      await attach(p, opts.startMs / 1000);
    } catch (e) {
      setError(e instanceof ApiError && e.message ? e.message : t("errors.network"));
    }
  }, [attach, id, quality, search, stopSession, t]);

  // Avvio: dalla posizione salvata.
  useEffect(() => {
    if (!item) return;
    triedFallback.current = false;
    const resume = item.user_data.position_ms > 0 && !item.user_data.played ? item.user_data.position_ms : 0;
    start({ startMs: resume });
    return () => {
      stopSession(videoRef.current ? videoRef.current.currentTime * 1000 : undefined);
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
    const v = videoRef.current;
    if (!v) return;
    v.currentTime = Math.max(0, Math.min(duration || v.duration || 0, sec));
    setTime(v.currentTime);
  }, [duration]);

  const skip = useCallback((d: number) => {
    const v = videoRef.current;
    if (!v) return;
    seekTo(v.currentTime + d);
    setSeekHint({ side: d < 0 ? "left" : "right", text: `${d > 0 ? "+" : "−"}${Math.abs(d)} s` });
    window.setTimeout(() => setSeekHint(null), 600);
    poke();
  }, [poke, seekTo]);

  const togglePlay = useCallback(() => {
    const v = videoRef.current;
    if (!v) return;
    if (v.paused) v.play().catch(() => undefined); else v.pause();
    poke();
  }, [poke]);

  const toggleFs = useCallback(() => {
    if (document.fullscreenElement) document.exitFullscreen().catch(() => undefined);
    else document.documentElement.requestFullscreen?.().catch(() => {
      // iPhone: schermo intero nativo dell'elemento video.
      (videoRef.current as unknown as { webkitEnterFullscreen?: () => void })?.webkitEnterFullscreen?.();
    });
  }, []);

  useEffect(() => {
    const onFs = () => setFullscreen(!!document.fullscreenElement);
    document.addEventListener("fullscreenchange", onFs);
    const onKey = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement)?.tagName === "INPUT") return;
      if (e.key === " " || e.key === "k") { e.preventDefault(); togglePlay(); }
      else if (e.key === "ArrowLeft" || e.key === "j") skip(-10);
      else if (e.key === "ArrowRight" || e.key === "l") skip(10);
      else if (e.key === "f") toggleFs();
      else if (e.key === "m") { const v = videoRef.current; if (v) { v.muted = !v.muted; setMuted(v.muted); } }
      else if (e.key === "Escape" && !document.fullscreenElement) nav(-1);
      poke();
    };
    window.addEventListener("keydown", onKey);
    return () => { document.removeEventListener("fullscreenchange", onFs); window.removeEventListener("keydown", onKey); };
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
      if (x < w / 3) skip(-10); else if (x > (2 * w) / 3) skip(10); else togglePlay();
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

  const goNext = async () => {
    if (item?.kind !== "episode") return;
    const r = await api<{ item: Item | null }>(`/api/v1/items/${item.id}/next`);
    if (r.item) nav(`/play/${r.item.id}`, { replace: true });
  };

  const onVideoError = () => {
    // Il browser rifiuta il file nonostante il profilo: si riprova una volta con la transcodifica.
    if (info?.method === "direct_play" && !triedFallback.current) {
      triedFallback.current = true;
      start({ startMs: (videoRef.current?.currentTime ?? 0) * 1000, audioIndex: info.audio_index, force: true });
    } else if (!hlsRef.current) {
      setError(t("player.errorDecode"));
    }
  };

  const shown = dragging ?? time;
  const reason = info?.reasons[0];
  const title = item?.kind === "episode" ? item.series_title : item?.title;
  const subtitleLine = item?.kind === "episode" ? `S${item.parent_index_number} · E${item.index_number} · ${item.title}` : item?.year ? String(item.year) : "";

  return (
    <div className="player" onPointerMove={(e) => e.pointerType === "mouse" && poke()}>
      <video
        ref={videoRef}
        playsInline
        crossOrigin={serverUrl() ? "anonymous" : undefined}
        onPlay={() => { setPlaying(true); poke(); }}
        onPause={() => { setPlaying(false); setVisible(true); }}
        onTimeUpdate={(e) => setTime(e.currentTarget.currentTime)}
        onProgress={(e) => { const b = e.currentTarget.buffered; if (b.length) setBuffered(b.end(b.length - 1)); }}
        onEnded={() => { stopSession(info?.duration_ms); goNext(); }}
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

        <div className="player__center" onPointerUp={onSurfacePointer}>
          <button className="player__btn player__btn--big player__hide-sm" onClick={() => skip(-10)} aria-label={t("player.back10")}><RotateCcw /></button>
          <button className="pl-btn pl-btn--play" style={{ width: 72, height: 72 }} onClick={togglePlay} aria-label={playing ? t("player.pause") : t("player.play")}>
            {playing ? <Pause fill="currentColor" /> : <Play fill="currentColor" />}
          </button>
          <button className="player__btn player__btn--big player__hide-sm" onClick={() => skip(10)} aria-label={t("player.forward10")}><RotateCw /></button>
        </div>

        <div className="player__bottom">
          <div
            className="player__bar"
            ref={barRef}
            onPointerDown={(e) => { e.currentTarget.setPointerCapture(e.pointerId); setDragging(posFromEvent(e.clientX)); poke(); }}
            onPointerMove={(e) => { if (dragging !== null) setDragging(posFromEvent(e.clientX)); }}
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
            </div>
            <div className="player__thumb" style={{ left: `${duration ? (shown / duration) * 100 : 0}%` }} />
          </div>
          <div className="player__controls">
            <span className="player__time">{clock(shown * 1000)} / {clock(duration * 1000)}</span>
            <span className="spacer" />
            <button className="player__btn" onClick={() => { const v = videoRef.current; if (v) { v.muted = !v.muted; setMuted(v.muted); } }} aria-label={muted ? t("player.unmute") : t("player.mute")}>{muted ? <VolumeX /> : <Volume2 />}</button>
            {item?.kind === "episode" && <button className="player__btn" onClick={goNext} aria-label={t("player.next")}><SkipForward /></button>}
            <button className="player__btn" onClick={() => setPanel(panel === "subs" ? null : "subs")} aria-label={t("player.subtitlesAudio")}><Captions /></button>
            <button className="player__btn" onClick={() => setPanel(panel === "settings" ? null : "settings")} aria-label={t("player.quality")}><Settings2 /></button>
            <button className="player__btn" onClick={() => setPanel(panel === "info" ? null : "info")} aria-label={t("player.info")}><Info /></button>
            <button className="player__btn" onClick={toggleFs} aria-label={t("player.fullscreen")}>{fullscreen ? <Minimize /> : <Maximize />}</button>
          </div>
        </div>
      </div>

      {seekHint && <div className="player__seek-hint" style={seekHint.side === "left" ? { left: "15%" } : { right: "15%" }}>{seekHint.text}</div>}

      {panel === "subs" && info && (
        <div className="player__panel">
          <h4>{t("item.subtitles")}</h4>
          <button className={subIndex === null ? "active" : ""} onClick={() => setSubIndex(null)}>{t("player.off")}</button>
          {info.subtitles.map((s) => (
            <button
              key={s.index}
              className={subIndex === s.index ? "active" : ""}
              onClick={() => {
                setSubIndex(s.index);
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
                <button key={a.index} className={info.audio_index === a.index ? "active" : ""} onClick={() => start({ startMs: (videoRef.current?.currentTime ?? 0) * 1000, audioIndex: a.index })}>
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
          {QUALITIES.map((q) => (
            <button key={q} className={quality === q ? "active" : ""} onClick={() => {
              setQuality(q);
              try { localStorage.setItem(K_QUALITY, String(q)); } catch { /* ignora */ }
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

      {error && (
        <div className="player__msg">
          <div className="t-h3">{t("player.cannotPlay")}</div>
          <div className="t-small" style={{ color: "#a2a9b4" }}>{error}</div>
          <div className="row" style={{ justifyContent: "center" }}>
            <button className="pl-btn pl-btn--secondary" onClick={() => nav(-1)}>{t("common.back")}</button>
            <button className="pl-btn pl-btn--primary" onClick={() => start({ startMs: time * 1000, force: true })}>{t("player.retry")}</button>
          </div>
        </div>
      )}
    </div>
  );
}
