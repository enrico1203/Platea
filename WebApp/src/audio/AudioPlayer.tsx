/**
 * Player degli audiolibri: un solo elemento <audio> per tutta l'app, che continua mentre si naviga.
 *
 * - Il libro può essere fatto di più file (parti): il server apre una sessione per parte, la posizione che il player
 *   riferisce è sempre quella nel libro intero. Alla fine di una parte si passa da soli alla successiva.
 * - Capitoli sull'intero libro (quelli dei file m4b, o una parte = un capitolo), velocità con il tono invariato,
 *   timer di spegnimento (minuti o fine del capitolo, con dissolvenza), salti avanti e indietro.
 * - Media Session: titolo del capitolo, copertina e comandi sulla schermata di blocco, cuffie e tastiere multimediali.
 * - Avanzamento salvato ogni 10 s, alla pausa e alla chiusura della scheda; dopo una pausa lunga la sessione si riapre.
 */
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useLocation } from "react-router-dom";
import { useSession } from "../session";
import { api, ApiError, getToken, imageUrl, mediaUrl, serverUrl } from "../api/client";
import type { BookChapter, BookPart, ItemDetail, PlaybackInfo } from "../api/types";
import { useI18n } from "../i18n";

export interface Book {
  id: string; title: string; authors: string | null; narrators: string[]; cover: string | null; color: string | null;
  parts: BookPart[]; chapters: BookChapter[]; duration: number;
}

export type Sleep = { kind: "time"; remaining: number } | { kind: "chapter" } | null;

interface Ctx {
  book: Book | null; playing: boolean; loading: boolean; position: number; rate: number; sleep: Sleep; expanded: boolean; error: string | null;
  chapter: number; skipBack: number; skipForward: number;
  play: (item: ItemDetail, startMs?: number) => Promise<void>;
  toggle: () => void; seek: (ms: number) => void; skip: (seconds: number) => void; chapterJump: (delta: number) => void;
  setRate: (r: number) => void; setSleep: (s: Sleep) => void; setSkips: (back: number, forward: number) => void;
  setExpanded: (v: boolean) => void; close: () => void;
}

const AudioCtx = createContext<Ctx | null>(null);

export function useAudio(): Ctx {
  const c = useContext(AudioCtx);
  if (!c) throw new Error("AudioProvider mancante");
  return c;
}

const K_STATE = "platea.audio.last";
const K_RATE = "platea.audio.rate";
const K_SKIP = "platea.audio.skip";

function load<T>(k: string, fallback: T): T {
  try { const v = localStorage.getItem(k); return v ? (JSON.parse(v) as T) : fallback; } catch { return fallback; }
}
function save(k: string, v: unknown) {
  try { v == null ? localStorage.removeItem(k) : localStorage.setItem(k, JSON.stringify(v)); } catch { /* storage non disponibile */ }
}

/** Ciò che l'elemento audio del browser sa riprodurre così com'è: il resto lo converte il server. */
let profileCache: object | null = null;
/** Manda al log del server il motivo di un errore del browser (dice solo un codice e una frase). */
function reportAudioError(session: string, a: HTMLAudioElement): Promise<unknown> {
  const me = a.error;
  return fetch(`${serverUrl()}/api/v1/playback/${session}/client-error`, {
    method: "POST", keepalive: true,
    headers: { Authorization: `Bearer ${getToken() ?? ""}`, "Content-Type": "application/json" },
    body: JSON.stringify({ message: `audio: ${me ? `codice ${me.code} ${me.message}` : "errore senza dettagli"} (${navigator.userAgent})`.slice(0, 500) }),
  }).catch(() => undefined);
}

