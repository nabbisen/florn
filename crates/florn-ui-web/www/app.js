// The application's loader (RFC 005 rules 32 to 35, 38): a file, no inline script, nothing from
// another origin. It registers the service worker, starts the Host's worker and waits for its
// readiness message, makes the display reader (the ring where the page is isolated, messages
// otherwise), loads the shell's module and starts it with a bridge of the few things the
// shell needs from the page: sending control messages, pacing, reading the display stream,
// files in and out, the labs. The shell reaches the Host through `florn_handle_bytes` only:
// every command, query and subscription is one encoded control message.
import init, { shell_start, shell_reply, shell_frame, shell_scenario, shell_module, shell_admit, shell_file, shell_labs, shell_offline, shell_state, shell_files, shell_evicted } from "./florn_shell.js";
import { createReader } from "./display.js";

const query = new URLSearchParams(location.search);
// Policy violations, if any, with what was refused: the journeys read them (RFC 005 rule 34).
const violations = [];
document.addEventListener("securitypolicyviolation", (e) => {
  violations.push({ directive: e.effectiveDirective, blocked: e.blockedURI, sample: e.sample, file: e.sourceFile, line: e.lineNumber });
});
const worker = new Worker("worker.js", { type: "module" });
const { reader, ring, transport } = createReader();
let shell = null;
let pacing = 1000 / 60;
let replies = 0;

function capacity() {
  const ua = navigator.userAgent;
  const chromium = /Chrome\/|Chromium\//.test(ua);
  return { instances: chromium ? 120 : 990, memory_bytes: 1024 * 1024 * 1024 };
}

async function fetchBytes(path) {
  const r = await fetch(path);
  if (!r.ok) throw new Error(`${path}: ${r.status}`);
  return new Uint8Array(await r.arrayBuffer());
}

// A scenario shipped with florn: its manifest and the modules it names, in that order.
async function loadLab(name) {
  const r = await fetch(`scenarios/${name}.toml`);
  if (!r.ok) throw new Error(`scenario ${name}: ${r.status}`);
  const text = await r.text();
  const names = [...text.matchAll(/^\[modules\.([A-Za-z0-9_-]+)\]/gm)].map((m) => m[1]);
  for (const n of names) shell_module(await fetchBytes(`scenarios/modules/${n}.wasm`));
  shell_scenario(new TextEncoder().encode(text));
}

// Origin storage (RFC 009 rule 16): a cache of what the person opened, saved or imported, each
// file by its hash with the name they gave and its kind, in a cache of this origin; an index
// beside it tells an eviction the browser made from a file never kept. The file on disk is the
// durable form; the list is shown under "Files" with its size.
const FILES_CACHE = "florn-files";
const INDEX_KEY = "florn-files-index";
const FILE_KIND = { scenario: "manifest", record: "run-record", module: "module", checkpoint: "checkpoint" };

async function sha256Hex(bytes) {
  const d = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(d)].map((b) => b.toString(16).padStart(2, "0")).join("");
}
function readIndex() {
  try {
    return JSON.parse(localStorage.getItem(INDEX_KEY) || "{}");
  } catch {
    return {};
  }
}
function writeIndex(index) {
  try {
    localStorage.setItem(INDEX_KEY, JSON.stringify(index));
  } catch (e) {
    console.warn("files index:", e);
  }
}
const fileUrl = (key) => `${location.origin}/florn-files/${key}`;
async function storeFile(kind, name, bytes) {
  try {
    const key = await sha256Hex(bytes);
    const cache = await caches.open(FILES_CACHE);
    await cache.put(new Request(fileUrl(key)), new Response(bytes, { headers: { "content-type": "application/octet-stream", "x-florn-kind": kind, "x-florn-name": name } }));
    const index = readIndex();
    index[key] = { kind, name, bytes: bytes.length };
    writeIndex(index);
  } catch (e) {
    console.warn("files:", e);
  }
  await refreshFiles();
}
async function refreshFiles() {
  const index = readIndex();
  let present = null;
  try {
    const cache = await caches.open(FILES_CACHE);
    present = new Set((await cache.keys()).map((r) => r.url.slice(r.url.lastIndexOf("/") + 1)));
  } catch (e) {
    console.warn("files:", e);
  }
  const lines = [];
  let total = 0;
  for (const [key, f] of Object.entries(index)) {
    if (present && !present.has(key)) {
      // The browser let it go: said as a fact, never as an error (rule 16).
      shell_evicted(f.kind, f.name);
      delete index[key];
      continue;
    }
    lines.push(`${f.kind}\t${f.name}\t${key}\t${f.bytes}`);
    total += f.bytes;
  }
  writeIndex(index);
  shell_files(lines.join("\n"), total);
}
async function readStored(key) {
  const cache = await caches.open(FILES_CACHE);
  const r = await cache.match(new Request(fileUrl(key)));
  if (!r) return null;
  return { kind: r.headers.get("x-florn-kind"), name: r.headers.get("x-florn-name"), bytes: new Uint8Array(await r.arrayBuffer()) };
}
async function openStored(key) {
  const f = await readStored(key);
  if (!f) return refreshFiles();
  if (f.kind === "manifest" || f.kind === "package") shell_scenario(f.bytes);
  else if (f.kind === "module") shell_admit(f.bytes);
  else if (f.kind === "run-record") shell_file("record", f.bytes);
  else if (f.kind === "checkpoint") shell_file("checkpoint", f.bytes);
}
async function exportStored(key) {
  const f = await readStored(key);
  if (!f) return refreshFiles();
  download(f.name, f.bytes, "application/octet-stream");
}

