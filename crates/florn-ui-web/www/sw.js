// The service worker (RFC 005 rules 33 and 35): after one visit the application, its scripts,
// the Host module, the shell and whatever it fetched (the labs' scenarios and modules) are
// served from the cache, with the isolation headers and the policy on every cached response.
// florn's own code, nothing from another origin. The cache is named by the version: a released
// version lives at its own path and never updates itself under a person.
"use strict";
const CACHE = "florn-0.2";
// The small files are cached on install; the modules and whatever a page fetches (the labs'
// scenarios and modules, the shell's and the Host's WebAssembly) are cached as they are
// fetched, so that the first visit is light and a page that ran once starts offline.
const FILES = ["./", "./index.html", "./shell.css", "./app.js", "./florn_shell.js", "./worker.js", "./display.js", "./labs.txt", "./palette.txt", "./kit.html", "./page.css", "./main.js"];
const HEADERS = {
  "Cross-Origin-Opener-Policy": "same-origin",
  "Cross-Origin-Embedder-Policy": "require-corp",
  "Content-Security-Policy":
    "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'; style-src 'self'; img-src 'self'",
};

function withHeaders(response) {
  const headers = new Headers(response.headers);
  for (const [k, v] of Object.entries(HEADERS)) headers.set(k, v);
  return new Response(response.body, { status: response.status, statusText: response.statusText, headers });
}

self.addEventListener("install", (e) => {
  e.waitUntil(
    caches
      .open(CACHE)
      .then(async (c) => {
        for (const f of FILES) {
          try {
            await c.add(f);
          } catch (err) {
            console.warn("cache:", f, err);
          }
        }
      })
      .then(() => self.skipWaiting())
  );
});

self.addEventListener("activate", (e) => {
  e.waitUntil(self.clients.claim());
});

self.addEventListener("fetch", (e) => {
  if (e.request.method !== "GET" || new URL(e.request.url).origin !== self.location.origin) return;
  e.respondWith(
    (async () => {
      let cache = null;
      try {
        cache = await caches.open(CACHE);
        const cached = await cache.match(e.request, { ignoreSearch: true });
        if (cached) return withHeaders(cached);
      } catch (err) {
        console.warn("cache:", err);
      }
      const response = await fetch(e.request);
      if (cache && response.ok) {
        try {
          await cache.put(e.request, response.clone());
        } catch (err) {
          console.warn("cache:", err);
        }
      }
      return response;
    })()
  );
});