function audioProfile() {
  if (profileCache) return profileCache;
  const a = document.createElement("audio");
  const can = (t: string) => a.canPlayType(t) !== "";
  const containers = ["mp4"];
  const codecs: { codec: string; max_channels: number }[] = [{ codec: "aac", max_channels: 8 }];
  if (can("audio/mpeg")) { containers.push("mp3"); codecs.push({ codec: "mp3", max_channels: 2 }); }
  if (can("audio/aac")) containers.push("aac");
  if (can("audio/flac") || can('audio/mp4; codecs="flac"')) { containers.push("flac"); codecs.push({ codec: "flac", max_channels: 8 }); }
  if (can('audio/ogg; codecs="vorbis"') || can('audio/ogg; codecs="opus"')) containers.push("ogg");
  if (can('audio/ogg; codecs="opus"') || can('audio/webm; codecs="opus"') || can('audio/mp4; codecs="opus"')) codecs.push({ codec: "opus", max_channels: 8 });
  if (can('audio/ogg; codecs="vorbis"') || can('audio/webm; codecs="vorbis"')) codecs.push({ codec: "vorbis", max_channels: 8 });
  if (can("audio/webm")) containers.push("webm");
  if (can("audio/wav") || can("audio/wave")) { containers.push("wav"); codecs.push({ codec: "pcm_s16le", max_channels: 2 }, { codec: "pcm_s24le", max_channels: 2 }); }
  if (can('audio/mp4; codecs="alac"')) codecs.push({ codec: "alac", max_channels: 8 });
  if (can('audio/mp4; codecs="ec-3"')) codecs.push({ codec: "eac3", max_channels: 8 });
  if (can('audio/mp4; codecs="ac-3"')) codecs.push({ codec: "ac3", max_channels: 6 });
  profileCache = {
    name: "Platea Web (audio)", containers, video_codecs: [], audio_codecs: codecs, subtitle_text: [], subtitle_image: [], hdr: [],
    max_bitrate: null, protocols: ["file", "hls"], hls_video_codecs: ["h264"], hls_audio_codecs: ["aac", "mp3"],
  };
  return profileCache;
}

export function bookFromItem(item: ItemDetail): Book | null {
  const ab = item.audiobook;
  if (!ab || ab.parts.length === 0) return null;
  return {
    id: item.id, title: item.title,
    authors: item.people.filter((p) => p.role === "author").map((p) => p.name).join(", ") || item.authors || null,
    narrators: item.people.filter((p) => p.role === "narrator").map((p) => p.name),
    cover: item.images.poster, color: item.images.poster_color,
    parts: ab.parts, chapters: ab.chapters, duration: ab.duration_ms || item.runtime_ms || 0,
  };
}

/** Indice del capitolo che contiene la posizione (−1 se il libro non ha capitoli). */
export function chapterAt(chapters: BookChapter[], ms: number): number {
  let i = -1;
  for (let k = 0; k < chapters.length; k++) {
    if (chapters[k].start_ms <= ms + 250) i = k;
    else break;
  }
  return chapters.length > 0 ? Math.max(0, i) : -1;
}

/** Inizio e fine del capitolo `i` (o del libro intero senza capitoli). */
export function chapterBounds(book: Book, i: number): [number, number] {
  if (i < 0 || book.chapters.length === 0) return [0, book.duration];
  const start = book.chapters[i].start_ms;
  const end = i + 1 < book.chapters.length ? book.chapters[i + 1].start_ms : book.duration;
  return [start, Math.max(start, end)];
}

function partAt(parts: BookPart[], ms: number): number {
  let idx = 0;
  for (let i = 0; i < parts.length; i++) {
    if (!parts[i].present) continue;
    if (parts[i].offset_ms <= ms) idx = i;
  }
  return idx;
}

interface Live { session: string; part: number; offset: number; ok: number; method: string }

