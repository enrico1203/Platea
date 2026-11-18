import { Link } from "react-router-dom";
import { Film, Tv, ChevronRight, ListVideo, Headphones, Radio } from "lucide-react";
import { useLiveStatus } from "./LiveTV";
import { useLibraries } from "../components/Layout";
import { Empty } from "../components/ui";
import { useI18n } from "../i18n";

/** Elenco delle librerie: su telefono è la scheda "Librerie". */
export default function Libraries() {
  const { t } = useI18n();
  const { data } = useLibraries();
  const { data: tv } = useLiveStatus();
  if (!data) return null;
  if (data.length === 0) return <Empty>{t("home.empty")}</Empty>;
  return (
    <div>
      <div className="page-head"><h1 className="t-h1">{t("nav.libraries")}</h1></div>
      <div className="pl-panel">
        {data.map((l) => (
          <Link key={l.id} to={`/library/${l.id}`} className="list-row" style={{ color: "inherit", textDecoration: "none" }}>
            {l.kind === "movies" ? <Film size={20} /> : l.kind === "audiobooks" ? <Headphones size={20} /> : <Tv size={20} />}
            <div className="spacer"><div className="t-label">{l.name}</div><div className="t-small subtle">{t(l.kind === "movies" ? "library.countMovies" : l.kind === "audiobooks" ? "library.countAudiobooks" : "library.countShows", { count: l.item_count })}</div></div>
            <ChevronRight size={18} className="subtle" />
          </Link>
        ))}
        {tv?.enabled && (
          <Link to="/tv" className="list-row" style={{ color: "inherit", textDecoration: "none" }}>
            <Radio size={20} />
            <div className="spacer"><div className="t-label">{t("nav.tv")}</div><div className="t-small subtle">{t("tv.count", { count: tv.channels })}</div></div>
            <ChevronRight size={18} className="subtle" />
          </Link>
        )}
        <Link to="/playlists" className="list-row" style={{ color: "inherit", textDecoration: "none" }}>
          <ListVideo size={20} />
          <div className="spacer"><div className="t-label">{t("nav.playlists")}</div></div>
          <ChevronRight size={18} className="subtle" />
        </Link>
      </div>
    </div>
  );
}
