import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ChevronDown, ChevronUp, Square } from "lucide-react";
import { api, imageUrl } from "../../api/client";
import type { Overview as OverviewT, SessionView } from "../../api/types";
import { Alert, Empty, MethodBadge } from "../../components/ui";
import { useI18n, type MessageKey } from "../../i18n";

/** Orologio che avanza ogni secondo: posizioni e durate restano vive tra un aggiornamento e l'altro. */
function useNow(): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, []);
  return now;
}

const STALE_MS = 45_000;

function channels(n: number) {
  return n === 6 ? "5.1" : n === 8 ? "7.1" : n === 2 ? "stereo" : n === 1 ? "mono" : `${n} ch`;
}

function resolution(w: number, h: number) {
  if (w >= 3800 || h >= 2100) return "4K";
  if (w >= 1900 || h >= 1060) return "1080p";
  if (w >= 1260 || h >= 700) return "720p";
  return `${h}p`;
}

export function SessionRow({ s, fetchedAt, onStop }: { s: SessionView; fetchedAt: number; onStop?: () => void }) {
  const { t, tag, bitrate, bytes, clock, duration } = useI18n();
  const [open, setOpen] = useState(false);
  const now = useNow();
  // Ora del server stimata: l'orologio del browser può essere sfasato.
  const serverNow = s.server_now + (now - fetchedAt);
  const stale = serverNow - s.last_seen > STALE_MS;
  const playing = !s.paused && !stale;
  const pos = Math.min(s.duration_ms || Infinity, s.position_ms + (playing ? Math.max(0, serverNow - s.position_at) : 0));
  const left = s.duration_ms ? Math.max(0, s.duration_ms - pos) : 0;
  const pct = (ms: number) => (s.duration_ms ? Math.min(100, (ms / s.duration_ms) * 100) : 0);
  const hhmm = (ms: number) => new Intl.DateTimeFormat(tag, { hour: "2-digit", minute: "2-digit" }).format(new Date(ms - (s.server_now - fetchedAt)));
  const d = s.details;
  const status = stale
    ? t("admin.overview.session.stale", { time: duration(serverNow - s.last_seen) })
    : s.paused
      ? s.paused_since ? t("admin.overview.session.pausedFor", { time: duration(serverNow - s.paused_since) }) : t("admin.overview.paused")
      : t("admin.overview.session.playing");
  const lang = (l: string | null | undefined) => (l ? l.toUpperCase() : t("admin.overview.session.undefinedLang"));
  const video = d.video ? [d.video.codec.toUpperCase(), resolution(d.video.width, d.video.height), d.video.hdr?.toUpperCase(), d.video.bit_depth && d.video.bit_depth > 8 ? `${d.video.bit_depth} bit` : null].filter(Boolean).join(" · ") : "—";
  const outVideo = d.out.video.action === "copy"
    ? t("admin.overview.session.copy")
    : [d.out.video.codec.toUpperCase(), `≤ ${d.out.video.max_height}p`, bitrate(d.out.video.bitrate), d.out.video.tonemap ? t("admin.overview.session.tonemap") : null, d.out.video.burn_subtitle ? t("admin.overview.session.burnIn") : null].filter(Boolean).join(" · ");
  const audio = d.audio ? `${lang(d.audio.language)} · ${d.audio.codec.toUpperCase()} ${channels(d.audio.channels)}` : "—";
  const outAudio = d.out.audio.action === "copy" ? t("admin.overview.session.copy") : d.out.audio.action === "none" ? "—" : `${d.out.audio.codec.toUpperCase()} ${channels(d.out.audio.channels)} · ${bitrate(d.out.audio.bitrate)}`;
  const sub = d.subtitle ? `${lang(d.subtitle.language)} · ${d.subtitle.codec}${d.subtitle.forced ? ` · ${t("admin.overview.session.forced")}` : ""}${d.subtitle.external ? ` · ${t("admin.overview.session.externalFile")}` : ""}` : t("admin.overview.session.none");
  const rows: [string, string, string][] = [
    [t("admin.overview.session.video"), video, outVideo],
    [t("admin.overview.session.audio"), `${audio}${d.audio_tracks > 1 ? ` (${t("admin.overview.session.ofTracks", { n: d.audio_tracks })})` : ""}`, outAudio],
    [t("admin.overview.session.subtitles"), sub, t(`admin.overview.session.sub.${d.out.subtitle}` as MessageKey)],
  ];
  return (
    <div className={`pl-session${open ? " pl-session--open" : ""}`}>
      <div className="pl-session__main">
        <div className="pl-session__thumb" style={s.poster_url ? { backgroundImage: `url(${imageUrl(s.poster_url)})`, backgroundSize: "cover" } : undefined} />
        <div style={{ minWidth: 0 }}>
          <div className="pl-session__title"><Link to={`/item/${s.item_id}`}>{s.title}</Link>{s.subtitle ? ` · ${s.subtitle}` : ""}</div>
          <div className="pl-session__who">{s.user} · {s.device_name} · {s.client}</div>
          <div className="pl-session__state">
            <span className={`pl-session__dot${playing ? " is-playing" : stale ? " is-stale" : ""}`} aria-hidden />
            <span>{status}</span>
            <span className="pl-session__time">{clock(pos)} / {clock(s.duration_ms)}</span>
            {s.duration_ms > 0 && <span className="subtle">{t("admin.overview.session.left", { time: clock(left) })}{playing ? ` · ${t("admin.overview.session.endsAt", { time: hhmm(serverNow + left) })}` : ""}</span>}
          </div>
          <div className="pl-session__bar" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(pct(pos))}>
            {s.buffered_until_ms != null && <span className="pl-session__buffer" style={{ width: `${pct(s.buffered_until_ms)}%` }} />}
            <span className="pl-session__pos" style={{ width: `${pct(pos)}%` }} />
          </div>
          <div className="pl-session__meta">
            <span>{t("admin.overview.session.started", { time: hhmm(s.started_at), ago: duration(serverNow - s.started_at) })}</span>
            {s.ip && <span>{s.ip} · {s.remote ? t("admin.overview.session.remote") : t("admin.overview.session.local")}</span>}
            {s.backend && <span>{t("admin.overview.session.encoder")}: {s.backend === "copia" ? t("admin.overview.session.copy") : s.backend}{s.throttled ? ` · ${t("admin.overview.throttled")}` : ""}</span>}
          </div>
        </div>
        <div className="pl-session__right">
          <MethodBadge method={s.method} reason={s.reasons[0]} />
          <span className="pl-session__rate" title={t("admin.overview.session.bandwidthHint")}>{s.bandwidth_bps > 0 ? bitrate(s.bandwidth_bps) : bitrate(s.bitrate)}</span>
          <div style={{ display: "flex", gap: 4 }}>
            <button className="icon-btn" onClick={() => setOpen((o) => !o)} aria-expanded={open} aria-label={t("admin.overview.session.details")} title={t("admin.overview.session.details")}>{open ? <ChevronUp size={16} /> : <ChevronDown size={16} />}</button>
            {onStop && <button className="icon-btn" onClick={onStop} aria-label={t("admin.overview.stop")} title={t("admin.overview.stop")}><Square size={16} /></button>}
          </div>
        </div>
      </div>
      {open && (
        <div className="pl-session__details">
          <table className="pl-session__table">
            <thead><tr><th /><th>{t("admin.overview.session.source")}</th><th>{t("admin.overview.session.output")}</th></tr></thead>
            <tbody>
              {rows.map(([k, a, b]) => <tr key={k}><th scope="row">{k}</th><td>{a}</td><td>{b}</td></tr>)}
            </tbody>
          </table>
          <dl className="pl-session__facts">
            <div><dt>{t("admin.overview.session.file")}</dt><dd className="t-mono-sm" title={d.file.name ?? ""}>{d.file.name ?? "—"}</dd></div>
            <div><dt>{t("admin.overview.session.size")}</dt><dd>{bytes(d.file.size)} · {d.file.container.toUpperCase()} · {bitrate(d.file.bitrate)}</dd></div>
            <div><dt>{t("admin.overview.session.delivery")}</dt><dd>{t(`admin.overview.session.container.${d.out.container}` as MessageKey)}</dd></div>
            <div><dt>{t("admin.overview.session.sent")}</dt><dd>{bytes(s.bytes_sent)}{s.bandwidth_bps > 0 ? ` · ${bitrate(s.bandwidth_bps)}` : ""}</dd></div>
            <div><dt>{t("admin.overview.session.chain")}</dt><dd className="t-mono-sm">{s.chain}</dd></div>
          </dl>
        </div>
      )}
    </div>
  );
}

