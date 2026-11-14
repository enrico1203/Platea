import { useEffect, useRef, useState } from "react";
import { useParams } from "react-router-dom";
import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { api } from "../api/client";
import type { Item, Library as Lib, Page } from "../api/types";
import { PosterCard, PosterSkeleton } from "../components/PosterCard";
import { Empty, SelectField } from "../components/ui";
import { useI18n } from "../i18n";

const LETTERS = "#ABCDEFGHIJKLMNOPQRSTUVWXYZ".split("");

export default function Library() {
  const { id } = useParams();
  const { t } = useI18n();
  const [sort, setSort] = useState("title");
  const [filter, setFilter] = useState("all");
  const [genre, setGenre] = useState("");
  const [letter, setLetter] = useState("");
  const [decade, setDecade] = useState("");
  const [video, setVideo] = useState("");
  const { data: lib } = useQuery({ queryKey: ["libraries", id], queryFn: () => api<Lib>(`/api/v1/libraries/${id}`) });
  const { data: genres } = useQuery({ queryKey: ["genres", id], queryFn: () => api<{ items: { name: string; count: number }[] }>(`/api/v1/genres?library_id=${id}`).then((r) => r.items) });
  const order = sort === "title" ? "asc" : "desc";
  const params = new URLSearchParams({ library_id: id!, sort, order, limit: "60" });
  if (filter === "unplayed") params.set("played", "false");
  if (filter === "played") params.set("played", "true");
  if (filter === "favorite") params.set("favorite", "true");
  if (filter === "collections") params.set("kind", "collection");
  if (genre) params.set("genre", genre);
  if (decade) params.set("decade", decade);
  if (video) params.set("video", video);
  if (letter && sort === "title") params.set("letter", letter);
  const q = useInfiniteQuery({
    queryKey: ["items", params.toString()],
    queryFn: ({ pageParam }) => api<Page<Item>>(`/api/v1/items?${params}${pageParam ? `&cursor=${pageParam}` : ""}`),
    initialPageParam: "",
    getNextPageParam: (p) => p.next_cursor ?? undefined,
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
          <h1 className="t-h1">{lib?.name}</h1>
          {lib && <div className="page-head__sub t-small subtle">{t(lib.kind === "movies" ? "library.countMovies" : "library.countShows", { count: lib.item_count })}</div>}
        </div>
        <div className="row">
          <SelectField label={t("library.sort")} value={sort} onChange={setSort} options={[
            { value: "title", label: t("library.sortTitle") }, { value: "added", label: t("library.sortAdded") }, { value: "year", label: t("library.sortYear") },
            { value: "rating", label: t("library.sortRating") }, { value: "played", label: t("library.sortPlayed") },
          ]} />
          <SelectField label={t("library.filter")} value={filter} onChange={setFilter} options={[
            { value: "all", label: t("library.filterAll") }, { value: "unplayed", label: t("library.filterUnplayed") }, { value: "played", label: t("library.filterPlayed") }, { value: "favorite", label: t("library.filterFavorite") },
            ...(lib?.kind === "movies" ? [{ value: "collections", label: t("library.filterCollections") }] : []),
          ]} />
          <SelectField label={t("library.decade")} value={decade} onChange={setDecade} options={[
            { value: "", label: t("library.filterAll") },
            ...[2020, 2010, 2000, 1990, 1980, 1970, 1960, 1950].map((d) => ({ value: String(d), label: t("library.decadeLabel", { d: String(d).slice(2) }) })),
            { value: "1940", label: t("library.decadeOld") },
          ]} />
          <SelectField label={t("library.video")} value={video} onChange={setVideo} options={[
            { value: "", label: t("library.filterAll") }, { value: "4k", label: "4K" }, { value: "1080", label: "1080p" }, { value: "720", label: "720p" }, { value: "sd", label: "SD" }, { value: "hdr", label: "HDR" },
          ]} />
          {genres && genres.length > 0 && (
            <SelectField label={t("library.genre")} value={genre} onChange={setGenre} options={[{ value: "", label: t("library.filterAll") }, ...genres.map((g) => ({ value: g.name, label: g.name }))]} />
          )}
        </div>
      </div>
      {sort === "title" && (
        <div className="letters" style={{ marginBottom: 24 }}>
          {LETTERS.map((l) => <button key={l} className={letter === l ? "active" : ""} onClick={() => setLetter(letter === l ? "" : l)}>{l}</button>)}
        </div>
      )}
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
