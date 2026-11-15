import { useQuery } from "@tanstack/react-query";
import { api } from "../api/client";
import type { JobProgress as Progress } from "../api/types";
import { useI18n, type MessageKey } from "../i18n";
import { Empty } from "./ui";

/** Avanzamento dei lavori: scansioni in corso e code per tipo e libreria, con percentuale e tempo stimato. */
export function JobProgress({ hideEmpty = false }: { hideEmpty?: boolean }) {
  const { t, num, duration } = useI18n();
  const { data } = useQuery({ queryKey: ["admin", "jobs", "progress"], queryFn: () => api<Progress>("/api/v1/admin/jobs/progress"), refetchInterval: 5000 });
  if (!data) return null;
  const empty = data.scans.length === 0 && data.queues.length === 0;
  if (empty && hideEmpty) return null;
  const eta = (ms: number | null) => {
    if (data.paused_until) return t("admin.progress.paused");
    if (ms == null) return t("admin.progress.etaUnknown");
    const h = Math.floor(ms / 3_600_000);
    const time = h >= 24 ? t("admin.progress.days", { d: Math.floor(h / 24), h: h % 24 }) : duration(ms);
    return t("admin.progress.eta", { time });
  };
  const pct = (p: number) => `${p < 10 ? num(p, 1) : num(Math.floor(p))}%`;
  return (
    <div className="pl-panel">
      <div className="panel-head"><span className="pl-eyebrow">{t("admin.progress.title")}</span></div>
      {empty ? <Empty>{t("admin.progress.none")}</Empty> : (
        <div className="job-progress">
          {data.scans.map((s) => (
            <Row key={`scan-${s.library_id}`} label={t("admin.jobs.kinds.scan_library")} library={s.library ?? ""} percent={s.saving ? 99 : s.percent}
              detail={s.saving ? t("admin.progress.saving") : s.expected_dirs ? t("admin.progress.dirs", { done: num(s.dirs), total: num(s.expected_dirs) }) : t("admin.progress.dirsFirst", { done: num(s.dirs) })}
              right={s.percent != null && !s.saving ? `${pct(s.percent)} · ${eta(s.eta_ms)}` : ""} />
          ))}
          {data.queues.map((q) => (
            <Row key={`${q.kind}-${q.library_id ?? ""}`} label={t(`admin.jobs.kinds.${q.kind}` as MessageKey)} library={q.library ?? t("admin.progress.noLibrary")} percent={q.percent}
              detail={`${t("admin.progress.items", { done: num(q.done), total: num(q.total) })}${q.per_minute > 0 ? ` · ${t("admin.progress.rate", { n: num(q.per_minute, q.per_minute < 10 ? 1 : 0) })}` : ""}`}
              right={`${pct(q.percent)} · ${eta(q.eta_ms)}`} />
          ))}
        </div>
      )}
    </div>
  );
}

function Row({ label, library, percent, detail, right }: { label: string; library: string; percent: number | null; detail: string; right: string }) {
  return (
    <div className="job-progress__row">
      <div className="job-progress__head">
        <span className="t-label">{label} <span className="muted">· {library}</span></span>
        <span className="t-small job-progress__eta">{right}</span>
      </div>
      <div className={`job-progress__bar${percent == null ? " is-indeterminate" : ""}`} role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent == null ? undefined : Math.round(percent)}>
        <span style={percent == null ? undefined : { width: `${Math.max(1, percent)}%` }} />
      </div>
      <div className="t-small subtle">{detail}</div>
    </div>
  );
}
