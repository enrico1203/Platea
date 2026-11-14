/** Eventi in tempo reale via WebSocket, con riconnessione automatica. */
import { getToken, serverUrl } from "./client";

export interface ServerEvent { type: string; data?: Record<string, unknown> }
type Listener = (e: ServerEvent) => void;

const listeners = new Set<Listener>();
let ws: WebSocket | null = null;
let retry = 0;
let timer: number | undefined;
let wanted = false;

function wsUrl(): string {
  const base = serverUrl() || window.location.origin;
  const u = new URL(base);
  u.protocol = u.protocol === "https:" ? "wss:" : "ws:";
  u.pathname = "/api/v1/events";
  u.search = `token=${encodeURIComponent(getToken() ?? "")}`;
  return u.toString();
}

function open() {
  if (!wanted || !getToken()) return;
  try {
    ws = new WebSocket(wsUrl());
  } catch {
    schedule();
    return;
  }
  ws.onopen = () => { retry = 0; };
  ws.onmessage = (m) => {
    try {
      const e = JSON.parse(m.data) as ServerEvent;
      listeners.forEach((l) => l(e));
    } catch { /* ignora */ }
  };
  ws.onclose = () => { ws = null; schedule(); };
}

function schedule() {
  if (!wanted) return;
  window.clearTimeout(timer);
  const delay = Math.min(30000, 1000 * 2 ** retry++);
  timer = window.setTimeout(open, delay);
}

export function connectEvents() {
  wanted = true;
  if (!ws) open();
}
export function disconnectEvents() {
  wanted = false;
  window.clearTimeout(timer);
  ws?.close();
  ws = null;
}
export function subscribe(l: Listener) {
  listeners.add(l);
  return () => { listeners.delete(l); };
}
