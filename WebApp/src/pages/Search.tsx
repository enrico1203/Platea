import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useSearchParams } from "react-router-dom";
import { Search as SearchIcon } from "lucide-react";
import { api } from "../api/client";
import type { Item, Page } from "../api/types";
import { PosterCard } from "../components/PosterCard";
import { Empty, TextField } from "../components/ui";
import { useT } from "../i18n";
import { useLibraries } from "../components/Layout";

export default function Search() {
  const t = useT();
  // La ricerca vive nell'indirizzo: si può condividere, e "indietro" dal dettaglio ritrova i risultati.
  const [params, setParams] = useSearchParams();
  const [q, setQ] = useState(() => params.get("q") ?? "");
  const [debounced, setDebounced] = useState(() => (params.get("q") ?? "").trim());
  useEffect(() => {
    const h = setTimeout(() => {
      const v = q.trim();
      setDebounced(v);
      setParams(v ? { q: v } : {}, { replace: true });
    }, 200);
    return () => clearTimeout(h);
  }, [q, setParams]);
  const { data } = useQuery({ queryKey: ["search", debounced], queryFn: () => api<Page<Item>>(`/api/v1/search?q=${encodeURIComponent(debounced)}&limit=100`), enabled: debounced.length > 0 });
  const { data: libs } = useLibraries();
  // Un gruppo per libreria, nell'ordine della barra laterale: lo stesso film in "Film HD" e "Film 4k" si distingue.
  const groups = (() => {
    if (!data) return [];
    const order = new Map((libs ?? []).map((l, i) => [l.id, i]));
    const by = new Map<string, Item[]>();
    for (const i of data.items) by.set(i.library_id, [...(by.get(i.library_id) ?? []), i]);
    return [...by.entries()]
      .map(([id, items]) => ({ id, name: libs?.find((l) => l.id === id)?.name ?? "", items }))
      .sort((a, b) => (order.get(a.id) ?? 999) - (order.get(b.id) ?? 999));
  })();
  return (
    <div className="stack-lg">
      <h1 className="t-h1">{t("nav.search")}</h1>
      <TextField label={t("search.label")} icon={<SearchIcon />} type="search" placeholder={t("search.placeholder")} value={q} onChange={(e) => setQ(e.target.value)} autoFocus />
      {debounced && data && data.items.length === 0 && <Empty>{t("search.none", { q: debounced })}</Empty>}
      {groups.map((g) => (
        <section key={g.id} className="section">
          {groups.length > 1 && <div className="section__head"><h2 className="t-h2">{g.name}</h2><span className="t-small muted">{g.items.length}</span></div>}
          <div className="poster-grid">{g.items.map((i) => <PosterCard key={i.id} item={i} />)}</div>
        </section>
      ))}
    </div>
  );
}
