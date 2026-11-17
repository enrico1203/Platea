import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, Pencil, Trash2 } from "lucide-react";
import { api, ApiError } from "../../api/client";
import type { Device, Library, Page, ScheduleSlot, UserAdmin } from "../../api/types";
import { Alert, Badge, Confirm, Dialog, Empty, MethodBadge, SelectField, Switch, TextField, toast } from "../../components/ui";
import { useSession } from "../../session";
import { useI18n } from "../../i18n";
import { useClientLabel } from "../../components/Logo";

function toDateInput(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

function UserDialog({ user, onClose }: { user: UserAdmin | null; onClose: () => void }) {
  const { t } = useI18n();
  const qc = useQueryClient();
  const { data: libs } = useQuery({ queryKey: ["libraries", "admin"], queryFn: () => api<Page<Library>>("/api/v1/libraries").then((r) => r.items) });
  const [f, setF] = useState({
    username: user?.username ?? "", password: "", is_admin: user?.is_admin ?? false, disabled: user?.disabled ?? false,
    all_libraries: user?.all_libraries ?? true, library_ids: user?.library_ids ?? [], allow_remux: user?.allow_remux ?? true, allow_transcode: user?.allow_transcode ?? true,
    max_bitrate: user?.max_bitrate ? String(user.max_bitrate / 1_000_000) : "", max_streams: user?.max_streams ? String(user.max_streams) : "", allow_remote: user?.allow_remote ?? true, manage_devices: user?.manage_devices ?? true,
    expires: user?.expires_at ? toDateInput(user.expires_at) : "",
    max_rating: user?.max_rating ?? "",
    allow_download: user?.allow_download ?? true,
    block_unrated: user?.block_unrated ?? false,
    access_schedule: user?.access_schedule ?? [],
    blocked_genres: user?.blocked_genres ?? [],
    allowed_genres: user?.allowed_genres ?? [],
  });
  const { data: genres } = useQuery({ queryKey: ["genres", "all"], queryFn: () => api<{ items: { name: string; count: number }[] }>("/api/v1/genres").then((r) => r.items) });
  const [error, setError] = useState<string | null>(null);
  const set = <K extends keyof typeof f>(k: K, v: (typeof f)[K]) => setF({ ...f, [k]: v });
  const save = async () => {
    setError(null);
    // La scadenza vale fino alla fine del giorno scelto, ora locale.
    const expires_at = f.expires ? new Date(`${f.expires}T23:59:59`).getTime() : null;
    const { expires: _e, ...rest } = f;
    void _e;
    const body: Record<string, unknown> = { ...rest, expires_at, max_rating: f.max_rating || null, max_bitrate: f.max_bitrate ? Math.round(Number(f.max_bitrate.replace(",", ".")) * 1_000_000) : null, max_streams: f.max_streams ? Number(f.max_streams) : null };
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
      <Switch checked={f.allow_download} onChange={(v) => set("allow_download", v)} label={t("admin.users.allowDownload")} hint={t("admin.users.allowDownloadHint")} />
      <Switch checked={f.block_unrated} onChange={(v) => set("block_unrated", v)} label={t("admin.users.blockUnrated")} hint={t("admin.users.blockUnratedHint")} />
      <ScheduleEditor value={f.access_schedule} onChange={(v) => set("access_schedule", v)} />
      {genres && genres.length > 0 && (["blocked_genres", "allowed_genres"] as const).map((k) => (
        <div key={k} className="stack" style={{ gap: 6 }}>
          <span className="pl-field__label">{t(k === "blocked_genres" ? "admin.users.blockedGenres" : "admin.users.allowedGenres")}</span>
          <span className="pl-field__hint">{t(k === "blocked_genres" ? "admin.users.blockedGenresHint" : "admin.users.allowedGenresHint")}</span>
          <div className="genre-picker">
            {genres.map((g) => {
              const on = f[k].includes(g.name);
              return <button key={g.name} type="button" className={`genre-chip${on ? " on" : ""}`} aria-pressed={on} onClick={() => set(k, on ? f[k].filter((x) => x !== g.name) : [...f[k], g.name])}>{g.name}</button>;
            })}
          </div>
        </div>
      ))}
      <SelectField label={t("admin.users.maxRating")} value={f.max_rating} onChange={(v) => set("max_rating", v)} hint={t("admin.users.maxRatingHint")} options={[
        { value: "", label: t("admin.users.ratingNone") },
        ...["0", "6", "10", "12", "14", "16"].map((a) => ({ value: a, label: a === "0" ? t("admin.users.ratingAll") : t("admin.users.ratingAge", { age: a }) })),
      ]} />
      <TextField label={t("admin.users.expires")} type="date" value={f.expires} onChange={(e) => set("expires", e.target.value)} hint={t("admin.users.expiresHint")} />
      <div className="form-grid">
        <TextField label={t("admin.users.maxBitrate")} inputMode="decimal" value={f.max_bitrate} onChange={(e) => set("max_bitrate", e.target.value)} hint={t("admin.users.unlimitedHint")} />
        <TextField label={t("admin.users.maxStreams")} inputMode="numeric" value={f.max_streams} onChange={(e) => set("max_streams", e.target.value)} hint={t("admin.users.unlimitedHint")} />
      </div>
    </Dialog>
  );
}

export default function Users() {
  const { t, relative, date } = useI18n();
  const { me } = useSession();
  const qc = useQueryClient();
  const { data } = useQuery({ queryKey: ["admin", "users"], queryFn: () => api<Page<UserAdmin>>("/api/v1/users").then((r) => r.items) });
  const [edit, setEdit] = useState<UserAdmin | null | "new">(null);
  const [devicesOf, setDevicesOf] = useState<UserAdmin | null>(null);
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
                <td>{u.is_admin ? <Badge>{t("admin.users.admin")}</Badge> : t("admin.users.user")} {u.disabled && <Badge tone="danger">{t("admin.users.disabledBadge")}</Badge>}
                  {u.expired ? <Badge tone="danger">{t("admin.users.expiredBadge")}</Badge> : u.expires_at ? <span className="t-small subtle"> {t("admin.users.expiresOn", { date: date(u.expires_at) })}</span> : null}</td>
                <td className="t-small muted">{u.last_login_at ? relative(u.last_login_at) : "—"}</td>
                <td>
                  <button className="link-btn" onClick={() => setDevicesOf(u)} disabled={u.devices === 0}>
                    {t("admin.users.devicesCount", { count: u.devices })}
                  </button>
                  {u.active_streams > 0 && <span className="t-small subtle"> · {t("admin.users.streaming", { count: u.active_streams })}</span>}
                </td>
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
      {devicesOf && <UserDevices user={devicesOf} onClose={() => setDevicesOf(null)} />}
      {del && <Confirm danger title={t("admin.users.deleteTitle", { name: del.username })} message={t("admin.users.deleteMessage")} confirmLabel={t("common.delete")} onClose={() => setDel(null)} onConfirm={async () => {
        try { await api(`/api/v1/users/${del.id}`, { method: "DELETE" }); qc.invalidateQueries({ queryKey: ["admin", "users"] }); } catch (e) { setErr((e as ApiError).message); }
      }} />}
    </div>
  );
}

