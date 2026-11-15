import { useEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useInfiniteQuery } from "@tanstack/react-query";
import { api } from "../api/client";
import type { Item, Page } from "../api/types";
import { PosterCard, PosterSkeleton } from "../components/PosterCard";
import { Empty, SelectField } from "../components/ui";
import { useI18n } from "../i18n";

/** Film e serie di tutte le librerie con un genere o uno studio: `/browse?genre=Commedia`, `/browse?studio=HBO`. */
export default function Browse() {
  const { t } = useI18n();
  const [urlParams] = useSearchParams();
  const genre = urlParams.get("genre") ?? "";
  const studio = urlParams.get("studio") ?? "";
  const [sort, setSort] = useState("added");
  const [kind, setKind] = useState("");
  const params = new URLSearchParams({ sort, order: sort === "title" ? "asc" : "desc", limit: "60" });
  if (genre) params.set("genre", genre);
  if (studio) params.set("studio", studio);
  if (kind) params.set("kind", kind);
  const q = useInfiniteQuery({
    queryKey: ["items", "browse", params.toString()],
    queryFn: ({ pageParam }) => api<Page<Item>>(`/api/v1/items?${params}${pageParam ? `&cursor=${pageParam}` : ""}`),
    initialPageParam: "",
    getNextPageParam: (p) => p.next_cursor ?? undefined,
    enabled: !!(genre || studio),
  });
  const sentinel = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = sentinel.current;
    if (!el) return;
    const io = new IntersectionObserver((e) => { if (e[0].isIntersecting && q.hasNextPage && !q.isFetchingNextPage) q.fetchNextPage(); }, { rootMargin: "800px" });
    io.observe(el);
    return () => io.disconnect();
  }, [q]);
  const items = q.data?.pages.flatMap((p) => p.items) ?? [];
  return (
    <div>
      <div className="page-head">
        <div>
          <div className="t-label subtle">{genre ? t("browse.genre") : t("browse.studio")}</div>
          <h1 className="t-h1">{genre || studio}</h1>
        </div>
        <div className="row">
          <SelectField label={t("library.sort")} value={sort} onChange={setSort} options={[
            { value: "rating", label: t("library.sortRating") }, { value: "year", label: t("library.sortYear") },
            { value: "added", label: t("library.sortAdded") }, { value: "title", label: t("library.sortTitle") },
          ]} />
          <SelectField label={t("browse.kind")} value={kind} onChange={setKind} options={[
            { value: "", label: t("library.filterAll") }, { value: "movie", label: t("library.movies") }, { value: "series", label: t("library.shows") },
          ]} />
        </div>
      </div>
      {q.isSuccess && items.length === 0 ? (
        <Empty>{t("library.empty")}</Empty>
      ) : (
        <div className="poster-grid">
          {items.map((i) => <PosterCard key={i.id} item={i} />)}
          {(q.isLoading || q.isFetchingNextPage) && <PosterSkeleton count={q.isLoading ? 18 : 6} />}
        </div>
      )}
      <div ref={sentinel} style={{ height: 1 }} />
    </div>
  );
}
