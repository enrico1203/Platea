import { useSession } from "../session";
import { useT } from "../i18n";
import { Mark } from "../components/Logo";

export default function About() {
  const t = useT();
  const { info } = useSession();
  return (
    <div className="stack-lg" style={{ maxWidth: 760 }}>
      <div className="row"><Mark size={48} /><div><h1 className="t-h1">Platea</h1><div className="t-small subtle">Open source media server</div></div></div>
      <dl className="kv">
        <dt>{t("about.server")}</dt><dd>{info?.name}</dd>
        <dt>{t("about.serverVersion")}</dt><dd className="t-mono">{info?.version}</dd>
        <dt>{t("about.webVersion")}</dt><dd className="t-mono">{__APP_VERSION__}</dd>
        <dt>{t("about.license")}</dt><dd>AGPL-3.0</dd>
        <dt>API</dt><dd><a href="/api/v1/openapi.json" className="t-mono">/api/v1/openapi.json</a></dd>
      </dl>
      <section className="pl-panel panel-pad stack">
        <h2 className="t-h3">{t("about.dataSources")}</h2>
        <p className="muted" style={{ margin: 0 }}>{t("about.tmdb")}</p>
        <a href="https://www.themoviedb.org" target="_blank" rel="noreferrer" className="t-label">The Movie Database (TMDB)</a>
        <p className="t-small subtle" style={{ margin: 0 }}>{t("about.tmdbDisclaimer")}</p>
      </section>
      <section className="pl-panel panel-pad stack">
        <h2 className="t-h3">{t("about.components")}</h2>
        <p className="t-small muted" style={{ margin: 0 }}>{t("about.componentsText")}</p>
      </section>
    </div>
  );
}
