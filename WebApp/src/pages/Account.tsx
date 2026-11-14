import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api, ApiError } from "../api/client";
import type { Device } from "../api/types";
import { Alert, Badge, SelectField, TextField, toast } from "../components/ui";
import { useSession } from "../session";
import { useI18n } from "../i18n";
import { LOCALES, type LocaleCode } from "../i18n/locales";
import { getTheme, setTheme, type Theme } from "../lib/theme";

export default function Account() {
  const { t, locale, setLocale, relative, date } = useI18n();
  const { me, logout } = useSession();
  const qc = useQueryClient();
  const [theme, setThemeState] = useState<Theme>(getTheme());
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [msg, setMsg] = useState<{ tone: "danger" | "success"; text: string } | null>(null);
  const { data: devices } = useQuery({ queryKey: ["me", "devices"], queryFn: () => api<{ items: Device[] }>("/api/v1/me/devices").then((r) => r.items) });
  return (
    <div className="stack-lg" style={{ maxWidth: 760 }}>
      <div className="page-head" style={{ marginBottom: 0 }}><h1 className="t-h1">{me?.username}</h1></div>
      {me?.permissions.expired ? <Alert tone="warning">{t("item.expired")}</Alert> : me?.permissions.expires_at ? <Alert>{t("account.expiresOn", { date: date(me.permissions.expires_at) })}</Alert> : null}

      <section className="pl-panel panel-pad stack">
        <h2 className="t-h3">{t("account.preferences")}</h2>
        <div className="form-grid">
          <SelectField label={t("account.language")} value={locale} onChange={(v) => setLocale(v as LocaleCode)} options={Object.entries(LOCALES).map(([k, v]) => ({ value: k, label: v.name }))} />
          <SelectField label={t("account.theme")} value={theme} onChange={(v) => { setTheme(v as Theme); setThemeState(v as Theme); }} options={[{ value: "dark", label: t("account.themeDark") }, { value: "light", label: t("account.themeLight") }, { value: "system", label: t("account.themeSystem") }]} />
        </div>
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

      <section className="pl-panel">
        <div className="panel-head"><h2 className="t-h3">{t("account.devices")}</h2></div>
        {devices?.map((d) => (
          <div className="list-row" key={d.id}>
            <div className="spacer">
              <div className="t-label">{d.device_name} {d.current && <Badge tone="direct">{t("account.thisDevice")}</Badge>}</div>
              <div className="t-small subtle">{d.client} {d.client_version} · {t("account.lastUsed", { when: relative(d.last_used_at) })}{d.last_ip ? ` · ${d.last_ip}` : ""}</div>
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
      </section>

      <div><button className="pl-btn pl-btn--ghost" onClick={logout}>{t("nav.logout")}</button></div>
    </div>
  );
}
