/**
 * Live TV: guida a griglia (canali in verticale, tempo in orizzontale, caricata a blocchi di canali man mano che si
 * scorre), elenco dei canali con il programma in onda, ricerca nei programmi, registrazioni (con Dispatcharr).
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { CircleDot, Heart, Info, List, LayoutGrid, Play, Radio, Search as SearchIcon, Trash2, Tv } from "lucide-react";
import { api, ApiError, imageUrl } from "../api/client";
import type { LiveChannel, LiveGuide, LiveProgram, LiveRecording, LiveStatus } from "../api/types";
import { Alert, Badge, Dialog, Empty, SelectField, TextField, toast } from "../components/ui";
import { useSession } from "../session";
import { useI18n } from "../i18n";

const BATCH = 40;
const ROW_H = 64;

export function useLiveStatus() {
  const { me } = useSession();
  return useQuery({
    queryKey: ["livetv", "status"],
    queryFn: () => api<LiveStatus>("/api/v1/livetv/status"),
    enabled: !!me && me.permissions.allow_livetv !== false,
    staleTime: 60_000,
  });
}

export function ChannelLogo({ c, size = 40 }: { c: LiveChannel; size?: number }) {
  return (
    <span className="tv-logo" style={{ width: size, height: size }}>
      {c.logo ? <img src={imageUrl(c.logo, size * 2)} alt="" loading="lazy" /> : <span className="tv-logo__num">{c.number ?? c.name.slice(0, 2)}</span>}
    </span>
  );
}

export function chNumber(n: number | null) {
  return n == null ? "" : Number.isInteger(n) ? String(n) : String(n);
}

function useNow(every = 30_000) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const h = window.setInterval(() => setNow(Date.now()), every);
    return () => window.clearInterval(h);
  }, [every]);
  return now;
}

/** Ore:minuti nella lingua dell'interfaccia. */
function useTime() {
  const { tag } = useI18n();
  const f = useMemo(() => new Intl.DateTimeFormat(tag, { hour: "2-digit", minute: "2-digit" }), [tag]);
  return (ms: number) => f.format(new Date(ms));
}

/** Scheda di un programma: dettagli, Guarda, Registra, Registra la serie. */
export function ProgramDialog({ programId, onClose }: { programId: string; onClose: () => void }) {
  const { t, date } = useI18n();
  const time = useTime();
  const nav = useNavigate();
  const qc = useQueryClient();
  const { data: status } = useLiveStatus();
  const { data: p, error } = useQuery({ queryKey: ["livetv", "program", programId], queryFn: () => api<LiveProgram>(`/api/v1/livetv/programs/${programId}`) });
  const [busy, setBusy] = useState(false);
  const now = Date.now();
  const airing = p && p.start_ms <= now && p.end_ms > now;
  const ended = p && p.end_ms <= now;
  const record = async (series: boolean) => {
    if (!p) return;
    setBusy(true);
    try {
      await api("/api/v1/livetv/recordings", { body: { program_id: p.id, series } });
      toast(series ? t("tv.seriesScheduled", { title: p.title }) : t("tv.recordingScheduled", { title: p.title }));
      qc.invalidateQueries({ queryKey: ["livetv"] });
    } catch (e) { toast((e as ApiError).message); } finally { setBusy(false); }
  };
  const cancel = async () => {
    if (!p?.recording) return;
    setBusy(true);
    try {
      await api(`/api/v1/livetv/recordings/${p.recording.id}`, { method: "DELETE" });
      toast(t("tv.recordingCancelled"));
      qc.invalidateQueries({ queryKey: ["livetv"] });
    } catch (e) { toast((e as ApiError).message); } finally { setBusy(false); }
  };
  return (
    <Dialog title={p?.title ?? "…"} onClose={onClose} footer={p && (
      <>
        {airing && <button className="pl-btn pl-btn--primary" onClick={() => nav(`/tv/watch/${p.channel_id}`)}><Play size={18} fill="currentColor" />{t("tv.watch")}</button>}
        {status?.can_record && p.channel?.recordable && !ended && !p.recording && (
          <>
            <button className="pl-btn pl-btn--secondary" disabled={busy} onClick={() => record(false)}><CircleDot size={18} />{t("tv.record")}</button>
            {(p.season != null || p.episode != null || p.subtitle) && <button className="pl-btn pl-btn--secondary" disabled={busy} onClick={() => record(true)}>{t("tv.recordSeries")}</button>}
          </>
        )}
        {p.recording && <button className="pl-btn pl-btn--ghost" disabled={busy} onClick={cancel}>{p.recording.kind === "series" ? t("tv.cancelSeries") : t("tv.cancelRecording")}</button>}
      </>
    )}>
      {error && <Alert tone="danger">{(error as ApiError).message}</Alert>}
      {p && (
        <div className="stack">
          {p.image && <img className="tv-program-image" src={p.image} alt="" referrerPolicy="no-referrer" />}
          {p.subtitle && <div className="t-label">{p.subtitle}</div>}
          <div className="row" style={{ gap: 8, alignItems: "center" }}>
            {p.channel && <ChannelLogo c={p.channel} size={28} />}
            <span className="t-small muted">{[p.channel?.name, `${date(p.start_ms)} · ${time(p.start_ms)}–${time(p.end_ms)}`].filter(Boolean).join(" · ")}</span>
          </div>
          <div className="chips">
            {airing && <Badge tone="direct">{t("tv.onAir")}</Badge>}
            {p.is_new && <Badge>{t("tv.new")}</Badge>}
            {p.category && <Badge>{p.category}</Badge>}
            {(p.season != null || p.episode != null) && <Badge>{[p.season != null ? `S${p.season}` : null, p.episode != null ? `E${p.episode}` : null].filter(Boolean).join(" · ")}</Badge>}
            {p.rating && <Badge>{p.rating}</Badge>}
            {p.recording && <Badge tone="danger">{p.recording.kind === "series" ? t("tv.seriesActive") : t(`tv.status.${p.recording.status}` as "tv.status.scheduled")}</Badge>}
          </div>
          {p.description && <p className="muted" style={{ margin: 0, whiteSpace: "pre-line" }}>{p.description}</p>}
        </div>
      )}
    </Dialog>
  );
}

