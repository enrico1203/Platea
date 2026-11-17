import { useEffect, useRef, useState } from "react";
import { Trash2, Upload } from "lucide-react";
import { Mark } from "../../components/Logo";
import { useNavigate, useParams } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api, ApiError, getToken, serverUrl } from "../../api/client";
import { Alert, SelectField, Switch, TextField, toast, TextAreaField } from "../../components/ui";
import { useI18n, type MessageKey } from "../../i18n";
import { useSession } from "../../session";

type Schema = { title?: string; description?: string; type?: string | string[]; enum?: string[]; items?: Schema; minimum?: number; maximum?: number; format?: string; $ref?: string; allOf?: Schema[]; writeOnly?: boolean; "x-restart"?: boolean; "x-multiline"?: boolean; properties?: Record<string, Schema>; definitions?: Record<string, Schema> };
interface SettingsData { schema: Record<string, Schema>; values: Record<string, Record<string, unknown>>; groups: string[] }

function resolve(s: Schema, root: Schema): Schema {
  const ref = s.$ref ?? s.allOf?.[0]?.$ref;
  if (ref) {
    const name = ref.split("/").pop()!;
    return { ...root.definitions?.[name], ...s, $ref: undefined, allOf: undefined };
  }
  return s;
}

/**
 * Moduli generati dallo schema delle impostazioni: una nuova impostazione nel server compare qui
 * senza scrivere codice di interfaccia. Le etichette si traducono con le chiavi `settings.<gruppo>.<campo>`;
 * in mancanza si usa il titolo dello schema.
 */
function GroupForm({ group, schema, values, onSaved }: { group: string; schema: Schema; values: Record<string, unknown>; onSaved: () => void }) {
  const { t, locale } = useI18n();
  const [draft, setDraft] = useState<Record<string, unknown>>({});
  const [error, setError] = useState<string | null>(null);
  useEffect(() => setDraft({}), [group, values]);
  const label = (field: string, s: Schema) => {
    const k = `settings.${group}.${field}` as MessageKey;
    const v = t(k);
    return v === k ? s.title ?? field : v;
  };
  const hint = (field: string, s: Schema) => {
    const k = `settings.${group}.${field}_hint` as MessageKey;
    const v = t(k);
    // Le descrizioni dello schema arrivano dal server in italiano: in altre lingue si usano solo le traduzioni.
    return v === k ? (locale === "it" ? s.description : undefined) : v;
  };
  const val = (f: string) => (f in draft ? draft[f] : values[f]);
  const set = (f: string, v: unknown) => setDraft({ ...draft, [f]: v });
  const save = async (patch: Record<string, unknown>) => {
    setError(null);
    try {
      await api(`/api/v1/admin/settings/${group}`, { method: "PUT", body: patch });
      toast(t("admin.settings.saved"));
      onSaved();
    } catch (e) { setError((e as ApiError).message); }
  };
  const fields = Object.entries(schema.properties ?? {});
  const dirty = Object.keys(draft).length > 0;
  return (
    <form className="form" onSubmit={(e) => { e.preventDefault(); save(draft); }}>
      {error && <Alert tone="danger">{error}</Alert>}
      {fields.map(([field, raw]) => {
        const s = resolve(raw, schema);
        const type = Array.isArray(s.type) ? s.type.find((x) => x !== "null") : s.type;
        const restart = s["x-restart"] ? ` · ${t("admin.settings.restart")}` : "";
        if (type === "boolean") {
          // Lo switch applica subito.
          return <Switch key={field} checked={!!val(field)} onChange={(v) => save({ [field]: v })} label={label(field, s)} hint={(hint(field, s) ?? "") + restart} />;
        }
        if (s.enum) {
          return <SelectField key={field} label={label(field, s)} hint={hint(field, s)} value={String(val(field) ?? "")} onChange={(v) => set(field, v)} options={s.enum.map((o) => {
            const k = `settings.enum.${o}` as MessageKey;
            const l = t(k);
            return { value: o, label: l === k ? o : l };
          })} />;
        }
        if (s.writeOnly) {
          const isSet = !!values[`${field}_set`];
          return <TextField key={field} label={label(field, s)} mono type="password" autoComplete="off" placeholder={isSet ? t("admin.settings.secretSet") : ""} value={String(draft[field] ?? "")} onChange={(e) => set(field, e.target.value)} hint={isSet ? t("admin.settings.secretHint") : hint(field, s)} />;
        }
        if (type === "array") {
          const arr = (val(field) as string[] | undefined) ?? [];
          return <TextField key={field} label={label(field, s)} mono value={field in draft ? String(draft[field + "__raw"] ?? arr.join(", ")) : arr.join(", ")} onChange={(e) => setDraft({ ...draft, [field]: e.target.value.split(",").map((x) => x.trim()).filter(Boolean), [field + "__raw"]: e.target.value })} hint={(hint(field, s) ?? "") + ` · ${t("admin.settings.commaHint")}`} />;
        }
        if (s["x-multiline"]) {
          return <TextAreaField key={field} label={label(field, s)} mono={field.includes("css")} value={String(val(field) ?? "")} onChange={(e) => set(field, e.target.value)} hint={hint(field, s)} spellCheck={!field.includes("css")} />;
        }
        if (type === "integer" || type === "number") {
          return <TextField key={field} label={label(field, s)} mono inputMode="decimal" value={String(val(field) ?? "")} onChange={(e) => set(field, e.target.value === "" ? null : Number(e.target.value.replace(",", ".")))} hint={[hint(field, s), s.minimum != null && s.maximum != null ? `${s.minimum}–${s.maximum}` : null].filter(Boolean).join(" · ") + restart} />;
        }
        return <TextField key={field} label={label(field, s)} mono={/path|url|device/.test(field)} value={String(val(field) ?? "")} onChange={(e) => set(field, e.target.value)} hint={(hint(field, s) ?? "") + restart} />;
      })}
      <div className="row">
        <button className="pl-btn pl-btn--primary" disabled={!dirty} onClick={(e) => { e.preventDefault(); const clean = Object.fromEntries(Object.entries(draft).filter(([k]) => !k.endsWith("__raw"))); save(clean); }}>{t("common.save")}</button>
        {dirty && <button type="button" className="pl-btn pl-btn--ghost" onClick={() => setDraft({})}>{t("common.cancel")}</button>}
      </div>
    </form>
  );
}

