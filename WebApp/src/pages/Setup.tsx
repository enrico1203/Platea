import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Folder, CornerLeftUp } from "lucide-react";
import { Mark } from "../components/Logo";
import { Alert, SelectField, Switch, TextField } from "../components/ui";
import { api, ApiError, deviceId, deviceName } from "../api/client";
import { useSession } from "../session";
import { useI18n } from "../i18n";
import { LOCALES } from "../i18n/locales";

type Step = "code" | "admin" | "library" | "metadata" | "transcode" | "done";
const STEPS: Step[] = ["code", "admin", "library", "metadata", "transcode", "done"];

export function FolderBrowser({ value, onChange }: { value: string; onChange: (p: string) => void }) {
  const { t } = useI18n();
  const [data, setData] = useState<{ path: string; parent: string | null; directories: { name: string; path: string }[] } | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    api<typeof data>(`/api/v1/fs/browse${value ? `?path=${encodeURIComponent(value)}` : ""}`)
      .then((d) => { setData(d); setError(null); if (!value && d) onChange(d.path); })
      .catch((e: ApiError) => setError(e.message));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value]);
  return (
    <div className="stack">
      <TextField label={t("setup.folder")} mono value={value} onChange={(e) => onChange(e.target.value)} />
      {error && <Alert tone="danger">{error}</Alert>}
      <div className="browser">
        {data?.parent && <button type="button" onClick={() => onChange(data.parent!)}><CornerLeftUp />..</button>}
        {data?.directories.map((d) => (
          <button type="button" key={d.path} onClick={() => onChange(d.path)}><Folder />{d.name}</button>
        ))}
        {data && data.directories.length === 0 && <div className="t-small subtle" style={{ padding: 12 }}>{t("setup.noSubfolders")}</div>}
      </div>
    </div>
  );
}

