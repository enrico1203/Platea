/** Amministrazione › Live TV: sorgenti (Dispatcharr, M3U + XMLTV, HDHomeRun, Xtream Codes) e canali nascosti. */
import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, Eye, EyeOff, Pencil, Plus, RefreshCw, Trash2 } from "lucide-react";
import { api, ApiError } from "../../api/client";
import type { LiveChannel, LiveSource, LiveSourceKind } from "../../api/types";
import { Alert, Badge, Confirm, Dialog, Empty, SelectField, Switch, TextField, toast } from "../../components/ui";
import { ChannelLogo, chNumber } from "../LiveTV";
import { useI18n } from "../../i18n";

interface TestResult { ok: boolean; message?: string; channels?: number; groups?: number; version?: string; epg_url?: string | null; recordable?: boolean }

function SourceDialog({ source, onClose }: { source: LiveSource | null; onClose: () => void }) {
  const { t, relative } = useI18n();
  const qc = useQueryClient();
  const [kind, setKind] = useState<LiveSourceKind>(source?.kind ?? "dispatcharr");
  const [name, setName] = useState(source?.name ?? "");
  const [url, setUrl] = useState(source?.url ?? "");
  const [epg, setEpg] = useState(source?.epg_url ?? "");
  const [apiKey, setApiKey] = useState("");
  const [username, setUsername] = useState(source?.username ?? "");
  const [password, setPassword] = useState("");
  const [profile, setProfile] = useState(source?.channel_profile ?? "");
  const [outputProfile, setOutputProfile] = useState(source?.output_profile != null ? String(source.output_profile) : "");
  const [recPath, setRecPath] = useState(source?.recordings_path ?? "");
  const [maxStreams, setMaxStreams] = useState(source?.max_streams != null ? String(source.max_streams) : "");
  const [refresh, setRefresh] = useState(String(source?.refresh_hours ?? 6));
  const [epgRefresh, setEpgRefresh] = useState(String(source?.epg_refresh_hours ?? 12));
  const [days, setDays] = useState(String(source?.epg_days ?? 7));
  const [enabled, setEnabled] = useState(source?.enabled ?? true);
  const [error, setError] = useState<string | null>(null);
  const [test, setTest] = useState<TestResult | null>(null);
  const [busy, setBusy] = useState(false);
  const n = (v: string) => (v.trim() === "" ? null : Number(v));
  const body = () => ({
    kind, name, url, epg_url: epg || null, ...(apiKey ? { api_key: apiKey } : {}), username: username || null, ...(password ? { password } : {}),
    channel_profile: profile || null, output_profile: n(outputProfile), recordings_path: recPath || null, max_streams: n(maxStreams),
    refresh_hours: n(refresh) ?? 6, epg_refresh_hours: n(epgRefresh) ?? 12, epg_days: n(days) ?? 7, enabled,
  });
  const runTest = async () => {
    setBusy(true); setTest(null); setError(null);
    try { setTest(await api<TestResult>("/api/v1/admin/livetv/test", { body: body() })); }
    catch (e) { setError((e as ApiError).message); } finally { setBusy(false); }
  };
  const save = async () => {
    setBusy(true); setError(null);
    try {
      if (source) await api(`/api/v1/admin/livetv/sources/${source.id}`, { method: "PATCH", body: body() });
      else await api("/api/v1/admin/livetv/sources", { body: body() });
      qc.invalidateQueries({ queryKey: ["admin", "livetv"] });
      qc.invalidateQueries({ queryKey: ["livetv"] });
      toast(t("admin.livetv.saved"));
      onClose();
    } catch (e) { setError((e as ApiError).message); } finally { setBusy(false); }
  };
  const urlLabel = kind === "m3u" ? t("admin.livetv.m3uUrl") : kind === "hdhomerun" ? t("admin.livetv.hdhrUrl") : kind === "xtream" ? t("admin.livetv.xtreamUrl") : t("admin.livetv.dispatcharrUrl");
  return (
    <Dialog wide title={source ? t("admin.livetv.edit") : t("admin.livetv.add")} onClose={onClose} footer={<>
      <button className="pl-btn pl-btn--ghost" onClick={onClose}>{t("common.cancel")}</button>
      <button className="pl-btn pl-btn--secondary" onClick={runTest} disabled={busy || !url}>{t("admin.livetv.test")}</button>
      <button className="pl-btn pl-btn--primary" onClick={save} disabled={busy || !url}>{t("common.save")}</button>
    </>}>
      {error && <Alert tone="danger">{error}</Alert>}
      {test && (test.ok
        ? <Alert tone="success">{t("admin.livetv.testOk", { channels: test.channels ?? 0, groups: test.groups ?? 0 })}{test.version ? ` · Dispatcharr ${test.version}` : ""}{test.recordable ? ` · ${t("admin.livetv.recordable")}` : ""}</Alert>
        : <Alert tone="danger">{test.message}</Alert>)}
      <div className="form-grid">
        <SelectField label={t("admin.livetv.kind")} value={kind} onChange={(v) => setKind(v as LiveSourceKind)} options={[
          { value: "dispatcharr", label: "Dispatcharr" }, { value: "m3u", label: t("admin.livetv.kindM3u") },
          { value: "hdhomerun", label: "HDHomeRun" }, { value: "xtream", label: "Xtream Codes" },
        ]} />
        <TextField label={t("admin.livetv.name")} value={name} onChange={(e) => setName(e.target.value)} placeholder={kind === "dispatcharr" ? "Dispatcharr" : ""} />
      </div>
      <div className="t-small subtle">{t(`admin.livetv.hint.${kind}` as "admin.livetv.hint.dispatcharr")}</div>
      <TextField label={urlLabel} value={url} onChange={(e) => setUrl(e.target.value)} mono placeholder={kind === "dispatcharr" ? "http://dispatcharr:9191" : kind === "hdhomerun" ? "http://192.168.1.50" : "https://…"} />
      {kind === "dispatcharr" && (
        <>
          <TextField label={t("admin.livetv.apiKey")} type="password" autoComplete="off" value={apiKey} onChange={(e) => setApiKey(e.target.value)} mono
            placeholder={source?.has_api_key ? t("admin.livetv.keySaved") : ""} hint={t("admin.livetv.apiKeyHint")} />
          <div className="form-grid">
            <TextField label={t("admin.livetv.channelProfile")} value={profile} onChange={(e) => setProfile(e.target.value)} hint={t("admin.livetv.channelProfileHint")} />
            <TextField label={t("admin.livetv.outputProfile")} inputMode="numeric" value={outputProfile} onChange={(e) => setOutputProfile(e.target.value)} hint={t("admin.livetv.outputProfileHint")} />
          </div>
          <TextField label={t("admin.livetv.recordingsPath")} value={recPath} onChange={(e) => setRecPath(e.target.value)} mono placeholder="/media/recordings" hint={t("admin.livetv.recordingsPathHint")} />
        </>
      )}
      {kind === "xtream" && (
        <div className="form-grid">
          <TextField label={t("admin.livetv.username")} value={username} onChange={(e) => setUsername(e.target.value)} autoComplete="off" />
          <TextField label={t("admin.livetv.password")} type="password" autoComplete="off" value={password} onChange={(e) => setPassword(e.target.value)} placeholder={source?.has_password ? t("admin.livetv.keySaved") : ""} />
        </div>
      )}
      <TextField label={t("admin.livetv.epgUrl")} value={epg} onChange={(e) => setEpg(e.target.value)} mono hint={kind === "m3u" || kind === "hdhomerun" ? t("admin.livetv.epgHintRequired") : t("admin.livetv.epgHintOptional")} />
      <div className="form-grid">
        <TextField label={t("admin.livetv.maxStreams")} inputMode="numeric" value={maxStreams} onChange={(e) => setMaxStreams(e.target.value)} hint={t("admin.livetv.maxStreamsHint")} />
        <TextField label={t("admin.livetv.epgDays")} inputMode="numeric" value={days} onChange={(e) => setDays(e.target.value)} />
        <TextField label={t("admin.livetv.refreshHours")} inputMode="numeric" value={refresh} onChange={(e) => setRefresh(e.target.value)} />
        <TextField label={t("admin.livetv.epgRefreshHours")} inputMode="numeric" value={epgRefresh} onChange={(e) => setEpgRefresh(e.target.value)} />
      </div>
      <Switch checked={enabled} onChange={setEnabled} label={t("admin.livetv.enabled")} />
      {source?.last_refresh_at && <div className="t-small subtle">{t("admin.livetv.lastRefresh", { when: relative(source.last_refresh_at) })}</div>}
    </Dialog>
  );
}