/** Logo del server (Impostazioni › Generali): sostituisce il marchio nella web UI, nella scheda e nell'icona dell'app. */
function LogoPanel() {
  const { t } = useI18n();
  const { info, refreshInfo } = useSession();
  const file = useRef<HTMLInputElement>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  return (
    <section className="pl-panel panel-pad stack">
      <h2 className="t-h3">{t("admin.settings.logo.title")}</h2>
      <p className="t-small muted" style={{ margin: 0 }}>{t("admin.settings.logo.help")}</p>
      {error && <Alert tone="danger">{error}</Alert>}
      <div className="row" style={{ flexWrap: "wrap" }}>
        <div className="logo-preview"><Mark size={48} /></div>
        <button className="pl-btn pl-btn--secondary" disabled={busy} onClick={() => file.current?.click()}><Upload />{t(info?.logo ? "admin.settings.logo.replace" : "admin.settings.logo.upload")}</button>
        {info?.logo && <button className="pl-btn pl-btn--ghost" disabled={busy} onClick={async () => { await api("/api/v1/admin/branding/logo", { method: "DELETE" }); await refreshInfo(); toast(t("admin.settings.logo.removed")); }}><Trash2 />{t("admin.settings.logo.remove")}</button>}
      </div>
      <input ref={file} type="file" accept="image/svg+xml,image/png,image/jpeg,image/webp" hidden onChange={async (e) => {
        const f = e.target.files?.[0];
        e.target.value = "";
        if (!f) return;
        setBusy(true);
        setError(null);
        try {
          const res = await fetch(serverUrl() + "/api/v1/admin/branding/logo", { method: "POST", headers: { Authorization: `Bearer ${getToken() ?? ""}`, "Content-Type": f.type || "application/octet-stream" }, body: f });
          if (!res.ok) throw new Error(((await res.json().catch(() => null)) as { message?: string } | null)?.message ?? res.statusText);
          await refreshInfo();
          toast(t("admin.settings.logo.saved"));
        } catch (err) { setError((err as Error).message); } finally { setBusy(false); }
      }} />
    </section>
  );
}

export default function Settings() {
  const { t } = useI18n();
  const { group } = useParams();
  const nav = useNavigate();
  const qc = useQueryClient();
  const { refreshInfo } = useSession();
  const { data } = useQuery({ queryKey: ["admin", "settings"], queryFn: () => api<SettingsData>("/api/v1/admin/settings") });
  if (!data) return null;
  const g = group && data.groups.includes(group) ? group : data.groups[0];
  return (
    <div className="stack-lg" style={{ maxWidth: 860 }}>
      <div className="page-head" style={{ marginBottom: 0 }}><h1 className="t-h1">{t("admin.settings.title")}</h1></div>
      <div className="tabs" role="tablist">
        {data.groups.map((x) => {
          const k = `settings.groups.${x}` as MessageKey;
          const l = t(k);
          return <button key={x} role="tab" className={`tab${x === g ? " active" : ""}`} onClick={() => nav(`/admin/settings/${x}`)}>{l === k ? data.schema[x].title ?? x : l}</button>;
        })}
      </div>
      {g === "general" && <LogoPanel />}
      <section className="pl-panel panel-pad">
        <GroupForm group={g} schema={data.schema[g]} values={data.values[g]} onSaved={() => { qc.invalidateQueries({ queryKey: ["admin", "settings"] }); if (g === "general") refreshInfo(); }} />
      </section>
    </div>
  );
}
