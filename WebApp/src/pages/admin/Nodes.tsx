import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, Pencil, Trash2, RefreshCw, FlaskConical, ScrollText, Power, Square, PauseCircle, PlayCircle, FolderOpen, ChevronLeft } from "lucide-react";
import { api, ApiError } from "../../api/client";
import { Alert, Badge, Confirm, Dialog, Empty, SelectField, Switch, TextField, toast } from "../../components/ui";
import { CapsMatrix, type Capabilities } from "../../components/CapsMatrix";
import { useI18n, type MessageKey } from "../../i18n";
import { useSession } from "../../session";

type Health = "unknown" | "online" | "offline" | "incompatible" | "unauthorized";
interface Rules { min_height: number; max_height: number; hdr: "any" | "only_hdr" | "only_sdr"; codecs: string[]; libraries: number[]; transcode: boolean; remux: boolean; require_hw: boolean }
interface NodeConfig { hw_backend: string; vaapi_device: string; max_hw_transcodes: number; max_sw_transcodes: number; sw_preset: string; sw_crf: number; threads: number; source_mode: string; source_transport: string; readahead_mb: number; source_parallel: number; source_cache_gb: number }
interface NodeStatus {
  version: string; protocol: number; name: string; ffmpeg_version: string; ffmpeg_available: boolean; ffmpeg_jellyfin: boolean;
  caps: Capabilities; current: { backend: string; device: string; encode: string[]; decode: string[] } | null; encoders: string[];
  slots: { hw_used: number; hw_max: number; sw_used: number; sw_max: number }; cpus: number; load_1m: number | null;
  memory_total: number | null; memory_available: number | null; rss: number | null; transcode_free_bytes: number | null;
  source_cache_bytes?: number; source_cache_files?: number;
  peer_ip: string | null; media_roots: string[]; uptime_ms: number;
}
interface ServerReach { url: string; ok: boolean | null; error: string | null; ms: number; checked_at: number }
interface NodePath { library_path_id: number; node_path: string; verified_at: number | null; verify_error: string | null }
interface NodeView {
  id: number; name: string; is_local: boolean; url: string; key_set: boolean; enabled: boolean; draining: boolean;
  role_transcode: boolean; role_cdn: boolean; tc_priority: number; tc_weight: number; tc_max_sessions: number; rules: Rules;
  cdn_priority: number; cdn_weight: number; cdn_max_streams: number; config: NodeConfig; public_url: string;
  health: Health; last_error: string | null; last_ok: number | null; latency_ms: number; breaker_until: number | null; config_synced: boolean;
  status: NodeStatus | null; active_transcodes: number; active_streams: number; server_url: string | null; server_reach: ServerReach | null; paths: NodePath[];
}
interface LibraryPath { id: number; library_id: number; library: string; path: string }
interface NodesData { nodes: NodeView[]; library_paths: LibraryPath[]; libraries: { id: number; name: string }[]; protocol: number }

const CODECS = ["h264", "hevc", "av1", "vp9", "mpeg2video", "mpeg4"];
const PRESETS = ["ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow"];
const BACKENDS = ["auto", "none", "qsv", "vaapi", "nvenc"];

function useNodeName() {
  const { t } = useI18n();
  const { info } = useSession();
  return (n: NodeView) => (n.is_local ? `${t("admin.nodes.thisServer")}${info?.name ? ` · ${info.name}` : ""}` : n.name);
}

function HealthBadge({ n }: { n: NodeView }) {
  const { t } = useI18n();
  if (!n.enabled) return <Badge>{t("admin.nodes.health.disabled")}</Badge>;
  const extra = [
    n.draining ? <Badge key="d" tone="transcode">{t("admin.nodes.health.draining")}</Badge> : null,
    n.breaker_until ? <Badge key="b" tone="danger">{t("admin.nodes.health.breaker")}</Badge> : null,
  ];
  const tone = n.health === "online" ? "direct" : n.health === "unknown" ? undefined : "danger";
  return <span className="row" style={{ gap: 6, flexWrap: "wrap" }}><Badge tone={tone}>{t(`admin.nodes.health.${n.health}` as MessageKey)}</Badge>{extra}</span>;
}

