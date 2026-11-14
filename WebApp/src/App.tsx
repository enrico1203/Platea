import { lazy, Suspense, type ReactNode } from "react";
import { BrowserRouter, Navigate, Route, Routes, useLocation } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { I18nProvider } from "./i18n";
import { SessionProvider, useSession } from "./session";
import { Layout } from "./components/Layout";
import { ToastHost } from "./components/ui";
import Login from "./pages/Login";
import Setup from "./pages/Setup";
import Connect from "./pages/Connect";
import Home from "./pages/Home";
import Libraries from "./pages/Libraries";
import Library from "./pages/Library";
import Search from "./pages/Search";
import ItemPage from "./pages/Item";
import Account from "./pages/Account";
import About from "./pages/About";

// Player e dashboard admin in pacchetti separati: la dashboard si carica solo per gli amministratori.
const Playlists = lazy(() => import("./pages/Playlists"));
const PlaylistPage = lazy(() => import("./pages/Playlists").then((m) => ({ default: m.PlaylistPage })));
const Person = lazy(() => import("./pages/Person"));
const Player = lazy(() => import("./pages/Player"));
const AdminOverview = lazy(() => import("./pages/admin/Overview"));
const AdminLibraries = lazy(() => import("./pages/admin/Libraries"));
const AdminUsers = lazy(() => import("./pages/admin/Users"));
const AdminJobs = lazy(() => import("./pages/admin/Jobs"));
const AdminReview = lazy(() => import("./pages/admin/Review"));
const AdminTranscode = lazy(() => import("./pages/admin/Transcode"));
const AdminSettings = lazy(() => import("./pages/admin/Settings"));
const AdminActivity = lazy(() => import("./pages/admin/Activity"));
const AdminSystem = lazy(() => import("./pages/admin/System"));
const AdminLogs = lazy(() => import("./pages/admin/Logs"));

const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: 30_000, refetchOnWindowFocus: false, retry: (n, e) => n < 2 && (e as { status?: number }).status !== 404 && (e as { status?: number }).status !== 403 } },
});

function Guard({ children, admin }: { children: ReactNode; admin?: boolean }) {
  const { me, info } = useSession();
  const loc = useLocation();
  if (info?.setup_required) return <Navigate to="/setup" replace />;
  if (!me) return <Navigate to="/login" replace state={{ from: loc.pathname }} />;
  if (admin && !me.is_admin) return <Navigate to="/" replace />;
  return <>{children}</>;
}

function Routed() {
  const { loading, error, info, me } = useSession();
  if (loading && !info) return null;
  if (error && !info) return <Connect />;
  return (
    <Suspense fallback={null}>
      <Routes>
        <Route path="/setup" element={info?.setup_required || me?.is_admin ? <Setup /> : <Navigate to="/" replace />} />
        <Route path="/login" element={info?.setup_required ? <Navigate to="/setup" replace /> : me ? <Navigate to="/" replace /> : <Login />} />
        <Route path="/play/:id" element={<Guard><Player /></Guard>} />
        <Route element={<Guard><Layout /></Guard>}>
          <Route index element={<Home />} />
          <Route path="libraries" element={<Libraries />} />
          <Route path="library/:id" element={<Library />} />
          <Route path="search" element={<Search />} />
          <Route path="playlists" element={<Playlists />} />
          <Route path="person/:id" element={<Person />} />
          <Route path="playlist/:id" element={<PlaylistPage />} />
          <Route path="item/:id" element={<ItemPage />} />
          <Route path="account" element={<Account />} />
          <Route path="about" element={<About />} />
          <Route path="admin" element={<Guard admin><AdminOverview /></Guard>} />
          <Route path="admin/libraries" element={<Guard admin><AdminLibraries /></Guard>} />
          <Route path="admin/users" element={<Guard admin><AdminUsers /></Guard>} />
          <Route path="admin/jobs" element={<Guard admin><AdminJobs /></Guard>} />
          <Route path="admin/review" element={<Guard admin><AdminReview /></Guard>} />
          <Route path="admin/transcode" element={<Guard admin><AdminTranscode /></Guard>} />
          <Route path="admin/settings/:group?" element={<Guard admin><AdminSettings /></Guard>} />
          <Route path="admin/activity" element={<Guard admin><AdminActivity /></Guard>} />
          <Route path="admin/system" element={<Guard admin><AdminSystem /></Guard>} />
          <Route path="admin/logs" element={<Guard admin><AdminLogs /></Guard>} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Route>
      </Routes>
    </Suspense>
  );
}

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <I18nProvider>
        <BrowserRouter>
          <SessionProvider>
            <Routed />
            <ToastHost />
          </SessionProvider>
        </BrowserRouter>
      </I18nProvider>
    </QueryClientProvider>
  );
}
