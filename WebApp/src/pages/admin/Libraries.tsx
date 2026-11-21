import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { BarChart3, Plus, RefreshCw, Pencil, Trash2, X } from "lucide-react";
import { api, ApiError } from "../../api/client";
import type { Library, Page } from "../../api/types";
import { Alert, Badge, Confirm, Dialog, Empty, SelectField, Switch, TextField, toast } from "../../components/ui";
import { FolderBrowser } from "../Setup";
import { useI18n } from "../../i18n";

function LibraryDialog({ lib, onClose }: { lib: Library | null; onClose: () => void }) {
  const { t } = useI18n();
  const qc = useQueryClient();
  const [name, setName] = useState(lib?.name ?? "");
  const [kind, setKind] = useState<"movies" | "shows" | "audiobooks">(lib?.kind ?? "movies");
  const [paths, setPaths] = useState<string[]>(lib?.paths ?? []);
  const [browse, setBrowse] = useState("");
  const [profile, setProfile] = useState(lib?.scan_profile ?? "local");
  const [language, setLanguage] = useState(lib?.language ?? "it-IT");
  const [interval, setIntervalV] = useState(String(lib?.scan_interval_minutes ?? (lib ? "" : 60)));
  const [conc, setConc] = useState(String(lib?.probe_concurrency ?? 4));
  const [perMin, setPerMin] = useState(String(lib?.probe_per_minute ?? ""));
  const [winStart, setWinStart] = useState(lib?.probe_window_start != null ? String(lib.probe_window_start) : "");
  const [winEnd, setWinEnd] = useState(lib?.probe_window_end != null ? String(lib.probe_window_end) : "");
  const [pause, setPause] = useState(lib?.probe_pause_on_play ?? false);
  const [kf, setKf] = useState(lib?.extract_keyframes ?? true);
  const [tp, setTp] = useState(lib?.trickplay ?? false);
  const [error, setError] = useState<string | null>(null);
  // Profilo Remoto: tutto ciò che legge i file per intero parte spento, con scansioni rade e un'analisi alla volta.
  // Per una libreria nuova si applicano i valori del profilo; per una esistente si spengono solo le letture complete.
  const changeProfile = (v: "local" | "remote") => {
    setProfile(v);
    if (v === "remote") { setKf(false); setTp(false); }
    if (!lib) {
      setIntervalV(v === "remote" ? "360" : "60");
      setConc(v === "remote" ? "1" : "4");
      if (v === "local") setKf(true);
    }
  };
  // Un valore non valido non diventa "vuoto" in silenzio: si ferma il salvataggio e si dice quale campo correggere.
  const n = (v: string) => (v.trim() === "" ? null : /^\d+$/.test(v.trim()) ? Number(v.trim()) : NaN);
  // Ore: "2", "02" o "2:00"; "2-5" nella prima casella riempie entrambe.
  const hour = (v: string) => {
    const m = v.trim().match(/^(\d{1,2})(?:[:.]00)?$/);
    return v.trim() === "" ? null : m && Number(m[1]) <= 23 ? Number(m[1]) : NaN;
  };
  const changeWinStart = (v: string) => {
    const r = v.match(/^\s*(\d{1,2})\s*[-–]\s*(\d{1,2})\s*$/);
    if (r) { setWinStart(r[1]); setWinEnd(r[2]); } else setWinStart(v);
  };
  const save = async () => {
    setError(null);
    const bad = [
      [interval, n(interval), "admin.libraries.interval"], [conc, n(conc), "admin.libraries.concurrency"], [perMin, n(perMin), "admin.libraries.perMinute"],
      [winStart, hour(winStart), "admin.libraries.windowStart"], [winEnd, hour(winEnd), "admin.libraries.windowEnd"],
    ].find(([, v]) => Number.isNaN(v));
    if (bad) { setError(t("admin.libraries.invalidNumber", { field: t(bad[2] as "admin.libraries.interval"), value: String(bad[0]) })); return; }
    if ((hour(winStart) === null) !== (hour(winEnd) === null)) { setError(t("admin.libraries.windowBoth")); return; }
    const body = { name, kind, paths, scan_profile: profile, language, country: language.split("-")[1], scan_interval_minutes: n(interval), probe_concurrency: n(conc) ?? 1, probe_per_minute: n(perMin), probe_window_start: hour(winStart), probe_window_end: hour(winEnd), probe_pause_on_play: pause, extract_keyframes: kf, trickplay: tp };
    try {
      if (lib) await api(`/api/v1/libraries/${lib.id}`, { method: "PATCH", body });
      else await api("/api/v1/libraries", { body });
      qc.invalidateQueries({ queryKey: ["libraries"] });
      onClose();
    } catch (e) { setError((e as ApiError).message); }
  };
  return (
    <Dialog title={lib ? t("admin.libraries.edit") : t("admin.libraries.add")} onClose={onClose} footer={<>
      <button className="pl-btn pl-btn--ghost" onClick={onClose}>{t("common.cancel")}</button>
      <button className="pl-btn pl-btn--primary" onClick={save} disabled={!name || paths.length === 0}>{t("common.save")}</button>
    </>}>
      {error && <Alert tone="danger">{error}</Alert>}
      <div className="form-grid">
        <TextField label={t("library.name")} value={name} onChange={(e) => setName(e.target.value)} />
        <SelectField label={t("library.kind")} value={kind} onChange={(v) => setKind(v as "movies" | "shows" | "audiobooks")}
          options={(lib && lib.kind !== "audiobooks" ? [{ value: "movies", label: t("library.movies") }, { value: "shows", label: t("library.shows") }]
            : lib ? [{ value: "audiobooks", label: t("library.audiobooks") }]
            : [{ value: "movies", label: t("library.movies") }, { value: "shows", label: t("library.shows") }, { value: "audiobooks", label: t("library.audiobooks") }])} />
      </div>
      {lib && kind !== lib.kind && <Alert tone="warning">{t("library.kindChange")}</Alert>}
      {kind === "audiobooks" && !lib && <div className="t-small subtle">{t("library.audiobooksHint")}</div>}
      <div className="stack" style={{ gap: 6 }}>
        <span className="pl-field__label">{t("admin.libraries.folders")}</span>
        {paths.map((p) => <div className="row" key={p}><span className="t-mono spacer">{p}</span><button className="icon-btn" onClick={() => setPaths(paths.filter((x) => x !== p))} aria-label={t("common.remove")}><X /></button></div>)}
      </div>
      <FolderBrowser value={browse} onChange={setBrowse} />
      <div><button className="pl-btn pl-btn--secondary pl-btn--sm" disabled={!browse || paths.includes(browse)} onClick={() => setPaths([...paths, browse])}>{t("admin.libraries.addFolder")}</button></div>
      <SelectField label={t("admin.libraries.profile")} value={profile} onChange={(v) => changeProfile(v as "local" | "remote")} hint={profile === "remote" ? t("library.remoteHint") : t("admin.libraries.localHint")} options={[{ value: "local", label: t("admin.libraries.local") }, { value: "remote", label: t("admin.libraries.remote") }]} />
      <div className="form-grid">
        <SelectField label={t("setup.metadataLanguage")} value={language} onChange={setLanguage} options={[{ value: "it-IT", label: "Italiano" }, { value: "en-US", label: "English (US)" }, { value: "en-GB", label: "English (UK)" }, { value: "de-DE", label: "Deutsch" }, { value: "fr-FR", label: "Français" }, { value: "es-ES", label: "Español" }]} />
        <TextField label={t("admin.libraries.interval")} inputMode="numeric" value={interval} onChange={(e) => setIntervalV(e.target.value)} hint={t("admin.libraries.intervalHint")} />
        <TextField label={t("admin.libraries.concurrency")} inputMode="numeric" value={conc} onChange={(e) => setConc(e.target.value)} />
        <TextField label={t("admin.libraries.perMinute")} inputMode="numeric" value={perMin} onChange={(e) => setPerMin(e.target.value)} hint={t("admin.libraries.perMinuteHint")} />
        <TextField label={t("admin.libraries.windowStart")} inputMode="numeric" value={winStart} onChange={(e) => changeWinStart(e.target.value)} hint={t("admin.libraries.windowHint")} />
        <TextField label={t("admin.libraries.windowEnd")} inputMode="numeric" value={winEnd} onChange={(e) => setWinEnd(e.target.value)} hint="0–23" />
      </div>
      <Switch checked={pause} onChange={setPause} label={t("admin.libraries.pauseOnPlay")} />
      <Switch checked={kf} onChange={setKf} label={t("admin.libraries.keyframes")} hint={t("admin.libraries.keyframesHint")} />
      <Switch checked={tp} onChange={setTp} label={t("admin.libraries.trickplay")} hint={t(profile === "remote" ? "admin.libraries.trickplayHintRemote" : "admin.libraries.trickplayHint")} />
    </Dialog>
  );
}