function NodeCard({ n, data, onEdit, onChanged }: { n: NodeView; data: NodesData; onEdit: () => void; onChanged: () => void }) {
  const { t, bytes, relative } = useI18n();
  const name = useNodeName();
  const [busy, setBusy] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [caps, setCaps] = useState(false);
  const [logs, setLogs] = useState(false);
  const [confirm, setConfirm] = useState<null | "delete" | "restart" | "stop">(null);
  const st = n.status;
  const act = async (key: string, fn: () => Promise<unknown>, okMsg?: string) => {
    setBusy(key);
    setErr(null);
    try {
      await fn();
      if (okMsg) toast(okMsg);
      onChanged();
    } catch (e) { setErr((e as ApiError).message); } finally { setBusy(null); }
  };
  const gpu = st?.current && st.current.backend !== "software" ? `${st.current.backend.toUpperCase()} · ${st.current.device}` : st ? t("admin.transcode.software") : "—";
  const slot = (u: number, m: number) => `${u}/${m || "∞"}`;
  const folders = n.paths.map((p) => {
    const lp = data.library_paths.find((x) => x.id === p.library_path_id);
    return { ...p, label: lp ? `${lp.library} · ${lp.path}` : `#${p.library_path_id}` };
  });
  return (
    <section className="pl-panel panel-pad stack node-card">
      <div className="node-card__head">
        <div className="stack" style={{ gap: 4, minWidth: 0 }}>
          <h2 className="t-h3" style={{ margin: 0 }}>{name(n)}</h2>
          <span className={`t-small muted${n.is_local ? "" : " t-mono"}`}>{n.is_local ? t("admin.nodes.localHint") : n.url}{st ? ` · v${st.version}` : ""}{!n.is_local && n.health === "online" ? ` · ${n.latency_ms} ms` : ""}</span>
        </div>
        <HealthBadge n={n} />
      </div>
      <div className="row" style={{ gap: 6, flexWrap: "wrap" }}>
        {n.role_transcode && <Badge tone="codec">{t("admin.nodes.roleTranscode")} · P{n.tc_priority} · {t("admin.nodes.weightShort", { n: n.tc_weight })}</Badge>}
        {n.role_cdn && <Badge tone="codec">{t("admin.nodes.roleCdn")} · P{n.cdn_priority}{n.is_local ? "" : ` · ${t("admin.nodes.foldersCount", { count: n.paths.length })}`}</Badge>}
        {!n.role_transcode && !n.role_cdn && <Badge>{t("admin.nodes.noRoles")}</Badge>}
        {!n.config_synced && n.health === "online" && <Badge tone="transcode">{t("admin.nodes.configPending")}</Badge>}
      </div>
      {n.last_error && n.health !== "online" && n.enabled && <Alert tone="danger">{n.last_error}</Alert>}
      {n.breaker_until && <Alert tone="warning">{t("admin.nodes.breakerHint", { time: relative(n.breaker_until) })}{n.last_error ? ` · ${n.last_error}` : ""}</Alert>}
      {err && <Alert tone="danger">{err}</Alert>}
      <dl className="kv">
        <dt>{t("admin.nodes.gpu")}</dt><dd className="t-mono">{gpu}</dd>
        <dt>{t("admin.nodes.activeTranscodes")}</dt><dd>{n.active_transcodes}{n.tc_max_sessions ? ` / ${n.tc_max_sessions}` : ""}{st ? ` · GPU ${slot(st.slots.hw_used, st.slots.hw_max)} · CPU ${slot(st.slots.sw_used, st.slots.sw_max)}` : ""}</dd>
        <dt>{t("admin.nodes.activeStreams")}</dt><dd>{n.active_streams}{n.cdn_max_streams ? ` / ${n.cdn_max_streams}` : ""}</dd>
        {st && <><dt>{t("admin.nodes.machine")}</dt><dd>{t("admin.nodes.machineValue", { cpus: st.cpus, load: st.load_1m != null ? st.load_1m.toFixed(2) : "—", mem: st.memory_available != null && st.memory_total != null ? `${bytes(st.memory_available)} / ${bytes(st.memory_total)}` : "—" })}</dd></>}
        {st && st.transcode_free_bytes != null && <><dt>{t("admin.nodes.freeSpace")}</dt><dd>{bytes(st.transcode_free_bytes)}</dd></>}
        {st && !n.is_local && (st.source_cache_files ?? 0) > 0 && <><dt>{t("admin.nodes.sourceCacheUsed")}</dt><dd>{t("admin.nodes.sourceCacheValue", { size: bytes(st.source_cache_bytes ?? 0), count: st.source_cache_files ?? 0 })}</dd></>}
        {st && !n.is_local && <><dt>FFmpeg</dt><dd className="t-mono-sm">{st.ffmpeg_available ? `${st.ffmpeg_version}${st.ffmpeg_jellyfin ? " · jellyfin-ffmpeg" : ""}` : t("admin.nodes.ffmpegMissing")}</dd></>}
        {!n.is_local && n.server_url && <><dt>{t("admin.nodes.serverUrl")}</dt><dd className="stack" style={{ gap: 2 }}>
          <span><span className="t-mono-sm">{n.server_url}</span>{" "}
            {n.server_reach?.url === n.server_url && n.server_reach.ok === true && <Badge tone="direct">{t("admin.nodes.reachOk", { ms: n.server_reach.ms })}</Badge>}
            {n.server_reach?.url === n.server_url && n.server_reach.ok === false && <Badge tone="danger">{t("admin.nodes.reachBad")}</Badge>}
          </span>
          {n.server_reach?.url === n.server_url && n.server_reach.ok === false && <span className="t-small">{t("admin.nodes.reachBadHint", { error: n.server_reach.error ?? "" })}</span>}
        </dd></>}
        {folders.length > 0 && <><dt>{t("admin.nodes.folders")}</dt><dd className="stack" style={{ gap: 2 }}>{folders.map((f) => (
          <span key={f.library_path_id} className="t-small"><span className="t-mono-sm">{f.node_path}</span> ← {f.label} {f.verify_error ? <Badge tone="danger">{t("admin.nodes.folderBad")}</Badge> : <Badge tone="direct">{t("admin.nodes.folderOk")}</Badge>}</span>
        ))}</dd></>}
      </dl>
      <div className="row node-card__actions">
        <button className="pl-btn pl-btn--secondary" onClick={onEdit}><Pencil />{t("common.edit")}</button>
        {!n.is_local && <button className="pl-btn pl-btn--ghost" disabled={busy !== null} onClick={() => act("check", () => api(`/api/v1/admin/nodes/${n.id}/check`, { method: "POST" }))}><RefreshCw />{busy === "check" ? t("admin.nodes.checking") : t("admin.nodes.check")}</button>}
        <button className="pl-btn pl-btn--ghost" disabled={!st} onClick={() => setCaps(true)}><FlaskConical />{t("admin.nodes.gpuTest")}</button>
        <button className="pl-btn pl-btn--ghost" disabled={!n.is_local && n.health !== "online"} onClick={() => setLogs(true)}><ScrollText />{t("admin.nodes.logs")}</button>
        <button className="pl-btn pl-btn--ghost" disabled={busy !== null} onClick={() => act("drain", () => api(`/api/v1/admin/nodes/${n.id}`, { method: "PATCH", body: { draining: !n.draining } }), n.draining ? t("admin.nodes.resumed") : t("admin.nodes.drained"))}>
          {n.draining ? <PlayCircle /> : <PauseCircle />}{n.draining ? t("admin.nodes.resume") : t("admin.nodes.drain")}
        </button>
        {(n.active_transcodes > 0 || n.active_streams > 0) && <button className="pl-btn pl-btn--ghost" onClick={() => setConfirm("stop")}><Square />{t("admin.nodes.stopSessions")}</button>}
        {!n.is_local && <button className="pl-btn pl-btn--ghost" disabled={n.health !== "online"} onClick={() => setConfirm("restart")}><Power />{t("admin.nodes.restart")}</button>}
        {!n.is_local && <button className="pl-btn pl-btn--ghost" onClick={() => setConfirm("delete")}><Trash2 />{t("common.delete")}</button>}
      </div>
      {caps && st && <CapsDialog n={n} onClose={() => setCaps(false)} onChanged={onChanged} />}
      {logs && <LogsDialog n={n} onClose={() => setLogs(false)} />}
      {confirm === "delete" && <Confirm danger title={t("admin.nodes.deleteTitle", { name: n.name })} message={t("admin.nodes.deleteMessage")} confirmLabel={t("common.delete")} onClose={() => setConfirm(null)} onConfirm={() => act("delete", () => api(`/api/v1/admin/nodes/${n.id}`, { method: "DELETE" }))} />}
      {confirm === "restart" && <Confirm title={t("admin.nodes.restartTitle", { name: n.name })} message={t("admin.nodes.restartMessage")} confirmLabel={t("admin.nodes.restart")} onClose={() => setConfirm(null)} onConfirm={() => act("restart", () => api(`/api/v1/admin/nodes/${n.id}/restart`, { method: "POST" }), t("admin.nodes.restarting"))} />}
      {confirm === "stop" && <Confirm danger title={t("admin.nodes.stopTitle")} message={t("admin.nodes.stopMessage")} confirmLabel={t("admin.nodes.stopSessions")} onClose={() => setConfirm(null)} onConfirm={() => act("stop", () => api(`/api/v1/admin/nodes/${n.id}/stop-sessions`, { method: "POST" }))} />}
    </section>
  );
}