const DAY_ORDER = [1, 2, 3, 4, 5, 6, 0];
const toHHMM = (m: number) => `${String(Math.floor(m / 60) % 24).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
const fromHHMM = (s: string) => { const [h, m] = s.split(":").map(Number); return (h || 0) * 60 + (m || 0); };

/** Fasce orarie in cui l'utente può riprodurre (ora del server). Nessuna fascia = sempre. */
function ScheduleEditor({ value, onChange }: { value: ScheduleSlot[]; onChange: (v: ScheduleSlot[]) => void }) {
  const { t, tag } = useI18n();
  const dayName = (d: number) => new Intl.DateTimeFormat(tag, { weekday: "short" }).format(new Date(2026, 10, 15 + d));
  const upd = (i: number, patch: Partial<ScheduleSlot>) => onChange(value.map((s, j) => (j === i ? { ...s, ...patch } : s)));
  return (
    <div className="stack" style={{ gap: 8 }}>
      <span className="pl-eyebrow">{t("admin.users.schedule")}</span>
      <span className="t-small subtle">{value.length === 0 ? t("admin.users.scheduleAlways") : t("admin.users.scheduleHint")}</span>
      {value.map((s, i) => (
        <div key={i} className="schedule-row">
          <div className="schedule-days">
            {DAY_ORDER.map((d) => (
              <button type="button" key={d} className={`chip${s.days.includes(d) ? " on" : ""}`} onClick={() => upd(i, { days: s.days.includes(d) ? s.days.filter((x) => x !== d) : [...s.days, d] })}>{dayName(d)}</button>
            ))}
          </div>
          <div className="row" style={{ flexWrap: "nowrap" }}>
            <span className="pl-field__control"><input type="time" value={toHHMM(s.start)} onChange={(e) => upd(i, { start: fromHHMM(e.target.value) })} aria-label={t("admin.users.from")} /></span>
            <span className="subtle">–</span>
            <span className="pl-field__control"><input type="time" value={toHHMM(s.end === 1440 ? 0 : s.end)} onChange={(e) => upd(i, { end: fromHHMM(e.target.value) || 1440 })} aria-label={t("admin.users.to")} /></span>
            <button type="button" className="icon-btn" onClick={() => onChange(value.filter((_, j) => j !== i))} aria-label={t("common.delete")}><Trash2 /></button>
          </div>
        </div>
      ))}
      <div><button type="button" className="pl-btn pl-btn--ghost pl-btn--sm" onClick={() => onChange([...value, { days: [1, 2, 3, 4, 5], start: 16 * 60, end: 21 * 60 }])}><Plus />{t("admin.users.addSlot")}</button></div>
    </div>
  );
}

/** Dispositivi di un utente: cosa usa, da dove, cosa guarda o ha guardato, e scollegamento. */
function UserDevices({ user, onClose }: { user: UserAdmin; onClose: () => void }) {
  const { t, relative, dateTime } = useI18n();
  const clientLabel = useClientLabel();
  const qc = useQueryClient();
  const { data: devices } = useQuery({ queryKey: ["admin", "devices", user.id], queryFn: () => api<{ items: Device[] }>(`/api/v1/users/${user.id}/devices`).then((r) => r.items) });
  const refresh = () => { qc.invalidateQueries({ queryKey: ["admin", "devices", user.id] }); qc.invalidateQueries({ queryKey: ["admin", "users"] }); };
  return (
    <Dialog title={t("admin.users.devicesOf", { name: user.username })} onClose={onClose} footer={devices && devices.length > 1 ? (
      <button className="pl-btn pl-btn--danger" onClick={async () => { await api(`/api/v1/users/${user.id}/devices`, { method: "DELETE" }); refresh(); toast(t("account.revoked")); }}>{t("admin.users.revokeAll")}</button>
    ) : undefined}>
      {!devices ? <div className="t-small subtle">{t("common.loading")}</div> : devices.length === 0 ? <Empty>{t("admin.users.noDevices")}</Empty> : (
        <div className="device-list">
          {devices.map((d) => (
            <div className="device" key={d.id}>
              <div className="device__main">
                <div className="t-label">{d.device_name}</div>
                <div className="t-small muted">{clientLabel(d.client)}{d.client_version ? ` ${d.client_version}` : ""}</div>
                {d.playing && (
                  <div className="t-small device__playing">
                    {d.playing.paused ? t("admin.users.devicePaused") : t("admin.users.deviceWatching")}: {d.playing.title}{d.playing.subtitle ? ` · ${d.playing.subtitle}` : ""} <MethodBadge method={d.playing.method} />
                  </div>
                )}
                <div className="t-small subtle">
                  {t("admin.users.deviceLastSeen", { when: relative(d.last_used_at) })}
                  {d.last_ip ? ` · ${d.last_ip}${d.remote == null ? "" : d.remote ? ` (${t("admin.overview.session.remote")})` : ` (${t("admin.overview.session.local")})`}` : ""}
                </div>
                <div className="t-small subtle">
                  {t("admin.users.deviceSince", { when: dateTime(d.created_at) })}
                  {d.plays ? ` · ${t("admin.users.devicePlays", { count: d.plays })}` : ""}
                  {d.last_played_title ? ` · ${t("admin.users.deviceLastPlayed", { title: d.last_played_title, when: relative(d.last_played_at ?? null) })}` : ""}
                </div>
              </div>
              <button className="pl-btn pl-btn--danger pl-btn--sm" onClick={async () => { await api(`/api/v1/users/${user.id}/devices/${d.id}`, { method: "DELETE" }); refresh(); toast(t("account.revoked")); }}>{t("account.revoke")}</button>
            </div>
          ))}
        </div>
      )}
    </Dialog>
  );
}

