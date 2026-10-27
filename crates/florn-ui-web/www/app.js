// The application's loader (RFC 005 rules 32 to 35, 38): a file, no inline script, nothing from
// another origin. It registers the service worker, starts the Host's worker and waits for its
// readiness message, makes the display reader (the ring where the page is isolated, messages
// otherwise), loads the shell's module and starts it with a bridge of the few things the
// shell needs from the page: sending control messages, pacing, reading the display stream,
// files in and out, the labs. The shell reaches the Host through `florn_handle_bytes` only:
// every command, query and subscription is one encoded control message.
import init, { shell_start, shell_reply, shell_frame, shell_scenario, shell_module, shell_admit, shell_file, shell_labs, shell_offline, shell_state } from "./florn_shell.js";
import { createReader } from "./display.js";

const query = new URLSearchParams(location.search);
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

function pickFile(kind) {
  const input = document.createElement("input");
  input.type = "file";
  input.accept = kind === "module" ? ".wasm" : kind === "record" ? ".florn-run,.zip" : ".toml,.florn";
  input.style.display = "none";
  input.addEventListener("change", async () => {
    const f = input.files && input.files[0];
    if (f) shell_file(kind, new Uint8Array(await f.arrayBuffer()));
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
    window.florn = { state: () => ({ ...JSON.parse(shell_state()), replies }), downloads, transport, reader, loadLab };
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
