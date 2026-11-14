import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { Pause, Play, RotateCcw } from "lucide-react";
import { api } from "../../api/client";
import type { Job } from "../../api/types";
import { Alert, Badge, Dialog, Empty, TextField } from "../../components/ui";
import { useI18n, type MessageKey } from "../../i18n";

const STATES = ["queued", "running", "failed", "done", "all"] as const;

export default function Jobs() {
  const { t, relative, num } = useI18n();
  const qc = useQueryClient();
  const [st, setSt] = useState<(typeof STATES)[number]>("queued");
  const { data } = useQuery({ queryKey: ["admin", "jobs", st], queryFn: () => api<{ items: Job[]; counts: Record<string, number>; paused_until: number | null }>(`/api/v1/admin/jobs?state=${st}&limit=200`) });
  const [pauseOpen, setPauseOpen] = useState(false);
  const paused = data?.paused_until ?? null;
  const resume = async () => { await api("/api/v1/admin/jobs/resume", { method: "POST" }); qc.invalidateQueries({ queryKey: ["admin", "jobs"] }); };
  return (
    <div className="stack-lg">
      <div className="page-head" style={{ marginBottom: 0 }}>
        <h1 className="t-h1">{t("admin.jobs.title")}</h1>
        <div className="row">
        {paused ? (
          <button className="pl-btn pl-btn--primary" onClick={resume}><Play />{t("admin.jobs.resume")}</button>
        ) : (
          <button className="pl-btn pl-btn--secondary" onClick={() => setPauseOpen(true)}><Pause />{t("admin.jobs.pause")}</button>
        )}
        {st === "failed" && (data?.items.length ?? 0) > 0 && <button className="pl-btn pl-btn--ghost" onClick={async () => { await api("/api/v1/admin/jobs/failed", { method: "DELETE" }); qc.invalidateQueries({ queryKey: ["admin", "jobs"] }); }}>{t("admin.jobs.clearFailed")}</button>}
        </div>
      </div>
      {paused && <PausedBanner until={paused} onResume={resume} />}
      {pauseOpen && <PauseDialog onClose={() => setPauseOpen(false)} onDone={() => qc.invalidateQueries({ queryKey: ["admin", "jobs"] })} />}
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
                  <td className="job-subject">
                    {j.subject ? (
                      <>
                        <div className="job-subject__title">{j.subject.item_id ? <Link to={`/item/${j.subject.item_id}`}>{j.subject.title}</Link> : j.subject.title}</div>
                        {j.subject.detail && <div className="t-mono-sm subtle job-subject__detail" title={j.subject.detail}>{j.subject.detail}</div>}
                      </>
                    ) : (
                      <span className="t-mono-sm subtle">{Object.entries(j.payload).filter(([k]) => k !== "kind").map(([k, v]) => `${k}=${v ?? ""}`).join(" ")}</span>
                    )}
                    {j.error && <div className="danger-text" style={{ marginTop: 4 }}>{j.error}</div>}
                  </td>
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

/** Avviso di coda in pausa: fino a quando, e "Riprendi". La scadenza la gestisce il server. */
function PausedBanner({ until, onResume }: { until: number; onResume: () => void }) {
  const { t, tag, relative } = useI18n();
  const forever = until > Date.now() + 365 * 86_400_000;
  const at = new Intl.DateTimeFormat(tag, { hour: "2-digit", minute: "2-digit" }).format(new Date(until));
  return (
    <Alert tone="warning">
      {forever ? t("admin.jobs.pausedForever") : t("admin.jobs.pausedUntil", { time: at, rel: relative(until) })}{" "}
      <button className="pl-btn pl-btn--ghost" onClick={onResume}>{t("admin.jobs.resume")}</button>
    </Alert>
  );
}

const PAUSE_CHOICES = [15, 30, 60, 120, 240, 480];

function PauseDialog({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const { t, duration } = useI18n();
  const [custom, setCustom] = useState("");
  const go = async (minutes: number | null) => {
    await api("/api/v1/admin/jobs/pause", { body: { minutes } });
    onDone();
    onClose();
  };
  const n = Number(custom);
  return (
    <Dialog title={t("admin.jobs.pauseTitle")} onClose={onClose}>
      <p className="t-small muted">{t("admin.jobs.pauseHint")}</p>
      <div className="row" style={{ flexWrap: "wrap" }}>
        {PAUSE_CHOICES.map((m) => <button key={m} className="pl-btn pl-btn--secondary" onClick={() => go(m)}>{duration(m * 60_000)}</button>)}
      </div>
      <div className="row" style={{ alignItems: "flex-end" }}>
        <TextField label={t("admin.jobs.pauseMinutes")} inputMode="numeric" value={custom} onChange={(e) => setCustom(e.target.value.replace(/[^0-9]/g, ""))} />
        <button className="pl-btn pl-btn--secondary" disabled={!(n > 0)} onClick={() => go(n)}>{t("admin.jobs.pause")}</button>
      </div>
      <button className="pl-btn pl-btn--ghost" onClick={() => go(null)}>{t("admin.jobs.pauseForever")}</button>
    </Dialog>
  );
}

