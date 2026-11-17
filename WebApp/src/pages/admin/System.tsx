import { useRef } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Download, Upload, Plus, Trash2, RotateCcw, Gauge } from "lucide-react";
import { api, ApiError, download, getToken, mediaUrl, serverUrl } from "../../api/client";
import { Alert, Confirm, Empty, SelectField, TextField, toast } from "../../components/ui";
import { useState } from "react";
import { useI18n } from "../../i18n";

export default function System() {
  const { t, bytes, relative } = useI18n();
  const qc = useQueryClient();
  const file = useRef<HTMLInputElement>(null);
  const { data } = useQuery({ queryKey: ["admin", "system"], queryFn: () => api<{ version: string; server_id: string; data_dir: string; cache_dir: string; transcode_dir: string; database_bytes: number; started_at: number }>("/api/v1/admin/system") });
  if (!data) return null;
  return (
    <div className="stack-lg" style={{ maxWidth: 860 }}>
      <div className="page-head" style={{ marginBottom: 0 }}><h1 className="t-h1">{t("admin.system.title")}</h1></div>
      <section className="pl-panel panel-pad">
        <dl className="kv">
          <dt>{t("about.serverVersion")}</dt><dd className="t-mono">{data.version}</dd>
          <dt>ID</dt><dd className="t-mono">{data.server_id}</dd>
          <dt>{t("admin.system.started")}</dt><dd>{relative(data.started_at)}</dd>
          <dt>{t("admin.system.database")}</dt><dd className="t-mono">{bytes(data.database_bytes)}</dd>
          <dt>{t("admin.system.dataDir")}</dt><dd className="t-mono">{data.data_dir}</dd>
          <dt>{t("admin.system.cacheDir")}</dt><dd className="t-mono">{data.cache_dir}</dd>
          <dt>{t("admin.system.transcodeDir")}</dt><dd className="t-mono">{data.transcode_dir}</dd>
          <dt>API</dt><dd><a className="t-mono" href={mediaUrl("/api/v1/openapi.json")} target="_blank" rel="noreferrer">/api/v1/openapi.json</a></dd>
        </dl>
      </section>
      <ApiKeys />
      <Maintenance />
      <Backups />
      <section className="pl-panel panel-pad stack">
        <h2 className="t-h3">{t("admin.system.backup")}</h2>
        <p className="t-small muted" style={{ margin: 0 }}>{t("admin.system.backupHelp")}</p>
        <div className="row">
          <button className="pl-btn pl-btn--secondary" onClick={() => download("/api/v1/admin/backup", "platea-backup.db", "POST")}><Download />{t("admin.system.downloadDb")}</button>
          <button className="pl-btn pl-btn--secondary" onClick={() => download("/api/v1/admin/settings/export", "platea-config.json")}><Download />{t("admin.system.exportConfig")}</button>
          <button className="pl-btn pl-btn--secondary" onClick={() => file.current?.click()}><Upload />{t("admin.system.importConfig")}</button>
          <input ref={file} type="file" accept="application/json" hidden onChange={async (e) => {
            const f = e.target.files?.[0];
            if (!f) return;
            try {
              await api("/api/v1/admin/settings/import", { body: JSON.parse(await f.text()) });
              toast(t("admin.system.imported"));
              qc.invalidateQueries();
            } catch (err) { toast((err as Error).message); }
            e.target.value = "";
          }} />
        </div>
      </section>
    </div>
  );
}