export default function Libraries() {
  const { t, relative, num } = useI18n();
  const qc = useQueryClient();
  const { data, isLoading, isError, error, refetch } = useQuery({ queryKey: ["libraries", "admin"], queryFn: () => api<Page<Library>>("/api/v1/libraries").then((r) => r.items) });
  const [edit, setEdit] = useState<Library | null | "new">(null);
  const [del, setDel] = useState<Library | null>(null);
  const [confirmMissing, setConfirmMissing] = useState<Library | null>(null);
  return (
    <div className="stack-lg">
      <div className="page-head" style={{ marginBottom: 0 }}>
        <h1 className="t-h1">{t("admin.libraries.title")}</h1>
        <div className="row" style={{ flexWrap: "wrap" }}>
          <Link to="/admin/libraries/stats" className="pl-btn pl-btn--secondary"><BarChart3 />{t("admin.stats.button")}</Link>
          <button className="pl-btn pl-btn--secondary" onClick={async () => { await api("/api/v1/libraries/scan-all", { method: "POST" }); toast(t("admin.libraries.scanQueued")); }}><RefreshCw />{t("admin.libraries.scanAll")}</button>
          <button className="pl-btn pl-btn--primary" onClick={() => setEdit("new")}><Plus />{t("admin.libraries.add")}</button>
        </div>
      </div>
      {isLoading && <div className="t-small subtle" role="status">{t("common.loading")}</div>}
      {isError && (
        <Alert tone="danger">
          {(error as ApiError)?.message || t("errors.network")}{" "}
          <button className="pl-btn pl-btn--ghost" onClick={() => refetch()}>{t("common.retry")}</button>
        </Alert>
      )}
      {data?.length === 0 && <Empty>{t("home.emptyAdmin")}</Empty>}
      <div className="stack">
        {data?.map((l) => (
          <div className="pl-panel panel-pad stack" key={l.id}>
            <div className="row row--between">
              <div>
                <div className="t-h3">{l.name} <Badge>{l.kind === "movies" ? t("library.movies") : l.kind === "audiobooks" ? t("library.audiobooks") : t("library.shows")}</Badge> <Badge>{l.scan_profile === "remote" ? t("admin.libraries.remote") : t("admin.libraries.local")}</Badge></div>
                <div className="t-small subtle">{t(l.kind === "movies" ? "library.countMovies" : l.kind === "audiobooks" ? "library.countAudiobooks" : "library.countShows", { count: l.item_count })} · {t("admin.libraries.files", { count: l.file_count ?? 0 })}{l.probe_pending ? ` · ${t("admin.libraries.probePending", { count: l.probe_pending })}` : ""}{l.trickplay ? ` · ${t("admin.libraries.trickplayDone", { done: l.trickplay_done ?? 0, total: l.file_count ?? 0 })}` : ""}</div>
              </div>
              <div className="row">
                <button className="pl-btn pl-btn--secondary pl-btn--sm" onClick={async () => { await api(`/api/v1/libraries/${l.id}/scan`, { method: "POST" }); toast(t("admin.libraries.scanQueued")); }}><RefreshCw />{t("admin.libraries.scan")}</button>
                <button className="pl-btn pl-btn--ghost pl-btn--sm" onClick={async () => { const r = await api<{ queued: number }>(`/api/v1/libraries/${l.id}/refresh-metadata`, { method: "POST" }); toast(t("admin.libraries.refreshMetadataQueued", { count: r.queued })); }}>{t("admin.libraries.refreshMetadata")}</button>
                <button className="icon-btn" onClick={() => setEdit(l)} aria-label={t("common.edit")}><Pencil /></button>
                <button className="icon-btn" onClick={() => setDel(l)} aria-label={t("common.delete")}><Trash2 /></button>
              </div>
            </div>
            <div className="t-mono subtle">{l.paths?.join(" · ")}</div>
            {l.last_scan_status === "mount_missing" ? (
              <Alert tone="danger">{t("admin.libraries.mountMissing")} <span className="t-mono-sm">{l.last_scan_message}</span></Alert>
            ) : l.last_scan_status === "missing_held" ? (
              <Alert tone="danger">
                {t("admin.libraries.missingHeld", { count: l.missing_held ?? 0 })}
                <div className="row" style={{ marginTop: 8 }}>
                  <button className="pl-btn pl-btn--secondary pl-btn--sm" onClick={async () => { await api(`/api/v1/libraries/${l.id}/scan`, { method: "POST" }); toast(t("admin.libraries.scanQueued")); }}><RefreshCw />{t("admin.libraries.rescan")}</button>
                  <button className="pl-btn pl-btn--danger pl-btn--sm" onClick={() => setConfirmMissing(l)}>{t("admin.libraries.confirmMissing")}</button>
                </div>
              </Alert>
            ) : l.last_scan_at ? (
              <div className="t-small muted">{t("admin.libraries.lastScan", { when: relative(l.last_scan_at) })}: <span className="t-mono-sm">{l.last_scan_message}</span></div>
            ) : (
              <div className="t-small subtle">{t("admin.libraries.neverScanned")}</div>
            )}
            {l.scan_interval_minutes ? <div className="t-small subtle">{t("admin.libraries.every", { min: num(l.scan_interval_minutes) })}</div> : null}
          </div>
        ))}
      </div>
      {edit && <LibraryDialog lib={edit === "new" ? null : edit} onClose={() => setEdit(null)} />}
      {confirmMissing && <Confirm danger title={t("admin.libraries.confirmMissingTitle", { name: confirmMissing.name })} message={t("admin.libraries.confirmMissingMessage", { count: confirmMissing.missing_held ?? 0 })} confirmLabel={t("admin.libraries.confirmMissing")} onClose={() => setConfirmMissing(null)} onConfirm={async () => { await api(`/api/v1/libraries/${confirmMissing.id}/confirm-missing`, { method: "POST" }); toast(t("admin.libraries.scanQueued")); qc.invalidateQueries(); }} />}
      {del && <Confirm danger title={t("admin.libraries.deleteTitle", { name: del.name })} message={t("admin.libraries.deleteMessage")} confirmLabel={t("common.delete")} onClose={() => setDel(null)} onConfirm={async () => { await api(`/api/v1/libraries/${del.id}`, { method: "DELETE" }); qc.invalidateQueries(); }} />}
    </div>
  );
}
