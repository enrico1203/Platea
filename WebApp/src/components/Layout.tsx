import { useEffect, useState } from "react";
import { Link, NavLink, Outlet, useLocation, useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Home, Search, Film, Tv, Headphones, LayoutDashboard, FolderOpen, Users, ListChecks, SearchCheck, Cpu, Settings, ScrollText, Server, Terminal, User, Info, LogOut, Library as LibraryIcon, ListVideo, PanelLeftClose, PanelLeftOpen, ArrowUp, Network, Radio } from "lucide-react";
import { Logo, Mark, useServerName } from "./Logo";
import { api, mediaUrl } from "../api/client";
import type { Library, Page } from "../api/types";
import { useSession } from "../session";
import { useT } from "../i18n";
import { AudioBar } from "../audio/AudioBar";
import { useLiveStatus } from "../pages/LiveTV";

export function useLibraries() {
  return useQuery({ queryKey: ["libraries"], queryFn: () => api<Page<Library>>("/api/v1/libraries").then((r) => r.items), staleTime: 60_000 });
}

/** Immagine del profilo, o le iniziali su un colore ricavato dal nome. */
export function Avatar({ name, url, size = 28 }: { name: string; url: string | null; size?: number }) {
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) % 360;
  const style = { width: size, height: size, fontSize: Math.round(size * 0.42) };
  if (url) return <img className="avatar" src={mediaUrl(url)} alt="" style={style} />;
  return <span className="avatar avatar--initials" style={{ ...style, background: `hsl(${h} 45% 32%)` }} aria-hidden>{name.slice(0, 1).toUpperCase()}</span>;
}

/** Sezioni dell'amministrazione: menu laterale su desktop, barra scorrevole su telefono. */
const ADMIN_LINKS = [
  { to: "/admin", end: true, icon: LayoutDashboard, label: "admin.overview.title" },
  { to: "/admin/libraries", icon: FolderOpen, label: "admin.libraries.title" },
  { to: "/admin/users", icon: Users, label: "admin.users.title" },
  { to: "/admin/review", icon: SearchCheck, label: "admin.review.title" },
  { to: "/admin/jobs", icon: ListChecks, label: "admin.jobs.title" },
  { to: "/admin/transcode", icon: Cpu, label: "admin.transcode.title" },
  { to: "/admin/nodes", icon: Network, label: "admin.nodes.title" },
  { to: "/admin/livetv", icon: Radio, label: "admin.livetv.title" },
  { to: "/admin/settings", icon: Settings, label: "admin.settings.title" },
  { to: "/admin/activity", icon: ScrollText, label: "admin.activity.title" },
  { to: "/admin/logs", icon: Terminal, label: "admin.logs.title" },
  { to: "/admin/system", icon: Server, label: "admin.system.title" },
] as const;

/** Su telefono il menu laterale non c'è: le sezioni admin stanno in una barra orizzontale. */
function AdminSubnav() {
  const t = useT();
  const { pathname } = useLocation();
  useEffect(() => {
    // La sezione attiva resta visibile anche se la barra è scorsa.
    document.querySelector(".admin-subnav a.active")?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [pathname]);
  return (
    <nav className="admin-subnav" aria-label={t("nav.admin")}>
      {ADMIN_LINKS.map(({ to, icon: Icon, label, ...rest }) => (
        <NavLink key={to} to={to} end={"end" in rest} className={({ isActive }) => (isActive ? "active" : undefined)}><Icon />{t(label)}</NavLink>
      ))}
    </nav>
  );
}

const COLLAPSED_KEY = "platea.sidebar.collapsed";

/** Menu compresso o no: preferenza di questo browser. */
function useCollapsed(): [boolean, () => void] {
  const [collapsed, set] = useState(() => {
    try { return localStorage.getItem(COLLAPSED_KEY) === "1"; } catch { return false; }
  });
  const toggle = () => set((c) => {
    try { localStorage.setItem(COLLAPSED_KEY, c ? "0" : "1"); } catch { /* storage non disponibile */ }
    return !c;
  });
  return [collapsed, toggle];
}

/** Pulsante "torna su": compare dopo un po' di scorrimento. */
function BackToTop() {
  const t = useT();
  const [show, setShow] = useState(false);
  useEffect(() => {
    let raf = 0;
    const onScroll = () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => setShow(window.scrollY > window.innerHeight * 0.8));
    };
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => { window.removeEventListener("scroll", onScroll); cancelAnimationFrame(raf); };
  }, []);
  const reduce = typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
  return (
    <button className={`back-to-top${show ? " is-visible" : ""}`} aria-label={t("nav.backToTop")} title={t("nav.backToTop")} tabIndex={show ? 0 : -1} aria-hidden={!show}
      onClick={() => window.scrollTo({ top: 0, behavior: reduce ? "auto" : "smooth" })}>
      <ArrowUp />
    </button>
  );
}

