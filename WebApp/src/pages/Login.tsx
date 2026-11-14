import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Mark } from "../components/Logo";
import { Alert, TextField } from "../components/ui";
import { ApiError } from "../api/client";
import { useSession } from "../session";
import { useT } from "../i18n";

export default function Login() {
  const t = useT();
  const { login, info } = useSession();
  const nav = useNavigate();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  return (
    <div className="center">
      <form
        className="auth-card"
        onSubmit={async (e) => {
          e.preventDefault();
          setBusy(true);
          setError(null);
          try {
            await login(username, password);
            nav("/", { replace: true });
          } catch (err) {
            setError(err instanceof ApiError && err.code !== "Network" ? err.message : t("errors.network"));
          } finally {
            setBusy(false);
          }
        }}
      >
        <div className="row"><Mark size={40} /><div><div className="t-h2">{info?.name ?? "Platea"}</div><div className="t-small subtle">Open source media server</div></div></div>
        {error && <Alert tone="danger">{error}</Alert>}
        <TextField label={t("auth.username")} autoComplete="username" value={username} onChange={(e) => setUsername(e.target.value)} autoFocus required />
        <TextField label={t("auth.password")} type="password" autoComplete="current-password" value={password} onChange={(e) => setPassword(e.target.value)} required />
        <button className="pl-btn pl-btn--primary pl-btn--lg" disabled={busy || !username || !password}>{t("auth.login")}</button>
      </form>
    </div>
  );
}
