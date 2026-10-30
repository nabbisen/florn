// A static server for the test page with the headers of RFC 005 rule 33, serving the page's
// files at the root and the kit's scenarios under /scenarios. Test tooling only. With
// `isolated: false` the isolation headers are left out, as a host that cannot set them would:
// the page then reads the display stream from messages (rule 14).
import http from "node:http";
import { readFile } from "node:fs/promises";
import { dirname, extname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".wasm": "application/wasm",
  ".toml": "text/plain; charset=utf-8",
  ".json": "application/json",
  ".webmanifest": "application/manifest+json",
  ".png": "image/png",
  ".svg": "image/svg+xml",
};

export const POLICY = {
  "Content-Security-Policy":
    "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'; style-src 'self'; img-src 'self'",
};
export const HEADERS = {
  "Cross-Origin-Opener-Policy": "same-origin",
  "Cross-Origin-Embedder-Policy": "require-corp",
  ...POLICY,
};

export function startServer({ www, scenarios, port = 0, isolated = true }) {
  const headers = isolated ? HEADERS : POLICY;
  const roots = [
    ["/scenarios/", resolve(scenarios)],
    ["/", resolve(www)],
  ];
  const server = http.createServer(async (req, res) => {
    const url = new URL(req.url, "http://x");
    let path = url.pathname === "/" ? "/index.html" : url.pathname;
    const [prefix, root] = roots.find(([p]) => path.startsWith(p));
    const file = join(root, path.slice(prefix.length));
    if (!file.startsWith(root)) {
      res.writeHead(403);
      return res.end();
    }
    try {
      const data = await readFile(file);
      res.writeHead(200, {
        "Content-Type": TYPES[extname(file)] || "application/octet-stream",
        // Revalidated on every request, as a host would serve a versioned path, and storable
        // by the service worker's cache (a response marked no-store is not, in every browser).
        "Cache-Control": "no-cache",
        ...headers,
      });
      res.end(data);
    } catch {
      res.writeHead(404, headers);
      res.end("not found");
    }
  });
  return new Promise((ok) => {
    server.listen(port, "127.0.0.1", () => {
      const origin = `http://127.0.0.1:${server.address().port}`;
      ok({ server, origin });
    });
  });
}

// Run directly: serve the page for a person, at the port given (8765 by default).
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
  const { origin } = await startServer({
    www: join(root, "crates/florn-ui-web/www"),
    scenarios: join(root, "scenarios"),
    port: parseInt(process.argv[2] || "8765", 10),
  });
  console.log(`serving the test page at ${origin}/index.html?scenario=echo-pair&seed=42`);
}
