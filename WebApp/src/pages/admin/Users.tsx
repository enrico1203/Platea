import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, Pencil, Trash2 } from "lucide-react";
import { api, ApiError } from "../../api/client";
import type { Device, Library, Page, UserAdmin } from "../../api/types";
import { Alert, Badge, Confirm, Dialog, Switch, TextField, toast } from "../../components/ui";
import { useSession } from "../../session";
import { useI18n } from "../../i18n";

function UserDialog({ user, onClose }: { user: UserAdmin | null; onClose: () => void }) {
  const { t, relative } = useI18n();
  const qc = useQueryClient();
  const { data: libs } = useQuery({ queryKey: ["libraries", "admin"], queryFn: () => api<Page<Library>>("/api/v1/libraries").then((r) => r.items) });
  const { data: devices } = useQuery({ queryKey: ["admin", "devices", user?.id], queryFn: () => api<{ items: Device[] }>(`/api/v1/users/${user!.id}/devices`).then((r) => r.items), enabled: !!user });
  const [f, setF] = useState({
    username: user?.username ?? "", password: "", is_admin: user?.is_admin ?? false, disabled: user?.disabled ?? false,
    all_libraries: user?.all_libraries ?? true, library_ids: user?.library_ids ?? [], allow_remux: user?.allow_remux ?? true, allow_transcode: user?.allow_transcode ?? true,
    max_bitrate: user?.max_bitrate ? String(user.max_bitrate / 1_000_000) : "", max_streams: user?.max_streams ? String(user.max_streams) : "", allow_remote: user?.allow_remote ?? true, manage_devices: user?.manage_devices ?? true,
  });
  const [error, setError] = useState<string | null>(null);
  const set = <K extends keyof typeof f>(k: K, v: (typeof f)[K]) => setF({ ...f, [k]: v });
  const save = async () => {
    setError(null);
    const body: Record<string, unknown> = { ...f, max_bitrate: f.max_bitrate ? Math.round(Number(f.max_bitrate.replace(",", ".")) * 1_000_000) : null, max_streams: f.max_streams ? Number(f.max_streams) : null };
    if (!f.password) delete body.password;
    try {
      if (user) await api(`/api/v1/users/${user.id}`, { method: "PATCH", body });
      else await api("/api/v1/users", { body });
      qc.invalidateQueries({ queryKey: ["admin", "users"] });
      onClose();
    } catch (e) { setError((e as ApiError).message); }
  };
  return (
    <Dialog title={user ? user.username : t("admin.users.add")} onClose={onClose} footer={<>
      <button className="pl-btn pl-btn--ghost" onClick={onClose}>{t("common.cancel")}</button>
      <button className="pl-btn pl-btn--primary" onClick={save} disabled={!f.username || (!user && !f.password)}>{t("common.save")}</button>
    </>}>
      {error && <Alert tone="danger">{error}</Alert>}
      <div className="form-grid">
        <TextField label={t("auth.username")} value={f.username} onChange={(e) => set("username", e.target.value)} autoComplete="off" />
        <TextField label={user ? t("admin.users.newPassword") : t("auth.password")} type="password" value={f.password} onChange={(e) => set("password", e.target.value)} autoComplete="new-password" />
      </div>
      <Switch checked={f.is_admin} onChange={(v) => set("is_admin", v)} label={t("admin.users.isAdmin")} />
      <Switch checked={f.disabled} onChange={(v) => set("disabled", v)} label={t("admin.users.disabled")} />
      <span className="pl-eyebrow">{t("admin.users.permissions")}</span>
      <Switch checked={f.all_libraries} onChange={(v) => set("all_libraries", v)} label={t("admin.users.allLibraries")} />
      {!f.all_libraries && (
        <div className="stack" style={{ gap: 8 }}>
          {libs?.map((l) => (
            <label className="checkbox" key={l.id}>
              <input type="checkbox" checked={f.library_ids.includes(l.id)} onChange={(e) => set("library_ids", e.target.checked ? [...f.library_ids, l.id] : f.library_ids.filter((x) => x !== l.id))} />
              {l.name}
            </label>
          ))}
        </div>
      )}
      <Switch checked={f.allow_remux} onChange={(v) => set("allow_remux", v)} label={t("admin.users.allowRemux")} />
      <Switch checked={f.allow_transcode} onChange={(v) => set("allow_transcode", v)} label={t("admin.users.allowTranscode")} />
      <Switch checked={f.allow_remote} onChange={(v) => set("allow_remote", v)} label={t("admin.users.allowRemote")} />
      <Switch checked={f.manage_devices} onChange={(v) => set("manage_devices", v)} label={t("admin.users.manageDevices")} />
      <div className="form-grid">
        <TextField label={t("admin.users.maxBitrate")} inputMode="decimal" value={f.max_bitrate} onChange={(e) => set("max_bitrate", e.target.value)} hint={t("admin.users.unlimitedHint")} />
        <TextField label={t("admin.users.maxStreams")} inputMode="numeric" value={f.max_streams} onChange={(e) => set("max_streams", e.target.value)} hint={t("admin.users.unlimitedHint")} />
      </div>
      {user && devices && devices.length > 0 && (
        <>
          <span className="pl-eyebrow">{t("account.devices")}</span>
          {devices.map((d) => (
            <div className="row" key={d.id}>
              <div className="spacer"><div className="t-label">{d.device_name}</div><div className="t-small subtle">{d.client} · {relative(d.last_used_at)}{d.last_ip ? ` · ${d.last_ip}` : ""}</div></div>
              <button className="pl-btn pl-btn--danger pl-btn--sm" onClick={async () => { await api(`/api/v1/users/${user.id}/devices/${d.id}`, { method: "DELETE" }); qc.invalidateQueries({ queryKey: ["admin", "devices", user.id] }); toast(t("account.revoked")); }}>{t("account.revoke")}</button>
            </div>
          ))}
        </>
      )}
    </Dialog>
  );
}