function CapsDialog({ n, onClose, onChanged }: { n: NodeView; onClose: () => void; onChanged: () => void }) {
  const { t, relative } = useI18n();
  const name = useNodeName();
  const [node, setNode] = useState(n);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const caps = node.status?.caps;
  return (
    <Dialog wide title={`${t("admin.nodes.gpuTest")} · ${name(n)}`} onClose={onClose} footer={<>
      <button className="pl-btn pl-btn--ghost" onClick={onClose}>{t("common.close")}</button>
      <button className="pl-btn pl-btn--primary" disabled={busy} onClick={async () => {
        setBusy(true);
        setErr(null);
        try { setNode(await api<NodeView>(`/api/v1/admin/nodes/${n.id}/hw-test`, { method: "POST" })); onChanged(); } catch (e) { setErr((e as ApiError).message); } finally { setBusy(false); }
      }}><FlaskConical />{busy ? t("setup.testing") : t("admin.transcode.testNow")}</button>
    </>}>
      {err && <Alert tone="danger">{err}</Alert>}
      {caps && <dl className="kv">
        <dt>{t("admin.transcode.devices")}</dt><dd className="t-mono">{caps.devices.join(", ") || t("admin.transcode.noDevices")}</dd>
        <dt>{t("admin.transcode.backend")}</dt><dd className="t-mono">{caps.selected ? `${caps.selected.backend.toUpperCase()} · ${caps.selected.encode.join(", ")}` : t("admin.transcode.software")}</dd>
        <dt>{t("admin.transcode.lastTest")}</dt><dd>{caps.tested_at ? relative(caps.tested_at) : "—"}</dd>
      </dl>}
      {caps && <CapsMatrix caps={caps} />}
      {caps && !caps.selected && <Alert>{n.is_local ? t("admin.transcode.noGpuHelp") : t("admin.nodes.noGpuHelp")}</Alert>}
    </Dialog>
  );
}

