import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { RotateCcw } from "lucide-react";
import { api } from "../../api/client";
import type { Job } from "../../api/types";
import { Badge, Empty } from "../../components/ui";
import { useI18n, type MessageKey } from "../../i18n";

const STATES = ["queued", "running", "failed", "done", "all"] as const;

export default function Jobs() {
  const { t, relative, num } = useI18n();
  const qc = useQueryClient();
  const [st, setSt] = useState<(typeof STATES)[number]>("queued");
  const { data } = useQuery({ queryKey: ["admin", "jobs", st], queryFn: () => api<{ items: Job[]; counts: Record<string, number> }>(`/api/v1/admin/jobs?state=${st}&limit=200`) });
  return (
    <div className="stack-lg">
      <div className="page-head" style={{ marginBottom: 0 }}>
        <h1 className="t-h1">{t("admin.jobs.title")}</h1>
        {st === "failed" && (data?.items.length ?? 0) > 0 && <button className="pl-btn pl-btn--ghost" onClick={async () => { await api("/api/v1/admin/jobs/failed", { method: "DELETE" }); qc.invalidateQueries({ queryKey: ["admin", "jobs"] }); }}>{t("admin.jobs.clearFailed")}</button>}
      </div>
      <div className="tabs" role="tablist">
        {STATES.map((s) => (
          <button key={s} role="tab" className={`tab${st === s ? " active" : ""}`} onClick={() => setSt(s)}>
            {t(`admin.jobs.state.${s}` as MessageKey)}{s !== "all" && data?.counts[s] != null ? ` · ${num(data.counts[s])}` : ""}
          </button>
        ))}
      </div>
      {data && data.items.length === 0 ? <Empty>{t("admin.jobs.empty")}</Empty> : (
        <div className="pl-panel table-wrap">
          <table className="table">
            <thead><tr><th>{t("admin.jobs.kind")}</th><th>{t("admin.jobs.detail")}</th><th>{t("admin.jobs.status")}</th><th>{t("admin.jobs.when")}</th><th /></tr></thead>
            <tbody>
              {data?.items.map((j) => (
                <tr key={j.id}>
                  <td className="t-label" style={{ whiteSpace: "nowrap" }}>{t(`admin.jobs.kinds.${j.kind}` as MessageKey)}</td>
                  <td className="t-mono-sm subtle">{Object.entries(j.payload).filter(([k]) => k !== "kind").map(([k, v]) => `${k}=${v ?? ""}`).join(" ")}{j.error && <div className="danger-text" style={{ marginTop: 4 }}>{j.error}</div>}</td>
                  <td><Badge tone={j.state === "failed" ? "danger" : j.state === "done" ? "direct" : undefined}>{t(`admin.jobs.state.${j.state}` as MessageKey)}</Badge>{j.attempts > 1 && <span className="t-small subtle"> ×{j.attempts}</span>}</td>
                  <td className="t-small muted" style={{ whiteSpace: "nowrap" }}>{relative(j.finished_at ?? j.started_at ?? j.run_after)}</td>
                  <td>{j.state === "failed" && <button className="icon-btn" aria-label={t("admin.jobs.retry")} title={t("admin.jobs.retry")} onClick={async () => { await api(`/api/v1/admin/jobs/${j.id}/retry`, { method: "POST" }); qc.invalidateQueries({ queryKey: ["admin", "jobs"] }); }}><RotateCcw /></button>}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
