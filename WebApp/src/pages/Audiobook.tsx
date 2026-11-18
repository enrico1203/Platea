/** Pagina di un audiolibro: copertina, autori e lettori, serie, capitoli, avanzamento, altri libri. */
import { useState } from "react";
import { Link } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, Heart, ListPlus, Pause, Pencil, Play, RefreshCw, SearchCheck } from "lucide-react";
import { api, imageUrl } from "../api/client";
import type { Item, ItemDetail, Page, PersonDetail } from "../api/types";
import { PosterCard } from "../components/PosterCard";
import { Alert, Badge, toast } from "../components/ui";
import { useSession } from "../session";
import { useI18n } from "../i18n";
import { chapterAt, useAudio } from "../audio/AudioPlayer";
import { AddToPlaylist } from "./Playlists";
import { EditDialog, IdentifyDialog, TechInfo } from "./Item";

const CHAPTERS_SHORT = 12;

export function AudiobookDetail({ item }: { item: ItemDetail }) {
  const { t, duration, clock, num, date } = useI18n();
  const { me } = useSession();
  const qc = useQueryClient();
  const audio = useAudio();
  const [dialog, setDialog] = useState<"identify" | "edit" | "playlist" | null>(null);
  const [allChapters, setAllChapters] = useState(false);
  const [fileIdx, setFileIdx] = useState(0);
  const book = item.audiobook;
  const current = audio.book?.id === item.id;
  const position = current ? audio.position : item.user_data.played ? 0 : item.user_data.position_ms;
  const total = book?.duration_ms || item.runtime_ms || 0;
  const started = position > 0 && !item.user_data.played;
  const authors = item.people.filter((p) => p.role === "author");
  const narrators = item.people.filter((p) => p.role === "narrator");
  const series = item.collections?.[0];
  const chapters = book?.chapters ?? [];
  const nowChapter = chapterAt(chapters, position);

  const { data: seriesBooks } = useQuery({
    queryKey: ["items", "children", series?.id],
    queryFn: () => api<Page<Item>>(`/api/v1/items/${series!.id}/children`).then((r) => r.items),
    enabled: !!series,
  });
  const firstAuthor = authors[0];
  const { data: byAuthor } = useQuery({
    queryKey: ["person", firstAuthor?.id],
    queryFn: () => api<PersonDetail>(`/api/v1/people/${firstAuthor!.id}`),
    enabled: !!firstAuthor,
  });
  const others = (byAuthor?.items ?? []).filter((i) => i.id !== item.id && i.kind === "audiobook" && !seriesBooks?.some((s) => s.id === i.id)).slice(0, 20);

  const toggle = async (what: "played" | "favorite", on: boolean) => {
    await api(`/api/v1/items/${item.id}/${what}`, { method: on ? "POST" : "DELETE" });
    qc.invalidateQueries();
  };
  const listen = () => {
    if (current && audio.playing) audio.toggle();
    else void audio.play(item);
  };
  const playFrom = (ms: number) => {
    if (current) audio.seek(ms);
    else void audio.play(item, ms);
  };
  const files = item.files ?? [];
  const file = files[fileIdx];
  const meta = [
    total ? duration(total) : null,
    item.year,
    item.community_rating ? `★ ${num(item.community_rating / 2, 1)}` : null,
    book && book.parts.length > 1 ? t("audio.parts", { count: book.parts.length }) : null,
  ].filter(Boolean);
  const label = current && audio.playing ? t("audio.listening")
    : started ? t("audio.resumeLeft", { time: duration(Math.max(0, total - position)) })
    : t("audio.listen");

  return (
    <div className="ab-detail">
      <div className="ab-detail__head">
        <div className="ab-detail__cover">
          <div className="ab-cover" style={{ background: item.images.poster_color ?? undefined }}>
            {item.images.poster && <img src={imageUrl(item.images.poster, 480)} alt="" />}
          </div>
        </div>
        <div className="ab-detail__info">
          {series && <Link to={`/item/${series.id}`} className="t-label">{item.index_number ? t("audio.seriesBook", { series: series.title, n: item.index_number }) : series.title}</Link>}
          <h1 className="t-display">{item.title}</h1>
          {item.tagline && <div className="t-body-lg muted">{item.tagline}</div>}
          {authors.length > 0 && (
            <div className="ab-people">{t("audio.by")} {authors.map((p, i) => <span key={p.id}>{i > 0 && ", "}<Link to={`/person/${p.id}`}>{p.name}</Link></span>)}</div>
          )}
          {narrators.length > 0 && (
            <div className="t-small muted">{t("audio.readByLabel")} {narrators.map((p, i) => <span key={p.id}>{i > 0 && ", "}<Link to={`/person/${p.id}`}>{p.name}</Link></span>)}</div>
          )}
          <div className="detail__meta t-small">{meta.map((m, i) => <span key={i}>{m}</span>)}</div>
          {item.genres.length > 0 && <div className="chips">{item.genres.map((g) => <Link key={g} to={`/browse?genre=${encodeURIComponent(g)}`} className="chip-link"><Badge>{g}</Badge></Link>)}</div>}
          {me?.permissions.expired && <Alert tone="warning">{t("item.expired")}</Alert>}
          <div className="detail__actions">
            {book && book.parts.length > 0 && !me?.permissions.expired && (
              <>
                <button className="pl-btn pl-btn--play" onClick={listen} aria-label={current && audio.playing ? t("player.pause") : t("audio.listen")}>
                  {current && audio.playing ? <Pause fill="currentColor" /> : <Play fill="currentColor" />}
                </button>
                <span className="t-label">{label}</span>
              </>
            )}
            <span className="spacer" />
            <button className={`icon-btn${item.user_data.played ? " on" : ""}`} onClick={() => toggle("played", !item.user_data.played)} aria-label={item.user_data.played ? t("audio.markUnfinished") : t("audio.markFinished")} title={item.user_data.played ? t("audio.markUnfinished") : t("audio.markFinished")}><Check /></button>
            <button className={`icon-btn${item.user_data.favorite ? " on" : ""}`} onClick={() => toggle("favorite", !item.user_data.favorite)} aria-label={t("item.favorite")} title={t("item.favorite")}><Heart fill={item.user_data.favorite ? "currentColor" : "none"} /></button>
            <button className="icon-btn" onClick={() => setDialog("playlist")} aria-label={t("playlist.addTo")} title={t("playlist.addTo")}><ListPlus /></button>
            {me?.is_admin && (
              <>
                <button className="icon-btn" onClick={() => setDialog("identify")} aria-label={t("item.identify")} title={t("item.identify")}><SearchCheck /></button>
                <button className="icon-btn" onClick={async () => { await api(`/api/v1/items/${item.id}/refresh`, { method: "POST" }); toast(t("item.refreshQueued")); }} aria-label={t("item.refresh")} title={t("item.refresh")}><RefreshCw /></button>
                <button className="icon-btn" onClick={() => setDialog("edit")} aria-label={t("item.edit")} title={t("item.edit")}><Pencil /></button>
              </>
            )}
          </div>
          {started && total > 0 && (
            <div className="ab-progress" aria-label={t("audio.progress")}>
              <div className="ab-progress__bar"><span style={{ width: `${Math.min(100, (position / total) * 100)}%` }} /></div>
              <span className="t-mono-sm subtle">{num((position / total) * 100, 0)}%{nowChapter >= 0 ? ` · ${chapters[nowChapter].title ?? t("player.chapterN", { n: nowChapter + 1 })}` : ""}</span>
            </div>
          )}
          {item.match_state === "review" && me?.is_admin && <Alert tone="warning">{t("audio.needsReview")}</Alert>}
        </div>
      </div>

      <div className="detail__content stack-lg">
        {item.overview && (
          <section className="stack">
            <h2 className="t-h3">{t("audio.summary")}</h2>
            <p className="t-body-lg muted ab-overview">{item.overview}</p>
            {item.studios.length > 0 && <div className="t-small subtle">{t("audio.publisher")} {item.studios.join(", ")}{item.premiere_date ? ` · ${date(item.premiere_date)}` : ""}</div>}
          </section>
        )}

        {chapters.length > 1 && (
          <section className="stack">
            <div className="row row--between">
              <h2 className="t-h3">{t("player.chapters")} <span className="subtle t-small">{chapters.length}</span></h2>
            </div>
            <ol className="ab-chapters ab-chapters--page">
              {(allChapters ? chapters : chapters.slice(0, CHAPTERS_SHORT)).map((c, i) => {
                const end = i + 1 < chapters.length ? chapters[i + 1].start_ms : total;
                const cur = (current || started) && i === nowChapter;
                return (
                  <li key={i} className={cur ? "is-current" : position > end && (current || started) ? "is-past" : undefined}>
                    <button type="button" onClick={() => playFrom(c.start_ms)} disabled={!!me?.permissions.expired}>
                      <span className="ab-chapters__n t-mono-sm">{i + 1}</span>
                      <span className="ab-chapters__title">{c.title ?? t("player.chapterN", { n: i + 1 })}</span>
                      <span className="t-mono-sm subtle">{clock(Math.max(0, end - c.start_ms))}</span>
                    </button>
                  </li>
                );
              })}
            </ol>
            {chapters.length > CHAPTERS_SHORT && (
              <button type="button" className="pl-btn pl-btn--ghost pl-btn--sm" style={{ alignSelf: "flex-start" }} onClick={() => setAllChapters(!allChapters)}>
                {allChapters ? t("audio.lessChapters") : t("audio.allChapters", { count: chapters.length })}
              </button>
            )}
          </section>
        )}

        {series && seriesBooks && seriesBooks.length > 1 && (
          <section className="stack">
            <div className="section__head"><h2 className="t-h3">{series.title}</h2><Link to={`/item/${series.id}`} className="t-label">{t("common.all")}</Link></div>
            <div className="poster-row poster-row--square">{seriesBooks.map((b) => <PosterCard key={b.id} item={b} />)}</div>
          </section>
        )}

        {firstAuthor && others.length > 0 && (
          <section className="stack">
            <div className="section__head"><h2 className="t-h3">{t("audio.moreBy", { name: firstAuthor.name })}</h2><Link to={`/person/${firstAuthor.id}`} className="t-label">{t("common.all")}</Link></div>
            <div className="poster-row poster-row--square">{others.map((b) => <PosterCard key={b.id} item={b} />)}</div>
          </section>
        )}

        {files.length > 0 && (
          <section className="stack">
            <div className="row row--between">
              <h2 className="t-h3">{t("item.technical")}</h2>
              {files.length > 1 && (
                <select className="select t-small" style={{ flex: "none" }} value={fileIdx} onChange={(e) => setFileIdx(Number(e.target.value))} aria-label={t("audio.part")}>
                  {files.map((f, i) => <option key={f.id} value={i}>{f.name}</option>)}
                </select>
              )}
            </div>
            {file && <TechInfo file={file} />}
          </section>
        )}
      </div>
      {dialog === "identify" && <IdentifyDialog item={item} onClose={() => setDialog(null)} />}
      {dialog === "edit" && <EditDialog item={item} onClose={() => setDialog(null)} />}
      {dialog === "playlist" && <AddToPlaylist itemId={item.id} onClose={() => setDialog(null)} />}
    </div>
  );
}
