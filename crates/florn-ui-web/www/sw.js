// The service worker (RFC 005 rules 33 and 35): after one visit the page, its scripts, the Host
// module and whatever it fetched are served from the cache, with the isolation headers and the
// policy on every cached response. florn's own code, nothing from another origin.
"use strict";
const CACHE = "florn-test-page-0.1";
const FILES = ["./", "./index.html", "./page.css", "./main.js", "./worker.js", "./florn_host.wasm"];
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
  e.waitUntil(caches.open(CACHE).then((c) => c.addAll(FILES)).then(() => self.skipWaiting()));
});

self.addEventListener("activate", (e) => {
  e.waitUntil(self.clients.claim());
});

self.addEventListener("fetch", (e) => {
  if (e.request.method !== "GET" || new URL(e.request.url).origin !== self.location.origin) return;
  e.respondWith(
    (async () => {
      const cache = await caches.open(CACHE);
      const cached = await cache.match(e.request, { ignoreSearch: true });
      if (cached) return withHeaders(cached);
      const response = await fetch(e.request);
      if (response.ok) cache.put(e.request, response.clone());
      return response;
    })()
  );
});