/** Un blocco di righe della guida: i programmi si chiedono quando il blocco sta per entrare in vista. */
function GuideBatch({ channels, start, end, px, onProgram, now }: { channels: LiveChannel[]; start: number; end: number; px: number; onProgram: (id: string) => void; now: number }) {
  const ref = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(false);
  const nav = useNavigate();
  const time = useTime();
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const io = new IntersectionObserver((e) => { if (e[0].isIntersecting) setVisible(true); }, { rootMargin: "600px 0px" });
    io.observe(el);
    return () => io.disconnect();
  }, []);
  const ids = channels.map((c) => c.id).join(",");
  const { data } = useQuery({
    queryKey: ["livetv", "guide", ids, start, end],
    queryFn: () => api<LiveGuide>(`/api/v1/livetv/guide?start=${start}&end=${end}&channels=${ids}`),
    enabled: visible,
    staleTime: 5 * 60_000,
  });
  const programs = new Map((data?.channels ?? []).map((c) => [c.id, c.programs ?? []]));
  return (
    <div ref={ref}>
      {channels.map((c) => (
        <div className="tv-row" key={c.id} style={{ height: ROW_H }}>
          <button type="button" className="tv-ch" onClick={() => nav(`/tv/watch/${c.id}`)} title={c.name}>
            <ChannelLogo c={c} size={36} />
            <span className="tv-ch__text"><span className="tv-ch__num t-mono-sm">{chNumber(c.number)}</span><span className="tv-ch__name">{c.name}</span></span>
            {c.favorite && <Heart size={12} fill="currentColor" className="tv-ch__fav" />}
          </button>
          <div className="tv-progs" style={{ width: (end - start) / 60000 * px }}>
            {(programs.get(c.id) ?? []).map((p) => {
              const left = Math.max(0, (p.start_ms - start) / 60000 * px);
              const right = Math.min((end - start) / 60000 * px, (p.end_ms - start) / 60000 * px);
              const w = right - left;
              if (w < 2) return null;
              const airing = p.start_ms <= now && p.end_ms > now;
              return (
                <button type="button" key={p.id} className={`tv-prog${airing ? " is-now" : p.end_ms <= now ? " is-past" : ""}`} style={{ left, width: w - 2 }} onClick={() => onProgram(p.id)}>
                  <span className="tv-prog__title">{p.title}</span>
                  {w > 70 && <span className="tv-prog__time t-mono-sm">{time(p.start_ms)}</span>}
                </button>
              );
            })}
          </div>
        </div>
      ))}
    </div>
  );
}

