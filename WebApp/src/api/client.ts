/**
 * Client dell'API pubblica. La web UI ufficiale usa solo ciò che un client di terzi può usare.
 * Il token viaggia nell'intestazione Authorization, mai in un cookie.
 */
const K_SERVER = "platea.server";
const K_TOKEN = "platea.token";
const K_DEVICE = "platea.device";

function read(k: string): string | null {
  try { return localStorage.getItem(k); } catch { return null; }
}
function write(k: string, v: string | null) {
  try { v == null ? localStorage.removeItem(k) : localStorage.setItem(k, v); } catch { /* ignora */ }
}

/** Indirizzo del server: vuoto = stessa origine (web UI servita accanto al server o dietro proxy). */
export function serverUrl(): string {
  return (read(K_SERVER) ?? "").replace(/\/+$/, "");
}
export function setServerUrl(u: string) {
  write(K_SERVER, u.trim() ? u.trim().replace(/\/+$/, "") : null);
}
export function getToken(): string | null {
  return read(K_TOKEN);
}
export function setToken(t: string | null) {
  write(K_TOKEN, t);
}
export function deviceId(): string {
  let d = read(K_DEVICE);
  if (!d) {
    d = Array.from(crypto.getRandomValues(new Uint8Array(12)), (b) => b.toString(16).padStart(2, "0")).join("");
    write(K_DEVICE, d);
  }
  return d;
}
export function deviceName(): string {
  const ua = navigator.userAgent;
  const browser = /Edg\//.test(ua) ? "Edge" : /Firefox\//.test(ua) ? "Firefox" : /Chrome\//.test(ua) ? "Chrome" : /Safari\//.test(ua) ? "Safari" : "Browser";
  const os = /iPhone|iPad/.test(ua) ? "iOS" : /Android/.test(ua) ? "Android" : /Mac OS X/.test(ua) ? "macOS" : /Windows/.test(ua) ? "Windows" : /Linux/.test(ua) ? "Linux" : "";
  return os ? `${browser} su ${os}` : browser;
}

/** URL assoluto per media e immagini. */
export function mediaUrl(path: string | null | undefined): string | undefined {
  if (!path) return undefined;
  if (/^https?:\/\//.test(path)) return path;
  return serverUrl() + path;
}

export function imageUrl(path: string | null | undefined, width?: number): string | undefined {
  const u = mediaUrl(path);
  if (!u || !width) return u;
  return `${u}${u.includes("?") ? "&" : "?"}w=${Math.round(width * Math.min(window.devicePixelRatio || 1, 2))}`;
}

export class ApiError extends Error {
  constructor(public status: number, public code: string, message: string) {
    super(message);
  }
}

type Opts = { method?: string; body?: unknown; signal?: AbortSignal; keepalive?: boolean };

const unauthorizedListeners = new Set<() => void>();
export function onUnauthorized(fn: () => void) {
  unauthorizedListeners.add(fn);
  return () => { unauthorizedListeners.delete(fn); };
}

export async function api<T = unknown>(path: string, opts: Opts = {}): Promise<T> {
  const headers: Record<string, string> = { Accept: "application/json" };
  const token = getToken();
  if (token) headers.Authorization = `Bearer ${token}`;
  let body: BodyInit | undefined;
  if (opts.body !== undefined) {
    headers["Content-Type"] = "application/json";
    body = JSON.stringify(opts.body);
  }
  let res: Response;
  try {
    res = await fetch(serverUrl() + path, { method: opts.method ?? (body ? "POST" : "GET"), headers, body, signal: opts.signal, keepalive: opts.keepalive });
  } catch (e) {
    if ((e as Error).name === "AbortError") throw e;
    throw new ApiError(0, "Network", "");
  }
  if (res.status === 401 && token && !path.startsWith("/api/v1/auth/login")) {
    setToken(null);
    unauthorizedListeners.forEach((f) => f());
  }
  const text = await res.text();
  let data: unknown = null;
  if (text) {
    try { data = JSON.parse(text); } catch { data = text; }
  }
  if (!res.ok) {
    const d = (data ?? {}) as { error?: string; message?: string };
    throw new ApiError(res.status, d.error ?? `HTTP${res.status}`, d.message ?? res.statusText);
  }
  return data as T;
}

/** Download con autenticazione (backup, esportazione). */
export async function download(path: string, filename: string, method = "GET") {
  const res = await fetch(serverUrl() + path, { method, headers: { Authorization: `Bearer ${getToken() ?? ""}` } });
  if (!res.ok) throw new ApiError(res.status, "Download", res.statusText);
  const blob = await res.blob();
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = filename;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 5000);
}
