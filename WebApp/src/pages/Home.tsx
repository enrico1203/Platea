import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { api } from "../api/client";
import type { Home as HomeData } from "../api/types";
import { PosterCard } from "../components/PosterCard";
import { Empty } from "../components/ui";
import { useSession } from "../session";
import { useT } from "../i18n";

export default function Home() {
  const t = useT();
  const { me } = useSession();
  const { data, isLoading } = useQuery({ queryKey: ["home"], queryFn: () => api<HomeData>("/api/v1/home") });
  if (isLoading || !data) return null;
  const nothing = data.resume.length === 0 && data.next_up.length === 0 && data.recent.every((r) => r.items.length === 0);
  if (nothing) {
    return (
      <Empty action={me?.is_admin ? <Link className="pl-btn pl-btn--primary" to="/admin/libraries">{t("home.addLibrary")}</Link> : undefined}>
        {me?.is_admin ? t("home.emptyAdmin") : t("home.empty")}
      </Empty>
    );
  }
  return (
    <div>
      {data.resume.length > 0 && (
        <section className="section">
          <div className="section__head"><h2 className="t-h2">{t("home.resume")}</h2></div>
          <div className="poster-row poster-row--wide">{data.resume.map((i) => <PosterCard key={i.id} item={i} wide />)}</div>
        </section>
      )}
      {data.next_up.length > 0 && (
        <section className="section">
          <div className="section__head"><h2 className="t-h2">{t("home.nextUp")}</h2></div>
          <div className="poster-row poster-row--wide">{data.next_up.map((i) => <PosterCard key={i.id} item={i} wide />)}</div>
        </section>
      )}
      {data.recent.filter((r) => r.items.length > 0).map((r) => (
        <section className="section" key={r.library_id}>
          <div className="section__head">
            <h2 className="t-h2">{t("home.recentIn", { name: r.library_name })}</h2>
            <Link to={`/library/${r.library_id}`} className="t-label">{t("common.all")}</Link>
          </div>
          <div className={`poster-row${r.kind === "shows" ? " poster-row--wide" : ""}`}>
            {r.items.map((i) => <PosterCard key={i.id} item={i} wide={r.kind === "shows"} />)}
          </div>
        </section>
      ))}
    </div>
  );
}
