import { Link } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { X } from "lucide-react";
import { api } from "../../api/client";
import type { ReviewItem } from "../../api/types";
import { Badge, Empty, toast } from "../../components/ui";
import { useI18n } from "../../i18n";

export default function Review() {
  const { t, num } = useI18n();
  const qc = useQueryClient();
  const { data } = useQuery({ queryKey: ["admin", "review"], queryFn: () => api<{ items: ReviewItem[] }>("/api/v1/admin/review").then((r) => r.items) });
  return (
    <div className="stack-lg">
      <div className="page-head" style={{ marginBottom: 0 }}>
        <div><h1 className="t-h1">{t("admin.review.title")}</h1><div className="page-head__sub t-small subtle">{t("admin.review.help")}</div></div>
        <button className="pl-btn pl-btn--secondary" onClick={async () => { const r = await api<{ queued: number }>("/api/v1/admin/metadata/match-pending", { method: "POST" }); toast(t("admin.review.queued", { count: r.queued })); }}>{t("admin.review.matchPending")}</button>
      </div>
      {data?.length === 0 && <Empty>{t("admin.review.empty")}</Empty>}
      <div className="stack">
        {data?.map((it) => (
          <div className="pl-panel panel-pad stack" key={it.id}>
            <div className="row row--between">
              <div>
                <Link to={`/item/${it.id}`} className="t-h3">{it.title}{it.year ? ` (${it.year})` : ""}</Link>
                <div className="t-small subtle">{it.library} · {it.kind === "movie" ? t("item.movie") : t("item.series")} · <Badge>{it.match_state === "review" ? t("admin.review.doubtful") : t("admin.review.notFound")}</Badge></div>
                {it.path && <div className="t-mono-sm subtle" style={{ marginTop: 4 }}>{it.path}</div>}
              </div>
              <Link className="pl-btn pl-btn--secondary pl-btn--sm" to={`/item/${it.id}`}>{t("item.identify")}</Link>
            </div>
            {it.candidates.map((c) => (
              <div className="cand" key={c.provider_id}>
                {c.poster ? <img src={c.poster} alt="" /> : <div className="ph" />}
                <div><div className="t-label">{c.title} {c.year && <span className="subtle">({c.year})</span>}</div><div className="t-mono-sm subtle">{t("item.score")} {num(c.score, 2)} · TMDB {c.provider_id}</div></div>
                <div className="row">
                  <button className="pl-btn pl-btn--secondary pl-btn--sm" onClick={async () => { await api(`/api/v1/items/${it.id}/identify`, { body: { provider_id: c.provider_id } }); qc.invalidateQueries(); toast(t("item.identified")); }}>{t("item.choose")}</button>
                  <button className="icon-btn" aria-label={t("admin.review.reject")} title={t("admin.review.reject")} onClick={async () => { await api(`/api/v1/items/${it.id}/reject`, { body: { provider_id: c.provider_id } }); qc.invalidateQueries({ queryKey: ["admin", "review"] }); }}><X /></button>
                </div>
              </div>
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}