function LogsDialog({ n, onClose }: { n: NodeView; onClose: () => void }) {
  const { t, dateTime } = useI18n();
  const name = useNodeName();
  const [level, setLevel] = useState("INFO");
  const { data, error, refetch, isFetching } = useQuery({
    queryKey: ["admin", "nodes", n.id, "logs", level],
    queryFn: () => api<{ lines: { seq: number; at: number; level: string; message: string }[] }>(`/api/v1/admin/nodes/${n.id}/logs?limit=400&level=${level}`),
  });
  return (
    <Dialog wide title={`${t("admin.nodes.logs")} · ${name(n)}`} onClose={onClose} footer={<>
      <button className="pl-btn pl-btn--ghost" onClick={() => refetch()} disabled={isFetching}><RefreshCw />{t("admin.nodes.refresh")}</button>
      <button className="pl-btn pl-btn--primary" onClick={onClose}>{t("common.close")}</button>
    </>}>
      <SelectField label={t("admin.nodes.logLevel")} value={level} onChange={setLevel} options={["DEBUG", "INFO", "WARN", "ERROR"].map((l) => ({ value: l, label: l }))} />
      {error && <Alert tone="danger">{(error as ApiError).message}</Alert>}
      <pre className="t-mono-sm node-logs">{data?.lines.length ? data.lines.map((l) => `${dateTime(l.at)} ${l.level.padEnd(5)} ${l.message}`).join("\n") : "—"}</pre>
    </Dialog>
  );
}

/** Sfoglia le cartelle del nodo per scegliere la copia di una cartella di libreria. */
function BrowseDialog({ node, start, onPick, onClose }: { node: NodeView; start: string; onPick: (p: string) => void; onClose: () => void }) {
  const { t } = useI18n();
  const [path, setPath] = useState(start);
  const { data, error, isFetching } = useQuery({
    queryKey: ["admin", "nodes", node.id, "browse", path],
    queryFn: () => api<{ path: string | null; parent: string | null; directories: { name: string; path: string }[] }>(`/api/v1/admin/nodes/${node.id}/browse${path ? `?path=${encodeURIComponent(path)}` : ""}`),
    retry: false,
  });
  return (
    <Dialog title={t("admin.nodes.browseTitle", { name: node.name })} onClose={onClose} footer={<>
      <button className="pl-btn pl-btn--ghost" onClick={onClose}>{t("common.cancel")}</button>
      <button className="pl-btn pl-btn--primary" disabled={!data?.path} onClick={() => { if (data?.path) onPick(data.path); }}>{t("admin.nodes.chooseFolder")}</button>
    </>}>
      {error && <Alert tone="danger">{(error as ApiError).message}</Alert>}
      <div className="row" style={{ gap: 8 }}>
        <button className="icon-btn" disabled={!data?.path} aria-label={t("common.back")} onClick={() => setPath(data?.parent ?? "")}><ChevronLeft /></button>
        <span className="t-mono-sm">{data?.path ?? t("admin.nodes.roots")}</span>
      </div>
      <div className="stack node-browse" style={{ gap: 2 }}>
        {isFetching && !data && <span className="muted t-small">{t("common.loading")}</span>}
        {data?.directories.map((d) => (
          <button key={d.path} type="button" className="node-browse__item" onClick={() => setPath(d.path)}><FolderOpen size={16} />{d.name}</button>
        ))}
        {data && data.directories.length === 0 && <span className="muted t-small">{t("admin.nodes.emptyFolder")}</span>}
      </div>
    </Dialog>
  );
}

const num = (v: string, d = 0) => {
  const x = Number(v.replace(",", "."));
  return Number.isFinite(x) ? Math.round(x) : d;
};

