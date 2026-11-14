import { useRef } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Download, Upload, Plus, Trash2 } from "lucide-react";
import { api, download, mediaUrl } from "../../api/client";
import { Alert, Empty, TextField, toast } from "../../components/ui";
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
        const r = await api<{ key: string }>("/api/v1/admin/api-keys", { body: { name, scopes: ["scan"] } });
        setCreated(r.key);
        setName("");
        qc.invalidateQueries({ queryKey: ["admin", "api-keys"] });
      }}>
        <div className="spacer"><TextField label={t("admin.apiKeys.name")} placeholder="Sonarr" value={name} onChange={(e) => setName(e.target.value)} /></div>
        <button className="pl-btn pl-btn--secondary" disabled={!name.trim()}><Plus />{t("admin.apiKeys.create")}</button>
      </form>
      <pre className="t-mono" style={{ margin: 0, padding: 12, background: "var(--surface-2)", borderRadius: 10, overflowX: "auto" }}>{`curl -X POST <server>/api/v1/scan/path \\
  -H "X-Platea-Key: <chiave>" -H "Content-Type: application/json" \\
  -d '{"path": "/media/Film/Nuovo film (2026)"}'`}</pre>
    </section>
  );
}
