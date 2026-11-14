import { useState } from "react";
import { Mark } from "../components/Logo";
import { Alert, TextField } from "../components/ui";
import { serverUrl, setServerUrl } from "../api/client";
import { useSession } from "../session";
import { useT } from "../i18n";

/** Server non raggiungibile: si chiede l'indirizzo, come fanno le app. */
export default function Connect() {
  const t = useT();
  const { reload } = useSession();
  const [url, setUrl] = useState(serverUrl());
  const [busy, setBusy] = useState(false);
  return (
    <div className="center">
      <form
        className="auth-card"
        onSubmit={async (e) => {
          e.preventDefault();
          setBusy(true);
          setServerUrl(url);
          await reload();
          setBusy(false);
        }}
      >
        <div className="row"><Mark size={40} /><div className="t-h2">{t("connect.title")}</div></div>
        <Alert tone="warning">{t("connect.unreachable")}</Alert>
        <TextField label={t("connect.address")} placeholder="http://192.168.1.10:8096" mono value={url} onChange={(e) => setUrl(e.target.value)} hint={t("connect.hint")} />
        <button className="pl-btn pl-btn--primary pl-btn--lg" disabled={busy}>{t("connect.connect")}</button>
      </form>
    </div>
  );
}
