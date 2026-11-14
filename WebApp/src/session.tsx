import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { api, deviceId, deviceName, getToken, onUnauthorized, setToken } from "./api/client";
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

  useEffect(() => {
    reload();
    return onUnauthorized(() => setMe(null));
  }, [reload]);

  // Eventi in tempo reale: la cache dei dati si aggiorna senza interrogare il server a intervalli.
  useEffect(() => {
    if (!me) return;
    connectEvents();
    const off = subscribe((e) => {
      if (e.type.startsWith("library.") || e.type.startsWith("item.")) qc.invalidateQueries({ predicate: (q) => ["items", "home", "item", "libraries", "search"].includes(String(q.queryKey[0])) });
      if (e.type === "user_data.changed") qc.invalidateQueries({ predicate: (q) => ["items", "home", "item"].includes(String(q.queryKey[0])) });
      if (e.type.startsWith("sessions.")) qc.invalidateQueries({ queryKey: ["admin", "overview"] });
      if (e.type.startsWith("jobs.") || e.type.startsWith("scan.")) qc.invalidateQueries({ queryKey: ["admin", "jobs"] });
      if (e.type === "resync") qc.invalidateQueries();
    });
    return () => { off(); disconnectEvents(); };
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

  return <Ctx.Provider value={{ info, me, loading, error, login, acceptToken, logout, reload }}>{children}</Ctx.Provider>;
}

export function useSession() {
  const v = useContext(Ctx);
  if (!v) throw new Error("useSession fuori da SessionProvider");
  return v;
}
