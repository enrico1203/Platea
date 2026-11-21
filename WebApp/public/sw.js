// Service worker di Platea: rende l'interfaccia installabile e mostra una pagina se manca la rete.
// Non tocca mai l'API, i video, le immagini del server né il manifest: quelli arrivano sempre dal server.
const VERSION = "1";
const SHELL = `platea-shell-${VERSION}`;
const ASSETS = "platea-assets";
const MAX_ASSETS = 150;

self.addEventListener("install", (e) => {
  e.waitUntil(caches.open(SHELL).then((c) => c.addAll(["/offline.html"])).then(() => self.skipWaiting()));
});

self.addEventListener("activate", (e) => {
  e.waitUntil(
    caches.keys()
      .then((keys) => Promise.all(keys.filter((k) => k.startsWith("platea-shell-") && k !== SHELL).map((k) => caches.delete(k))))
      .then(() => self.clients.claim()),
  );
});

async function trim() {
  const c = await caches.open(ASSETS);
  const keys = await c.keys();
  for (const k of keys.slice(0, Math.max(0, keys.length - MAX_ASSETS))) await c.delete(k);
}

self.addEventListener("fetch", (e) => {
  const req = e.request;
  if (req.method !== "GET" || req.headers.has("range")) return;
  const url = new URL(req.url);
  if (url.origin !== self.location.origin) return;

  // Pagine: sempre dalla rete (nome, icone e versione nuova); senza rete la pagina "sei offline".
  if (req.mode === "navigate") {
    e.respondWith(fetch(req).catch(async () => (await caches.match("/offline.html", { cacheName: SHELL })) || Response.error()));
    return;
  }
  // File dell'interfaccia con l'hash nel nome: non cambiano mai, si tengono in cache.
  if (url.pathname.startsWith("/assets/")) {
    e.respondWith(
      caches.open(ASSETS).then(async (c) => {
        const hit = await c.match(req);
        if (hit) return hit;
        const res = await fetch(req);
        if (res.ok) {
          c.put(req, res.clone()).then(trim).catch(() => {});
        }
        return res;
      }),
    );
  }
});