function ApiKeys() {
  const { t, relative } = useI18n();
  const qc = useQueryClient();
  const [name, setName] = useState("");
  const [scope, setScope] = useState("scan");
  const [created, setCreated] = useState<string | null>(null);
  const { data } = useQuery({ queryKey: ["admin", "api-keys"], queryFn: () => api<{ items: { id: string; name: string; scopes: string[]; created_at: number; last_used_at: number | null }[] }>("/api/v1/admin/api-keys").then((r) => r.items) });
  return (
    <section className="pl-panel panel-pad stack">
      <h2 className="t-h3">{t("admin.apiKeys.title")}</h2>
      <p className="t-small muted" style={{ margin: 0 }}>{t("admin.apiKeys.help")}</p>
      {created && <Alert tone="success">{t("admin.apiKeys.created")} <span className="t-mono" style={{ userSelect: "all" }}>{created}</span></Alert>}
      {data?.length === 0 && <Empty>{t("admin.apiKeys.empty")}</Empty>}
      {data?.map((k) => (
        <div className="row" key={k.id}>
          <div className="spacer"><div className="t-label">{k.name}</div><div className="t-small subtle">{k.scopes.join(", ")} · {t("admin.apiKeys.lastUsed", { when: k.last_used_at ? relative(k.last_used_at) : "—" })}</div></div>
          <button className="icon-btn" aria-label={t("account.revoke")} onClick={async () => { await api(`/api/v1/admin/api-keys/${k.id}`, { method: "DELETE" }); qc.invalidateQueries({ queryKey: ["admin", "api-keys"] }); }}><Trash2 /></button>
        </div>
      ))}
      <form className="row" style={{ alignItems: "flex-end" }} onSubmit={async (e) => {
        e.preventDefault();
        const r = await api<{ key: string }>("/api/v1/admin/api-keys", { body: { name, scopes: [scope] } });
        setCreated(r.key);
        setName("");
        setScope("scan");
        qc.invalidateQueries({ queryKey: ["admin", "api-keys"] });
      }}>
        <div className="spacer"><TextField label={t("admin.apiKeys.name")} placeholder="Sonarr" value={name} onChange={(e) => setName(e.target.value)} /></div>
        <SelectField label={t("admin.apiKeys.scope")} value={scope} onChange={setScope}
          options={[{ value: "scan", label: t("admin.apiKeys.scopeScan") }, { value: "admin", label: t("admin.apiKeys.scopeAdmin") }]} />
        <button className="pl-btn pl-btn--secondary" disabled={!name.trim()}><Plus />{t("admin.apiKeys.create")}</button>
      </form>
      {scope === "admin" && <Alert tone="warning">{t("admin.apiKeys.adminWarning")}</Alert>}
      <pre className="t-mono" style={{ margin: 0, padding: 12, background: "var(--surface-2)", borderRadius: 10, overflowX: "auto" }}>{`curl -X POST <server>/api/v1/scan/path \\
  -H "X-Platea-Key: <chiave>" -H "Content-Type: application/json" \\
  -d '{"path": "/media/Film/Nuovo film (2026)"}'`}</pre>
    </section>
  );
}

interface BackupEntry { name: string; size: number; created_at: number; automatic: boolean }

/** Attende che il server torni dopo il riavvio del ripristino, poi ricarica la pagina. */
async function waitForRestart() {
  await new Promise((r) => setTimeout(r, 2500));
  for (let i = 0; i < 120; i++) {
    try {
      const r = await fetch(serverUrl() + "/api/v1/system/info", { cache: "no-store" });
      if (r.ok) { window.location.reload(); return; }
    } catch { /* il server sta ripartendo */ }
    await new Promise((r) => setTimeout(r, 1000));
  }
}

