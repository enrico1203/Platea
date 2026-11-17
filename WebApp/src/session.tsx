import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { api, deviceId, deviceName, getToken, onUnauthorized, setToken, serverUrl } from "./api/client";
import { connectEvents, disconnectEvents, subscribe } from "./api/events";
import type { Me, SystemInfo } from "./api/types";
import { useI18n } from "./i18n";

/** Nome, logo e personalizzazione del server: dall'ultima risposta, o dall'ultima vista se il server non risponde. */
export interface Brand { name: string; white_label: boolean; logo: string | null }
const BRAND_KEY = "platea.brand";
function cachedBrand(): Brand | null {
  try {
    const b = JSON.parse(localStorage.getItem(BRAND_KEY) ?? "null") as Brand | null;
    return b && typeof b.name === "string" ? b : null;
  } catch { return null; }
}

interface SessionCtx {
  info: SystemInfo | null;
  brand: Brand | null;
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
  const { setAppName } = useI18n();
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

  const [brand, setBrand] = useState<Brand | null>(cachedBrand);
  useEffect(() => {
    if (!info) return;
    const b: Brand = { name: info.name, white_label: !!info.white_label, logo: info.logo ?? null };
    setBrand(b);
    try { localStorage.setItem(BRAND_KEY, JSON.stringify(b)); } catch { /* ignora */ }
  }, [info]);

  // Il nome del server nel titolo della scheda e in ogni testo (`{app}`).
  useEffect(() => {
    if (!brand) return;
    if (brand.name.trim()) document.title = brand.name.trim();
    setAppName(brand.name);
  }, [brand, setAppName]);

  // Icona della scheda: al caricamento la mette nginx; qui si aggiorna dopo un cambio di logo, nome o personalizzazione.
  const iconKey = info ? `${info.logo ?? ""}|${info.white_label ? info.name : ""}` : null;
  const lastIconKey = useRef<string | null>(null);
  useEffect(() => {
    if (iconKey == null || !info) return;
    const first = lastIconKey.current == null;
    lastIconKey.current = iconKey;
    if (first && document.querySelector("link[rel='icon']")) return;
    const href = info.logo ? `${serverUrl()}${info.logo}` : info.white_label ? `${serverUrl()}/api/v1/branding/icon.svg?n=${encodeURIComponent(info.name)}` : "/platea-app-icon.svg";
    let l = document.querySelector<HTMLLinkElement>("link[rel='icon']");
    if (!l) {
      l = document.createElement("link");
      l.rel = "icon";
      document.head.appendChild(l);
    }
    l.removeAttribute("type");
    l.href = href;
  }, [iconKey]);

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

  return <Ctx.Provider value={{ info, brand, me, loading, error, login, acceptToken, logout, reload, refreshInfo }}>{children}</Ctx.Provider>;
}

export function useSession() {
  const v = useContext(Ctx);
  if (!v) throw new Error("useSession fuori da SessionProvider");
  return v;
}
