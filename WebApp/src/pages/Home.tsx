import { Fragment, type ReactNode } from "react";
import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { api } from "../api/client";
import type { Home as HomeData } from "../api/types";
import { PosterCard, PosterSkeleton } from "../components/PosterCard";
import { Hero, pickFeatured } from "../components/Hero";
import { Empty } from "../components/ui";
import { useSession } from "../session";
import { useT } from "../i18n";

export default function Home() {
  const t = useT();
  const { me } = useSession();
  const { data, isLoading } = useQuery({ queryKey: ["home"], queryFn: () => api<HomeData>("/api/v1/home") });
  if (isLoading || !data) {
    return (
      <div aria-busy="true">
        <div className="hero hero--skeleton skeleton" />
        <section className="section"><div className="skeleton skeleton--line" style={{ width: 220, height: 22 }} /><div className="poster-row" style={{ marginTop: 16 }}><PosterSkeleton count={8} /></div></section>
      </div>
    );
  }
  const nothing = data.resume.length === 0 && data.next_up.length === 0 && data.recent.every((r) => r.items.length === 0);
  if (nothing) {
    return (
      <Empty action={me?.is_admin ? <Link className="pl-btn pl-btn--primary" to="/admin/libraries">{t("home.addLibrary")}</Link> : undefined}>
        {me?.is_admin ? t("home.emptyAdmin") : t("home.empty")}
      </Empty>
    );
  }
  // Sezioni nell'ordine scelto dall'utente; quelle tolte non compaiono.
  const order = me?.preferences?.home_sections ?? ["resume", "next_up", "recent"];
  // In scena: ciò che si stava guardando, o un titolo appena arrivato. Non si ripete nella riga sotto.
  const featured = pickFeatured(order.includes("resume") ? data.resume : [], data.recent.flatMap((r) => r.items));
  const resumeRow = data.resume.filter((i) => !(featured?.resume && i.id === featured.item.id));
  const sections: Record<string, ReactNode> = {
    resume: resumeRow.length > 0 && (
      <section className="section" key="resume">
        <div className="section__head"><h2 className="t-h2">{t("home.resume")}</h2></div>
        <div className="poster-row poster-row--wide">{resumeRow.map((i) => <PosterCard key={i.id} item={i} wide />)}</div>
      </section>
    ),
    next_up: data.next_up.length > 0 && (
      <section className="section" key="next_up">
        <div className="section__head"><h2 className="t-h2">{t("home.nextUp")}</h2></div>
        <div className="poster-row poster-row--wide">{data.next_up.map((i) => <PosterCard key={i.id} item={i} wide />)}</div>
      </section>
    ),
    recent: data.recent.filter((r) => r.items.length > 0).map((r) => (
      <section className="section" key={r.library_id}>
        <div className="section__head">
          <h2 className="t-h2">{t("home.recentIn", { name: r.library_name })}</h2>
          <Link to={`/library/${r.library_id}`} className="t-label">{t("common.all")}</Link>
        </div>
        <div className={`poster-row${r.kind === "shows" ? " poster-row--wide" : ""}`}>
          {r.items.map((i) => <PosterCard key={i.id} item={i} wide={r.kind === "shows"} />)}
        </div>
      </section>
    )),
  };
  return (
    <div>
      {featured && <Hero item={featured.item} resume={featured.resume} />}
      {order.map((s) => <Fragment key={s}>{sections[s]}</Fragment>)}
    </div>
  );
}