export function Layout() {
  const t = useT();
  const { me, logout } = useSession();
  const nav = useNavigate();
  const { data: libs } = useLibraries();
  const { data: tv } = useLiveStatus();
  const inAdmin = useLocation().pathname.startsWith("/admin");
  const serverName = useServerName();
  const [collapsed, toggleCollapsed] = useCollapsed();
  const link = ({ isActive }: { isActive: boolean }) => `nav__item${isActive ? " active" : ""}`;
  // Compresso: solo icone, il nome nel tooltip e per i lettori di schermo.
  const tip = (label: string) => (collapsed ? { title: label, "aria-label": label } : {});
  return (
    <div className={`app${collapsed ? " app--collapsed" : ""}`}>
      <aside className="sidebar">
        <div className="sidebar__logo">
          {collapsed ? <Link to="/" className="logo" aria-label={serverName} title={serverName}><Mark /></Link> : <Logo />}
          <button className="sidebar__toggle" onClick={toggleCollapsed} aria-expanded={!collapsed} title={t(collapsed ? "nav.expand" : "nav.collapse")} aria-label={t(collapsed ? "nav.expand" : "nav.collapse")}>
            {collapsed ? <PanelLeftOpen /> : <PanelLeftClose />}
          </button>
        </div>
        <nav className="nav" aria-label={t("nav.main")}>
          <NavLink to="/" end className={link} {...tip(t("nav.home"))}><Home /><span className="nav__label">{t("nav.home")}</span></NavLink>
          <NavLink to="/search" className={link} {...tip(t("nav.search"))}><Search /><span className="nav__label">{t("nav.search")}</span></NavLink>
          <NavLink to="/playlists" className={link} {...tip(t("nav.playlists"))}><ListVideo /><span className="nav__label">{t("nav.playlists")}</span></NavLink>
          {tv?.enabled && <NavLink to="/tv" className={link} {...tip(t("nav.tv"))}><Radio /><span className="nav__label">{t("nav.tv")}</span></NavLink>}
          {libs?.map((l) => (
            <NavLink key={l.id} to={`/library/${l.id}`} className={link} {...tip(l.name)}>
              {l.kind === "movies" ? <Film /> : l.kind === "audiobooks" ? <Headphones /> : <Tv />}
              <span className="nav__label">{l.name}</span>
            </NavLink>
          ))}
        </nav>
        {me?.is_admin && (
          <nav className="nav__group" aria-label={t("nav.admin")}>
            <span className="pl-eyebrow">{t("nav.admin")}</span>
            {ADMIN_LINKS.map(({ to, icon: Icon, label, ...rest }) => (
              <NavLink key={to} to={to} end={"end" in rest} className={link} {...tip(t(label))}><Icon /><span className="nav__label">{t(label)}</span></NavLink>
            ))}
          </nav>
        )}
        <div className="sidebar__foot">
          <NavLink to="/account" className={link} {...tip(me?.username ?? "")}><Avatar name={me?.username ?? ""} url={me?.avatar ?? null} size={22} /><span className="nav__label">{me?.username}</span></NavLink>
          <NavLink to="/about" className={link} {...tip(t("nav.about"))}><Info /><span className="nav__label">{t("nav.about")}</span></NavLink>
          <button className="nav__item nav__button" {...tip(t("nav.logout"))} onClick={async () => { await logout(); nav("/login"); }}>
            <LogOut /><span className="nav__label">{t("nav.logout")}</span>
          </button>
        </div>
      </aside>
      <div className="app__content">
        <header className="topbar">
          <Logo />
          <NavLink to="/account" className="icon-btn" aria-label={t("nav.account")}><Avatar name={me?.username ?? ""} url={me?.avatar ?? null} size={30} /></NavLink>
        </header>
        {me?.is_admin && inAdmin && <AdminSubnav />}
        <main className="main">
          <Outlet />
        </main>
        <BackToTop />
        <AudioBar />
        <nav className="tabbar" aria-label={t("nav.main")}>
          <NavLink to="/" end><Home />{t("nav.home")}</NavLink>
          <NavLink to="/search"><Search />{t("nav.search")}</NavLink>
          <NavLink to="/libraries"><LibraryIcon />{t("nav.libraries")}</NavLink>
          {me?.is_admin ? <NavLink to="/admin"><LayoutDashboard />{t("nav.admin")}</NavLink> : <NavLink to="/account"><User />{t("nav.account")}</NavLink>}
        </nav>
      </div>
    </div>
  );
}
