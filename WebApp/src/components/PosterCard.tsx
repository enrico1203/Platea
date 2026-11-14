import { Link } from "react-router-dom";
import { Check } from "lucide-react";
import { imageUrl } from "../api/client";
import type { Item } from "../api/types";
import { useI18n } from "../i18n";

/** Locandina 2:3 (o miniatura 16:9 per gli episodi). Il fondo è il colore segnaposto finché l'immagine non arriva. */
export function PosterCard({ item, wide = false, to, metaOverride }: { item: Item; wide?: boolean; to?: string; metaOverride?: string }) {
  const { t, duration } = useI18n();
  const src = wide ? item.images.still ?? item.images.backdrop ?? item.images.poster : item.images.poster;
  const progress = item.runtime_ms && item.user_data.position_ms > 0 && !item.user_data.played ? Math.min(1, item.user_data.position_ms / item.runtime_ms) : 0;
  let title = item.title;
  let meta: string;
  if (item.kind === "episode") {
    title = wide && item.series_title ? item.series_title : item.title;
    meta = `S${item.parent_index_number ?? 0} · E${item.index_number ?? 0}${wide ? ` · ${item.title}` : ""}`;
  } else if (item.kind === "series") {
    meta = [t("item.series"), item.child_count ? t("item.seasons", { count: item.child_count }) : null].filter(Boolean).join(" · ");
  } else if (item.kind === "collection") {
    meta = [t("item.collection"), item.child_count ? t("item.collectionCount", { count: item.child_count }) : null].filter(Boolean).join(" · ");
  } else if (item.kind === "season") {
    meta = item.child_count ? t("item.episodes", { count: item.child_count }) : "";
  } else {
    meta = [item.year, item.runtime_ms ? duration(item.runtime_ms) : null].filter(Boolean).join(" · ");
  }
  return (
    <Link to={to ?? `/item/${item.id}`} className="pl-poster">
      <div className={`pl-poster__art${wide ? " wide" : ""}`} style={{ background: item.images.poster_color ?? undefined }}>
        {src && <img src={imageUrl(src, wide ? 400 : 300)} alt="" loading="lazy" decoding="async" onLoad={(e) => (e.currentTarget.style.opacity = "1")} style={{ opacity: 0 }} />}
        {progress > 0 && (
          <div className="pl-poster__progress"><span style={{ width: `${progress * 100}%` }} /></div>
        )}
        {item.user_data.played && (
          <div className="pl-poster__watched" aria-label={t("item.watched")}><Check strokeWidth={3} /></div>
        )}
        {!item.available && <div className="unavailable">{t("item.unavailable")}</div>}
      </div>
      <div className="pl-poster__title">{title}</div>
      {(metaOverride ?? meta) && <div className="pl-poster__meta">{metaOverride ?? meta}</div>}
    </Link>
  );
}

/** Segnaposto mentre una riga o una griglia di locandine si carica. */
export function PosterSkeleton({ count = 8, wide = false }: { count?: number; wide?: boolean }) {
  return (
    <>
      {Array.from({ length: count }, (_, i) => (
        <div className="pl-poster" key={i} aria-hidden>
          <div className={`pl-poster__art skeleton${wide ? " wide" : ""}`} />
          <div className="skeleton skeleton--line" style={{ width: "72%" }} />
          <div className="skeleton skeleton--line" style={{ width: "40%" }} />
        </div>
      ))}
    </>
  );
}
