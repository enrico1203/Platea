/** Home › "In onda ora": i canali preferiti con il programma in onda. Niente se la Live TV non c'è o non ci sono preferiti. */
import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { api } from "../api/client";
import type { LiveChannel } from "../api/types";
import { useI18n } from "../i18n";
import { ChannelLogo, chNumber, useLiveStatus } from "./LiveTV";

export function OnAir() {
  const { t } = useI18n();
  const { data: status } = useLiveStatus();
  const { data } = useQuery({
    queryKey: ["livetv", "channels", "favorites=true"],
    queryFn: () => api<{ items: LiveChannel[] }>("/api/v1/livetv/channels?favorites=true").then((r) => r.items),
    enabled: !!status?.enabled,
    refetchInterval: 60_000,
  });
  if (!status?.enabled || !data || data.length === 0) return null;
  const now = Date.now();
  return (
    <section className="section">
      <div className="section__head">
        <h2 className="t-h2">{t("home.onAir")}</h2>
        <Link to="/tv" className="t-label">{t("tv.guide")}</Link>
      </div>
      <div className="poster-row poster-row--wide">
        {data.map((c) => {
          const p = c.now;
          const pct = p ? Math.min(100, Math.max(0, (now - p.start_ms) / (p.end_ms - p.start_ms) * 100)) : 0;
          return (
            <Link key={c.id} to={`/tv/watch/${c.id}`} className="tv-card">
              <div className="tv-card__top"><ChannelLogo c={c} size={36} /><span className="t-label">{chNumber(c.number)} {c.name}</span></div>
              <div className="tv-card__title">{p?.title ?? t("tv.noGuide")}</div>
              {p && <span className="tv-progress"><span style={{ width: `${pct}%` }} /></span>}
            </Link>
          );
        })}
      </div>
    </section>
  );
}