export default function Overview() {
  const { t, num, bytes, bitrate, duration } = useI18n();
  const qc = useQueryClient();
  const { data, dataUpdatedAt } = useQuery({
    queryKey: ["admin", "overview"],
    queryFn: () => api<OverviewT>("/api/v1/admin/overview"),
    // Le posizioni cambiano solo durante le riproduzioni: si aggiorna solo se ce ne sono.
    refetchInterval: (q) => ((q.state.data?.sessions.length ?? 0) > 0 ? 5_000 : 30_000),
  });
  if (!data) return null;
  const s = data.system;
  return (
    <div className="stack-lg">
      <div className="page-head" style={{ marginBottom: 0 }}><h1 className="t-h1">{t("admin.overview.title")}</h1></div>
      {data.alerts.map((a) => {
        // Messaggi senza parametri tradotti per codice; gli altri arrivano già composti dal server.
        const key = `admin.overview.alerts.${a.code}` as MessageKey;
        const text = t(key);
        return <Alert key={a.code + a.message} tone={a.level}>{text === key ? a.message : text} <span className="t-mono-sm">{a.code}</span></Alert>;
      })}
      <div className="cards">
        <div className="stat"><span className="pl-eyebrow">{t("admin.overview.streams")}</span><span className="t-numeral">{num(data.sessions.length)}</span><span className="t-small subtle">{t("admin.overview.split", { d: data.counts.direct_play, r: data.counts.remux, t: data.counts.transcode })}</span></div>
        <div className="stat"><span className="pl-eyebrow">{t("admin.overview.bandwidth")}</span><span className="t-numeral">{bitrate(data.bandwidth_bps) === "—" ? "0" : bitrate(data.bandwidth_bps)}</span></div>
        <div className="stat"><span className="pl-eyebrow">{t("admin.overview.transcodes")}</span><span className="t-numeral">{data.transcode_slots.hw_used + data.transcode_slots.sw_used}</span><span className="t-small subtle">GPU {data.transcode_slots.hw_used}/{data.transcode_slots.hw_max || "∞"} · CPU {data.transcode_slots.sw_used}/{data.transcode_slots.sw_max || "∞"}</span></div>
        <div className="stat"><span className="pl-eyebrow">{t("admin.overview.jobs")}</span><span className="t-numeral">{num(data.jobs.queued + data.jobs.running)}</span><span className="t-small subtle">{t("admin.overview.jobsSplit", { running: data.jobs.running, failed: data.jobs.failed })}</span></div>
        <div className="stat"><span className="pl-eyebrow">{t("admin.overview.memory")}</span><span className="t-numeral">{bytes(s.memory_rss_bytes)}</span><span className="t-small subtle">{t("admin.overview.load", { load: s.load_1m != null ? num(s.load_1m, 2) : "—", cpus: s.cpus })} · {t("admin.overview.uptime", { time: duration(s.uptime_ms) })}</span></div>
        <div className="stat"><span className="pl-eyebrow">{t("admin.overview.library")}</span><span className="t-numeral">{num(data.library.files)}</span><span className="t-small subtle">{t("admin.overview.librarySplit", { m: data.library.movies, s: data.library.series, e: data.library.episodes })}</span>{data.library.review > 0 && <Link to="/admin/review" className="t-small">{t("admin.overview.toReview", { count: data.library.review })}</Link>}</div>
      </div>
      <section>
        <div className="pl-panel">
          <div className="panel-head"><span className="pl-eyebrow">{t("admin.overview.active")}</span></div>
          {data.sessions.length === 0 ? <Empty>{t("admin.overview.noStreams")}</Empty> : data.sessions.map((x) => (
            <SessionRow key={x.id} s={x} fetchedAt={dataUpdatedAt} onStop={async () => { await api(`/api/v1/admin/sessions/${x.id}`, { method: "DELETE" }); qc.invalidateQueries({ queryKey: ["admin", "overview"] }); }} />
          ))}
        </div>
      </section>
    </div>
  );
}