function NodeDialog({ node, data, onClose, onCreated }: { node: NodeView | null; data: NodesData; onClose: () => void; onCreated: (id: number) => void }) {
  const { t } = useI18n();
  const name = useNodeName();
  const qc = useQueryClient();
  const isNew = node === null;
  const local = node?.is_local ?? false;
  const r = node?.rules ?? { min_height: 0, max_height: 0, hdr: "any", codecs: [], libraries: [], transcode: true, remux: true, require_hw: false };
  const c = node?.config ?? { hw_backend: "auto", vaapi_device: "/dev/dri/renderD128", max_hw_transcodes: 0, max_sw_transcodes: 0, sw_preset: "veryfast", sw_crf: 23, threads: 0, source_mode: "readahead", source_transport: "auto", readahead_mb: 1024, source_parallel: 4, source_cache_gb: 50 };
  const [f, setF] = useState({
    name: node?.name ?? "", url: node?.url ?? "", key: "", role_transcode: node?.role_transcode ?? true, role_cdn: node?.role_cdn ?? false,
    tc_priority: String(node?.tc_priority ?? 1), tc_weight: String(node?.tc_weight ?? 100), tc_max_sessions: String(node?.tc_max_sessions ?? 0),
    cdn_priority: String(node?.cdn_priority ?? 1), cdn_weight: String(node?.cdn_weight ?? 100), cdn_max_streams: String(node?.cdn_max_streams ?? 0),
    public_url: node?.public_url ?? "", enabled: node?.enabled ?? true,
  });
  const [rules, setRules] = useState<Rules>(r as Rules);
  const [cfg, setCfg] = useState<NodeConfig & { _max_hw: string; _max_sw: string; _crf: string; _threads: string; _ra: string; _par: string; _cache: string }>({
    ...c, _max_hw: String(c.max_hw_transcodes), _max_sw: String(c.max_sw_transcodes), _crf: String(c.sw_crf), _threads: String(c.threads),
    source_mode: c.source_mode ?? "readahead", source_transport: c.source_transport ?? "auto", _ra: String(c.readahead_mb ?? 1024), _par: String(c.source_parallel ?? 4), _cache: String(c.source_cache_gb ?? 50),
  });
  const [paths, setPaths] = useState<Record<number, string>>(Object.fromEntries((node?.paths ?? []).map((p) => [p.library_path_id, p.node_path])));
  const [browse, setBrowse] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [results, setResults] = useState<{ library_path_id: number; ok: boolean; error: string | null }[] | null>(null);
  const [saving, setSaving] = useState(false);
  const set = <K extends keyof typeof f>(k: K, v: (typeof f)[K]) => setF({ ...f, [k]: v });
  const heightPreset = rules.min_height === 0 && rules.max_height === 0 ? "any" : rules.min_height === 2160 && rules.max_height === 0 ? "uhd" : rules.min_height === 0 && rules.max_height === 1080 ? "hd" : "custom";
  const [customHeight, setCustomHeight] = useState(heightPreset === "custom");
  const toggle = <T,>(list: T[], v: T) => (list.includes(v) ? list.filter((x) => x !== v) : [...list, v]);
  const pathsChanged = !isNew && !local && JSON.stringify(Object.entries(paths).filter(([, v]) => v.trim()).sort()) !== JSON.stringify((node?.paths ?? []).map((p) => [String(p.library_path_id), p.node_path]).sort());

  const save = async () => {
    setError(null);
    setResults(null);
    setSaving(true);
    const body: Record<string, unknown> = {
      name: f.name, role_transcode: f.role_transcode, role_cdn: f.role_cdn, enabled: f.enabled,
      tc_priority: num(f.tc_priority, 1), tc_weight: num(f.tc_weight, 100), tc_max_sessions: num(f.tc_max_sessions),
      cdn_priority: num(f.cdn_priority, 1), cdn_weight: num(f.cdn_weight, 100), cdn_max_streams: num(f.cdn_max_streams),
      rules,
    };
    if (!local) {
      body.url = f.url;
      if (f.key) body.key = f.key;
      body.public_url = f.public_url;
      body.config = { hw_backend: cfg.hw_backend, vaapi_device: cfg.vaapi_device, max_hw_transcodes: num(cfg._max_hw), max_sw_transcodes: num(cfg._max_sw), sw_preset: cfg.sw_preset, sw_crf: num(cfg._crf, 23), threads: num(cfg._threads),
        source_mode: cfg.source_mode, source_transport: cfg.source_transport, readahead_mb: num(cfg._ra, 1024), source_parallel: num(cfg._par, 4), source_cache_gb: num(cfg._cache, 50) };
    }
    try {
      if (isNew) {
        const created = await api<NodeView>("/api/v1/admin/nodes", { body });
        qc.invalidateQueries({ queryKey: ["admin", "nodes"] });
        toast(created.health === "online" ? t("admin.nodes.addedOnline") : t("admin.nodes.addedOffline"));
        onCreated(created.id);
        return;
      }
      await api(`/api/v1/admin/nodes/${node!.id}`, { method: "PATCH", body });
      if (pathsChanged) {
        const res = await api<{ results: { library_path_id: number; ok: boolean; error: string | null }[] }>(`/api/v1/admin/nodes/${node!.id}/paths`, {
          method: "PUT",
          body: { paths: Object.entries(paths).filter(([, v]) => v.trim()).map(([k, v]) => ({ library_path_id: Number(k), node_path: v.trim() })) },
        });
        qc.invalidateQueries({ queryKey: ["admin", "nodes"] });
        if (res.results.some((x) => !x.ok)) { setResults(res.results); return; }
      }
      qc.invalidateQueries({ queryKey: ["admin", "nodes"] });
      toast(t("admin.nodes.saved"));
      onClose();
    } catch (e) { setError((e as ApiError).message); } finally { setSaving(false); }
  };

  return (
    <Dialog wide title={isNew ? t("admin.nodes.add") : name(node!)} onClose={onClose} footer={<>
      <button className="pl-btn pl-btn--ghost" onClick={onClose}>{t("common.cancel")}</button>
      <button className="pl-btn pl-btn--primary" onClick={save} disabled={saving || (!local && (!f.name.trim() || !f.url.trim() || (isNew && f.key.trim().length < 16)))}>{saving ? t("admin.nodes.saving") : t("common.save")}</button>
    </>}>
      {error && <Alert tone="danger">{error}</Alert>}
      {results && <Alert tone="warning">{t("admin.nodes.folderErrors")}<ul style={{ margin: "6px 0 0", paddingLeft: 18 }}>{results.filter((x) => !x.ok).map((x) => {
        const lp = data.library_paths.find((p) => p.id === x.library_path_id);
        return <li key={x.library_path_id}>{lp ? `${lp.library} · ${lp.path}` : x.library_path_id}: {x.error}</li>;
      })}</ul></Alert>}

      {!local && <>
        <span className="pl-eyebrow">{t("admin.nodes.sectionGeneral")}</span>
        <div className="form-grid">
          <TextField label={t("admin.nodes.name")} value={f.name} onChange={(e) => set("name", e.target.value)} autoComplete="off" />
          <TextField label={t("admin.nodes.url")} mono value={f.url} onChange={(e) => set("url", e.target.value)} placeholder="100.64.0.2:8097" hint={t("admin.nodes.urlHint")} autoComplete="off" />
        </div>
        <TextField label={t("admin.nodes.key")} mono type="password" autoComplete="off" value={f.key} onChange={(e) => set("key", e.target.value)} placeholder={node?.key_set ? t("admin.settings.secretSet") : ""} hint={node?.key_set ? t("admin.nodes.keyKeep") : t("admin.nodes.keyHint")} />
        {!isNew && <Switch checked={f.enabled} onChange={(v) => set("enabled", v)} label={t("admin.nodes.enabled")} hint={t("admin.nodes.enabledHint")} />}
      </>}
      <span className="pl-eyebrow">{t("admin.nodes.sectionRoles")}</span>
      <Switch checked={f.role_transcode} onChange={(v) => set("role_transcode", v)} label={t("admin.nodes.roleTranscode")} hint={local ? t("admin.nodes.roleTranscodeLocalHint") : t("admin.nodes.roleTranscodeHint")} />
      <Switch checked={f.role_cdn} onChange={(v) => set("role_cdn", v)} label={t("admin.nodes.roleCdn")} hint={local ? t("admin.nodes.roleCdnLocalHint") : t("admin.nodes.roleCdnHint")} />

      {f.role_transcode && <>
        <span className="pl-eyebrow">{t("admin.nodes.sectionBalance")}</span>
        <div className="form-grid form-grid--3">
          <TextField label={t("admin.nodes.priority")} inputMode="numeric" value={f.tc_priority} onChange={(e) => set("tc_priority", e.target.value)} hint={t("admin.nodes.priorityHint")} />
          <TextField label={t("admin.nodes.weight")} inputMode="numeric" value={f.tc_weight} onChange={(e) => set("tc_weight", e.target.value)} hint={t("admin.nodes.weightHint")} />
          <TextField label={t("admin.nodes.maxSessions")} inputMode="numeric" value={f.tc_max_sessions} onChange={(e) => set("tc_max_sessions", e.target.value)} hint={t("admin.nodes.unlimitedHint")} />
        </div>
        <span className="pl-eyebrow">{t("admin.nodes.sectionRules")}</span>
        <p className="t-small muted" style={{ margin: 0 }}>{t("admin.nodes.rulesHint")}</p>
        <SelectField label={t("admin.nodes.rules.height")} value={customHeight ? "custom" : heightPreset} onChange={(v) => {
          if (v === "custom") { setCustomHeight(true); return; }
          setCustomHeight(false);
          setRules({ ...rules, min_height: v === "uhd" ? 2160 : 0, max_height: v === "hd" ? 1080 : 0 });
        }} options={["any", "uhd", "hd", "custom"].map((v) => ({ value: v, label: t(`admin.nodes.rules.height_${v}` as MessageKey) }))} />
        {customHeight && <div className="form-grid">
          <TextField label={t("admin.nodes.rules.minHeight")} inputMode="numeric" value={String(rules.min_height)} onChange={(e) => setRules({ ...rules, min_height: num(e.target.value) })} hint={t("admin.nodes.zeroNone")} />
          <TextField label={t("admin.nodes.rules.maxHeight")} inputMode="numeric" value={String(rules.max_height)} onChange={(e) => setRules({ ...rules, max_height: num(e.target.value) })} hint={t("admin.nodes.zeroNone")} />
        </div>}
        <SelectField label={t("admin.nodes.rules.hdr")} value={rules.hdr} onChange={(v) => setRules({ ...rules, hdr: v as Rules["hdr"] })} options={["any", "only_hdr", "only_sdr"].map((v) => ({ value: v, label: t(`admin.nodes.rules.hdr_${v}` as MessageKey) }))} />
        <div className="stack" style={{ gap: 6 }}>
          <span className="pl-field__label">{t("admin.nodes.rules.codecs")}</span>
          <span className="pl-field__hint">{t("admin.nodes.rules.allIfNone")}</span>
          <div className="genre-picker">{CODECS.map((x) => <button key={x} type="button" className={`genre-chip${rules.codecs.includes(x) ? " on" : ""}`} aria-pressed={rules.codecs.includes(x)} onClick={() => setRules({ ...rules, codecs: toggle(rules.codecs, x) })}>{x.toUpperCase()}</button>)}</div>
        </div>
        {data.libraries.length > 0 && <div className="stack" style={{ gap: 6 }}>
          <span className="pl-field__label">{t("admin.nodes.rules.libraries")}</span>
          <span className="pl-field__hint">{t("admin.nodes.rules.allIfNone")}</span>
          <div className="genre-picker">{data.libraries.map((l) => <button key={l.id} type="button" className={`genre-chip${rules.libraries.includes(l.id) ? " on" : ""}`} aria-pressed={rules.libraries.includes(l.id)} onClick={() => setRules({ ...rules, libraries: toggle(rules.libraries, l.id) })}>{l.name}</button>)}</div>
        </div>}
        <Switch checked={rules.transcode} onChange={(v) => setRules({ ...rules, transcode: v })} label={t("admin.nodes.rules.transcode")} />
        <Switch checked={rules.remux} onChange={(v) => setRules({ ...rules, remux: v })} label={t("admin.nodes.rules.remux")} hint={local ? undefined : t("admin.nodes.rules.remuxHint")} />
        <Switch checked={rules.require_hw} onChange={(v) => setRules({ ...rules, require_hw: v })} label={t("admin.nodes.rules.requireHw")} hint={t("admin.nodes.rules.requireHwHint")} />
      </>}

      {f.role_cdn && <>
        <span className="pl-eyebrow">{t("admin.nodes.sectionCdn")}</span>
        <div className="form-grid form-grid--3">
          <TextField label={t("admin.nodes.priority")} inputMode="numeric" value={f.cdn_priority} onChange={(e) => set("cdn_priority", e.target.value)} hint={t("admin.nodes.priorityHint")} />
          <TextField label={t("admin.nodes.weight")} inputMode="numeric" value={f.cdn_weight} onChange={(e) => set("cdn_weight", e.target.value)} hint={t("admin.nodes.weightHint")} />
          <TextField label={t("admin.nodes.maxStreams")} inputMode="numeric" value={f.cdn_max_streams} onChange={(e) => set("cdn_max_streams", e.target.value)} hint={t("admin.nodes.unlimitedHint")} />
        </div>
      </>}

      {!local && !isNew && (f.role_cdn || f.role_transcode) && <>
        <span className="pl-eyebrow">{t("admin.nodes.sectionFolders")}</span>
        <p className="t-small muted" style={{ margin: 0 }}>{t("admin.nodes.foldersHint")}</p>
        {data.library_paths.length === 0 && <span className="t-small muted">{t("admin.nodes.noLibraries")}</span>}
        {data.library_paths.map((lp) => (
          <div key={lp.id} className="node-folder">
            <TextField label={`${lp.library} · ${lp.path}`} mono value={paths[lp.id] ?? ""} onChange={(e) => setPaths({ ...paths, [lp.id]: e.target.value })} placeholder={t("admin.nodes.folderPlaceholder")} autoComplete="off" />
            <button type="button" className="pl-btn pl-btn--secondary" disabled={node?.health !== "online"} onClick={() => setBrowse(lp.id)}><FolderOpen />{t("admin.nodes.browse")}</button>
          </div>
        ))}
      </>}

      {!local && <>
        <span className="pl-eyebrow">{t("admin.nodes.sectionGpu")}</span>
        <div className="form-grid">
          <SelectField label={t("settings.transcode.hw_backend")} value={cfg.hw_backend} onChange={(v) => setCfg({ ...cfg, hw_backend: v })} options={BACKENDS.map((b) => ({ value: b, label: t(`settings.enum.${b}` as MessageKey) }))} />
          <TextField label={t("settings.transcode.vaapi_device")} mono value={cfg.vaapi_device} onChange={(e) => setCfg({ ...cfg, vaapi_device: e.target.value })} />
          <TextField label={t("settings.transcode.max_hw_transcodes")} inputMode="numeric" value={cfg._max_hw} onChange={(e) => setCfg({ ...cfg, _max_hw: e.target.value })} hint={t("admin.nodes.unlimitedHint")} />
          <TextField label={t("settings.transcode.max_sw_transcodes")} inputMode="numeric" value={cfg._max_sw} onChange={(e) => setCfg({ ...cfg, _max_sw: e.target.value })} hint={t("admin.nodes.unlimitedHint")} />
          <SelectField label={t("settings.transcode.sw_preset")} value={cfg.sw_preset} onChange={(v) => setCfg({ ...cfg, sw_preset: v })} options={PRESETS.map((p) => ({ value: p, label: p }))} />
          <TextField label={t("settings.transcode.sw_crf")} inputMode="numeric" value={cfg._crf} onChange={(e) => setCfg({ ...cfg, _crf: e.target.value })} hint="16–35" />
          <TextField label={t("settings.transcode.threads")} inputMode="numeric" value={cfg._threads} onChange={(e) => setCfg({ ...cfg, _threads: e.target.value })} />
        </div>
        <span className="pl-eyebrow">{t("admin.nodes.sectionSource")}</span>
        <p className="t-small muted" style={{ margin: 0 }}>{t("admin.nodes.sourceHint")}</p>
        <div className="form-grid">
          <SelectField label={t("admin.nodes.sourceTransport")} value={cfg.source_transport} onChange={(v) => setCfg({ ...cfg, source_transport: v })}
            options={["auto", "pull", "push"].map((m) => ({ value: m, label: t(`admin.nodes.sourceTransports.${m}` as MessageKey) }))} hint={t(`admin.nodes.sourceTransportHints.${cfg.source_transport}` as MessageKey)} />
          <SelectField label={t("admin.nodes.sourceMode")} value={cfg.source_mode} onChange={(v) => setCfg({ ...cfg, source_mode: v })}
            options={["readahead", "full", "stream"].map((m) => ({ value: m, label: t(`admin.nodes.sourceModes.${m}` as MessageKey) }))} hint={t(`admin.nodes.sourceModeHints.${cfg.source_mode}` as MessageKey)} />
          {cfg.source_mode === "readahead" && <TextField label={t("admin.nodes.readahead")} inputMode="numeric" value={cfg._ra} onChange={(e) => setCfg({ ...cfg, _ra: e.target.value })} hint={t("admin.nodes.readaheadHint")} />}
          {cfg.source_mode !== "stream" && <TextField label={t("admin.nodes.sourceParallel")} inputMode="numeric" value={cfg._par} onChange={(e) => setCfg({ ...cfg, _par: e.target.value })} hint="1–16" />}
          {cfg.source_mode !== "stream" && <TextField label={t("admin.nodes.sourceCache")} inputMode="numeric" value={cfg._cache} onChange={(e) => setCfg({ ...cfg, _cache: e.target.value })} hint={t("admin.nodes.sourceCacheHint")} />}
        </div>
        <span className="pl-eyebrow">{t("admin.nodes.sectionDelivery")}</span>
        <TextField label={t("admin.nodes.publicUrl")} mono value={f.public_url} onChange={(e) => set("public_url", e.target.value)} placeholder="https://nodo.example.com" hint={t("admin.nodes.publicUrlHint")} autoComplete="off" />
      </>}
      {local && <p className="t-small muted" style={{ margin: 0 }}>{t("admin.nodes.localGpuHint")}</p>}
      {browse != null && node && <BrowseDialog node={node} start={paths[browse] ?? ""} onClose={() => setBrowse(null)} onPick={(p) => { setPaths({ ...paths, [browse]: p }); setBrowse(null); }} />}
    </Dialog>
  );
}

