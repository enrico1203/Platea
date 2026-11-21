// Installazione come app (PWA): service worker, prompt di Android e stato "già installata".
import { useEffect, useState } from "react";

interface InstallPromptEvent extends Event {
  prompt: () => Promise<void>;
  userChoice: Promise<{ outcome: "accepted" | "dismissed" }>;
}

let deferred: InstallPromptEvent | null = null;
let installed = false;
const listeners = new Set<() => void>();
const notify = () => listeners.forEach((f) => f());

/** Aperta come app installata (schermata Home), non nel browser. */
export function isStandalone(): boolean {
  return window.matchMedia("(display-mode: standalone)").matches
    || window.matchMedia("(display-mode: fullscreen)").matches
    || (navigator as Navigator & { standalone?: boolean }).standalone === true;
}

export function mobilePlatform(): "ios" | "android" | null {
  const ua = navigator.userAgent;
  if (/iPhone|iPad|iPod/.test(ua) || (navigator.platform === "MacIntel" && navigator.maxTouchPoints > 1)) return "ios";
  if (/Android/.test(ua)) return "android";
  return null;
}

/** Da chiamare all'avvio: il prompt di Chrome arriva una volta sola, presto, e va tenuto da parte. */
export function initPwa() {
  window.addEventListener("beforeinstallprompt", (e) => {
    e.preventDefault();
    deferred = e as InstallPromptEvent;
    notify();
  });
  window.addEventListener("appinstalled", () => {
    installed = true;
    deferred = null;
    notify();
  });
  if ("serviceWorker" in navigator && import.meta.env.PROD) {
    window.addEventListener("load", () => {
      navigator.serviceWorker.register("/sw.js").catch(() => {});
    });
  }
}

/** Prompt nativo (Android/Chrome). `false` se il browser non lo offre: allora si mostrano le istruzioni. */
export async function promptInstall(): Promise<boolean> {
  if (!deferred) return false;
  const e = deferred;
  deferred = null;
  notify();
  await e.prompt();
  const { outcome } = await e.userChoice;
  if (outcome === "accepted") {
    installed = true;
    notify();
  }
  return true;
}

export function usePwaInstall() {
  const [, force] = useState(0);
  useEffect(() => {
    const f = () => force((n) => n + 1);
    listeners.add(f);
    const mq = window.matchMedia("(display-mode: standalone)");
    mq.addEventListener?.("change", f);
    return () => {
      listeners.delete(f);
      mq.removeEventListener?.("change", f);
    };
  }, []);
  const platform = mobilePlatform();
  // Solo da telefono o tablet, e mai se è già aperta come app.
  const available = platform !== null && !installed && !isStandalone();
  return { available, platform, canPrompt: deferred !== null };
}