function Guide({ channels, onProgram }: { channels: LiveChannel[]; onProgram: (id: string) => void }) {
  const { t, tag } = useI18n();
  const time = useTime();
  const now = useNow();
  const narrow = typeof window !== "undefined" && window.matchMedia("(max-width: 720px)").matches;
  const px = narrow ? 4 : 5;
  const [day, setDay] = useState(0);
  const dayStart = useMemo(() => { const d = new Date(); d.setHours(0, 0, 0, 0); d.setDate(d.getDate() + day); return d.getTime(); }, [day]);
  // Oggi si parte da un'ora fa; gli altri giorni da mezzanotte.
  const start = day === 0 ? Math.floor((Date.now() - 3_600_000) / 1_800_000) * 1_800_000 : dayStart;
  const end = dayStart + 24 * 3_600_000 + 3 * 3_600_000;
  const scroller = useRef<HTMLDivElement>(null);
  const ticks: number[] = [];
  for (let x = start; x < end; x += 1_800_000) ticks.push(x);
  const days = Array.from({ length: 7 }, (_, i) => {
    const d = new Date(); d.setDate(d.getDate() + i);
    return { value: String(i), label: i === 0 ? t("tv.today") : i === 1 ? t("tv.tomorrow") : new Intl.DateTimeFormat(tag, { weekday: "long", day: "numeric" }).format(d) };
  });
  const batches: LiveChannel[][] = [];
  for (let i = 0; i < channels.length; i += BATCH) batches.push(channels.slice(i, i + BATCH));
  const nowLeft = (now - start) / 60000 * px;
  const toNow = () => scroller.current?.scrollTo({ left: Math.max(0, nowLeft - 120), behavior: "smooth" });
  return (
    <div className="stack">
      <div className="row" style={{ gap: 8, alignItems: "flex-end" }}>
        <SelectField label={t("tv.day")} value={String(day)} onChange={(v) => { setDay(Number(v)); scroller.current?.scrollTo({ left: 0 }); }} options={days} />
        {day === 0 && <button type="button" className="pl-btn pl-btn--secondary" onClick={toNow}>{t("tv.now")}</button>}
      </div>
      <div className="tv-guide" ref={scroller}>
        <div className="tv-guide__inner" style={{ width: 180 + (end - start) / 60000 * px }}>
          <div className="tv-ruler">
            <div className="tv-ruler__corner" />
            <div className="tv-ruler__times" style={{ width: (end - start) / 60000 * px }}>
              {ticks.map((x) => <span key={x} style={{ left: (x - start) / 60000 * px }} className="t-mono-sm">{time(x)}</span>)}
            </div>
          </div>
          <div className="tv-body">
            {now >= start && now < end && <div className="tv-nowline" style={{ left: 180 + nowLeft }} aria-hidden />}
            {batches.map((b, i) => <GuideBatch key={`${i}-${start}`} channels={b} start={start} end={end} px={px} onProgram={onProgram} now={now} />)}
          </div>
        </div>
      </div>
    </div>
  );
}

function ChannelList({ channels, onProgram }: { channels: LiveChannel[]; onProgram: (id: string) => void }) {
  const { t } = useI18n();
  const time = useTime();
  const now = useNow();
  const nav = useNavigate();
  const qc = useQueryClient();
  return (
    <div className="pl-panel tv-list">
      {channels.map((c) => {
        const p = c.now;
        const pct = p ? Math.min(100, Math.max(0, (now - p.start_ms) / (p.end_ms - p.start_ms) * 100)) : 0;
        return (
          <div key={c.id} className="tv-list__row">
            <button type="button" className="tv-list__main" onClick={() => nav(`/tv/watch/${c.id}`)}>
              <ChannelLogo c={c} size={44} />
              <span className="tv-list__text">
                <span className="t-label">{chNumber(c.number)} {c.name} {c.radio && <Radio size={14} className="subtle" />}</span>
                <span className="t-small muted tv-list__now">{p ? p.title : t("tv.noGuide")}</span>
                {p && <span className="tv-progress"><span style={{ width: `${pct}%` }} /></span>}
                {c.next && <span className="t-small subtle tv-list__next">{t("tv.next", { time: time(c.next.start_ms), title: c.next.title })}</span>}
              </span>
            </button>
            <div className="tv-list__actions">
              {p && <button type="button" className="icon-btn" aria-label={t("tv.details")} title={t("tv.details")} onClick={() => onProgram(p.id)}><Info /></button>}
              <button type="button" className={`icon-btn${c.favorite ? " on" : ""}`} aria-label={t("tv.favorite")} title={t("tv.favorite")} onClick={async () => {
                await api(`/api/v1/livetv/channels/${c.id}/favorite`, { method: c.favorite ? "DELETE" : "POST" });
                qc.invalidateQueries({ queryKey: ["livetv"] });
              }}><Heart fill={c.favorite ? "currentColor" : "none"} /></button>
            </div>
          </div>
        );
      })}
    </div>
  );
}

