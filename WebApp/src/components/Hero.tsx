import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Play, Info } from "lucide-react";
import { api, imageUrl } from "../api/client";
import type { Item, ItemDetail } from "../api/types";
import { useI18n } from "../i18n";

/**
 * "In scena": il titolo in cima alla home. Lo sfondo occupa tutta la larghezza e scende nel fondo della pagina;
 * l'interfaccia resta neutra e l'unico blu è il Play. Prima ciò che si stava guardando, altrimenti un titolo
 * aggiunto di recente con uno sfondo, scelto in modo stabile per tutta la giornata.
 */
export function pickFeatured(resume: Item[], recent: Item[]): { item: Item; resume: boolean } | null {
  const r = resume.find((i) => i.images.backdrop);
  if (r) return { item: r, resume: true };
  const pool = recent.filter((i) => i.images.backdrop && (i.kind === "movie" || i.kind === "series"));
  if (pool.length === 0) return null;
  const day = Math.floor(Date.now() / 86_400_000);
  return { item: pool[day % pool.length], resume: false };
}

export function Hero({ item, resume }: { item: Item; resume: boolean }) {
  const { t, duration, num, date } = useI18n();
  const nav = useNavigate();
  const [loaded, setLoaded] = useState(false);
  // Il dettaglio porta logo, generi e trama completa.
  const detailId = item.kind === "episode" && item.series_id ? item.series_id : item.id;
  const { data: d } = useQuery({ queryKey: ["item", detailId], queryFn: () => api<ItemDetail>(`/api/v1/items/${detailId}`) });
  const left = item.runtime_ms ? Math.max(0, item.runtime_ms - item.user_data.position_ms) : 0;
  const eyebrow = resume
    ? [t("hero.resume"), item.kind === "episode" ? `S${item.parent_index_number} · E${item.index_number}` : null, left ? t("hero.left", { time: duration(left) }) : null]
    : [t("hero.added", { date: date(item.added_at) })];
  const meta = [
    d?.year ?? item.year,
    item.kind === "movie" && item.runtime_ms ? duration(item.runtime_ms) : null,
    d?.official_rating,
    d?.community_rating ? `★ ${num(d.community_rating, 1)}` : null,
    ...(d?.genres ?? []).slice(0, 3),
  ].filter(Boolean);
  const title = item.kind === "episode" ? item.series_title ?? item.title : item.title;
  const progress = resume && item.runtime_ms ? Math.min(1, item.user_data.position_ms / item.runtime_ms) : 0;
  return (
    <section className="hero" aria-label={title}>
      <div className={`hero__backdrop${loaded ? " is-loaded" : ""}`}>
        <img src={imageUrl(item.images.backdrop, 1600)} alt="" onLoad={() => setLoaded(true)} />
      </div>
      <div className="hero__body">
        <div className="hero__eyebrow t-label">{eyebrow.filter(Boolean).join(" · ")}</div>
        {d?.images.logo ? <img className="hero__logo" src={imageUrl(d.images.logo, 500)} alt={title} /> : <h1 className="hero__title">{title}</h1>}
        {meta.length > 0 && <div className="hero__meta t-small">{meta.map((m, i) => <span key={i}>{m}</span>)}</div>}
        {(item.kind === "episode" ? item.overview : d?.overview ?? item.overview) && (
          <p className="hero__overview">{item.kind === "episode" ? item.overview : d?.overview ?? item.overview}</p>
        )}
        <div className="hero__actions">
          <button className="pl-btn pl-btn--primary hero__play" onClick={() => nav(`/play/${item.id}`)}>
            <Play fill="currentColor" />{resume ? t("hero.resumeAction") : t("player.play")}
          </button>
          <Link className="pl-btn pl-btn--secondary" to={`/item/${detailId}`}><Info />{t("hero.details")}</Link>
        </div>
        {progress > 0 && <div className="hero__progress" aria-hidden><span style={{ width: `${progress * 100}%` }} /></div>}
      </div>
    </section>
  );
}
