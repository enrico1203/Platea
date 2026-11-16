import { useEffect, useState } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight, Search } from "lucide-react";
import { api } from "../../api/client";
import type { Method, Page, UserAdmin } from "../../api/types";
import { Empty, MethodBadge, SelectField, TextField } from "../../components/ui";
import { useI18n } from "../../i18n";

const PAGE = 50;

interface ActivityRow { id: number; at: number; kind: string; message: string; ip: string | null; user: string | null }
interface HistoryRow { id: number; user: string | null; title: string | null; client: string | null; device_name: string | null; method: Method; reasons: string[]; started_at: number; watched_ms: number }
interface Paged<T> { items: T[]; total: number; page: number; limit: number }

/** Testo che si stabilizza dopo una breve pausa: niente richiesta a ogni tasto. */
function useDebounced<T>(v: T, ms = 300): T {
  const [d, setD] = useState(v);
  useEffect(() => {
    const id = window.setTimeout(() => setD(v), ms);
    return () => window.clearTimeout(id);
  }, [v, ms]);
  return d;
}

function Pager({ page, total, onPage }: { page: number; total: number; onPage: (p: number) => void }) {
  const { t, num } = useI18n();
  const pages = Math.max(1, Math.ceil(total / PAGE));
  if (total <= PAGE) return null;
  return (
    <nav className="pager" aria-label={t("admin.activity.pages")}>
      <button className="pl-btn pl-btn--ghost" disabled={page <= 1} onClick={() => onPage(page - 1)}><ChevronLeft />{t("admin.activity.prev")}</button>
      <span className="t-small muted">{t("admin.activity.pageOf", { page: num(page), pages: num(pages), total: num(total) })}</span>
      <button className="pl-btn pl-btn--ghost" disabled={page >= pages} onClick={() => onPage(page + 1)}>{t("admin.activity.next")}<ChevronRight /></button>
    </nav>
  );
}

export default function Activity() {
  const { t, dateTime, duration, num } = useI18n();
  const [tab, setTab] = useState<"activity" | "history">("history");
  const [text, setText] = useState("");
  const [user, setUser] = useState("");
  const [page, setPage] = useState(1);
  const q = useDebounced(text.trim());
  // Filtri o scheda nuovi: si riparte dalla prima pagina.
  useEffect(() => setPage(1), [q, user, tab]);
  const params = new URLSearchParams({ limit: String(PAGE), page: String(page) });
  if (q) params.set("q", q);
  if (user) params.set("user_id", user);

  const { data: users } = useQuery({ queryKey: ["admin", "users"], queryFn: () => api<Page<UserAdmin>>("/api/v1/users").then((r) => r.items) });
  const { data: act, isFetching: fa } = useQuery({
    queryKey: ["admin", "activity", page, q, user],
    queryFn: () => api<Paged<ActivityRow>>(`/api/v1/admin/activity?${params}`),
    enabled: tab === "activity",
    placeholderData: keepPreviousData,
  });
  const { data: hist, isFetching: fh } = useQuery({
    queryKey: ["admin", "history", page, q, user],
    queryFn: () => api<Paged<HistoryRow> & { last_30_days: Record<string, number> }>(`/api/v1/admin/history?${params}`),
    enabled: tab === "history",
    placeholderData: keepPreviousData,
  });
  const onPage = (p: number) => { setPage(p); window.scrollTo({ top: 0, behavior: "smooth" }); };
  const busy = tab === "history" ? fh : fa;
  const filters = (
    <div className="activity-filters" aria-busy={busy}>
      <TextField label={t("admin.activity.search")} type="search" value={text} onChange={(e) => setText(e.target.value)}
        placeholder={tab === "history" ? t("admin.activity.searchHistory") : t("admin.activity.searchLog")} icon={<Search size={16} aria-hidden />} />
      <SelectField label={t("admin.activity.user")} value={user} onChange={setUser}
        options={[{ value: "", label: t("admin.activity.allUsers") }, ...(users ?? []).map((u) => ({ value: u.id, label: u.username }))]} />
    </div>
  );

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
          {filters}
          {hist.items.length === 0 ? <Empty>{t("admin.activity.none")}</Empty> : (
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
          )}
          <Pager page={page} total={hist.total} onPage={onPage} />
        </>
      )}
      {tab === "activity" && (
        <>
          {filters}
          {act && (act.items.length === 0 ? <Empty>{t("admin.activity.none")}</Empty> : (
            <div className="pl-panel">
              {act.items.map((a) => (
                <div className="list-row" key={a.id}>
                  <div className="spacer"><div>{a.message}</div><div className="t-small subtle">{dateTime(a.at)}{a.ip ? ` · ${a.ip}` : ""} · <span className="t-mono-sm">{a.kind}</span></div></div>
                </div>
              ))}
            </div>
          ))}
          {act && <Pager page={page} total={act.total} onPage={onPage} />}
        </>
      )}
    </div>
  );
}