export default function Setup() {
  const { t, locale, setLocale } = useI18n();
  const { acceptToken, reload, info } = useSession();
  const nav = useNavigate();
  const [step, setStep] = useState<Step>(info?.setup_required ? "code" : "library");
  const [code, setCode] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [password2, setPassword2] = useState("");
  const [libName, setLibName] = useState("");
  const [libKind, setLibKind] = useState<"movies" | "shows">("movies");
  const [libPath, setLibPath] = useState("");
  const [remote, setRemote] = useState(false);
  const [tmdbKey, setTmdbKey] = useState("");
  const [language, setLanguage] = useState<string>(LOCALES[locale].tag);
  const [caps, setCaps] = useState<{ capabilities: { selected: { backend: string; encode: string[]; decode: string[] } | null; software_encoders: string[]; devices: string[] }; ffmpeg: { version: string; available: boolean } } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const idx = STEPS.indexOf(step);
  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try { await fn(); } catch (e) { setError(e instanceof ApiError ? e.message : String(e)); } finally { setBusy(false); }
  };

  useEffect(() => {
    if (step === "transcode") run(async () => setCaps(await api("/api/v1/admin/transcode/test", { method: "POST" })));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [step]);

  return (
    <div className="center">
      <div className="auth-card wizard">
        <div className="row row--between">
          <div className="row"><Mark size={36} /><div className="t-h3">{t("setup.title")}</div></div>
          <select className="select t-small" style={{ flex: "none" }} value={locale} onChange={(e) => setLocale(e.target.value as keyof typeof LOCALES)} aria-label={t("account.language")}>
            {Object.entries(LOCALES).map(([k, v]) => <option key={k} value={k}>{v.name}</option>)}
          </select>
        </div>
        <div className="steps">{STEPS.map((s, i) => <span key={s} className={i < idx ? "done" : i === idx ? "current" : ""} />)}</div>
        {error && <Alert tone="danger">{error}</Alert>}

        {step === "code" && (
          <form className="form" onSubmit={(e) => { e.preventDefault(); run(async () => { await api("/api/v1/setup/verify", { body: { code } }); setStep("admin"); }); }}>
            <h1 className="t-h1">{t("setup.welcome")}</h1>
            <p className="muted" style={{ margin: 0 }}>{t("setup.codeHelp")}</p>
            <pre className="t-mono subtle" style={{ margin: 0, padding: 12, background: "var(--surface-2)", borderRadius: 10, overflowX: "auto" }}>docker compose logs server | grep -i codice</pre>
            <TextField label={t("setup.code")} mono inputMode="numeric" placeholder="1234-5678-9012" value={code} onChange={(e) => setCode(e.target.value)} autoFocus />
            <button className="pl-btn pl-btn--primary pl-btn--lg" disabled={busy || code.replace(/\D/g, "").length < 12}>{t("common.continue")}</button>
          </form>
        )}

        {step === "admin" && (
          <form className="form" onSubmit={(e) => {
            e.preventDefault();
            if (password !== password2) { setError(t("setup.passwordMismatch")); return; }
            run(async () => {
              const r = await api<{ token: string }>("/api/v1/setup/admin", { body: { code, username, password, device_id: deviceId(), device_name: deviceName(), client: "Platea Web", client_version: __APP_VERSION__ } });
              await acceptToken(r.token);
              setStep("library");
            });
          }}>
            <h1 className="t-h2">{t("setup.adminTitle")}</h1>
            <TextField label={t("auth.username")} autoComplete="username" value={username} onChange={(e) => setUsername(e.target.value)} autoFocus />
            <TextField label={t("auth.password")} type="password" autoComplete="new-password" value={password} onChange={(e) => setPassword(e.target.value)} hint={t("setup.passwordHint")} />
            <TextField label={t("setup.passwordRepeat")} type="password" autoComplete="new-password" value={password2} onChange={(e) => setPassword2(e.target.value)} />
            <button className="pl-btn pl-btn--primary pl-btn--lg" disabled={busy || !username || password.length < 8}>{t("setup.createAdmin")}</button>
          </form>
        )}

        {step === "library" && (
          <form className="form" onSubmit={(e) => {
            e.preventDefault();
            run(async () => {
              await api("/api/v1/libraries", { body: { name: libName || (libKind === "movies" ? t("library.movies") : t("library.shows")), kind: libKind, paths: [libPath], scan_profile: remote ? "remote" : "local", language } });
              setStep("metadata");
            });
          }}>
            <h1 className="t-h2">{t("setup.libraryTitle")}</h1>
            <p className="muted" style={{ margin: 0 }}>{t("setup.libraryHelp")}</p>
            <div className="form-grid">
              <SelectField label={t("library.kind")} value={libKind} onChange={(v) => setLibKind(v as "movies" | "shows")} options={[{ value: "movies", label: t("library.movies") }, { value: "shows", label: t("library.shows") }]} />
              <TextField label={t("library.name")} value={libName} placeholder={libKind === "movies" ? t("library.movies") : t("library.shows")} onChange={(e) => setLibName(e.target.value)} />
            </div>
            <FolderBrowser value={libPath} onChange={setLibPath} />
            <Switch checked={remote} onChange={setRemote} label={t("library.remoteProfile")} hint={t("library.remoteHint")} />
            <div className="row">
              <button type="button" className="pl-btn pl-btn--ghost" onClick={() => setStep("metadata")}>{t("common.skip")}</button>
              <span className="spacer" />
              <button className="pl-btn pl-btn--primary" disabled={busy || !libPath}>{t("setup.addLibrary")}</button>
            </div>
          </form>
        )}

        {step === "metadata" && (
          <form className="form" onSubmit={(e) => {
            e.preventDefault();
            run(async () => {
              if (tmdbKey) {
                const r = await api<{ ok: boolean; message?: string }>("/api/v1/admin/metadata/test-key", { body: { key: tmdbKey } });
                if (!r.ok) throw new Error(r.message ?? t("setup.tmdbInvalid"));
              }
              await api("/api/v1/admin/settings/metadata", { method: "PUT", body: { ...(tmdbKey ? { tmdb_api_key: tmdbKey } : {}), language, country: language.split("-")[1] ?? "IT" } });
              setStep("transcode");
            });
          }}>
            <h1 className="t-h2">{t("setup.metadataTitle")}</h1>
            <p className="muted" style={{ margin: 0 }}>{t("setup.metadataHelp")} <a href="https://www.themoviedb.org/settings/api" target="_blank" rel="noreferrer">themoviedb.org</a></p>
            <TextField label={t("setup.tmdbKey")} mono value={tmdbKey} onChange={(e) => setTmdbKey(e.target.value)} autoComplete="off" />
            <SelectField label={t("setup.metadataLanguage")} value={language} onChange={setLanguage} options={[{ value: "it-IT", label: "Italiano" }, { value: "en-US", label: "English (US)" }, { value: "en-GB", label: "English (UK)" }, { value: "de-DE", label: "Deutsch" }, { value: "fr-FR", label: "Français" }, { value: "es-ES", label: "Español" }]} />
            <div className="row">
              <button type="button" className="pl-btn pl-btn--ghost" onClick={() => setStep("transcode")}>{t("common.skip")}</button>
              <span className="spacer" />
              <button className="pl-btn pl-btn--primary" disabled={busy}>{t("common.save")}</button>
            </div>
          </form>
        )}

        {step === "transcode" && (
          <div className="form">
            <h1 className="t-h2">{t("setup.transcodeTitle")}</h1>
            {!caps && <p className="muted">{t("setup.testing")}</p>}
            {caps && (
              <>
                <dl className="kv">
                  <dt>FFmpeg</dt><dd className="t-mono">{caps.ffmpeg.available ? caps.ffmpeg.version : t("admin.transcode.ffmpegMissing")}</dd>
                  <dt>{t("admin.transcode.gpu")}</dt>
                  <dd>{caps.capabilities.selected ? <span className="t-mono">{caps.capabilities.selected.backend.toUpperCase()} · {caps.capabilities.selected.encode.join(", ").toUpperCase()}</span> : t("admin.transcode.noGpu")}</dd>
                  <dt>{t("admin.transcode.software")}</dt><dd className="t-mono">{caps.capabilities.software_encoders.join(", ").toUpperCase() || "—"}</dd>
                </dl>
                <p className="t-small subtle" style={{ margin: 0 }}>{t("setup.transcodeHelp")}</p>
              </>
            )}
            <div className="row"><span className="spacer" /><button className="pl-btn pl-btn--primary" disabled={busy && !caps} onClick={() => setStep("done")}>{t("common.continue")}</button></div>
          </div>
        )}

        {step === "done" && (
          <div className="form">
            <h1 className="t-h1">{t("setup.doneTitle")}</h1>
            <p className="muted" style={{ margin: 0 }}>{t("setup.doneHelp")}</p>
            <button className="pl-btn pl-btn--primary pl-btn--lg" onClick={async () => { await reload(); nav("/", { replace: true }); }}>{t("setup.open")}</button>
          </div>
        )}
      </div>
    </div>
  );
}
