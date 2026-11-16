import { useParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { api, imageUrl } from "../api/client";
import type { PersonDetail } from "../api/types";
import { PosterCard, PosterSkeleton } from "../components/PosterCard";
import { Empty } from "../components/ui";
import { useLibraries } from "../components/Layout";
import { useI18n } from "../i18n";

/** Una persona e i titoli della libreria in cui compare, dal più recente. */
export default function Person() {
  const { id } = useParams();
  const { t } = useI18n();
  const { data } = useQuery({ queryKey: ["person", id], queryFn: () => api<PersonDetail>(`/api/v1/people/${id}`) });
  const { data: libs } = useLibraries();
  if (!data) return <div className="poster-grid" style={{ marginTop: 160 }}><PosterSkeleton count={12} /></div>;
  const role = (r: { role: string; character: string | null }) => (r.role === "actor" ? (r.character ? t("person.as", { character: r.character }) : t("item.role.actor")) : t(`item.role.${r.role}` as "item.role.director"));
  // Un gruppo per libreria, nell'ordine della barra laterale (come la ricerca): lo stesso film in "Film HD" e
  // "Film 4k" si distingue.
  type Row = PersonDetail["items"][number];
  const order = new Map((libs ?? []).map((l, i) => [l.id, i]));
  const by = new Map<string, Row[]>();
  for (const i of data.items) by.set(i.library_id, [...(by.get(i.library_id) ?? []), i]);
  const groups = [...by.entries()]
    .map(([lid, items]) => ({ id: lid, name: libs?.find((l) => l.id === lid)?.name ?? "", items }))
    .sort((a, b) => (order.get(a.id) ?? 999) - (order.get(b.id) ?? 999));
  return (
    <div>
      <div className="person-head">
        <div className="person-head__img">{data.image && <img src={imageUrl(data.image, 240)} alt="" />}</div>
        <div>
          <h1 className="t-h1">{data.name}</h1>
          <div className="t-small subtle">{t("person.inLibrary", { count: data.items.length })}</div>
        </div>
      </div>
      {data.items.length === 0 ? <Empty>{t("person.none")}</Empty> : (
        <div className="stack-lg">
          {groups.map((g) => (
            <section key={g.id} className="section">
              {groups.length > 1 && <div className="section__head"><h2 className="t-h2">{g.name}</h2><span className="t-small muted">{g.items.length}</span></div>}
              <div className="poster-grid">
                {g.items.map((i) => <PosterCard key={i.id} item={i} metaOverride={[i.year, i.roles.map(role).join(", ")].filter(Boolean).join(" · ")} />)}
              </div>
            </section>
          ))}
        </div>
      )}
    </div>
  );
}