function pickFile(kind) {
  const input = document.createElement("input");
  input.type = "file";
  input.accept = kind === "module" ? ".wasm" : kind === "record" ? ".florn-run,.zip" : kind === "checkpoint" ? ".florn-checkpoint" : ".toml,.florn";
  input.hidden = true;
  input.addEventListener("change", async () => {
    const f = input.files && input.files[0];
    if (f) {
      const bytes = new Uint8Array(await f.arrayBuffer());
      shell_file(kind, bytes);
      // Opened or imported: kept in this browser (rule 16).
      const fileKind = kind === "scenario" && /\.florn$/i.test(f.name) ? "package" : FILE_KIND[kind] || kind;
      storeFile(fileKind, f.name, bytes);
    }
    input.remove();
  });
  document.body.appendChild(input);
  input.click();
}

const downloads = [];
function download(name, bytes, mime) {
  const blob = new Blob([bytes], { type: mime });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  downloads.push({ name, bytes: bytes.length });
  setTimeout(() => URL.revokeObjectURL(url), 10000);
}

const bridge = {
  send(bytes) {
    const copy = bytes.slice();
    worker.postMessage({ type: "control", buffer: copy.buffer }, [copy.buffer]);
  },
  pace(ticksPerSlice) {
    pacing = ticksPerSlice;
    worker.postMessage({ type: "pace", ticks: Math.max(1, Math.min(4294967295, Math.round(ticksPerSlice))) });
  },
  readDisplay() {
    return reader.read();
  },
  transport() {
    return transport;
  },
  download,
  loadLab(name) {
    loadLab(name).catch((e) => console.error("lab:", e));
  },
  pickFile,
  store(kind, name, bytes) {
    storeFile(kind, name, bytes.slice()).catch((e) => console.warn("files:", e));
  },
  openStored(key) {
    openStored(key).catch((e) => console.warn("files:", e));
  },
  exportStored(key) {
    exportStored(key).catch((e) => console.warn("files:", e));
  },
  reducedMotion() {
    return window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  },
  origin() {
    return location.origin + location.pathname;
  },
  isolated() {
    return self.crossOriginIsolated === true;
  },
};

if ("serviceWorker" in navigator && query.get("sw") !== "0") {
  navigator.serviceWorker.register("sw.js").then(
    () => navigator.serviceWorker.ready.then(() => shell && shell_offline(true)),
    (e) => {
      console.warn("service worker:", e);
      shell && shell_offline(false);
    }
  );
}

worker.onmessage = async (e) => {
  const m = e.data;
  if (m.type === "ready") {
    worker.postMessage({ type: "shell", capacity: capacity(), ring, pace: Math.round(pacing) });
    shell = await init();
    shell_start(bridge);
    // The journeys read the shell's facts here (checklist C20).
    window.florn = { state: () => ({ ...JSON.parse(shell_state()), replies }), downloads, transport, reader, loadLab, violations };
    try {
      const labs = await (await fetch("labs.txt")).text();
      shell_labs(labs);
    } catch (err) {
      console.warn("labs:", err);
    }
    try {
      const palette = (await (await fetch("palette.txt")).text()).split("\n").map((s) => s.trim()).filter(Boolean);
      for (const name of palette) shell_admit(await fetchBytes(`scenarios/modules/${name}.wasm`));
    } catch (err) {
      console.warn("palette:", err);
    }
    if (navigator.serviceWorker && navigator.serviceWorker.controller) shell_offline(true);
    refreshFiles().catch((e) => console.warn("files:", e));
    const lab = query.get("lab");
    if (lab) loadLab(lab).catch((err) => console.error("lab:", err));
    const frame = (ts) => {
      shell_frame(ts);
      requestAnimationFrame(frame);
    };
    requestAnimationFrame(frame);
  } else if (m.type === "reply") {
    replies += 1;
    shell_reply(new Uint8Array(m.buffer));
  } else if (m.type === "display") {
    const dropped = reader.push(m.buffer);
    if (dropped > 0) worker.postMessage({ type: "display-dropped", records: dropped });
  } else if (m.type === "error") {
    console.error("worker:", m.error);
  }
};
worker.onerror = (e) => console.error("worker:", e.message || e);
