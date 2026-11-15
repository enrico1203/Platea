import { useEffect, useRef, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Play, Check, Heart, SearchCheck, RefreshCw, Pencil, ListPlus, Download, Upload } from "lucide-react";
import { AddToPlaylist } from "./Playlists";
import { api, ApiError, getToken, imageUrl, mediaUrl, serverUrl } from "../api/client";
import type { Candidate, Item as ItemT, ItemDetail, MediaFile, Page } from "../api/types";
import { PosterCard } from "../components/PosterCard";
import { Alert, Badge, Dialog, TextField, toast } from "../components/ui";
import { useSession } from "../session";
import { useI18n } from "../i18n";

function channels(n: number | null) {
  if (!n) return "";
  return n > 2 ? `${n - 1}.1` : `${n}.0`;
}

function TechInfo({ file }: { file: MediaFile }) {
  const { t, bytes, bitrate, duration } = useI18n();
  const v = file.video[0];
  return (
    <dl className="kv t-small">
      <dt>{t("item.file")}</dt><dd className="t-mono">{file.name}</dd>
      <dt>{t("item.container")}</dt><dd className="t-mono">{file.container?.toUpperCase() ?? "—"} · {bytes(file.size)} · {bitrate(file.bitrate)} · {duration(file.duration_ms)}</dd>
      {v && (<><dt>{t("item.video")}</dt><dd className="t-mono">{[v.codec.toUpperCase(), v.profile, v.width && v.height ? `${v.width}×${v.height}` : null, v.bit_depth ? `${v.bit_depth} bit` : null, v.hdr?.toUpperCase(), v.frame_rate ? `${v.frame_rate.toFixed(3).replace(/\.?0+$/, "")} fps` : null].filter(Boolean).join(" · ")}</dd></>)}
      {file.audio.length > 0 && (<><dt>{t("item.audio")}</dt><dd><div className="chips">{file.audio.map((a) => <Badge tone="codec" key={a.index}>{[a.language?.toUpperCase(), a.codec.toUpperCase(), channels(a.channels)].filter(Boolean).join(" ")}</Badge>)}</div></dd></>)}
      {file.subtitles.length > 0 && (<><dt>{t("item.subtitles")}</dt><dd><div className="chips">{file.subtitles.map((s) => <Badge tone="codec" key={s.index}>{[s.language?.toUpperCase() ?? "?", s.codec.replace("hdmv_pgs_subtitle", "pgs").toUpperCase(), s.forced ? t("item.forced") : null, s.external ? t("item.external") : null].filter(Boolean).join(" ")}</Badge>)}</div></dd></>)}
      {file.probe_state !== "done" && (<><dt>{t("item.probe")}</dt><dd>{file.probe_state === "failed" ? <span className="danger-text">{file.probe_error}</span> : t("item.probePending")}</dd></>)}
    </dl>
  );
}

