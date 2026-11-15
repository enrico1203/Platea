import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ListVideo, Play, Plus, Pencil, Trash2, ArrowUp, ArrowDown, X } from "lucide-react";
import { api, imageUrl } from "../api/client";
import type { PlaylistDetail, PlaylistSummary } from "../api/types";
import { Confirm, Dialog, Empty, TextField, toast, Switch } from "../components/ui";
import { useI18n } from "../i18n";

export function usePlaylists() {
  return useQuery({ queryKey: ["playlists"], queryFn: () => api<{ items: PlaylistSummary[] }>("/api/v1/playlists").then((r) => r.items) });
}

/** Indirizzo del player dentro una playlist: alla fine di ogni voce si passa alla successiva. */
export function playlistPlayUrl(playlistId: string, index: number, itemId: string) {
  return `/play/${itemId}?pl=${playlistId}&i=${index}`;
}

function NameDialog({ title, initial, onSave, onClose }: { title: string; initial?: string; onSave: (name: string) => Promise<void>; onClose: () => void }) {
  const { t } = useI18n();
  const [name, setName] = useState(initial ?? "");
  return (
    <Dialog title={title} onClose={onClose} footer={<>
      <button className="pl-btn pl-btn--ghost" onClick={onClose}>{t("common.cancel")}</button>
      <button className="pl-btn pl-btn--primary" disabled={!name.trim()} onClick={async () => { await onSave(name.trim()); onClose(); }}>{t("common.save")}</button>
    </>}>
      <TextField label={t("playlist.name")} value={name} onChange={(e) => setName(e.target.value)} autoFocus onKeyDown={async (e) => { if (e.key === "Enter" && name.trim()) { await onSave(name.trim()); onClose(); } }} />
    </Dialog>
  );
}

/** Elenco delle playlist dell'utente. */
export default function Playlists() {
  const { t, duration } = useI18n();
  const qc = useQueryClient();
  const nav = useNavigate();
  const { data } = usePlaylists();
  const [creating, setCreating] = useState(false);
  return (
    <div className="stack-lg">
      <div className="page-head" style={{ marginBottom: 0 }}>
        <h1 className="t-h1">{t("nav.playlists")}</h1>
        <button className="pl-btn pl-btn--secondary" onClick={() => setCreating(true)}><Plus />{t("playlist.new")}</button>
      </div>
      {data && data.length === 0 && <Empty>{t("playlist.none")}</Empty>}
      {data && data.length > 0 && (
        <div className="poster-grid">
          {data.map((p) => (
            <Link key={p.id} to={`/playlist/${p.id}`} className="pl-poster">
              <div className="pl-poster__art playlist-art">
                {p.poster ? <img src={imageUrl(p.poster, 300)} alt="" loading="lazy" /> : <ListVideo />}
              </div>
              <div className="pl-poster__title">{p.name}</div>
              <div className="pl-poster__meta">{[p.mine === false ? t("playlist.by", { owner: p.owner ?? "" }) : p.shared ? t("playlist.sharedShort") : null, t("playlist.count", { count: p.count }), p.runtime_ms ? duration(p.runtime_ms) : null].filter(Boolean).join(" · ")}</div>
            </Link>
          ))}
        </div>
      )}
      {creating && <NameDialog title={t("playlist.new")} onClose={() => setCreating(false)} onSave={async (name) => {
        const r = await api<{ id: string }>("/api/v1/playlists", { body: { name } });
        qc.invalidateQueries({ queryKey: ["playlists"] });
        nav(`/playlist/${r.id}`);
      }} />}
    </div>
  );
}

