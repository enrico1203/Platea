import { useParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { api, imageUrl } from "../api/client";
import type { PersonDetail } from "../api/types";
import { PosterCard, PosterSkeleton } from "../components/PosterCard";
import { Empty } from "../components/ui";
import { useI18n } from "../i18n";

/** Una persona e i titoli della libreria in cui compare, dal più recente. */
export default function Person() {
  const { id } = useParams();
  const { t } = useI18n();
  const { data } = useQuery({ queryKey: ["person", id], queryFn: () => api<PersonDetail>(`/api/v1/people/${id}`) });
  if (!data) return <div className="poster-grid" style={{ marginTop: 160 }}><PosterSkeleton count={12} /></div>;
  const role = (r: { role: string; character: string | null }) => (r.role === "actor" ? (r.character ? t("person.as", { character: r.character }) : t("item.role.actor")) : t(`item.role.${r.role}` as "item.role.director"));
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
        <div className="poster-grid">
          {data.items.map((i) => <PosterCard key={i.id} item={i} metaOverride={[i.year, i.roles.map(role).join(", ")].filter(Boolean).join(" · ")} />)}
        </div>
      )}
    </div>
  );
}