const COMPOSE = `services:
  platea-node:
    image: ghcr.io/enrico1203/platea-server:latest
    command: ["node"]
    restart: unless-stopped
    ports:
      - "8097:8097"
    environment:
      PLATEA_ROLE: node
      PLATEA_NODE_NAME: nodo-gpu
    volumes:
      - ./node-data:/data
      # Facoltativo, per le copie (CDN): sempre in sola lettura
      # - /mnt/film:/media/film:ro,rslave
    # GPU Intel o AMD (gruppo di /dev/dri/renderD128: stat -c %g)
    # devices:
    #   - /dev/dri:/dev/dri
    # group_add:
    #   - "993"`;

export default function Nodes() {
  const { t } = useI18n();
  const qc = useQueryClient();
  const { data, error } = useQuery({ queryKey: ["admin", "nodes"], queryFn: () => api<NodesData>("/api/v1/admin/nodes"), refetchInterval: 5000 });
  const [edit, setEdit] = useState<number | "new" | null>(null);
  const refresh = () => qc.invalidateQueries({ queryKey: ["admin", "nodes"] });
  if (error) return <Alert tone="danger">{(error as ApiError).message}</Alert>;
  if (!data) return null;
  const remote = data.nodes.filter((n) => !n.is_local);
  const editing = typeof edit === "number" ? data.nodes.find((n) => n.id === edit) ?? null : null;
  return (
    <div className="stack-lg">
      <div className="page-head" style={{ marginBottom: 0 }}>
        <h1 className="t-h1">{t("admin.nodes.title")}</h1>
        <button className="pl-btn pl-btn--primary" onClick={() => setEdit("new")}><Plus />{t("admin.nodes.add")}</button>
      </div>
      <p className="muted" style={{ margin: 0, maxWidth: 820 }}>{t("admin.nodes.intro")}</p>
      {remote.length === 0 && <Empty>{t("admin.nodes.empty")}</Empty>}
      <div className="node-grid">
        {data.nodes.map((n) => <NodeCard key={n.id} n={n} data={data} onEdit={() => setEdit(n.id)} onChanged={refresh} />)}
      </div>
      <details className="pl-panel panel-pad">
        <summary className="t-label">{t("admin.nodes.installTitle")}</summary>
        <div className="stack" style={{ marginTop: 12 }}>
          <p className="t-small muted" style={{ margin: 0 }}>{t("admin.nodes.installHelp")}</p>
          <pre className="t-mono node-logs">{COMPOSE}</pre>
          <p className="t-small muted" style={{ margin: 0 }}>{t("admin.nodes.installKey")}</p>
          <pre className="t-mono node-logs">docker compose exec platea-node platea node key</pre>
        </div>
      </details>
      {edit === "new" && <NodeDialog node={null} data={data} onClose={() => setEdit(null)} onCreated={(id) => setEdit(id)} />}
      {editing && <NodeDialog key={editing.id} node={editing} data={data} onClose={() => setEdit(null)} onCreated={() => undefined} />}
    </div>
  );
}
