import { useEffect, useState } from "react";
import { api, ApiError, deviceId, deviceName } from "../api/client";
import { Alert, TextField, toast } from "./ui";
import { useT } from "../i18n";

/** Codice di 6 cifre leggibile: "123 456". */
const spaced = (c: string) => `${c.slice(0, 3)} ${c.slice(3)}`;

/**
 * Accesso con Quick Connect: mostra un codice, aspetta che un dispositivo già collegato lo confermi,
 * poi scambia il segreto con un token. Un codice scaduto (10 minuti) si rinnova con un tocco.
 */
export function QuickConnectLogin({ onToken, onCancel }: { onToken: (token: string) => Promise<void>; onCancel: () => void }) {
  const t = useT();
  const [req, setReq] = useState<{ code: string; secret: string } | null>(null);
  const [expired, setExpired] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let alive = true;
    setReq(null); setExpired(false); setError(null);
    api<{ code: string; secret: string }>("/api/v1/quickconnect/initiate", {
      body: { device_id: deviceId(), device_name: deviceName(), client: "Platea Web", client_version: __APP_VERSION__ },
    })
      .then((r) => alive && setReq(r))
      .catch((e) => alive && setError(e instanceof ApiError ? e.message : t("errors.network")));
    return () => { alive = false; };
  }, [attempt, t]);

  useEffect(() => {
    if (!req || expired) return;
    let alive = true;
    const tick = async () => {
      try {
        const s = await api<{ state: "pending" | "authorized" }>(`/api/v1/quickconnect/status/${req.secret}`);
        if (!alive) return;
        if (s.state === "authorized") {
          const r = await api<{ token: string }>("/api/v1/quickconnect/redeem", { body: { secret: req.secret } });
          if (alive) await onToken(r.token);
          return;
        }
      } catch (e) {
        if (!alive) return;
        if (e instanceof ApiError && e.status === 404) { setExpired(true); return; }
      }
      timer = window.setTimeout(tick, 2000);
    };
    let timer = window.setTimeout(tick, 2000);
    return () => { alive = false; window.clearTimeout(timer); };
  }, [req, expired, onToken]);

  return (
    <div className="stack">
      <div className="t-h3">{t("quickConnect.title")}</div>
      {error && <Alert tone="danger">{error}</Alert>}
      {expired ? (
        <Alert tone="warning">{t("quickConnect.expired")}</Alert>
      ) : (
        <div className="qc-code" aria-live="polite">{req ? spaced(req.code) : "··· ···"}</div>
      )}
      <p className="t-small subtle">{t("quickConnect.instructions")}</p>
      <div className="row">
        {(expired || error) && <button type="button" className="pl-btn pl-btn--primary" onClick={() => setAttempt((a) => a + 1)}>{t("quickConnect.newCode")}</button>}
        <button type="button" className="pl-btn pl-btn--ghost" onClick={onCancel}>{t("quickConnect.usePassword")}</button>
      </div>
    </div>
  );
}

/** Conferma del codice mostrato da un altro dispositivo (Account e pagina /link). */
export function LinkDevice({ autoFocus }: { autoFocus?: boolean }) {
  const t = useT();
  const [code, setCode] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const digits = code.replace(/\D/g, "");
  return (
    <form
      className="stack"
      onSubmit={async (e) => {
        e.preventDefault();
        setBusy(true); setError(null);
        try {
          const r = await api<{ device_name: string }>("/api/v1/quickconnect/authorize", { body: { code: digits } });
          toast(t("quickConnect.linked", { device: r.device_name }));
          setCode("");
        } catch (err) {
          setError(err instanceof ApiError ? err.message : t("errors.network"));
        } finally { setBusy(false); }
      }}
    >
      <h2 className="t-h3">{t("quickConnect.linkTitle")}</h2>
      <p className="t-small subtle">{t("quickConnect.linkHint")}</p>
      {error && <Alert tone="danger">{error}</Alert>}
      <div className="row" style={{ alignItems: "flex-end" }}>
        <TextField label={t("quickConnect.code")} inputMode="numeric" autoComplete="one-time-code" maxLength={7} value={code} onChange={(e) => setCode(e.target.value)} autoFocus={autoFocus} />
        <button className="pl-btn pl-btn--primary" disabled={busy || digits.length !== 6}>{t("quickConnect.authorize")}</button>
      </div>
    </form>
  );
}