function IdentifyDialog({ item, onClose }: { item: ItemDetail; onClose: () => void }) {
  const { t, num } = useI18n();
  const qc = useQueryClient();
  const [q, setQ] = useState(item.title);
  const [year, setYear] = useState(item.year ? String(item.year) : "");
  const [list, setList] = useState<Candidate[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const search = async (custom: boolean) => {
    setError(null);
    try {
      const p = custom ? `?q=${encodeURIComponent(q)}${year ? `&year=${year}` : ""}` : "";
      setList((await api<{ items: Candidate[] }>(`/api/v1/items/${item.id}/candidates${p}`)).items);
    } catch (e) { setError((e as ApiError).message); }
  };
  useEffect(() => { search(false); // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return (
    <Dialog title={t("item.identify")} onClose={onClose}>
      <form className="row" onSubmit={(e) => { e.preventDefault(); search(true); }} style={{ alignItems: "flex-end" }}>
        <div style={{ flex: 3 }}><TextField label={t("item.title")} value={q} onChange={(e) => setQ(e.target.value)} /></div>
        <div style={{ flex: 1 }}><TextField label={t("item.year")} inputMode="numeric" value={year} onChange={(e) => setYear(e.target.value)} /></div>
        <button className="pl-btn pl-btn--secondary">{t("common.search")}</button>
      </form>
      {error && <Alert tone="danger">{error}</Alert>}
      <div className="stack" style={{ gap: 4 }}>
        {list?.map((c) => (
          <div className="cand" key={c.provider_id}>
            {c.poster ? <img src={c.poster} alt="" /> : <div className="ph" />}
            <div>
              <div className="t-label">{c.title} {c.year && <span className="subtle">({c.year})</span>}</div>
              <div className="t-mono-sm subtle">TMDB {c.provider_id} · {t("item.score")} {num(c.score, 2)}</div>
            </div>
            <button className="pl-btn pl-btn--secondary pl-btn--sm" onClick={async () => {
              try {
                await api(`/api/v1/items/${item.id}/identify`, { body: { provider_id: c.provider_id } });
                qc.invalidateQueries();
                toast(t("item.identified"));
                onClose();
              } catch (e) { setError((e as ApiError).message); }
            }}>{t("item.choose")}</button>
          </div>
        ))}
        {list && list.length === 0 && <p className="muted">{t("item.noCandidates")}</p>}
      </div>
    </Dialog>
  );
}

function EditDialog({ item, onClose }: { item: ItemDetail; onClose: () => void }) {
  const { t } = useI18n();
  const qc = useQueryClient();
  const [title, setTitle] = useState(item.title);
  const [year, setYear] = useState(item.year ? String(item.year) : "");
  const [overview, setOverview] = useState(item.overview ?? "");
  return (
    <Dialog title={t("item.edit")} onClose={onClose} footer={<>
      <button className="pl-btn pl-btn--ghost" onClick={onClose}>{t("common.cancel")}</button>
      <button className="pl-btn pl-btn--primary" onClick={async () => {
        await api(`/api/v1/items/${item.id}`, { method: "PATCH", body: { title, year: year ? Number(year) : undefined, overview } });
        qc.invalidateQueries();
        onClose();
      }}>{t("common.save")}</button>
    </>}>
      <p className="t-small subtle" style={{ margin: 0 }}>{t("item.lockHint")}</p>
      <TextField label={t("item.title")} value={title} onChange={(e) => setTitle(e.target.value)} />
      <TextField label={t("item.year")} inputMode="numeric" value={year} onChange={(e) => setYear(e.target.value)} />
      <label className="pl-field"><span className="pl-field__label">{t("item.overview")}</span><span className="pl-field__control is-area"><textarea rows={5} value={overview} onChange={(e) => setOverview(e.target.value)} /></span></label>
      <span className="pl-eyebrow">{t("item.images")}</span>
      <div className="image-slots">
        {(["poster", "backdrop", "logo"] as const).map((k) => <ImageSlot key={k} item={item} kind={k} />)}
      </div>
    </Dialog>
  );
}

/** Una delle immagini dell'elemento: si sostituisce caricando un file, si torna a quella dei metadati. */
function ImageSlot({ item, kind }: { item: ItemDetail; kind: "poster" | "backdrop" | "logo" }) {
  const { t } = useI18n();
  const qc = useQueryClient();
  const input = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const src = kind === "poster" ? item.images.poster : kind === "backdrop" ? item.images.backdrop : item.images.logo;
  const custom = item.locked_fields?.includes(kind);
  return (
    <div className={`image-slot image-slot--${kind}`}>
      <div className="image-slot__art">{src ? <img src={imageUrl(src, 300)} alt="" /> : <span className="t-small subtle">{t("item.noImage")}</span>}</div>
      <div className="t-label">{t(`item.imageKind.${kind}` as "item.imageKind.poster")}</div>
      <div className="row" style={{ gap: 4 }}>
        <button type="button" className="pl-btn pl-btn--secondary pl-btn--sm" disabled={busy} onClick={() => input.current?.click()}><Upload />{t("item.uploadImage")}</button>
        {custom && <button type="button" className="pl-btn pl-btn--ghost pl-btn--sm" disabled={busy} onClick={async () => {
          await api(`/api/v1/items/${item.id}/images/${kind}`, { method: "DELETE" });
          qc.invalidateQueries();
          toast(t("item.imageReset"));
        }}>{t("item.resetImage")}</button>}
      </div>
      <input ref={input} type="file" accept="image/jpeg,image/png,image/webp" hidden onChange={async (e) => {
        const f = e.target.files?.[0];
        e.target.value = "";
        if (!f) return;
        setBusy(true);
        try {
          const res = await fetch(serverUrl() + `/api/v1/items/${item.id}/images/${kind}`, { method: "POST", headers: { Authorization: `Bearer ${getToken() ?? ""}` }, body: f });
          if (!res.ok) throw new Error(((await res.json().catch(() => ({}))) as { message?: string }).message ?? res.statusText);
          qc.invalidateQueries();
        } catch (err) { toast((err as Error).message); } finally { setBusy(false); }
      }} />
    </div>
  );
}

export default function ItemPage() {
  const { id } = useParams();
  const nav = useNavigate();
  const qc = useQueryClient();
  const { me } = useSession();
  const { t, duration, date, num } = useI18n();
  const [season, setSeason] = useState<string | null>(null);
  const [fileIdx, setFileIdx] = useState(0);
  const [dialog, setDialog] = useState<"identify" | "edit" | "playlist" | null>(null);
  const { data: item, error } = useQuery({ queryKey: ["item", id], queryFn: () => api<ItemDetail>(`/api/v1/items/${id}`) });
  const seriesId = item?.kind === "series" ? item.id : item?.kind === "season" ? item.parent_id : null;
  const { data: seasons } = useQuery({ queryKey: ["items", "children", seriesId], queryFn: () => api<Page<ItemT>>(`/api/v1/items/${seriesId}/children`).then((r) => r.items), enabled: !!seriesId });
  const currentSeason = season ?? (item?.kind === "season" ? item.id : seasons?.find((s) => (s.unplayed_count ?? 0) > 0 && (s.index_number ?? 0) > 0)?.id ?? seasons?.[0]?.id ?? null);
  const { data: episodes } = useQuery({ queryKey: ["items", "children", currentSeason], queryFn: () => api<Page<ItemT>>(`/api/v1/items/${currentSeason}/children`).then((r) => r.items), enabled: !!currentSeason });
  const similarFor = item && (item.kind === "movie" || item.kind === "series") ? item.id : null;
  const { data: similar } = useQuery({ queryKey: ["items", "similar", similarFor], queryFn: () => api<Page<ItemT>>(`/api/v1/items/${similarFor}/similar`).then((r) => r.items), enabled: !!similarFor });
  const { data: members } = useQuery({ queryKey: ["items", "children", id, "members"], queryFn: () => api<Page<ItemT>>(`/api/v1/items/${id}/children`).then((r) => r.items), enabled: item?.kind === "collection" });
  useEffect(() => { setSeason(null); setFileIdx(0); }, [id]);

  if (error) return <Alert tone="danger">{(error as ApiError).message}</Alert>;
  if (!item) return null;

  const isPlayable = item.kind === "movie" || item.kind === "episode";
  const nextEpisode = episodes?.find((e) => !e.user_data.played) ?? episodes?.[0];
  const resume = isPlayable && item.user_data.position_ms > 0 && !item.user_data.played;
  const files = item.files ?? [];
  const file = files[fileIdx];
  const toggle = async (what: "played" | "favorite", on: boolean) => {
    await api(`/api/v1/items/${item.id}/${what}`, { method: on ? "POST" : "DELETE" });
    qc.invalidateQueries();
  };
  const play = () => {
    if (isPlayable) nav(`/play/${item.id}${file ? `?file=${file.id}` : ""}`);
    else if (nextEpisode) nav(`/play/${nextEpisode.id}`);
  };
  const meta = [
    item.kind === "episode" ? `S${item.parent_index_number} · E${item.index_number}` : null,
    item.year,
    item.runtime_ms ? duration(item.runtime_ms) : null,
    item.official_rating,
    item.community_rating ? `★ ${num(item.community_rating, 1)}` : null,
    item.kind === "series" && seasons ? t("item.seasons", { count: seasons.filter((s) => (s.index_number ?? 0) > 0).length }) : null,
    item.kind === "collection" && members ? t("item.collectionCount", { count: members.length }) : null,
  ].filter(Boolean);

  return (
    <div className="detail">
      <div className="detail__backdrop">{(item.images.backdrop || item.images.still) && <img src={imageUrl(item.images.backdrop ?? item.images.still, 1280)} alt="" />}</div>
      <div className="detail__body">
        <div className="detail__poster">
          <div className="pl-poster__art" style={{ background: item.images.poster_color ?? undefined }}>{item.images.poster && <img src={imageUrl(item.images.poster, 400)} alt="" />}</div>
        </div>
        <div className="detail__info">
          {item.series_title && item.series_id && <Link to={`/item/${item.series_id}`} className="t-label">{item.series_title}</Link>}
          {item.images.logo && item.kind !== "episode" ? <img className="detail__logo" src={imageUrl(item.images.logo, 500)} alt={item.title} /> : <h1 className="t-display">{item.title}</h1>}
          <div className="detail__meta t-small">{meta.map((m, i) => <span key={i}>{m}</span>)}</div>
          {item.genres.length > 0 && <div className="chips">{item.genres.map((g) => <Link key={g} to={`/browse?genre=${encodeURIComponent(g)}`} className="chip-link"><Badge>{g}</Badge></Link>)}</div>}
          {item.studios.length > 0 && item.kind !== "episode" && (
            <div className="t-small muted">{t("item.studios")} {item.studios.slice(0, 4).map((s, i) => <span key={s}>{i > 0 && ", "}<Link to={`/browse?studio=${encodeURIComponent(s)}`}>{s}</Link></span>)}</div>
          )}
          {item.collections && item.collections.length > 0 && (
            <div className="t-small muted">{t("item.partOf")} {item.collections.map((c, i) => <span key={c.id}>{i > 0 && ", "}<Link to={`/item/${c.id}`}>{c.title}</Link></span>)}</div>
          )}
          {me?.permissions.expired && <Alert tone="warning">{t("item.expired")}</Alert>}
          <div className="detail__actions">
            {(isPlayable || nextEpisode) && !me?.permissions.expired && (
              <>
                <button className="pl-btn pl-btn--play" onClick={play} aria-label={t("player.play")}><Play fill="currentColor" /></button>
                <span className="t-label">
                  {resume ? t("item.resumeFrom", { time: duration(item.user_data.position_ms) }) : isPlayable ? t("player.play") : nextEpisode ? `${t("player.play")} S${nextEpisode.parent_index_number} · E${nextEpisode.index_number}` : ""}
                </span>
              </>
            )}
            <span className="spacer" />
            {item.kind !== "collection" && <>
            <button className={`icon-btn${item.user_data.played ? " on" : ""}`} onClick={() => toggle("played", !item.user_data.played)} aria-label={item.user_data.played ? t("item.markUnplayed") : t("item.markPlayed")} title={item.user_data.played ? t("item.markUnplayed") : t("item.markPlayed")}><Check /></button>
            <button className={`icon-btn${item.user_data.favorite ? " on" : ""}`} onClick={() => toggle("favorite", !item.user_data.favorite)} aria-label={t("item.favorite")} title={t("item.favorite")}><Heart fill={item.user_data.favorite ? "currentColor" : "none"} /></button>
            </>}
            {isPlayable && me?.permissions.allow_download && !me.permissions.expired && (
              <button className="icon-btn" aria-label={t("item.download")} title={t("item.download")} onClick={async () => {
                try {
                  // Link firmato: il browser scarica il file originale direttamente dal server, con ripresa.
                  const r = await api<{ url: string; name: string; size: number }>(`/api/v1/items/${item.id}/download`, { body: { file_id: file?.id } });
                  const a = document.createElement("a");
                  a.href = mediaUrl(r.url)!;
                  a.download = r.name;
                  document.body.appendChild(a);
                  a.click();
                  a.remove();
                  toast(t("item.downloadStarted", { name: r.name }));
                } catch (e) { toast((e as ApiError).message); }
              }}><Download /></button>
            )}
            <button className="icon-btn" onClick={() => setDialog("playlist")} aria-label={t("playlist.addTo")} title={t("playlist.addTo")}><ListPlus /></button>
            {me?.is_admin && (item.kind === "movie" || item.kind === "series") && (
              <>
                <button className="icon-btn" onClick={() => setDialog("identify")} aria-label={t("item.identify")} title={t("item.identify")}><SearchCheck /></button>
                <button className="icon-btn" onClick={async () => { await api(`/api/v1/items/${item.id}/refresh`, { method: "POST" }); toast(t("item.refreshQueued")); }} aria-label={t("item.refresh")} title={t("item.refresh")}><RefreshCw /></button>
              </>
            )}
            {me?.is_admin && <button className="icon-btn" onClick={() => setDialog("edit")} aria-label={t("item.edit")} title={t("item.edit")}><Pencil /></button>}
          </div>
          {item.tagline && <div className="t-label muted">{item.tagline}</div>}
          {item.overview && <p className="t-body-lg muted" style={{ margin: 0 }}>{item.overview}</p>}
          {item.match_state === "review" && me?.is_admin && <Alert tone="warning">{t("item.needsReview")}</Alert>}
        </div>
      </div>

      <div className="detail__content stack-lg">
        {members && members.length > 0 && (
          <section className="stack">
            <h2 className="t-h3">{t("item.collectionMovies")}</h2>
            <div className="poster-grid">{members.map((m) => <PosterCard key={m.id} item={m} />)}</div>
          </section>
        )}

        {seasons && seasons.length > 0 && (
          <section>
            <div className="tabs" role="tablist">
              {seasons.map((s) => (
                <button key={s.id} role="tab" aria-selected={s.id === currentSeason} className={`tab${s.id === currentSeason ? " active" : ""}`} onClick={() => setSeason(s.id)}>{s.title}</button>
              ))}
            </div>
            <div className="episodes">
              {episodes?.map((e) => (
                <div key={e.id} className="episode" onClick={() => nav(`/item/${e.id}`)} role="link" tabIndex={0} onKeyDown={(k) => k.key === "Enter" && nav(`/item/${e.id}`)}>
                  <div className="episode__still">
                    {(e.images.still) && <img src={imageUrl(e.images.still, 300)} alt="" loading="lazy" />}
                    {e.runtime_ms && e.user_data.position_ms > 0 && !e.user_data.played && <div className="pl-poster__progress"><span style={{ width: `${Math.min(100, (e.user_data.position_ms / e.runtime_ms) * 100)}%` }} /></div>}
                    {e.user_data.played && <div className="pl-poster__watched"><Check strokeWidth={3} /></div>}
                  </div>
                  <div style={{ minWidth: 0 }}>
                    <div className="t-label">{e.index_number}. {e.title}</div>
                    <div className="t-small subtle">{[e.premiere_date ? date(e.premiere_date) : null, e.runtime_ms ? duration(e.runtime_ms) : null].filter(Boolean).join(" · ")}</div>
                    {e.overview && <div className="t-small muted" style={{ marginTop: 4, display: "-webkit-box", WebkitLineClamp: 2, WebkitBoxOrient: "vertical", overflow: "hidden" }}>{e.overview}</div>}
                  </div>
                  <button className="icon-btn" onClick={(ev) => { ev.stopPropagation(); nav(`/play/${e.id}`); }} aria-label={t("player.play")}><Play /></button>
                </div>
              ))}
            </div>
          </section>
        )}

        {files.length > 0 && (
          <section className="stack">
            <div className="row row--between">
              <h2 className="t-h3">{t("item.technical")}</h2>
              {files.length > 1 && (
                <select className="select t-small" style={{ flex: "none" }} value={fileIdx} onChange={(e) => setFileIdx(Number(e.target.value))} aria-label={t("item.version")}>
                  {files.map((f, i) => <option key={f.id} value={i}>{f.version_label ?? f.name}</option>)}
                </select>
              )}
            </div>
            {file && <TechInfo file={file} />}
          </section>
        )}

        {item.people.length > 0 && (
          <section className="stack">
            <h2 className="t-h3">{t("item.cast")}</h2>
            <div className="people">
              {item.people.map((p, i) => (
                <Link className="person" key={`${p.name}-${p.role}-${i}`} to={`/person/${p.id}`}>
                  <div className="person__img">{p.image && <img src={imageUrl(p.image, 185)} alt="" loading="lazy" />}</div>
                  <div className="t-small">{p.name}</div>
                  <div className="t-small subtle" style={{ marginTop: -6 }}>{p.role === "actor" ? p.character : t(`item.role.${p.role}` as "item.role.director")}</div>
                </Link>
              ))}
            </div>
          </section>
        )}

        {similar && similar.length > 0 && (
          <section className="stack">
            <h2 className="t-h3">{t("item.similar")}</h2>
            <div className="poster-row">{similar.map((s) => <PosterCard key={s.id} item={s} />)}</div>
          </section>
        )}

        {item.kind === "season" && episodes === undefined && <PosterCard item={item} />}
      </div>
      {dialog === "identify" && <IdentifyDialog item={item} onClose={() => setDialog(null)} />}
      {dialog === "edit" && <EditDialog item={item} onClose={() => setDialog(null)} />}
      {dialog === "playlist" && <AddToPlaylist itemId={item.id} onClose={() => setDialog(null)} />}
    </div>
  );
}
