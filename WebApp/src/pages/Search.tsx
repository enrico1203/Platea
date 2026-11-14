import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Search as SearchIcon } from "lucide-react";
import { api } from "../api/client";
import type { Item, Page } from "../api/types";
import { PosterCard } from "../components/PosterCard";
import { Empty, TextField } from "../components/ui";
import { useT } from "../i18n";

export default function Search() {
  const t = useT();
  const [q, setQ] = useState("");
  const [debounced, setDebounced] = useState("");
  useEffect(() => {
    const h = setTimeout(() => setDebounced(q.trim()), 200);
    return () => clearTimeout(h);
  }, [q]);
  const { data } = useQuery({ queryKey: ["search", debounced], queryFn: () => api<Page<Item>>(`/api/v1/search?q=${encodeURIComponent(debounced)}`), enabled: debounced.length > 0 });
  return (
    <div className="stack-lg">
      <h1 className="t-h1">{t("nav.search")}</h1>
      <TextField label={t("search.label")} icon={<SearchIcon />} type="search" placeholder={t("search.placeholder")} value={q} onChange={(e) => setQ(e.target.value)} autoFocus />
      {debounced && data && data.items.length === 0 && <Empty>{t("search.none", { q: debounced })}</Empty>}
      {data && data.items.length > 0 && <div className="poster-grid">{data.items.map((i) => <PosterCard key={i.id} item={i} />)}</div>}
    </div>
  );
}
