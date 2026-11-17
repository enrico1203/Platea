import { useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Camera, Trash2, ArrowUp, ArrowDown } from "lucide-react";
import { api, ApiError, getToken, serverUrl } from "../api/client";
import type { Device, UserPrefs } from "../api/types";
import { Alert, Badge, SelectField, Switch, TextField, toast } from "../components/ui";
import { Avatar, useLibraries } from "../components/Layout";
import { LinkDevice } from "../components/QuickConnect";
import { useSession } from "../session";
import { useI18n } from "../i18n";
import { useClientLabel } from "../components/Logo";
import { LOCALES, type LocaleCode } from "../i18n/locales";
import { getTheme, setTheme, type Theme } from "../lib/theme";
import { DEFAULT_PREFS, LANGUAGES, languageName, savePrefs } from "../lib/prefs";

export default function Account() {
  const { t, locale, setLocale, relative, date, tag, bitrate } = useI18n();
  const clientLabel = useClientLabel();
  const { me, logout, reload, info } = useSession();
  const qc = useQueryClient();
  const { data: libs } = useLibraries();
  const [theme, setThemeState] = useState<Theme>(getTheme());
  const [prefs, setPrefs] = useState<UserPrefs>({ ...DEFAULT_PREFS, ...me?.preferences });
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [allDevices, setAllDevices] = useState(false);
  const [msg, setMsg] = useState<{ tone: "danger" | "success"; text: string } | null>(null);
  const file = useRef<HTMLInputElement>(null);
  const { data: devices } = useQuery({ queryKey: ["me", "devices"], queryFn: () => api<{ items: Device[] }>("/api/v1/me/devices").then((r) => r.items) });

  // Ogni modifica si salva subito: niente pulsante "Salva" da dimenticare.
  const update = async (patch: Partial<UserPrefs>) => {
    const nextPrefs = { ...prefs, ...patch };
    setPrefs(nextPrefs);
    try {
      setPrefs(await savePrefs(nextPrefs));
      qc.invalidateQueries({ queryKey: ["home"] });
      await reload();
    } catch (e) { toast((e as ApiError).message); }
  };
  const langOptions = (empty: string) => [{ value: "", label: empty }, ...LANGUAGES.map((l) => ({ value: l, label: languageName(l, tag) }))];
  const moveSection = (s: string, d: -1 | 1) => {
    const list = [...prefs.home_sections];
    const i = list.indexOf(s);
    if (i < 0 || i + d < 0 || i + d >= list.length) return;
    [list[i], list[i + d]] = [list[i + d], list[i]];
    update({ home_sections: list });
  };
  const allSections = ["resume", "next_up", "recent"];

  return (
    <div className="stack-lg" style={{ maxWidth: 760 }}>
      <div className="profile-head">
        <button className="profile-head__avatar" onClick={() => file.current?.click()} aria-label={t("account.changeAvatar")} title={t("account.changeAvatar")}>
          <Avatar name={me?.username ?? ""} url={me?.avatar ?? null} size={88} />
          <span className="profile-head__camera"><Camera size={16} /></span>
        </button>
        <input ref={file} type="file" accept="image/jpeg,image/png,image/webp" hidden onChange={async (e) => {
          const f = e.target.files?.[0];
          e.target.value = "";
          if (!f) return;
          const res = await fetch(serverUrl() + "/api/v1/me/avatar", { method: "POST", headers: { Authorization: `Bearer ${getToken() ?? ""}` }, body: f });
          if (!res.ok) { toast(((await res.json().catch(() => ({}))) as { message?: string }).message ?? res.statusText); return; }
          await reload();
          toast(t("account.avatarSaved"));
        }} />
        <div>
          <h1 className="t-h1">{me?.username}</h1>
          <div className="t-small subtle">{me?.is_admin ? t("account.roleAdmin") : t("account.roleUser")}</div>
          {me?.avatar && <button className="pl-btn pl-btn--ghost pl-btn--sm" style={{ marginTop: 8, paddingLeft: 0 }} onClick={async () => { await api("/api/v1/me/avatar", { method: "DELETE" }); await reload(); }}><Trash2 />{t("account.removeAvatar")}</button>}
        </div>
      </div>
      {me?.permissions.expired ? <Alert tone="warning">{t("item.expired")}</Alert> : me?.permissions.expires_at ? <Alert>{t("account.expiresOn", { date: date(me.permissions.expires_at) })}</Alert> : null}
      {me && !me.permissions.within_schedule && <Alert tone="warning">{t("account.outsideSchedule")}</Alert>}

      <section className="pl-panel panel-pad stack">
        <h2 className="t-h3">{t("account.appearance")}</h2>
        <div className="form-grid">
          <SelectField label={t("account.language")} value={locale} onChange={(v) => setLocale(v as LocaleCode)} options={Object.entries(LOCALES).map(([k, v]) => ({ value: k, label: v.name }))} />
          <SelectField label={t("account.theme")} value={theme} onChange={(v) => { setTheme(v as Theme); setThemeState(v as Theme); update({ theme: v === "system" ? "auto" : v }); }} options={[{ value: "dark", label: t("account.themeDark") }, { value: "light", label: t("account.themeLight") }, { value: "system", label: t("account.themeSystem") }]} />
        </div>
      </section>

      <section className="pl-panel panel-pad stack">
        <h2 className="t-h3">{t("account.playback")}</h2>
        <div className="form-grid">
          <SelectField label={t("account.audioLanguage")} value={prefs.audio_language} onChange={(v) => update({ audio_language: v })} options={langOptions(t("account.fileDefault"))} />
          <SelectField label={t("account.subtitleLanguage")} value={prefs.subtitle_language} onChange={(v) => update({ subtitle_language: v })} options={langOptions(t("account.uiLanguage"))} />
        </div>
        <SelectField label={t("account.subtitleMode")} value={prefs.subtitle_mode} onChange={(v) => update({ subtitle_mode: v as UserPrefs["subtitle_mode"] })} hint={t(`account.subtitleModeHint.${prefs.subtitle_mode}` as "account.subtitleModeHint.default")} options={(["default", "smart", "always", "only_forced", "none"] as const).map((m) => ({ value: m, label: t(`account.subtitleModes.${m}` as "account.subtitleModes.default") }))} />
        <div className="form-grid">
          <SelectField label={t("account.subtitleSize")} value={prefs.subtitle_size} onChange={(v) => update({ subtitle_size: v as UserPrefs["subtitle_size"] })} options={(["s", "m", "l", "xl"] as const).map((s) => ({ value: s, label: t(`account.sizes.${s}` as "account.sizes.m") }))} />
          <div style={{ alignSelf: "end" }}><Switch checked={prefs.subtitle_background} onChange={(v) => update({ subtitle_background: v })} label={t("account.subtitleBackground")} /></div>
        </div>
        <div className="form-grid">
          {(["max_bitrate_local", "max_bitrate_remote"] as const).map((k) => (
            <SelectField key={k} label={t(k === "max_bitrate_local" ? "account.qualityLocal" : "account.qualityRemote")} value={String(prefs[k])} onChange={(v) => update({ [k]: Number(v) })}
              options={[0, 40_000_000, 20_000_000, 8_000_000, 4_000_000, 2_000_000, 1_000_000].map((b) => ({ value: String(b), label: b === 0 ? t("player.original") : bitrate(b) }))} />
          ))}
        </div>
        <p className="t-small subtle" style={{ marginTop: -8 }}>{t("account.qualityHint")}</p>
        <Switch checked={prefs.remember_tracks} onChange={(v) => update({ remember_tracks: v })} label={t("account.rememberTracks")} hint={t("account.rememberTracksHint")} />
        <Switch checked={prefs.autoplay_next} onChange={(v) => update({ autoplay_next: v })} label={t("account.autoplay")} hint={t("account.autoplayHint")} />
        <div className="form-grid">
          <SelectField label={t("account.countdown")} value={String(prefs.autoplay_countdown_seconds)} onChange={(v) => update({ autoplay_countdown_seconds: Number(v) })} options={[0, 5, 10, 15, 20, 30].map((s) => ({ value: String(s), label: s === 0 ? t("account.immediately") : t("account.seconds", { s }) }))} />
          <div />
          <SelectField label={t("account.skipBack")} value={String(prefs.skip_back_seconds)} onChange={(v) => update({ skip_back_seconds: Number(v) })} options={[5, 10, 15, 30].map((s) => ({ value: String(s), label: t("account.seconds", { s }) }))} />
          <SelectField label={t("account.skipForward")} value={String(prefs.skip_forward_seconds)} onChange={(v) => update({ skip_forward_seconds: Number(v) })} options={[10, 15, 30, 60, 90].map((s) => ({ value: String(s), label: t("account.seconds", { s }) }))} />
        </div>
      </section>

      <section className="pl-panel panel-pad stack">
        <h2 className="t-h3">{t("account.home")}</h2>
        <div className="pl-panel">
          {[...prefs.home_sections, ...allSections.filter((s) => !prefs.home_sections.includes(s))].map((s) => {
            const on = prefs.home_sections.includes(s);
            return (
              <div className="list-row" key={s}>
                <div className="spacer"><Switch checked={on} onChange={(v) => update({ home_sections: v ? [...prefs.home_sections, s] : prefs.home_sections.filter((x) => x !== s) })} label={t(`account.sections.${s}` as "account.sections.resume")} /></div>
                {on && <>
                  <button className="icon-btn" disabled={prefs.home_sections.indexOf(s) === 0} onClick={() => moveSection(s, -1)} aria-label={t("playlist.up")}><ArrowUp /></button>
                  <button className="icon-btn" disabled={prefs.home_sections.indexOf(s) === prefs.home_sections.length - 1} onClick={() => moveSection(s, 1)} aria-label={t("playlist.down")}><ArrowDown /></button>
                </>}
              </div>
            );
          })}
        </div>
        <Switch checked={prefs.hide_played_in_recent} onChange={(v) => update({ hide_played_in_recent: v })} label={t("account.hidePlayed")} />
        {libs && libs.length > 1 && (
          <div className="stack" style={{ gap: 8 }}>
            <span className="pl-eyebrow">{t("account.homeLibraries")}</span>
            {libs.map((l) => (
              <label className="checkbox" key={l.id}>
                <input type="checkbox" checked={!prefs.home_hidden_libraries.includes(l.id)} onChange={(e) => update({ home_hidden_libraries: e.target.checked ? prefs.home_hidden_libraries.filter((x) => x !== l.id) : [...prefs.home_hidden_libraries, l.id] })} />
                {l.name}
              </label>
            ))}
          </div>
        )}
      </section>

      <section className="pl-panel panel-pad">
        <form className="form" onSubmit={async (e) => {
          e.preventDefault();
          try {
            await api("/api/v1/me/password", { body: { current, new: next } });
            setMsg({ tone: "success", text: t("account.passwordChanged") });
            setCurrent(""); setNext("");
            qc.invalidateQueries({ queryKey: ["me", "devices"] });
          } catch (err) { setMsg({ tone: "danger", text: (err as ApiError).message }); }
        }}>
          <h2 className="t-h3">{t("account.password")}</h2>
          {msg && <Alert tone={msg.tone}>{msg.text}</Alert>}
          <div className="form-grid">
            <TextField label={t("account.currentPassword")} type="password" autoComplete="current-password" value={current} onChange={(e) => setCurrent(e.target.value)} />
            <TextField label={t("account.newPassword")} type="password" autoComplete="new-password" value={next} onChange={(e) => setNext(e.target.value)} hint={t("account.passwordHint")} />
          </div>
          <div><button className="pl-btn pl-btn--secondary" disabled={!current || !next}>{t("account.changePassword")}</button></div>
        </form>
      </section>

      {info?.quick_connect && <section className="pl-panel panel-pad" id="link"><LinkDevice /></section>}

      <section className="pl-panel">
        <div className="panel-head row row--between">
          <h2 className="t-h3">{t("account.devices")}{devices ? <span className="t-small subtle"> · {devices.length}</span> : null}</h2>
          {devices && devices.length > 1 && (
            <button className="pl-btn pl-btn--ghost pl-btn--sm" onClick={async () => {
              const r = await api<{ revoked: number }>("/api/v1/me/devices/revoke-others", { method: "POST" });
              qc.invalidateQueries({ queryKey: ["me", "devices"] });
              toast(t("account.revokedOthers", { count: r.revoked }));
            }}>{t("account.revokeOthers")}</button>
          )}
        </div>
        {/* Il dispositivo in uso e i più recenti; gli altri a richiesta. */}
        {devices?.filter((d, i) => allDevices || d.current || i < 10).map((d) => (
          <div className="list-row" key={d.id}>
            <div className="spacer">
              <div className="t-label">{d.device_name} {d.current && <Badge tone="direct">{t("account.thisDevice")}</Badge>}</div>
              <div className="t-small subtle">{clientLabel(d.client)} {d.client_version} · {t("account.lastUsed", { when: relative(d.last_used_at) })}{d.last_ip ? ` · ${d.last_ip}` : ""}</div>
            </div>
            {!d.current && (
              <button className="pl-btn pl-btn--danger pl-btn--sm" onClick={async () => {
                await api(`/api/v1/me/devices/${d.id}`, { method: "DELETE" });
                qc.invalidateQueries({ queryKey: ["me", "devices"] });
                toast(t("account.revoked"));
              }}>{t("account.revoke")}</button>
            )}
          </div>
        ))}
        {devices && devices.length > 10 && !allDevices && (
          <div className="list-row"><button className="pl-btn pl-btn--ghost pl-btn--sm" onClick={() => setAllDevices(true)}>{t("account.showAllDevices", { count: devices.length })}</button></div>
        )}
      </section>

      <div><button className="pl-btn pl-btn--ghost" onClick={logout}>{t("nav.logout")}</button></div>
    </div>
  );
}
