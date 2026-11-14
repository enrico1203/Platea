import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { Method } from "../../api/types";
import { MethodBadge } from "../../components/ui";
import { useI18n } from "../../i18n";

export default function Activity() {
  const { t, dateTime, duration, num } = useI18n();
  const [tab, setTab] = useState<"activity" | "history">("history");
  const { data: act } = useQuery({ queryKey: ["admin", "activity"], queryFn: () => api<{ items: { id: number; at: number; kind: string; message: string; ip: string | null; user: string | null }[] }>("/api/v1/admin/activity?limit=200").then((r) => r.items), enabled: tab === "activity" });
  const { data: hist } = useQuery({ queryKey: ["admin", "history"], queryFn: () => api<{ items: { id: number; user: string | null; title: string | null; client: string | null; device_name: string | null; method: Method; reasons: string[]; started_at: number; watched_ms: number }[]; last_30_days: Record<string, number> }>("/api/v1/admin/history?limit=200"), enabled: tab === "history" });
  return (
    <div className="stack-lg">
      <div className="page-head" style={{ marginBottom: 0 }}><h1 className="t-h1">{t("admin.activity.title")}</h1></div>
      <div className="tabs" role="tablist">
        <button role="tab" className={`tab${tab === "history" ? " active" : ""}`} onClick={() => setTab("history")}>{t("admin.activity.history")}</button>
        <button role="tab" className={`tab${tab === "activity" ? " active" : ""}`} onClick={() => setTab("activity")}>{t("admin.activity.log")}</button>
      </div>
      {tab === "history" && hist && (
        <>
          <div className="cards">
            {(["direct_play", "remux", "transcode"] as Method[]).map((m) => (
              <div className="stat" key={m}><MethodBadge method={m} /><span className="t-numeral">{num(hist.last_30_days[m] ?? 0)}</span><span className="t-small subtle">{t("admin.activity.last30")}</span></div>
            ))}
          </div>
          <div className="pl-panel table-wrap">
            <table className="table">
              <thead><tr><th>{t("admin.activity.when")}</th><th>{t("admin.activity.user")}</th><th>{t("item.title")}</th><th>{t("admin.activity.method")}</th><th>{t("admin.activity.watched")}</th></tr></thead>
              <tbody>{hist.items.map((h) => (
                <tr key={h.id}>
                  <td className="t-small muted" style={{ whiteSpace: "nowrap" }}>{dateTime(h.started_at)}</td>
                  <td>{h.user ?? "—"}<div className="t-small subtle">{h.device_name}</div></td>
                  <td>{h.title ?? "—"}</td>
                  <td><MethodBadge method={h.method} reason={h.reasons[0]} /></td>
                  <td className="t-mono">{duration(h.watched_ms)}</td>
                </tr>
              ))}</tbody>
            </table>
          </div>
        </>
      )}
      {tab === "activity" && (
        <div className="pl-panel">
          {act?.map((a) => (
            <div className="list-row" key={a.id}>
              <div className="spacer"><div>{a.message}</div><div className="t-small subtle">{dateTime(a.at)}{a.ip ? ` · ${a.ip}` : ""} · <span className="t-mono-sm">{a.kind}</span></div></div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
