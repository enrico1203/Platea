/**
 * Player dei canali dal vivo: HLS (hls.js o nativo di Safari), niente barra di avanzamento ma l'avanzamento del
 * programma in onda, canale precedente/successivo, mini guida, cambio canale con i numeri o le frecce.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import type Hls from "hls.js";
import { ArrowLeft, ChevronDown, ChevronUp, Info, ListVideo, Maximize, Minimize, Pause, Play, Volume2, VolumeX } from "lucide-react";
import { api, ApiError, getToken, mediaUrl, serverUrl } from "../api/client";
import type { LiveChannel, LivePlay } from "../api/types";
import { buildProfile } from "../lib/profile";
import { MethodBadge } from "../components/ui";
import { useI18n } from "../i18n";
import { ChannelLogo, chNumber } from "./LiveTV";
import type { Method } from "../api/types";

function stop(sessionId: string) {
  fetch(`${serverUrl()}/api/v1/livetv/sessions/${sessionId}/stop`, { method: "POST", keepalive: true, headers: { Authorization: `Bearer ${getToken() ?? ""}` } }).catch(() => undefined);
}

export default function LivePlayer() {
  const { id } = useParams();
  const nav = useNavigate();
  const { t, tag } = useI18n();
  const videoRef = useRef<HTMLVideoElement>(null);
  const hlsRef = useRef<Hls | null>(null);
  const sessionRef = useRef<string | null>(null);
  const [info, setInfo] = useState<LivePlay | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [paused, setPaused] = useState(false);
  const [muted, setMuted] = useState(false);
  const [full, setFull] = useState(false);
  const [showUi, setShowUi] = useState(true);
  const [guide, setGuide] = useState(false);
  const [details, setDetails] = useState(false);
  const [typed, setTyped] = useState("");
  const [now, setNow] = useState(Date.now());
  const retried = useRef(false);
  /** Generazione dell'apertura: un'apertura superata da un cambio di canale si chiude da sola. */
  const gen = useRef(0);
  const fmt = new Intl.DateTimeFormat(tag, { hour: "2-digit", minute: "2-digit" });
  const { data: channels } = useQuery({
    queryKey: ["livetv", "channels", ""],
    queryFn: () => api<{ items: LiveChannel[] }>("/api/v1/livetv/channels").then((r) => r.items),
    refetchInterval: 60_000,
  });
  const idx = channels?.findIndex((c) => c.id === id) ?? -1;
  const channel = (idx >= 0 ? channels?.[idx] : undefined) ?? info?.channel;
  const program = channel?.now ?? null;

  const start = useCallback(async (force = false) => {
    if (!id) return;
    const mine = ++gen.current;
    setLoading(true);
    setError(null);
    hlsRef.current?.destroy();
    hlsRef.current = null;
    if (sessionRef.current) { stop(sessionRef.current); sessionRef.current = null; }
    try {
      const r = await api<LivePlay>(`/api/v1/livetv/channels/${id}/play`, { body: { profile: buildProfile(null), force_transcode: force } });
      if (mine !== gen.current) { stop(r.session_id); return; }
      sessionRef.current = r.session_id;
      setInfo(r);
      const v = videoRef.current!;
      const url = mediaUrl(r.hls_url)!;
      const safariNative = "WebKitPlaybackTargetAvailabilityEvent" in window && v.canPlayType("application/vnd.apple.mpegurl") !== "";
      const { default: HlsLib } = await import("hls.js");
      if (mine !== gen.current) return;
      if (!safariNative && HlsLib.isSupported()) {
        const hls = new HlsLib({ liveSyncDurationCount: 3, liveMaxLatencyDurationCount: 8, manifestLoadingMaxRetry: 6, levelLoadingMaxRetry: 6, fragLoadingMaxRetry: 6 });
        hls.on(HlsLib.Events.ERROR, (_e, d) => {
          if (!d.fatal) return;
          if (d.type === HlsLib.ErrorTypes.MEDIA_ERROR && !retried.current) {
            // Il browser non decodifica il flusso copiato: si riprova con la conversione.
            retried.current = true;
            void start(true);
            return;
          }
          if (d.type === HlsLib.ErrorTypes.NETWORK_ERROR) { hls.startLoad(); return; }
          setError(t("tv.errorStream"));
        });
        hls.loadSource(url);
        hls.attachMedia(v);
        hlsRef.current = hls;
      } else {
        v.src = url;
      }
      await v.play().catch(() => setPaused(true));
    } catch (e) {
      setError((e as ApiError).message || t("tv.errorStream"));
    } finally {
      setLoading(false);
    }
  }, [id, t]);

  useEffect(() => {
    retried.current = false;
    void start();
    return () => {
      gen.current++;
      hlsRef.current?.destroy();
      hlsRef.current = null;
      if (sessionRef.current) stop(sessionRef.current);
      sessionRef.current = null;
    };
  }, [start]);

  // La sessione resta viva; l'ora fa avanzare la barra del programma.
  useEffect(() => {
    const h = window.setInterval(() => {
      setNow(Date.now());
      if (sessionRef.current) api(`/api/v1/livetv/sessions/${sessionRef.current}/ping`, { method: "POST" }).catch(() => undefined);
    }, 20_000);
    const tick = window.setInterval(() => setNow(Date.now()), 1000);
    const onHide = () => { if (sessionRef.current) stop(sessionRef.current); };
    window.addEventListener("pagehide", onHide);
    return () => { window.clearInterval(h); window.clearInterval(tick); window.removeEventListener("pagehide", onHide); };
  }, []);

  const go = useCallback((delta: number) => {
    if (!channels || channels.length === 0) return;
    const next = channels[((idx < 0 ? 0 : idx) + delta + channels.length) % channels.length];
    nav(`/tv/watch/${next.id}`, { replace: true });
  }, [channels, idx, nav]);

  // Numeri digitati: si cambia canale dopo un attimo di pausa.
  useEffect(() => {
    if (!typed) return;
    const h = window.setTimeout(() => {
      const c = channels?.find((x) => chNumber(x.number) === typed);
      if (c) nav(`/tv/watch/${c.id}`, { replace: true });
      setTyped("");
    }, 1200);
    return () => window.clearTimeout(h);
  }, [typed, channels, nav]);

  const toggleFull = () => {
    const el = document.documentElement;
    if (document.fullscreenElement) void document.exitFullscreen();
    else if (el.requestFullscreen) void el.requestFullscreen();
    else (videoRef.current as unknown as { webkitEnterFullscreen?: () => void })?.webkitEnterFullscreen?.();
  };
  useEffect(() => {
    const f = () => setFull(!!document.fullscreenElement);
    document.addEventListener("fullscreenchange", f);
    return () => document.removeEventListener("fullscreenchange", f);
  }, []);

  // Comandi nascosti dopo 3 secondi senza movimento.
  const hideTimer = useRef<number | null>(null);
  const poke = useCallback(() => {
    setShowUi(true);
    if (hideTimer.current) window.clearTimeout(hideTimer.current);
    hideTimer.current = window.setTimeout(() => setShowUi(false), 3000);
  }, []);
  useEffect(() => { poke(); }, [poke, id]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      poke();
      const v = videoRef.current;
      if (/^[0-9]$/.test(e.key)) { setTyped((x) => (x + e.key).slice(-4)); return; }
      switch (e.key) {
        case "ArrowUp": case "PageUp": case "ChannelUp": e.preventDefault(); go(-1); break;
        case "ArrowDown": case "PageDown": case "ChannelDown": e.preventDefault(); go(1); break;
        case " ": case "k": e.preventDefault(); if (v) { if (v.paused) void v.play(); else v.pause(); } break;
        case "m": if (v) { v.muted = !v.muted; setMuted(v.muted); } break;
        case "f": toggleFull(); break;
        case "g": setGuide((g) => !g); break;
        case "Escape": if (guide) setGuide(false); else nav(-1); break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [go, guide, nav, poke]);

  const pct = program ? Math.min(100, Math.max(0, (now - program.start_ms) / (program.end_ms - program.start_ms) * 100)) : 0;
  const method = (info?.method === "live_transcode" ? "transcode" : info?.method === "live_direct" ? "direct_play" : "remux") as Method;
  return (
    <div className={`player tv-player${showUi || paused || guide ? " show-ui" : ""}`} onMouseMove={poke} onClick={poke}>
      <video ref={videoRef} className="player__video" playsInline autoPlay
        onPlaying={() => { setLoading(false); setPaused(false); }} onPause={() => setPaused(true)} onWaiting={() => setLoading(true)}
        onVolumeChange={(e) => setMuted(e.currentTarget.muted)} />
      {loading && !error && <div className="tv-player__spinner" aria-label={t("tv.loading")} />}
      {typed && <div className="tv-player__typed t-numeral">{typed}</div>}
      {error && (
        <div className="tv-player__error">
          <div className="t-h3">{t("tv.cannotPlay")}</div>
          <div className="muted">{error}</div>
          <div className="row" style={{ justifyContent: "center" }}>
            <button className="pl-btn pl-btn--primary" onClick={() => { retried.current = false; void start(); }}>{t("tv.retry")}</button>
            <button className="pl-btn pl-btn--ghost" onClick={() => nav(-1)}>{t("common.back")}</button>
          </div>
        </div>
      )}
      <div className="tv-player__top">
        <button className="icon-btn" onClick={() => nav(-1)} aria-label={t("common.back")}><ArrowLeft /></button>
        {channel && <ChannelLogo c={channel} size={40} />}
        <div className="tv-player__title">
          <div className="t-label">{channel ? `${chNumber(channel.number)} ${channel.name}` : ""} <span className="tv-live">{t("tv.live")}</span></div>
          {program && <div className="t-small muted">{program.title}{program.subtitle ? ` · ${program.subtitle}` : ""}</div>}
        </div>
        <span className="spacer" />
        {info && <button className="icon-btn" onClick={() => setDetails(!details)} aria-label={t("player.info")} title={t("player.info")}><Info /></button>}
      </div>
      {details && info && (
        <div className="tv-player__details">
          <MethodBadge method={method} />
          <span className="t-mono-sm">{info.chain}</span>
        </div>
      )}
      <div className="tv-player__bottom">
        {program && (
          <div className="tv-player__program">
            <span className="t-mono-sm">{fmt.format(program.start_ms)}</span>
            <div className="tv-progress"><span style={{ width: `${pct}%` }} /></div>
            <span className="t-mono-sm">{fmt.format(program.end_ms)}</span>
          </div>
        )}
        {channel?.next && <div className="t-small subtle">{t("tv.next", { time: fmt.format(channel.next.start_ms), title: channel.next.title })}</div>}
        <div className="tv-player__controls">
          <button className="icon-btn" onClick={() => go(-1)} aria-label={t("tv.prevChannel")} title={t("tv.prevChannel")}><ChevronUp /></button>
          <button className="pl-btn pl-btn--play" onClick={() => { const v = videoRef.current; if (v) { if (v.paused) void v.play(); else v.pause(); } }} aria-label={paused ? t("player.play") : t("player.pause")}>
            {paused ? <Play fill="currentColor" /> : <Pause fill="currentColor" />}
          </button>
          <button className="icon-btn" onClick={() => go(1)} aria-label={t("tv.nextChannel")} title={t("tv.nextChannel")}><ChevronDown /></button>
          <span className="spacer" />
          <button className="icon-btn" onClick={() => { const v = videoRef.current; if (v) { v.muted = !v.muted; } }} aria-label={muted ? t("player.unmute") : t("player.mute")}>{muted ? <VolumeX /> : <Volume2 />}</button>
          <button className={`icon-btn${guide ? " on" : ""}`} onClick={() => setGuide(!guide)} aria-label={t("tv.miniGuide")} title={t("tv.miniGuide")}><ListVideo /></button>
          <button className="icon-btn" onClick={toggleFull} aria-label={t("player.fullscreen")}>{full ? <Minimize /> : <Maximize />}</button>
        </div>
      </div>
      {guide && channels && (
        <div className="tv-mini" role="dialog" aria-label={t("tv.miniGuide")}>
          {channels.map((c) => (
            <button key={c.id} type="button" className={`tv-mini__row${c.id === id ? " is-current" : ""}`} onClick={() => { setGuide(false); nav(`/tv/watch/${c.id}`, { replace: true }); }}>
              <ChannelLogo c={c} size={32} />
              <span className="tv-mini__text"><span className="t-label">{chNumber(c.number)} {c.name}</span><span className="t-small muted">{c.now?.title ?? ""}</span></span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
