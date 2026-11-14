import { Link } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Square } from "lucide-react";
import { api, imageUrl } from "../../api/client";
import type { Overview as OverviewT, SessionView } from "../../api/types";
import { Alert, Empty, MethodBadge } from "../../components/ui";
import { useI18n } from "../../i18n";

export function SessionRow({ s, onStop }: { s: SessionView; onStop?: () => void }) {
  const { t, bitrate } = useI18n();
  const progress = s.duration_ms ? Math.min(100, (s.position_ms / s.duration_ms) * 100) : 0;
  return (
    <div className="pl-session">
      <div className="pl-session__thumb" style={s.poster_url ? { backgroundImage: `url(${imageUrl(s.poster_url)})`, backgroundSize: "cover" } : undefined} />
      <div style={{ minWidth: 0 }}>
        <div className="pl-session__title">{s.title}{s.subtitle ? ` · ${s.subtitle}` : ""}</div>
        <div className="pl-session__who">{s.user} · {s.device_name} · {s.client}{s.paused ? ` · ${t("admin.overview.paused")}` : ""}{s.throttled ? ` · ${t("admin.overview.throttled")}` : ""}</div>
        <div className="pl-session__tech">{s.chain}</div>
        <div className="pl-session__bar"><span style={{ width: `${progress}%` }} /></div>
      </div>
      <div className="pl-session__right">
        <MethodBadge method={s.method} reason={s.reasons[0]} />
        <span className="pl-session__rate">{bitrate(s.bitrate)}</span>
        {onStop && <button className="icon-btn" onClick={onStop} aria-label={t("admin.overview.stop")} title={t("admin.overview.stop")}><Square size={16} /></button>}
      </div>
    </div>
  );
}

export default function Overview() {
  const { t, num, bytes, bitrate, duration } = useI18n();
  const qc = useQueryClient();
  const { data } = useQuery({
    queryKey: ["admin", "overview"],
    queryFn: () => api<OverviewT>("/api/v1/admin/overview"),
    // Le posizioni cambiano solo durante le riproduzioni: si aggiorna solo se ce ne sono.
    refetchInterval: (q) => ((q.state.data?.sessions.length ?? 0) > 0 ? 10_000 : false),
  });
  if (!data) return null;
  const s = data.system;
  return (
    <div className="stack-lg">
      <div className="page-head" style={{ marginBottom: 0 }}><h1 className="t-h1">{t("admin.overview.title")}</h1></div>
      {data.alerts.map((a) => <Alert key={a.code + a.message} tone={a.level}>{a.message} <span className="t-mono-sm">{a.code}</span></Alert>)}
      <div className="cards">
        <div className="stat"><span className="pl-eyebrow">{t("admin.overview.streams")}</span><span className="t-numeral">{num(data.sessions.length)}</span><span className="t-small subtle">{t("admin.overview.split", { d: data.counts.direct_play, r: data.counts.remux, t: data.counts.transcode })}</span></div>
        <div className="stat"><span className="pl-eyebrow">{t("admin.overview.bandwidth")}</span><span className="t-numeral">{bitrate(data.bandwidth_bps) === "—" ? "0" : bitrate(data.bandwidth_bps)}</span></div>
        <div className="stat"><span className="pl-eyebrow">{t("admin.overview.transcodes")}</span><span className="t-numeral">{data.transcode_slots.hw_used + data.transcode_slots.sw_used}</span><span className="t-small subtle">GPU {data.transcode_slots.hw_used}/{data.transcode_slots.hw_max} · CPU {data.transcode_slots.sw_used}/{data.transcode_slots.sw_max}</span></div>
        <div className="stat"><span className="pl-eyebrow">{t("admin.overview.jobs")}</span><span className="t-numeral">{num(data.jobs.queued + data.jobs.running)}</span><span className="t-small subtle">{t("admin.overview.jobsSplit", { running: data.jobs.running, failed: data.jobs.failed })}</span></div>
        <div className="stat"><span className="pl-eyebrow">{t("admin.overview.memory")}</span><span className="t-numeral">{bytes(s.memory_rss_bytes)}</span><span className="t-small subtle">{t("admin.overview.load", { load: s.load_1m != null ? num(s.load_1m, 2) : "—", cpus: s.cpus })} · {t("admin.overview.uptime", { time: duration(s.uptime_ms) })}</span></div>
        <div className="stat"><span className="pl-eyebrow">{t("admin.overview.library")}</span><span className="t-numeral">{num(data.library.files)}</span><span className="t-small subtle">{t("admin.overview.librarySplit", { m: data.library.movies, s: data.library.series, e: data.library.episodes })}</span>{data.library.review > 0 && <Link to="/admin/review" className="t-small">{t("admin.overview.toReview", { count: data.library.review })}</Link>}</div>
      </div>
      <section>
        <div className="pl-panel">
          <div className="panel-head"><span className="pl-eyebrow">{t("admin.overview.active")}</span></div>
          {data.sessions.length === 0 ? <Empty>{t("admin.overview.noStreams")}</Empty> : data.sessions.map((x) => (
            <SessionRow key={x.id} s={x} onStop={async () => { await api(`/api/v1/admin/sessions/${x.id}`, { method: "DELETE" }); qc.invalidateQueries({ queryKey: ["admin", "overview"] }); }} />
          ))}
        </div>
      </section>
    </div>
  );
}
