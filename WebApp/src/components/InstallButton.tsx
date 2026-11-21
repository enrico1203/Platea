import { useState } from "react";
import { Download, EllipsisVertical, Share, SquarePlus } from "lucide-react";
import { Dialog } from "./ui";
import { useI18n } from "../i18n";
import { promptInstall, usePwaInstall } from "../lib/pwa";

/** Pulsante "Installa app" nella barra in alto (solo da telefono, nascosto se già installata). */
export function InstallButton() {
  const { t } = useI18n();
  const { available, platform } = usePwaInstall();
  const [help, setHelp] = useState(false);
  if (!available) return null;
  const click = async () => {
    // Android con Chrome: il prompt di sistema. Altrove (iPhone, Firefox, prompt già usato): le istruzioni.
    if (!(await promptInstall())) setHelp(true);
  };
  return (
    <>
      <button type="button" className="icon-btn" aria-label={t("install.button")} title={t("install.button")} onClick={click}><Download /></button>
      {help && (
        <Dialog title={t("install.title")} onClose={() => setHelp(false)}>
          {platform === "ios" ? (
            <ol className="install-steps">
              <li><Share aria-hidden /><span>{t("install.iosStep1")}</span></li>
              <li><SquarePlus aria-hidden /><span>{t("install.iosStep2")}</span></li>
              <li><span className="install-steps__n" aria-hidden>3</span><span>{t("install.iosStep3")}</span></li>
            </ol>
          ) : (
            <ol className="install-steps">
              <li><EllipsisVertical aria-hidden /><span>{t("install.androidStep1")}</span></li>
              <li><Download aria-hidden /><span>{t("install.androidStep2")}</span></li>
            </ol>
          )}
          <div className="t-small subtle">{platform === "ios" ? t("install.iosNote") : t("install.androidNote")}</div>
        </Dialog>
      )}
    </>
  );
}
