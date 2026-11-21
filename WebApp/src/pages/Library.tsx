import { useEffect, useRef, useState, type ReactNode } from "react";
import { useParams, useSearchParams } from "react-router-dom";
import { ArrowUpDown, ChevronDown, Search as SearchIcon, X } from "lucide-react";
import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { api } from "../api/client";
import type { Item, Library as Lib, Page } from "../api/types";
import { PosterCard, PosterSkeleton } from "../components/PosterCard";
import { Empty } from "../components/ui";
import { useI18n } from "../i18n";

const LETTERS = "#ABCDEFGHIJKLMNOPQRSTUVWXYZ".split("");

type Opt = { value: string; label: string };

/** Filtro a pillola: un select nativo invisibile ci sta sopra (su iPhone apre la ruota di sistema; 16px, niente zoom).
 *  Al valore predefinito mostra il nome del filtro; scelto un valore mostra quello, evidenziato.
 *  `always`: mostra sempre il valore e non si evidenzia (l'ordinamento non è un filtro). */
function Pick({ label, value, def, onChange, options, icon, always }: { label: string; value: string; def: string; onChange: (v: string) => void; options: Opt[]; icon?: ReactNode; always?: boolean }) {
  const on = value !== def;
  const current = options.find((o) => o.value === value)?.label ?? label;
  return (
    <label className={`pick${on && !always ? " is-on" : ""}`}>
      {icon}
      <span className="pick__text">{on || always ? current : label}</span>
      <ChevronDown className="pick__chev" aria-hidden />
      <select aria-label={label} value={value} onChange={(e) => onChange(e.target.value)}>
        {options.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
      </select>
    </label>
  );
}

export default function Library() {
  const { id } = useParams();
  const { t } = useI18n();
  const [sort, setSort] = useState("title");
  const [filter, setFilter] = useState("all");
  const [genre, setGenre] = useState("");
  const [letter, setLetter] = useState("");
  const [decade, setDecade] = useState("");
  const [video, setVideo] = useState("");
  // Ricerca nella libreria: nell'indirizzo, così "indietro" dal dettaglio ritrova i risultati.
  const [urlParams, setUrlParams] = useSearchParams();
  const [text, setText] = useState(() => urlParams.get("q") ?? "");
  const [query, setQuery] = useState(() => (urlParams.get("q") ?? "").trim());
  useEffect(() => {
    const h = setTimeout(() => {
      const v = text.trim();
      setQuery(v);
      setUrlParams(v ? { q: v } : {}, { replace: true });
    }, 200);
    return () => clearTimeout(h);
  }, [text, setUrlParams]);
  const { data: lib } = useQuery({ queryKey: ["libraries", id], queryFn: () => api<Lib>(`/api/v1/libraries/${id}`) });
  const { data: genres } = useQuery({ queryKey: ["genres", id], queryFn: () => api<{ items: { name: string; count: number }[] }>(`/api/v1/genres?library_id=${id}`).then((r) => r.items) });
  const filtered = filter !== "all" || !!genre || !!decade || !!video;
  const order = sort === "title" ? "asc" : "desc";
  const params = new URLSearchParams({ library_id: id!, sort, order, limit: "60" });
  if (filter === "unplayed") params.set("played", "false");
  if (filter === "played") params.set("played", "true");
  if (filter === "favorite") params.set("favorite", "true");
  if (filter === "collections") params.set("kind", "collection");
  if (genre) params.set("genre", genre);
  if (decade) params.set("decade", decade);
  if (video) params.set("video", video);
  if (query) params.set("q", query);
  else if (letter && sort === "title") params.set("letter", letter);
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
          {lib && <div className="page-head__sub t-small subtle">{t(lib.kind === "movies" ? "library.countMovies" : lib.kind === "audiobooks" ? "library.countAudiobooks" : "library.countShows", { count: lib.item_count })}</div>}
        </div>
      </div>
      <div className="lib-toolbar">
        <label className="pl-field__control lib-toolbar__search">
          <SearchIcon aria-hidden />
          <input type="search" aria-label={t("library.searchIn", { name: lib?.name ?? "" })} placeholder={t("library.searchIn", { name: lib?.name ?? "" })} value={text} onChange={(e) => setText(e.target.value)} />
        </label>
        <div className="lib-toolbar__picks">
          <Pick always label={t("library.sort")} icon={<ArrowUpDown aria-hidden />} value={sort} def="title" onChange={setSort} options={[
            { value: "title", label: t("library.sortTitle") }, { value: "added", label: t("library.sortAdded") }, { value: "year", label: t("library.sortYear") },
            { value: "rating", label: t("library.sortRating") }, { value: "played", label: lib?.kind === "audiobooks" ? t("library.sortListened") : t("library.sortPlayed") },
          ]} />
          <span className="lib-toolbar__sep" aria-hidden />
          <Pick label={t("library.filter")} value={filter} def="all" onChange={setFilter} options={[
            { value: "all", label: t("library.filterAll") },
            ...(lib?.kind === "audiobooks"
              ? [{ value: "unplayed", label: t("library.filterUnfinished") }, { value: "played", label: t("library.filterFinished") }]
              : [{ value: "unplayed", label: t("library.filterUnplayed") }, { value: "played", label: t("library.filterPlayed") }]),
            { value: "favorite", label: t("library.filterFavorite") },
            ...(lib?.kind === "movies" ? [{ value: "collections", label: t("library.filterCollections") }] : []),
            ...(lib?.kind === "audiobooks" ? [{ value: "collections", label: t("library.filterSeries") }] : []),
          ]} />
          {genres && genres.length > 0 && (
            <Pick label={t("library.genre")} value={genre} def="" onChange={setGenre} options={[{ value: "", label: t("library.filterAll") }, ...genres.map((g) => ({ value: g.name, label: g.name }))]} />
          )}
          <Pick label={t("library.decade")} value={decade} def="" onChange={setDecade} options={[
            { value: "", label: t("library.filterAll") },
            ...[2020, 2010, 2000, 1990, 1980, 1970, 1960, 1950].map((d) => ({ value: String(d), label: t("library.decadeLabel", { d: String(d).slice(2) }) })),
            { value: "1940", label: t("library.decadeOld") },
          ]} />
          {lib?.kind !== "audiobooks" && <Pick label={t("library.video")} value={video} def="" onChange={setVideo} options={[
            { value: "", label: t("library.filterAll") }, { value: "4k", label: "4K" }, { value: "1080", label: "1080p" }, { value: "720", label: "720p" }, { value: "sd", label: "SD" }, { value: "hdr", label: "HDR" },
          ]} />}
          {filtered && (
            <button type="button" className="lib-toolbar__clear" aria-label={t("library.clearFilters")} onClick={() => { setFilter("all"); setGenre(""); setDecade(""); setVideo(""); }}>
              <X aria-hidden /><span className="lib-toolbar__clear-text">{t("library.clearFilters")}</span>
            </button>
          )}
        </div>
      </div>
      {sort === "title" && !query && (
        <div className="letters lib-letters">
          {LETTERS.map((l) => <button key={l} type="button" aria-pressed={letter === l} className={letter === l ? "active" : ""} onClick={() => setLetter(letter === l ? "" : l)}>{l}</button>)}
        </div>
      )}
      {q.isSuccess && items.length === 0 ? (
        <Empty>{query ? t("search.none", { q: query }) : t("library.empty")}</Empty>
      ) : (
        <div className={`poster-grid${lib?.kind === "audiobooks" ? " poster-grid--square" : ""}`}>
          {items.map((i) => <PosterCard key={i.id} item={i} />)}
          {(q.isLoading || q.isFetchingNextPage) && <PosterSkeleton count={q.isLoading ? 18 : 6} />}
        </div>
      )}
      <div ref={sentinel} style={{ height: 1 }} />
    </div>
  );
}