export default function Users() {
  const { t, relative } = useI18n();
  const { me } = useSession();
  const qc = useQueryClient();
  const { data } = useQuery({ queryKey: ["admin", "users"], queryFn: () => api<Page<UserAdmin>>("/api/v1/users").then((r) => r.items) });
  const [edit, setEdit] = useState<UserAdmin | null | "new">(null);
  const [del, setDel] = useState<UserAdmin | null>(null);
  const [err, setErr] = useState<string | null>(null);
  return (
    <div className="stack-lg">
      <div className="page-head" style={{ marginBottom: 0 }}>
        <h1 className="t-h1">{t("admin.users.title")}</h1>
        <button className="pl-btn pl-btn--primary" onClick={() => setEdit("new")}><Plus />{t("admin.users.add")}</button>
      </div>
      {err && <Alert tone="danger">{err}</Alert>}
      <div className="pl-panel table-wrap">
        <table className="table">
          <thead><tr><th>{t("auth.username")}</th><th>{t("admin.users.role")}</th><th>{t("admin.users.lastLogin")}</th><th>{t("account.devices")}</th><th /></tr></thead>
          <tbody>
            {data?.map((u) => (
              <tr key={u.id}>
                <td className="t-label">{u.username} {u.id === me?.id && <span className="subtle t-small">({t("admin.users.you")})</span>}</td>
                <td>{u.is_admin ? <Badge>{t("admin.users.admin")}</Badge> : t("admin.users.user")} {u.disabled && <Badge tone="danger">{t("admin.users.disabledBadge")}</Badge>}</td>
                <td className="t-small muted">{u.last_login_at ? relative(u.last_login_at) : "—"}</td>
                <td className="t-mono">{u.devices}{u.active_streams > 0 ? ` · ${t("admin.users.streaming", { count: u.active_streams })}` : ""}</td>
                <td style={{ textAlign: "right", whiteSpace: "nowrap" }}>
                  <button className="icon-btn" onClick={() => setEdit(u)} aria-label={t("common.edit")}><Pencil /></button>
                  {u.id !== me?.id && <button className="icon-btn" onClick={() => setDel(u)} aria-label={t("common.delete")}><Trash2 /></button>}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {edit && <UserDialog user={edit === "new" ? null : edit} onClose={() => setEdit(null)} />}
      {del && <Confirm danger title={t("admin.users.deleteTitle", { name: del.username })} message={t("admin.users.deleteMessage")} confirmLabel={t("common.delete")} onClose={() => setDel(null)} onConfirm={async () => {
        try { await api(`/api/v1/users/${del.id}`, { method: "DELETE" }); qc.invalidateQueries({ queryKey: ["admin", "users"] }); } catch (e) { setErr((e as ApiError).message); }
      }} />}
    </div>
  );
}