function Channels() {
  const { t } = useI18n();
  const qc = useQueryClient();
  const [q, setQ] = useState("");
  const { data } = useQuery({ queryKey: ["admin", "livetv", "channels"], queryFn: () => api<{ items: LiveChannel[] }>("/api/v1/admin/livetv/channels").then((r) => r.items) });
  if (!data || data.length === 0) return null;
  const list = data.filter((c) => !q || c.name.toLowerCase().includes(q.toLowerCase()) || (c.group ?? "").toLowerCase().includes(q.toLowerCase()));
  return (
    <section className="stack">
      <div className="row row--between" style={{ alignItems: "flex-end" }}>
        <h2 className="t-h3">{t("admin.livetv.channels")} <span className="t-small subtle">{data.length}</span></h2>
        <div style={{ minWidth: 240 }}><TextField label={t("common.search")} type="search" value={q} onChange={(e) => setQ(e.target.value)} /></div>
      </div>
      <div className="t-small subtle">{t("admin.livetv.channelsHint")}</div>
      <div className="pl-panel tv-admin-channels">
        {list.slice(0, 500).map((c) => (
          <div key={c.id} className={`list-row${c.hidden ? " is-hidden" : ""}`}>
            <ChannelLogo c={c} size={32} />
            <div className="spacer" style={{ minWidth: 0 }}><div className="t-label">{chNumber(c.number)} {c.name}</div><div className="t-small subtle">{c.group ?? "—"}</div></div>
            {c.recordable && <Badge>{t("admin.livetv.recordable")}</Badge>}
            <button type="button" className="icon-btn" aria-label={c.hidden ? t("admin.livetv.show") : t("admin.livetv.hide")} title={c.hidden ? t("admin.livetv.show") : t("admin.livetv.hide")} onClick={async () => {
              await api(`/api/v1/admin/livetv/channels/${c.id}`, { method: "PATCH", body: { hidden: !c.hidden } });
              qc.invalidateQueries({ queryKey: ["admin", "livetv", "channels"] });
              qc.invalidateQueries({ queryKey: ["livetv"] });
            }}>{c.hidden ? <EyeOff /> : <Eye />}</button>
          </div>
        ))}
        {list.length > 500 && <div className="list-row t-small subtle">{t("admin.livetv.more", { count: list.length - 500 })}</div>}
      </div>
    </section>
  );
}

