import { useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useParams, useSearchParams } from "react-router-dom";
import { ArrowUpDown, Search as SearchIcon, SlidersHorizontal, X } from "lucide-react";
import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { api } from "../api/client";
import type { Item, Library as Lib, Page } from "../api/types";
import { PosterCard, PosterSkeleton } from "../components/PosterCard";
import { Empty } from "../components/ui";
import { useI18n } from "../i18n";

const LETTERS = "#ABCDEFGHIJKLMNOPQRSTUVWXYZ".split("");

type Opt = { value: string; label: string };
type Group = { key: string; label: string; value: string; def: string; set: (v: string) => void; options: Opt[] };

/** Un gruppo di scelte a pillola (una sola attiva), dentro il pannello dei filtri. */
function ChipGroup({ g }: { g: Group }) {
  const id = useId();
  return (
    <section className="fsheet__group">
      <h3 className="fsheet__label" id={id}>{g.label}</h3>
      <div className="fsheet__chips" role="radiogroup" aria-labelledby={id}>
        {g.options.map((o) => (
          <button key={o.value} type="button" role="radio" aria-checked={g.value === o.value} className={`fchip${g.value === o.value ? " is-on" : ""}`} onClick={() => g.set(o.value)}>{o.label}</button>
        ))}
      </div>
    </section>
  );
}

/** Pannello dei filtri: dal basso su telefono, di lato su desktop. I filtri si applicano subito, la griglia dietro si aggiorna. */
function FilterSheet({ groups, active, onClear, onClose, t }: { groups: Group[]; active: number; onClear: () => void; onClose: () => void; t: ReturnType<typeof useI18n>["t"] }) {
  const ref = useRef<HTMLDivElement>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  useEffect(() => {
    const back = document.activeElement as HTMLElement | null;
    const k = (e: KeyboardEvent) => e.key === "Escape" && closeRef.current();
    window.addEventListener("keydown", k);
    const overflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    ref.current?.focus();
    return () => { window.removeEventListener("keydown", k); document.body.style.overflow = overflow; back?.focus(); };
  }, []);
  return createPortal(
    <div className="fsheet-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="fsheet" role="dialog" aria-modal="true" aria-labelledby="fsheet-title" tabIndex={-1} ref={ref}>
        <div className="fsheet__grip" aria-hidden />
        <header className="fsheet__head">
          <h2 className="t-h3" id="fsheet-title">{t("library.filters")}</h2>
          <button type="button" className="icon-btn" onClick={onClose} aria-label={t("common.close")}><X /></button>
        </header>
        <div className="fsheet__body">
          {groups.map((g) => <ChipGroup key={g.key} g={g} />)}
        </div>
        <footer className="fsheet__foot">
          <button type="button" className="pl-btn pl-btn--ghost" onClick={onClear} disabled={active === 0}>{t("library.clearFilters")}</button>
          <button type="button" className="pl-btn pl-btn--primary" onClick={onClose}>{t("library.showResults")}</button>
        </footer>
      </div>
    </div>,
    document.body,
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
  const [sheet, setSheet] = useState(false);
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
  const all = t("library.filterAll");
  const groups: Group[] = [
    { key: "sort", label: t("library.sort"), value: sort, def: "title", set: setSort, options: [
      { value: "title", label: t("library.sortTitle") }, { value: "added", label: t("library.sortAdded") }, { value: "year", label: t("library.sortYear") },
      { value: "rating", label: t("library.sortRating") }, { value: "played", label: lib?.kind === "audiobooks" ? t("library.sortListened") : t("library.sortPlayed") },
    ] },
    { key: "filter", label: t("library.filter"), value: filter, def: "all", set: setFilter, options: [
      { value: "all", label: all },
      ...(lib?.kind === "audiobooks"
        ? [{ value: "unplayed", label: t("library.filterUnfinished") }, { value: "played", label: t("library.filterFinished") }]
        : [{ value: "unplayed", label: t("library.filterUnplayed") }, { value: "played", label: t("library.filterPlayed") }]),
      { value: "favorite", label: t("library.filterFavorite") },
      ...(lib?.kind === "movies" ? [{ value: "collections", label: t("library.filterCollections") }] : []),
      ...(lib?.kind === "audiobooks" ? [{ value: "collections", label: t("library.filterSeries") }] : []),
    ] },
    ...(lib?.kind !== "audiobooks" ? [{ key: "video", label: t("library.video"), value: video, def: "", set: setVideo, options: [
      { value: "", label: all }, { value: "4k", label: "4K" }, { value: "1080", label: "1080p" }, { value: "720", label: "720p" }, { value: "sd", label: "SD" }, { value: "hdr", label: "HDR" },
    ] }] : []),
    { key: "decade", label: t("library.decade"), value: decade, def: "", set: setDecade, options: [
      { value: "", label: all },
      ...[2020, 2010, 2000, 1990, 1980, 1970, 1960, 1950].map((d) => ({ value: String(d), label: t("library.decadeLabel", { d: String(d).slice(2) }) })),
      { value: "1940", label: t("library.decadeOld") },
    ] },
    ...(genres && genres.length > 0 ? [{ key: "genre", label: t("library.genre"), value: genre, def: "", set: setGenre, options: [{ value: "", label: all }, ...genres.map((g) => ({ value: g.name, label: g.name }))] }] : []),
  ];
  const active = groups.slice(1).filter((g) => g.value !== g.def).length;
  const labelOf = (g: Group) => g.options.find((o) => o.value === g.value)?.label ?? g.value;
  const clearAll = () => { setFilter("all"); setGenre(""); setDecade(""); setVideo(""); };
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
        <button type="button" className={`lib-toolbar__filters${active > 0 ? " is-on" : ""}`} aria-haspopup="dialog" aria-expanded={sheet} aria-label={active > 0 ? `${t("library.filters")}, ${t("library.activeFilters", { count: active })}` : undefined} onClick={() => setSheet(true)}>
          <SlidersHorizontal aria-hidden />
          <span className="lib-toolbar__filters-text">{t("library.filters")}</span>
          {active > 0 && <span className="lib-toolbar__count" aria-hidden>{active}</span>}
        </button>
      </div>
      {(active > 0 || sort !== "title") && (
        <div className="lib-active">
          {sort !== "title" && (
            <button type="button" className="lib-active__chip lib-active__chip--sort" onClick={() => setSort("title")} aria-label={`${t("common.remove")}: ${t("library.sort")} ${labelOf(groups[0])}`}>
              <ArrowUpDown aria-hidden />{labelOf(groups[0])}<X aria-hidden />
            </button>
          )}
          {groups.slice(1).filter((g) => g.value !== g.def).map((g) => (
            <button key={g.key} type="button" className="lib-active__chip" onClick={() => g.set(g.def)} aria-label={`${t("common.remove")}: ${g.label} ${labelOf(g)}`}>
              {labelOf(g)}<X aria-hidden />
            </button>
          ))}
          {active > 1 && <button type="button" className="lib-active__clear" onClick={clearAll}>{t("library.clearFilters")}</button>}
        </div>
      )}
      {sheet && <FilterSheet groups={groups} active={active} onClear={clearAll} onClose={() => setSheet(false)} t={t} />}
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