/** Una playlist: voci in ordine, riproduzione in sequenza, riordino. */
export function PlaylistPage() {
  const { id } = useParams();
  const { t, duration } = useI18n();
  const qc = useQueryClient();
  const nav = useNavigate();
  const { data } = useQuery({ queryKey: ["playlist", id], queryFn: () => api<PlaylistDetail>(`/api/v1/playlists/${id}`) });
  const [dialog, setDialog] = useState<"rename" | "delete" | null>(null);
  if (!data) return null;
  const refresh = () => { qc.invalidateQueries({ queryKey: ["playlist", id] }); qc.invalidateQueries({ queryKey: ["playlists"] }); };
  const move = async (entry: string, index: number) => { await api(`/api/v1/playlists/${id}/move`, { body: { entry_id: entry, index } }); refresh(); };
  return (
    <div className="stack-lg">
      <div className="page-head" style={{ marginBottom: 0 }}>
        <div>
          <h1 className="t-h1">{data.name}</h1>
          <div className="page-head__sub t-small subtle">{[data.mine === false ? t("playlist.by", { owner: data.owner ?? "" }) : null, t("playlist.count", { count: data.items.length }), data.runtime_ms ? duration(data.runtime_ms) : null].filter(Boolean).join(" · ")}</div>
        </div>
        <div className="row">
          {data.items.length > 0 && <button className="pl-btn pl-btn--primary" onClick={() => nav(playlistPlayUrl(data.id, 0, data.items[0].id))}><Play fill="currentColor" />{t("playlist.playAll")}</button>}
          {data.mine !== false && <>
            <button className="icon-btn" onClick={() => setDialog("rename")} aria-label={t("playlist.rename")} title={t("playlist.rename")}><Pencil /></button>
            <button className="icon-btn" onClick={() => setDialog("delete")} aria-label={t("common.delete")} title={t("common.delete")}><Trash2 /></button>
          </>}
        </div>
      </div>
      {data.mine !== false && (
        <Switch checked={!!data.shared} onChange={async (v) => { await api(`/api/v1/playlists/${id}`, { method: "PATCH", body: { shared: v } }); refresh(); }} label={t("playlist.share")} hint={t("playlist.shareHint")} />
      )}
      {data.items.length === 0 && <Empty>{t("playlist.empty")}</Empty>}
      <div className="episodes">
        {data.items.map((e, i) => {
          const title = e.kind === "episode" ? `${e.series_title ?? ""} · S${e.parent_index_number} E${e.index_number}` : e.title;
          const sub = e.kind === "episode" ? e.title : [e.year, e.runtime_ms ? duration(e.runtime_ms) : null].filter(Boolean).join(" · ");
          const art = e.images.still ?? e.images.backdrop ?? e.images.poster;
          return (
            <div key={e.entry_id} className="episode episode--actions" onClick={() => nav(`/item/${e.id}`)} role="link" tabIndex={0} onKeyDown={(k) => k.key === "Enter" && nav(`/item/${e.id}`)}>
              <div className="episode__still">{art && <img src={imageUrl(art, 300)} alt="" loading="lazy" />}</div>
              <div style={{ minWidth: 0 }}>
                <div className="t-label">{i + 1}. {title}</div>
                <div className="t-small subtle">{sub}</div>
              </div>
              <div className="row" style={{ gap: 4, flexWrap: "nowrap" }} onClick={(ev) => ev.stopPropagation()}>
                <button className="icon-btn" onClick={() => nav(playlistPlayUrl(data.id, i, e.id))} aria-label={t("player.play")}><Play /></button>
                {data.mine !== false && <>
                  <button className="icon-btn" disabled={i === 0} onClick={() => move(e.entry_id, i - 1)} aria-label={t("playlist.up")}><ArrowUp /></button>
                  <button className="icon-btn" disabled={i === data.items.length - 1} onClick={() => move(e.entry_id, i + 1)} aria-label={t("playlist.down")}><ArrowDown /></button>
                  <button className="icon-btn" onClick={async () => { await api(`/api/v1/playlists/${id}/items/${e.entry_id}`, { method: "DELETE" }); refresh(); }} aria-label={t("playlist.remove")}><X /></button>
                </>}
              </div>
            </div>
          );
        })}
      </div>
      {dialog === "rename" && <NameDialog title={t("playlist.rename")} initial={data.name} onClose={() => setDialog(null)} onSave={async (name) => { await api(`/api/v1/playlists/${id}`, { method: "PATCH", body: { name } }); refresh(); }} />}
      {dialog === "delete" && <Confirm danger title={t("playlist.deleteTitle", { name: data.name })} message={t("playlist.deleteMessage")} confirmLabel={t("common.delete")} onClose={() => setDialog(null)} onConfirm={async () => {
        await api(`/api/v1/playlists/${id}`, { method: "DELETE" });
        qc.invalidateQueries({ queryKey: ["playlists"] });
        nav("/playlists");
      }} />}
    </div>
  );
}

/** Dialogo "Aggiungi a playlist" dal dettaglio di un film, episodio, stagione, serie o collezione. */
export function AddToPlaylist({ itemId, onClose }: { itemId: string; onClose: () => void }) {
  const { t } = useI18n();
  const qc = useQueryClient();
  const { data } = usePlaylists();
  const [name, setName] = useState("");
  const done = (added: number, playlist: string) => {
    qc.invalidateQueries({ queryKey: ["playlists"] });
    qc.invalidateQueries({ queryKey: ["playlist"] });
    toast(t("playlist.added", { count: added, name: playlist }));
    onClose();
  };
  return (
    <Dialog title={t("playlist.addTo")} onClose={onClose} footer={<button className="pl-btn pl-btn--ghost" onClick={onClose}>{t("common.cancel")}</button>}>
      {data && data.length > 0 && (
        <div className="pl-panel">
          {data.filter((p) => p.mine !== false).map((p) => (
            <button key={p.id} className="list-row" style={{ width: "100%", textAlign: "left", background: "none", border: 0, color: "inherit", cursor: "pointer" }} onClick={async () => {
              const r = await api<{ added: number }>(`/api/v1/playlists/${p.id}/items`, { body: { item_ids: [itemId] } });
              done(r.added, p.name);
            }}>
              <ListVideo size={20} />
              <div className="spacer"><div className="t-label">{p.name}</div><div className="t-small subtle">{t("playlist.count", { count: p.count })}</div></div>
            </button>
          ))}
        </div>
      )}
      <div className="row" style={{ alignItems: "flex-end", flexWrap: "nowrap" }}>
        <div className="spacer"><TextField label={t("playlist.newName")} value={name} onChange={(e) => setName(e.target.value)} /></div>
        <button className="pl-btn pl-btn--secondary" disabled={!name.trim()} onClick={async () => {
          const r = await api<{ added: number }>("/api/v1/playlists", { body: { name: name.trim(), item_ids: [itemId] } });
          done(r.added, name.trim());
        }}><Plus />{t("playlist.create")}</button>
      </div>
    </Dialog>
  );
}
