import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { api, deviceId, deviceName, getToken, onUnauthorized, setToken, serverUrl } from "./api/client";
import { connectEvents, disconnectEvents, subscribe } from "./api/events";
import type { Me, SystemInfo } from "./api/types";

interface SessionCtx {
  info: SystemInfo | null;
  me: Me | null;
  loading: boolean;
  error: boolean;
  login: (username: string, password: string) => Promise<void>;
  acceptToken: (token: string) => Promise<void>;
  logout: () => Promise<void>;
  reload: () => Promise<void>;
  /** Rilegge solo le informazioni del server (nome…), senza schermata di caricamento. */
  refreshInfo: () => Promise<void>;
}

const Ctx = createContext<SessionCtx | null>(null);

export function SessionProvider({ children }: { children: ReactNode }) {
  const qc = useQueryClient();
  const [info, setInfo] = useState<SystemInfo | null>(null);
  const [me, setMe] = useState<Me | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(false);
    try {
      const i = await api<SystemInfo>("/api/v1/system/info");
      let m: Me | null = null;
      if (getToken() && !i.setup_required) {
        try {
          m = await api<Me>("/api/v1/me");
        } catch {
          m = null;
        }
      }
      // Insieme: le route non vedono mai uno stato intermedio (server configurato ma utente non ancora caricato).
      setInfo(i);
      setMe(m);
    } catch {
      setError(true);
    } finally {
      setLoading(false);
    }
  }, []);

  // CSS personalizzato dell'amministratore (Impostazioni › Generali): un <link> che cambia con la sua versione.
  useEffect(() => {
    const id = "platea-custom-css";
    document.getElementById(id)?.remove();
    if (!info?.custom_css) return;
    const l = document.createElement("link");
    l.id = id;
    l.rel = "stylesheet";
    l.href = `${serverUrl()}/api/v1/system/custom.css?v=${info.custom_css}`;
    document.head.appendChild(l);
  }, [info?.custom_css]);

  const refreshInfo = useCallback(async () => {
    try { setInfo(await api<SystemInfo>("/api/v1/system/info")); } catch { /* resta il nome precedente */ }
  }, []);

  // Il nome del server anche nel titolo della scheda del browser.
  useEffect(() => {
    if (info?.name.trim()) document.title = info.name.trim();
  }, [info?.name]);

  useEffect(() => {
    reload();
    return onUnauthorized(() => setMe(null));
  }, [reload]);

  // Eventi in tempo reale: la cache dei dati si aggiorna senza interrogare il server a intervalli.
  useEffect(() => {
    if (!me) return;
    connectEvents();
    // Gli eventi arrivano a raffica durante una scansione o un'analisi (migliaia di elementi): si raccolgono
    // e si ricarica al massimo una volta ogni pochi secondi per gruppo, mai una richiesta per evento.
    const pending = new Set<string>();
    const timers = new Map<string, number>();
    const GROUPS: Record<string, { keys: string[]; wait: number }> = {
      catalog: { keys: ["items", "home", "item", "libraries", "search"], wait: 10_000 },
      userdata: { keys: ["items", "home", "item"], wait: 1_500 },
      overview: { keys: ["admin:overview"], wait: 2_000 },
      jobs: { keys: ["admin:jobs"], wait: 5_000 },
    };
    const flush = (g: string) => {
      timers.delete(g);
      if (!pending.delete(g)) return;
      const keys = GROUPS[g].keys;
      qc.invalidateQueries({
        predicate: (q) => {
          const k0 = String(q.queryKey[0]);
          return keys.includes(k0) || keys.includes(`${k0}:${String(q.queryKey[1])}`);
        },
      });
    };
    const schedule = (g: string) => {
      pending.add(g);
      if (!timers.has(g)) timers.set(g, window.setTimeout(() => flush(g), GROUPS[g].wait));
    };
    const off = subscribe((e) => {
      if (e.type.startsWith("library.") || e.type.startsWith("item.")) schedule("catalog");
      if (e.type === "user_data.changed") schedule("userdata");
      if (e.type.startsWith("sessions.")) schedule("overview");
      if (e.type.startsWith("jobs.") || e.type.startsWith("scan.")) schedule("jobs");
      if (e.type === "resync") qc.invalidateQueries();
    });
    return () => { off(); disconnectEvents(); timers.forEach((t) => window.clearTimeout(t)); };
  }, [me, qc]);

  const acceptToken = useCallback(async (token: string) => {
    setToken(token);
    await reload();
  }, [reload]);

  const login = useCallback(async (username: string, password: string) => {
    const r = await api<{ token: string }>("/api/v1/auth/login", {
      body: { username, password, device_id: deviceId(), device_name: deviceName(), client: "Platea Web", client_version: __APP_VERSION__ },
    });
    await acceptToken(r.token);
  }, [acceptToken]);

  const logout = useCallback(async () => {
    try { await api("/api/v1/auth/logout", { method: "POST" }); } catch { /* il token è comunque rimosso */ }
    setToken(null);
    setMe(null);
    qc.clear();
  }, [qc]);

  return <Ctx.Provider value={{ info, me, loading, error, login, acceptToken, logout, reload, refreshInfo }}>{children}</Ctx.Provider>;
}

export function useSession() {
  const v = useContext(Ctx);
  if (!v) throw new Error("useSession fuori da SessionProvider");
  return v;
}