function Recordings() {
  const { t, date } = useI18n();
  const time = useTime();
  const qc = useQueryClient();
  const { data } = useQuery({ queryKey: ["livetv", "recordings"], queryFn: () => api<{ items: LiveRecording[] }>("/api/v1/livetv/recordings").then((r) => r.items), refetchInterval: 30_000 });
  if (!data) return null;
  if (data.length === 0) return <Empty>{t("tv.noRecordings")}</Empty>;
  const series = data.filter((r) => r.kind === "series");
  const once = data.filter((r) => r.kind === "once");
  const row = (r: LiveRecording) => (
    <div key={r.id} className="list-row">
      {r.channel ? <ChannelLogo c={r.channel} size={36} /> : <Tv size={20} />}
      <div className="spacer" style={{ minWidth: 0 }}>
        <div className="t-label">{r.title}{r.subtitle ? ` · ${r.subtitle}` : ""}</div>
        <div className="t-small subtle">{[r.channel?.name, r.kind === "series" ? t("tv.seriesRule") : `${date(r.start_ms)} · ${time(r.start_ms)}–${time(r.end_ms)}`].filter(Boolean).join(" · ")}</div>
      </div>
      <Badge tone={r.status === "recording" ? "danger" : r.status === "completed" ? "direct" : undefined}>{r.kind === "series" ? t("tv.seriesActive") : t(`tv.status.${r.status}` as "tv.status.scheduled")}</Badge>
      {r.item_id && <Link className="pl-btn pl-btn--secondary pl-btn--sm" to={`/item/${r.item_id}`}><Play size={14} fill="currentColor" />{t("tv.watch")}</Link>}
      {r.mine && r.status !== "completed" && (
        <button type="button" className="icon-btn" aria-label={t("tv.cancelRecording")} title={t("tv.cancelRecording")} onClick={async () => {
          try { await api(`/api/v1/livetv/recordings/${r.id}`, { method: "DELETE" }); qc.invalidateQueries({ queryKey: ["livetv"] }); }
          catch (e) { toast((e as ApiError).message); }
        }}><Trash2 /></button>
      )}
    </div>
  );
  return (
    <div className="stack-lg">
      {series.length > 0 && <section className="stack"><h2 className="t-h3">{t("tv.seriesRules")}</h2><div className="pl-panel">{series.map(row)}</div></section>}
      {once.length > 0 && <section className="stack"><h2 className="t-h3">{t("tv.recordings")}</h2><div className="pl-panel">{once.map(row)}</div></section>}
      <div className="t-small subtle">{t("tv.recordingsHint")}</div>
    </div>
  );
}

function ProgramSearch({ q, onProgram }: { q: string; onProgram: (id: string) => void }) {
  const { t, date } = useI18n();
  const time = useTime();
  const { data } = useQuery({ queryKey: ["livetv", "search", q], queryFn: () => api<{ items: LiveProgram[] }>(`/api/v1/livetv/search?q=${encodeURIComponent(q)}`).then((r) => r.items) });
  if (!data) return null;
  if (data.length === 0) return <Empty>{t("search.none", { q })}</Empty>;
  return (
    <div className="pl-panel">
      {data.map((p) => (
        <button type="button" key={p.id} className="list-row tv-search-row" onClick={() => onProgram(p.id)}>
          {p.channel && <ChannelLogo c={p.channel} size={36} />}
          <div className="spacer" style={{ minWidth: 0, textAlign: "left" }}>
            <div className="t-label">{p.title}{p.subtitle ? ` · ${p.subtitle}` : ""}</div>
            <div className="t-small subtle">{[p.channel?.name, `${date(p.start_ms)} · ${time(p.start_ms)}`].filter(Boolean).join(" · ")}</div>
          </div>
        </button>
      ))}
    </div>
  );
}

