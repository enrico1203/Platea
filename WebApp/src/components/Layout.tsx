import { NavLink, Outlet, useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Home, Search, Film, Tv, LayoutDashboard, FolderOpen, Users, ListChecks, SearchCheck, Cpu, Settings, ScrollText, Server, User, Info, LogOut, Library as LibraryIcon } from "lucide-react";
import { Logo } from "./Logo";
import { api } from "../api/client";
import type { Library, Page } from "../api/types";
import { useSession } from "../session";
import { useT } from "../i18n";

export function useLibraries() {
  return useQuery({ queryKey: ["libraries"], queryFn: () => api<Page<Library>>("/api/v1/libraries").then((r) => r.items), staleTime: 60_000 });
}

export function Layout() {
  const t = useT();
  const { me, logout } = useSession();
  const nav = useNavigate();
  const { data: libs } = useLibraries();
  const link = ({ isActive }: { isActive: boolean }) => `nav__item${isActive ? " active" : ""}`;
  return (
    <div className="app">
      <aside className="sidebar">
        <div className="sidebar__logo"><Logo /></div>
        <nav className="nav" aria-label={t("nav.main")}>
          <NavLink to="/" end className={link}><Home />{t("nav.home")}</NavLink>
          <NavLink to="/search" className={link}><Search />{t("nav.search")}</NavLink>
          {libs?.map((l) => (
            <NavLink key={l.id} to={`/library/${l.id}`} className={link}>
              {l.kind === "movies" ? <Film /> : <Tv />}
              {l.name}
            </NavLink>
          ))}
        </nav>
        {me?.is_admin && (
          <nav className="nav__group" aria-label={t("nav.admin")}>
            <span className="pl-eyebrow">{t("nav.admin")}</span>
            <NavLink to="/admin" end className={link}><LayoutDashboard />{t("admin.overview.title")}</NavLink>
            <NavLink to="/admin/libraries" className={link}><FolderOpen />{t("admin.libraries.title")}</NavLink>
            <NavLink to="/admin/users" className={link}><Users />{t("admin.users.title")}</NavLink>
            <NavLink to="/admin/review" className={link}><SearchCheck />{t("admin.review.title")}</NavLink>
            <NavLink to="/admin/jobs" className={link}><ListChecks />{t("admin.jobs.title")}</NavLink>
            <NavLink to="/admin/transcode" className={link}><Cpu />{t("admin.transcode.title")}</NavLink>
            <NavLink to="/admin/settings" className={link}><Settings />{t("admin.settings.title")}</NavLink>
            <NavLink to="/admin/activity" className={link}><ScrollText />{t("admin.activity.title")}</NavLink>
            <NavLink to="/admin/system" className={link}><Server />{t("admin.system.title")}</NavLink>
          </nav>
        )}
        <div className="sidebar__foot">
          <NavLink to="/account" className={link}><User />{me?.username}</NavLink>
          <NavLink to="/about" className={link}><Info />{t("nav.about")}</NavLink>
          <button className="nav__item" style={{ border: 0, background: "none", cursor: "pointer" }} onClick={async () => { await logout(); nav("/login"); }}>
            <LogOut />{t("nav.logout")}
          </button>
        </div>
      </aside>
      <div>
        <header className="topbar">
          <Logo />
          <NavLink to="/account" className="icon-btn" aria-label={t("nav.account")}><User /></NavLink>
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
