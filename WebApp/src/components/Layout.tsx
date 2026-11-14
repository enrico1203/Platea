import { useState } from "react";
import { Link, NavLink, Outlet, useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Home, Search, Film, Tv, LayoutDashboard, FolderOpen, Users, ListChecks, SearchCheck, Cpu, Settings, ScrollText, Server, Terminal, User, Info, LogOut, Library as LibraryIcon, ListVideo, PanelLeftClose, PanelLeftOpen } from "lucide-react";
import { Logo, Mark } from "./Logo";
import { api, mediaUrl } from "../api/client";
import type { Library, Page } from "../api/types";
import { useSession } from "../session";
import { useT } from "../i18n";

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

export function Layout() {
  const t = useT();
  const { me, logout } = useSession();
  const nav = useNavigate();
  const { data: libs } = useLibraries();
  const [collapsed, toggleCollapsed] = useCollapsed();
  const link = ({ isActive }: { isActive: boolean }) => `nav__item${isActive ? " active" : ""}`;
  // Compresso: solo icone, il nome nel tooltip e per i lettori di schermo.
  const tip = (label: string) => (collapsed ? { title: label, "aria-label": label } : {});
  return (
    <div className={`app${collapsed ? " app--collapsed" : ""}`}>
      <aside className="sidebar">
        <div className="sidebar__logo">
          {collapsed ? <Link to="/" className="logo" aria-label="Platea"><Mark /></Link> : <Logo />}
          <button className="sidebar__toggle" onClick={toggleCollapsed} aria-expanded={!collapsed} title={t(collapsed ? "nav.expand" : "nav.collapse")} aria-label={t(collapsed ? "nav.expand" : "nav.collapse")}>
            {collapsed ? <PanelLeftOpen /> : <PanelLeftClose />}
          </button>
        </div>
        <nav className="nav" aria-label={t("nav.main")}>
          <NavLink to="/" end className={link} {...tip(t("nav.home"))}><Home /><span className="nav__label">{t("nav.home")}</span></NavLink>
          <NavLink to="/search" className={link} {...tip(t("nav.search"))}><Search /><span className="nav__label">{t("nav.search")}</span></NavLink>
          <NavLink to="/playlists" className={link} {...tip(t("nav.playlists"))}><ListVideo /><span className="nav__label">{t("nav.playlists")}</span></NavLink>
          {libs?.map((l) => (
            <NavLink key={l.id} to={`/library/${l.id}`} className={link} {...tip(l.name)}>
              {l.kind === "movies" ? <Film /> : <Tv />}
              <span className="nav__label">{l.name}</span>
            </NavLink>
          ))}
        </nav>
        {me?.is_admin && (
          <nav className="nav__group" aria-label={t("nav.admin")}>
            <span className="pl-eyebrow">{t("nav.admin")}</span>
            <NavLink to="/admin" end className={link} {...tip(t("admin.overview.title"))}><LayoutDashboard /><span className="nav__label">{t("admin.overview.title")}</span></NavLink>
            <NavLink to="/admin/libraries" className={link} {...tip(t("admin.libraries.title"))}><FolderOpen /><span className="nav__label">{t("admin.libraries.title")}</span></NavLink>
            <NavLink to="/admin/users" className={link} {...tip(t("admin.users.title"))}><Users /><span className="nav__label">{t("admin.users.title")}</span></NavLink>
            <NavLink to="/admin/review" className={link} {...tip(t("admin.review.title"))}><SearchCheck /><span className="nav__label">{t("admin.review.title")}</span></NavLink>
            <NavLink to="/admin/jobs" className={link} {...tip(t("admin.jobs.title"))}><ListChecks /><span className="nav__label">{t("admin.jobs.title")}</span></NavLink>
            <NavLink to="/admin/transcode" className={link} {...tip(t("admin.transcode.title"))}><Cpu /><span className="nav__label">{t("admin.transcode.title")}</span></NavLink>
            <NavLink to="/admin/settings" className={link} {...tip(t("admin.settings.title"))}><Settings /><span className="nav__label">{t("admin.settings.title")}</span></NavLink>
            <NavLink to="/admin/activity" className={link} {...tip(t("admin.activity.title"))}><ScrollText /><span className="nav__label">{t("admin.activity.title")}</span></NavLink>
            <NavLink to="/admin/logs" className={link} {...tip(t("admin.logs.title"))}><Terminal /><span className="nav__label">{t("admin.logs.title")}</span></NavLink>
            <NavLink to="/admin/system" className={link} {...tip(t("admin.system.title"))}><Server /><span className="nav__label">{t("admin.system.title")}</span></NavLink>
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
        <main className="main">
          <Outlet />
        </main>
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