function Backups() {
  const { t, bytes, dateTime } = useI18n();
  const qc = useQueryClient();
  const upload = useRef<HTMLInputElement>(null);
  const [restore, setRestore] = useState<{ name?: string; file?: File } | null>(null);
  const [restarting, setRestarting] = useState(false);
  const { data } = useQuery({ queryKey: ["admin", "backups"], queryFn: () => api<{ items: BackupEntry[]; interval_hours: number; keep: number }>("/api/v1/admin/backups") });
  const doRestore = async () => {
    if (!restore) return;
    if (restore.file) {
      const res = await fetch(serverUrl() + "/api/v1/admin/restore", { method: "POST", headers: { Authorization: `Bearer ${getToken() ?? ""}` }, body: restore.file });
      if (!res.ok) { const j = await res.json().catch(() => ({})); throw new ApiError(res.status, j.error ?? "Restore", j.message ?? res.statusText); }
    } else {
      await api(`/api/v1/admin/backups/${restore.name}/restore`, { method: "POST" });
    }
    setRestarting(true);
    void waitForRestart();
  };
  return (
    <section className="pl-panel panel-pad stack">
      <h2 className="t-h3">{t("admin.system.backups")}</h2>
      <p className="t-small muted" style={{ margin: 0 }}>
        {data && data.interval_hours > 0 ? t("admin.system.backupsAuto", { hours: data.interval_hours, keep: data.keep }) : t("admin.system.backupsAutoOff")}
      </p>
      {restarting && <Alert tone="warning">{t("admin.system.restarting")}</Alert>}
      <div className="row">
        <button className="pl-btn pl-btn--secondary" onClick={async () => { await api("/api/v1/admin/backups", { method: "POST" }); qc.invalidateQueries({ queryKey: ["admin", "backups"] }); toast(t("admin.system.backupCreated")); }}><Plus />{t("admin.system.backupNow")}</button>
        <button className="pl-btn pl-btn--ghost" onClick={() => upload.current?.click()}><Upload />{t("admin.system.restoreFromFile")}</button>
        <input ref={upload} type="file" accept=".db,application/vnd.sqlite3,application/octet-stream" hidden onChange={(e) => { const f = e.target.files?.[0]; if (f) setRestore({ file: f }); e.target.value = ""; }} />
      </div>
      {data && data.items.length === 0 && <Empty>{t("admin.system.noBackups")}</Empty>}
      {data && data.items.length > 0 && (
        <table className="table">
          <tbody>
            {data.items.map((b) => (
              <tr key={b.name}>
                <td>{dateTime(b.created_at)}<div className="t-small subtle">{b.automatic ? t("admin.system.backupAuto") : t("admin.system.backupManual")}</div></td>
                <td className="t-mono t-small">{bytes(b.size)}</td>
                <td style={{ textAlign: "right", whiteSpace: "nowrap" }}>
                  <button className="icon-btn" title={t("admin.system.download")} aria-label={t("admin.system.download")} onClick={() => download(`/api/v1/admin/backups/${b.name}`, b.name)}><Download /></button>
                  <button className="icon-btn" title={t("admin.system.restore")} aria-label={t("admin.system.restore")} onClick={() => setRestore({ name: b.name })}><RotateCcw /></button>
                  <button className="icon-btn" title={t("common.delete")} aria-label={t("common.delete")} onClick={async () => { await api(`/api/v1/admin/backups/${b.name}`, { method: "DELETE" }); qc.invalidateQueries({ queryKey: ["admin", "backups"] }); }}><Trash2 /></button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {restore && (
        <Confirm danger title={t("admin.system.restoreTitle")} message={t("admin.system.restoreMessage", { name: restore.file?.name ?? restore.name ?? "" })} confirmLabel={t("admin.system.restore")} onClose={() => setRestore(null)} onConfirm={doRestore} />
      )}
    </section>
  );
}

interface MaintenanceReport { at: number; ms: number; jobs_removed: number; analyzed: boolean; vacuumed: boolean; size_before: number; size_after: number; wal_before: number; wal_after: number; free_pages_percent: number; error: string | null }

/** Manutenzione del database: quando gira, l'esito dell'ultima, e il pulsante per avviarla ora. */
function Maintenance() {
  const { t, bytes, relative, num, duration } = useI18n();
  const qc = useQueryClient();
  const { data } = useQuery({
    queryKey: ["admin", "maintenance"],
    queryFn: () => api<{ last: MaintenanceReport | null; running: boolean; hour: number }>("/api/v1/admin/maintenance"),
    refetchInterval: (q) => (q.state.data?.running ? 3000 : false),
  });
  const last = data?.last;
  return (
    <section className="pl-panel panel-pad stack">
      <h2 className="t-h3">{t("admin.system.maintenance")}</h2>
      <p className="t-small muted">{t("admin.system.maintenanceHint", { hour: String(data?.hour ?? 4).padStart(2, "0") })}</p>
      {last && (
        <dl className="kv">
          <dt>{t("admin.system.maintenanceLast")}</dt><dd>{relative(last.at)} · {last.ms < 60_000 ? `${num(last.ms / 1000, 1)} s` : duration(last.ms)}</dd>
          <dt>{t("admin.system.database")}</dt><dd className="t-mono">{bytes(last.size_before)} → {bytes(last.size_after)}</dd>
          <dt>WAL</dt><dd className="t-mono">{bytes(last.wal_before)} → {bytes(last.wal_after)}</dd>
          <dt>{t("admin.system.maintenanceJobs")}</dt><dd>{num(last.jobs_removed)}</dd>
          <dt>{t("admin.system.maintenanceFree")}</dt><dd>{num(last.free_pages_percent, 1)}%{last.vacuumed ? ` · ${t("admin.system.maintenanceVacuumed")}` : ""}</dd>
        </dl>
      )}
      {last?.error && <Alert tone="danger">{last.error}</Alert>}
      <div className="row">
        <button className="pl-btn pl-btn--secondary" disabled={data?.running} onClick={async () => {
          await api("/api/v1/admin/maintenance", { method: "POST" });
          toast(t("admin.system.maintenanceStarted"));
          qc.invalidateQueries({ queryKey: ["admin", "maintenance"] });
        }}><Gauge />{data?.running ? t("admin.system.maintenanceRunning") : t("admin.system.maintenanceNow")}</button>
      </div>
    </section>
  );
}