export default function LiveTVAdmin() {
  const { t, relative } = useI18n();
  const qc = useQueryClient();
  const [edit, setEdit] = useState<LiveSource | "new" | null>(null);
  const [del, setDel] = useState<LiveSource | null>(null);
  const { data } = useQuery({ queryKey: ["admin", "livetv", "sources"], queryFn: () => api<{ items: LiveSource[] }>("/api/v1/admin/livetv/sources").then((r) => r.items), refetchInterval: 10_000 });
  return (
    <div className="stack-lg">
      <div className="page-head" style={{ marginBottom: 0 }}>
        <div><h1 className="t-h1">{t("admin.livetv.title")}</h1><div className="page-head__sub t-small subtle">{t("admin.livetv.help")}</div></div>
        <button className="pl-btn pl-btn--primary" onClick={() => setEdit("new")}><Plus size={18} />{t("admin.livetv.add")}</button>
      </div>
      {data?.length === 0 && <Empty>{t("admin.livetv.empty")}</Empty>}
      <div className="stack">
        {data?.map((s) => (
          <div className="pl-panel panel-pad stack" key={s.id}>
            <div className="row row--between">
              <div style={{ minWidth: 0 }}>
                <div className="t-h3">{s.name} <Badge>{s.kind === "dispatcharr" ? "Dispatcharr" : s.kind === "m3u" ? "M3U" : s.kind === "hdhomerun" ? "HDHomeRun" : "Xtream"}</Badge> {!s.enabled && <Badge tone="danger">{t("admin.livetv.disabled")}</Badge>}</div>
                <div className="t-mono-sm subtle" style={{ overflowWrap: "anywhere" }}>{s.url}</div>
                <div className="t-small subtle">
                  {t("admin.livetv.summary", { channels: s.channels, streams: s.active_streams })}
                  {s.max_streams ? ` / ${s.max_streams}` : ""}
                  {s.last_epg_at ? ` · ${t("admin.livetv.epgAt", { when: relative(s.last_epg_at) })}` : ""}
                  {s.kind === "dispatcharr" && s.has_api_key ? ` · ${t("admin.livetv.recordable")}` : ""}
                </div>
                {s.last_message && <div className={`t-small ${s.last_status === "error" ? "danger-text" : "muted"}`}>{s.last_status === "ok" && <CheckCircle2 size={14} className="success-text" style={{ verticalAlign: -2 }} />} {s.last_message}</div>}
              </div>
              <div className="row" style={{ flexWrap: "nowrap" }}>
                <button className="icon-btn" aria-label={t("admin.livetv.refresh")} title={t("admin.livetv.refresh")} onClick={async () => { await api(`/api/v1/admin/livetv/sources/${s.id}/refresh`, { method: "POST" }); toast(t("admin.livetv.refreshQueued")); setTimeout(() => qc.invalidateQueries({ queryKey: ["admin", "livetv"] }), 3000); }}><RefreshCw /></button>
                <button className="icon-btn" aria-label={t("common.edit")} title={t("common.edit")} onClick={() => setEdit(s)}><Pencil /></button>
                <button className="icon-btn" aria-label={t("common.delete")} title={t("common.delete")} onClick={() => setDel(s)}><Trash2 /></button>
              </div>
            </div>
          </div>
        ))}
      </div>
      <Channels />
      {edit && <SourceDialog source={edit === "new" ? null : edit} onClose={() => setEdit(null)} />}
      {del && <Confirm danger title={t("admin.livetv.deleteTitle", { name: del.name })} message={t("admin.livetv.deleteMessage")} confirmLabel={t("common.delete")} onClose={() => setDel(null)} onConfirm={async () => {
        await api(`/api/v1/admin/livetv/sources/${del.id}`, { method: "DELETE" });
        qc.invalidateQueries({ queryKey: ["admin", "livetv"] });
        qc.invalidateQueries({ queryKey: ["livetv"] });
      }} />}
    </div>
  );
}