export default function LiveTV() {
  const { t } = useI18n();
  const [params, setParams] = useSearchParams();
  const narrow = typeof window !== "undefined" && window.matchMedia("(max-width: 720px)").matches;
  const view = params.get("view") ?? (narrow ? "list" : "guide");
  const setView = (v: string) => setParams((p) => { p.set("view", v); return p; }, { replace: true });
  const [group, setGroup] = useState(params.get("group") ?? "");
  const [favorites, setFavorites] = useState(false);
  const [text, setText] = useState("");
  const [query, setQuery] = useState("");
  const [program, setProgram] = useState<string | null>(null);
  const { data: status } = useLiveStatus();
  useEffect(() => { const h = setTimeout(() => setQuery(text.trim()), 250); return () => clearTimeout(h); }, [text]);
  const { data: groups } = useQuery({ queryKey: ["livetv", "groups"], queryFn: () => api<{ items: { name: string; count: number }[] }>("/api/v1/livetv/groups").then((r) => r.items) });
  const qs = new URLSearchParams();
  if (group) qs.set("group", group);
  if (favorites) qs.set("favorites", "true");
  const { data: channels, error } = useQuery({
    queryKey: ["livetv", "channels", qs.toString()],
    queryFn: () => api<{ items: LiveChannel[] }>(`/api/v1/livetv/channels?${qs}`).then((r) => r.items),
    refetchInterval: 60_000,
  });
  if (error) return <Alert tone="danger">{(error as ApiError).message}</Alert>;
  return (
    <div>
      <div className="page-head">
        <div>
          <h1 className="t-h1">{t("tv.title")}</h1>
          {status && <div className="page-head__sub t-small subtle">{t("tv.count", { count: status.channels })}</div>}
        </div>
        <div className="row" style={{ alignItems: "flex-end" }}>
          {groups && groups.length > 1 && (
            <SelectField label={t("tv.group")} value={group} onChange={setGroup} options={[{ value: "", label: t("library.filterAll") }, ...groups.map((g) => ({ value: g.name, label: g.name || t("tv.noGroup") }))]} />
          )}
          <button type="button" className={`pl-btn pl-btn--secondary${favorites ? " is-on" : ""}`} aria-pressed={favorites} onClick={() => setFavorites(!favorites)}><Heart size={18} fill={favorites ? "currentColor" : "none"} />{t("tv.favorites")}</button>
        </div>
      </div>
      <div className="tabs" role="tablist" style={{ marginBottom: 16 }}>
        <button role="tab" aria-selected={view === "guide"} className={`tab${view === "guide" ? " active" : ""}`} onClick={() => setView("guide")}><LayoutGrid size={16} /> {t("tv.guide")}</button>
        <button role="tab" aria-selected={view === "list"} className={`tab${view === "list" ? " active" : ""}`} onClick={() => setView("list")}><List size={16} /> {t("tv.channels")}</button>
        {status?.can_record && <button role="tab" aria-selected={view === "rec"} className={`tab${view === "rec" ? " active" : ""}`} onClick={() => setView("rec")}><CircleDot size={16} /> {t("tv.recordings")}</button>}
      </div>
      {view !== "rec" && (
        <div className="library-search">
          <TextField label={t("tv.searchPrograms")} icon={<SearchIcon />} type="search" value={text} onChange={(e) => setText(e.target.value)} placeholder={t("tv.searchPlaceholder")} />
        </div>
      )}
      {query && view !== "rec" ? <ProgramSearch q={query} onProgram={setProgram} />
        : view === "rec" ? <Recordings />
        : !channels ? null
        : channels.length === 0 ? <Empty>{favorites ? t("tv.noFavorites") : t("tv.noChannels")}</Empty>
        : view === "guide" ? <Guide channels={channels} onProgram={setProgram} />
        : <ChannelList channels={channels} onProgram={setProgram} />}
      {program && <ProgramDialog programId={program} onClose={() => setProgram(null)} />}
    </div>
  );
}