export function AudioProvider({ children }: { children: ReactNode }) {
  const { t, locale } = useI18n();
  const qc = useQueryClient();
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const [book, setBook] = useState<Book | null>(null);
  const bookRef = useRef<Book | null>(null);
  const live = useRef<Live | null>(null);
  const [playing, setPlaying] = useState(false);
  const [loading, setLoading] = useState(false);
  const [position, setPosition] = useState(0);
  const posRef = useRef(0);
  const [rate, setRateState] = useState(() => load<number>(K_RATE, 1));
  const [skips, setSkipsState] = useState(() => load<{ back: number; forward: number }>(K_SKIP, { back: 15, forward: 30 }));
  const [sleep, setSleepState] = useState<Sleep>(null);
  const sleepRef = useRef<Sleep>(null);
  const [expanded, setExpanded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const opening = useRef<Promise<void> | null>(null);
  const hlsRef = useRef<{ destroy: () => void } | null>(null);
  const lastTick = useRef(0);
  const sleepChapter = useRef<number | null>(null);
  /** L'utente vuole che suoni (per riprendere dopo un errore) e tentativi di ripresa recenti. */
  const intent = useRef(false);
  const retries = useRef<number[]>([]);
  /** Il browser non è riuscito a leggere il file diretto: per questo libro si passa alla transcodifica. */
  const forceTranscode = useRef(false);

  useEffect(() => { bookRef.current = book; }, [book]);
  useEffect(() => { sleepRef.current = sleep; }, [sleep]);

  const audio = () => audioRef.current!;
  const bookPos = useCallback(() => {
    const a = audioRef.current;
    const l = live.current;
    if (!a || !l) return posRef.current;
    return Math.round(l.offset + a.currentTime * 1000);
  }, []);

  const report = useCallback(async (paused: boolean) => {
    const l = live.current;
    if (!l) return;
    const pos = bookPos();
    save(K_STATE, { id: bookRef.current?.id, position: pos, at: Date.now() });
    try {
      await api(`/api/v1/playback/${l.session}/progress`, { body: { position_ms: pos, paused } });
      l.ok = Date.now();
    } catch (e) {
      // Sessione chiusa dal server (inattività, riavvio): si riapre al prossimo play.
      if ((e as ApiError).status === 404 && live.current === l) live.current = null;
    }
  }, [bookPos]);

  const stopSession = useCallback((pos: number) => {
    const l = live.current;
    live.current = null;
    if (!l) return;
    fetch(`${serverUrl()}/api/v1/playback/${l.session}/stop`, {
      method: "POST", keepalive: true,
      headers: { Authorization: `Bearer ${getToken() ?? ""}`, "Content-Type": "application/json" },
      body: JSON.stringify({ position_ms: Math.round(pos) }),
    }).catch(() => undefined);
  }, []);

  /** Apre la sessione della parte che contiene `ms` e ci si posiziona. */
  const open = useCallback(async (ms: number, autoplay: boolean) => {
    const b = bookRef.current;
    if (!b) return;
    const a = audio();
    const i = partAt(b.parts, ms);
    const part = b.parts[i];
    setLoading(true);
    setError(null);
    hlsRef.current?.destroy();
    hlsRef.current = null;
    try {
      const info = await api<PlaybackInfo>("/api/v1/playback/info", {
        body: { item_id: b.id, file_id: part.file_id, start_ms: Math.round(ms), profile: audioProfile(), ui_language: locale, force_transcode: forceTranscode.current },
      });
      const offset = info.part_offset_ms ?? part.offset_ms;
      const local = Math.max(0, (ms - offset) / 1000);
      live.current = { session: info.session_id, part: i, offset, ok: Date.now(), method: info.method };
      if (info.stream_url) {
        a.src = `${mediaUrl(info.stream_url)}#t=${local.toFixed(1)}`;
      } else if (info.hls_url) {
        const url = mediaUrl(info.hls_url)!;
        if (a.canPlayType("application/vnd.apple.mpegurl")) a.src = url;
        else {
          const { default: Hls } = await import("hls.js");
          const h = new Hls({ startPosition: local });
          h.loadSource(url);
          h.attachMedia(a);
          hlsRef.current = h;
        }
      }
      // La velocità resta quella scelta anche al cambio di file (il caricamento la riporta al valore predefinito).
      const r = load<number>(K_RATE, 1);
      a.defaultPlaybackRate = r;
      a.playbackRate = r;
      a.preservesPitch = true;
      await new Promise<void>((res) => {
        if (a.readyState >= 1) return res();
        const done = () => { a.removeEventListener("loadedmetadata", done); a.removeEventListener("error", done); res(); };
        a.addEventListener("loadedmetadata", done);
        a.addEventListener("error", done);
      });
      // Il file non si apre (errore già all'avvio): una volta si riprova con la transcodifica, poi si mostra l'errore.
      if (a.error) {
        // Prima il motivo, poi la chiusura: a sessione chiusa il server non saprebbe più a chi attribuirlo.
        await reportAudioError(info.session_id, a);
        stopSession(ms);
        if (info.method === "direct_play" && !forceTranscode.current) {
          forceTranscode.current = true;
          return await open(ms, autoplay);
        }
        throw new ApiError(0, "AudioError", t("audio.errorStream"));
      }
      if (Math.abs(a.currentTime - local) > 1) a.currentTime = local;
      a.playbackRate = r;
      posRef.current = ms;
      setPosition(ms);
      intent.current = autoplay;
      if (autoplay) await a.play().catch(() => undefined);
    } catch (e) {
      const err = e as ApiError;
      setError(err.message || t("audio.errorOpen"));
      setPlaying(false);
    } finally {
      setLoading(false);
    }
  }, [locale, stopSession, t]);

  const openOnce = useCallback((ms: number, autoplay: boolean) => {
    if (opening.current) return opening.current;
    const p = open(ms, autoplay).finally(() => { opening.current = null; });
    opening.current = p;
    return p;
  }, [open]);

  const play = useCallback(async (item: ItemDetail, startMs?: number) => {
    const b = bookFromItem(item);
    if (!b) return;
    const a = audio();
    // Lo stesso libro: si riprende da dove è (anche dopo un ricaricamento, senza sessione).
    if (bookRef.current?.id === b.id && startMs === undefined) {
      intent.current = true;
      if (live.current && Date.now() - live.current.ok < 75_000) await a.play().catch(() => undefined);
      else {
        if (live.current) stopSession(bookPos());
        await openOnce(posRef.current, true);
      }
      return;
    }
    if (live.current) stopSession(bookPos());
    const start = startMs ?? (item.user_data.played ? 0 : item.user_data.position_ms);
    forceTranscode.current = false;
    retries.current = [];
    bookRef.current = b;
    setBook(b);
    posRef.current = start;
    setPosition(start);
    save(K_STATE, { id: b.id, position: start, at: Date.now() });
    await openOnce(start, true);
  }, [bookPos, openOnce, stopSession]);

  const seek = useCallback((ms: number) => {
    const b = bookRef.current;
    if (!b) return;
    const target = Math.max(0, Math.min(ms, b.duration - 500));
    const a = audio();
    const l = live.current;
    const i = partAt(b.parts, target);
    posRef.current = target;
    setPosition(target);
    if (l && l.part === i && a.readyState >= 1) {
      a.currentTime = (target - l.offset) / 1000;
      return;
    }
    const wasPlaying = !a.paused || !l;
    if (l) stopSession(bookPos());
    void openOnce(target, wasPlaying && !!l);
  }, [bookPos, openOnce, stopSession]);

  const toggle = useCallback(() => {
    const a = audio();
    if (!bookRef.current) return;
    // Nessuna sessione (libro ripreso dopo un ricaricamento) o sessione scaduta durante una pausa lunga: si riapre.
    const stale = live.current && Date.now() - live.current.ok > 75_000 && a.paused;
    if (!live.current || stale) {
      const pos = live.current ? bookPos() : posRef.current;
      if (stale) stopSession(pos);
      void openOnce(pos, true);
      return;
    }
    intent.current = a.paused;
    if (a.paused) void a.play().catch(() => undefined);
    else a.pause();
  }, [bookPos, openOnce, stopSession]);

  const skip = useCallback((seconds: number) => seek(bookPos() + seconds * 1000), [bookPos, seek]);

  const chapterJump = useCallback((delta: number) => {
    const b = bookRef.current;
    if (!b) return;
    const pos = bookPos();
    if (b.chapters.length === 0) return skip(delta * 30);
    const i = chapterAt(b.chapters, pos);
    // "Indietro" nei primi 3 secondi del capitolo va al precedente, altrimenti all'inizio di questo (come i lettori musicali).
    const target = delta < 0 && pos - b.chapters[i].start_ms > 3000 ? i : i + delta;
    if (target < 0) return seek(0);
    if (target >= b.chapters.length) return;
    seek(b.chapters[target].start_ms);
  }, [bookPos, seek, skip]);

  const setRate = useCallback((r: number) => {
    const v = Math.round(Math.max(0.5, Math.min(3, r)) * 100) / 100;
    setRateState(v);
    save(K_RATE, v);
    if (audioRef.current) audioRef.current.playbackRate = v;
  }, []);

  const setSkips = useCallback((back: number, forward: number) => {
    setSkipsState({ back, forward });
    save(K_SKIP, { back, forward });
  }, []);

  const setSleep = useCallback((s: Sleep) => {
    setSleepState(s);
    if (audioRef.current) audioRef.current.volume = 1;
    lastTick.current = performance.now();
  }, []);

  const close = useCallback(() => {
    const a = audioRef.current;
    if (a) {
      a.pause();
      stopSession(bookPos());
      a.removeAttribute("src");
      a.load();
    }
    hlsRef.current?.destroy();
    hlsRef.current = null;
    setBook(null);
    bookRef.current = null;
    setExpanded(false);
    setSleepState(null);
    save(K_STATE, null);
    qc.invalidateQueries({ queryKey: ["home"] });
  }, [bookPos, qc, stopSession]);

  // Libro dell'ultima volta (dopo un ricaricamento): pronto nel mini player, in pausa, senza sessione.
  useEffect(() => {
    const last = load<{ id?: string; position: number; at: number } | null>(K_STATE, null);
    if (!last?.id) return;
    api<ItemDetail>(`/api/v1/items/${last.id}`).then((item) => {
      if (bookRef.current) return;
      const b = bookFromItem(item);
      if (!b) return;
      // Vale la posizione più recente: quella del server (altro dispositivo) o quella di questo browser.
      const server = item.user_data.last_played_at ?? 0;
      const pos = server > last.at ? item.user_data.position_ms : last.position;
      bookRef.current = b;
      setBook(b);
      posRef.current = pos;
      setPosition(pos);
    }).catch(() => save(K_STATE, null));
  }, []);

  // Eventi dell'elemento audio.
  useEffect(() => {
    const a = audioRef.current!;
    const onTime = () => {
      const l = live.current;
      const b = bookRef.current;
      if (!l || !b) return;
      const pos = Math.round(l.offset + a.currentTime * 1000);
      posRef.current = pos;
      setPosition(pos);
      // Timer di spegnimento: conta solo mentre suona.
      const now = performance.now();
      const dt = lastTick.current ? Math.min(2000, now - lastTick.current) : 0;
      lastTick.current = now;
      const s = sleepRef.current;
      if (s?.kind === "time" && !a.paused) {
        const left = s.remaining - dt;
        if (left <= 0) {
          intent.current = false;
          a.pause();
          a.volume = 1;
          setSleepState(null);
        } else {
          // Ultimi 10 secondi: il volume scende piano.
          a.volume = left < 10_000 ? Math.max(0.05, left / 10_000) : 1;
          sleepRef.current = { kind: "time", remaining: left };
          if (Math.floor(left / 1000) !== Math.floor(s.remaining / 1000)) setSleepState({ kind: "time", remaining: left });
        }
      } else if (s?.kind === "chapter" && b.chapters.length > 0) {
        const i = chapterAt(b.chapters, pos);
        const end = chapterBounds(b, i)[1];
        if (sleepChapter.current === null) sleepChapter.current = i;
        if (i !== sleepChapter.current || pos >= end - 300) {
          intent.current = false;
          a.pause();
          sleepChapter.current = null;
          setSleepState(null);
        }
      }
    };
    const onPlay = () => { setPlaying(true); lastTick.current = performance.now(); void report(false); };
    const onPause = () => { setPlaying(false); void report(true); };
    const onEnded = () => {
      const b = bookRef.current;
      const l = live.current;
      if (!b || !l) return;
      const next = b.parts.findIndex((p, i) => i > l.part && p.present);
      if (next >= 0) {
        stopSession(b.parts[next].offset_ms);
        void openOnce(b.parts[next].offset_ms, true);
      } else {
        // Fine del libro: il server lo segna come ascoltato.
        stopSession(b.duration);
        setPlaying(false);
        posRef.current = 0;
        setPosition(0);
        save(K_STATE, { id: b.id, position: 0, at: Date.now() });
        qc.invalidateQueries();
      }
    };
    const onError = () => {
      const b = bookRef.current;
      if (!b || !live.current || opening.current || !a.src) return;
      // Flusso interrotto (sessione scaduta, rete): si riapre una volta dalla posizione corrente.
      // Al massimo tre tentativi in un minuto: poi si mostra l'errore.
      const now = Date.now();
      retries.current = retries.current.filter((x) => now - x < 60_000);
      if (retries.current.length >= 3) {
        setError(t("audio.errorStream"));
        setPlaying(false);
        return;
      }
      retries.current.push(now);
      // Il motivo arriva nel log del server (il browser dice solo un codice e una frase): serve per capire i casi
      // che non si riproducono qui. Dopo un errore in diretta si riapre con la transcodifica, come per i video.
      const l = live.current;
      if (l) {
        void reportAudioError(l.session, a);
        if (l.method === "direct_play") forceTranscode.current = true;
      }
      const pos = bookPos();
      live.current = null;
      void openOnce(pos, intent.current);
    };
    const onWaiting = () => setLoading(true);
    const onPlaying = () => setLoading(false);
    a.addEventListener("timeupdate", onTime);
    a.addEventListener("play", onPlay);
    a.addEventListener("pause", onPause);
    a.addEventListener("ended", onEnded);
    a.addEventListener("error", onError);
    a.addEventListener("waiting", onWaiting);
    a.addEventListener("playing", onPlaying);
    a.addEventListener("canplay", onPlaying);
    return () => {
      a.removeEventListener("timeupdate", onTime);
      a.removeEventListener("play", onPlay);
      a.removeEventListener("pause", onPause);
      a.removeEventListener("ended", onEnded);
      a.removeEventListener("error", onError);
      a.removeEventListener("waiting", onWaiting);
      a.removeEventListener("playing", onPlaying);
      a.removeEventListener("canplay", onPlaying);
    };
  }, [bookPos, openOnce, qc, report, stopSession, t]);
  useEffect(() => { if (sleep?.kind !== "chapter") sleepChapter.current = null; }, [sleep]);

  // Avanzamento ogni 10 s mentre suona; in pausa ogni 40 s, perché la sessione non scada.
  useEffect(() => {
    if (!book) return;
    const h = window.setInterval(() => {
      const a = audioRef.current;
      if (!a || !live.current) return;
      if (!a.paused) void report(false);
      else if (Date.now() - live.current.ok > 40_000) void report(true);
    }, 10_000);
    return () => window.clearInterval(h);
  }, [book, report]);

  // Chiusura della scheda: posizione salvata e sessione chiusa.
  useEffect(() => {
    const onHide = () => {
      if (!live.current) return;
      const pos = bookPos();
      save(K_STATE, { id: bookRef.current?.id, position: pos, at: Date.now() });
      stopSession(pos);
    };
    const onVisibility = () => { if (document.visibilityState === "hidden" && live.current) void report(audioRef.current?.paused ?? true); };
    window.addEventListener("pagehide", onHide);
    document.addEventListener("visibilitychange", onVisibility);
    return () => { window.removeEventListener("pagehide", onHide); document.removeEventListener("visibilitychange", onVisibility); };
  }, [bookPos, report, stopSession]);

  const chapter = book ? chapterAt(book.chapters, position) : -1;

  // Un video in riproduzione ferma l'audiolibro; l'uscita dall'account lo chiude.
  const { pathname } = useLocation();
  useEffect(() => {
    if (pathname.startsWith("/play/") && audioRef.current && !audioRef.current.paused) {
      intent.current = false;
      audioRef.current.pause();
    }
  }, [pathname]);
  const { me } = useSession();
  const hadUser = useRef(false);
  useEffect(() => {
    if (me) hadUser.current = true;
    else if (hadUser.current && bookRef.current) close();
  }, [me, close]);

  // Media Session: schermata di blocco, cuffie, tasti multimediali.
  useEffect(() => {
    if (!("mediaSession" in navigator) || !book) return;
    const ch = chapter >= 0 ? book.chapters[chapter] : null;
    const cover = book.cover ? new URL(imageUrl(book.cover, 512)!, window.location.href).href : undefined;
    navigator.mediaSession.metadata = new MediaMetadata({
      title: ch ? ch.title ?? t("player.chapterN", { n: chapter + 1 }) : book.title,
      artist: book.authors ?? "",
      album: book.title,
      artwork: cover ? [{ src: cover, sizes: "512x512", type: "image/jpeg" }] : [],
    });
  }, [book, chapter, t]);
  useEffect(() => {
    if (!("mediaSession" in navigator)) return;
    const ms = navigator.mediaSession;
    const set = (a: MediaSessionAction, h: MediaSessionActionHandler | null) => { try { ms.setActionHandler(a, h); } catch { /* azione non supportata */ } };
    if (!book) {
      for (const a of ["play", "pause", "seekbackward", "seekforward", "previoustrack", "nexttrack", "seekto"] as MediaSessionAction[]) set(a, null);
      return;
    }
    set("play", () => toggle());
    set("pause", () => { intent.current = false; audioRef.current?.pause(); });
    set("seekbackward", (d) => skip(-(d.seekOffset ?? skips.back)));
    set("seekforward", (d) => skip(d.seekOffset ?? skips.forward));
    set("previoustrack", () => chapterJump(-1));
    set("nexttrack", () => chapterJump(1));
    set("seekto", (d) => {
      if (d.seekTime == null) return;
      const b = bookRef.current!;
      const [start] = chapterBounds(b, chapterAt(b.chapters, posRef.current));
      seek(start + d.seekTime * 1000);
    });
  }, [book, chapterJump, seek, skip, skips, toggle]);
  // Barra della schermata di blocco: il capitolo corrente (come Audible), aggiornata una volta al secondo.
  const lastState = useRef(0);
  useEffect(() => {
    if (!book || !("mediaSession" in navigator) || !navigator.mediaSession.setPositionState) return;
    const now = Date.now();
    if (now - lastState.current < 1000) return;
    lastState.current = now;
    const [start, end] = chapterBounds(book, chapter);
    try {
      navigator.mediaSession.setPositionState({ duration: Math.max(1, (end - start) / 1000), position: Math.max(0, Math.min((end - start) / 1000, (position - start) / 1000)), playbackRate: rate });
    } catch { /* valori fuori intervallo durante un salto */ }
  }, [book, chapter, position, rate]);
  useEffect(() => {
    if ("mediaSession" in navigator) navigator.mediaSession.playbackState = book ? (playing ? "playing" : "paused") : "none";
  }, [book, playing]);

  const value = useMemo<Ctx>(() => ({
    book, playing, loading, position, rate, sleep, expanded, error, chapter, skipBack: skips.back, skipForward: skips.forward,
    play, toggle, seek, skip, chapterJump, setRate, setSleep, setSkips, setExpanded, close,
  }), [book, playing, loading, position, rate, sleep, expanded, error, chapter, skips, play, toggle, seek, skip, chapterJump, setRate, setSleep, setSkips, close]);

  return (
    <AudioCtx.Provider value={value}>
      {children}
      <audio ref={audioRef} preload="auto" hidden />
    </AudioCtx.Provider>
  );
}
