// The test page's script (RFC 005 rules 14, 32 to 34): no inline script, nothing from another
// origin. It registers the service worker, starts the Host's worker, waits for its readiness
// message, and then asks it to run one shipped scenario or one benchmark, as the query says.
// With `display=1` the page reads the display stream once per animation frame while the run
// goes, through the one reader interface over the ring or over messages, and reports what it
// read beside what the worker wrote. The result is written into #out as JSON for a person or
// the Playwright driver.
import { createReader, count } from "./display.js";

const out = document.getElementById("out");
const query = new URLSearchParams(location.search);
let violations = 0;
document.addEventListener("securitypolicyviolation", () => {
  violations += 1;
});

function show(result) {
  const full = {
    ...result,
    isolated: self.crossOriginIsolated === true,
    csp_violations: violations,
    user_agent: navigator.userAgent,
    from_service_worker: Boolean(navigator.serviceWorker && navigator.serviceWorker.controller),
  };
  out.textContent = JSON.stringify(full, null, 1);
  out.dataset.done = "1";
}

if ("serviceWorker" in navigator && query.get("sw") !== "0") {
  navigator.serviceWorker.register("sw.js").catch((e) => console.warn("service worker:", e));
}

// The capacity (RFC 003 rule 20): the family's measured ceiling with a margin for the Host's
// own module. The kit's probe (?bench=probe) found 123 instances in Chromium and 998 in Firefox,
// as POC-4 did, and no ceiling below 5,000 in WebKit; a probe at start-up would use up what it
// measures, so the figure is assumed.
function capacity() {
  const ua = navigator.userAgent;
  const chromium = /Chrome\/|Chromium\//.test(ua);
  const instances = chromium ? 120 : 990;
  return { instances, memory_bytes: 1024 * 1024 * 1024 };
}

async function sha256Hex(chunks) {
  let total = 0;
  for (const c of chunks) total += c.length;
  const all = new Uint8Array(total);
  let at = 0;
  for (const c of chunks) {
    all.set(c, at);
    at += c.length;
  }
  const digest = await crypto.subtle.digest("SHA-256", all);
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

async function fetchBytes(path) {
  const r = await fetch(path);
  if (!r.ok) throw new Error(`${path}: ${r.status}`);
  return new Uint8Array(await r.arrayBuffer());
}

function percentile(sorted, p) {
  if (sorted.length === 0) return null;
  return sorted[Math.min(sorted.length - 1, Math.round((sorted.length - 1) * p))];
}

// The display reader of this page (rule 14): reads once per animation frame, counts what it
// read, measures the read and the interval between frames, and reports the queue's drops to
// the worker. The conformance kit's cell: `stall` milliseconds without reading after the
// first batch force the transport to drop, so that `telemetry.dropped` can be checked.
class Display {
  constructor(worker, stallMs) {
    const { reader, ring, transport } = createReader();
    this.reader = reader;
    this.ring = ring;
    this.transport = transport;
    this.worker = worker;
    this.stallMs = stallMs;
    this.stallUntil = null;
    this.chunks = [];
    this.counts = { records: 0, instants: 0, flights: 0, activity: 0, unknown: 0 };
    this.intervals = [];
    this.reads = [];
    this.last = null;
    this.running = true;
    this.frame = this.frame.bind(this);
    requestAnimationFrame(this.frame);
  }
  push(buffer) {
    const dropped = this.reader.push(buffer);
    if (dropped > 0) this.worker.postMessage({ type: "display-dropped", records: dropped });
  }
  readOnce() {
    const t0 = performance.now();
    const bytes = this.reader.read();
    this.reads.push(performance.now() - t0);
    if (bytes.length > 0) {
      this.chunks.push(bytes);
      count(bytes, this.counts);
    }
  }
  frame(now) {
    if (!this.running) return;
    if (this.last !== null) this.intervals.push(now - this.last);
    this.last = now;
    if (this.stallMs > 0 && this.counts.records > 0 && this.stallUntil === null) {
      this.stallUntil = now + this.stallMs;
    }
    if (this.stallUntil === null || now >= this.stallUntil) this.readOnce();
    requestAnimationFrame(this.frame);
  }
  async finish() {
    this.running = false;
    // The last batch may have landed after the last frame.
    this.readOnce();
    const intervals = [...this.intervals].sort((a, b) => a - b);
    const reads = [...this.reads].sort((a, b) => a - b);
    return {
      transport: this.transport,
      path: this.reader.kind,
      ...this.counts,
      received_digest: await sha256Hex(this.chunks),
      dropped: this.reader.dropped(),
      frames: this.intervals.length,
      interval_p95_ms: percentile(intervals, 0.95),
      interval_max_ms: percentile(intervals, 1),
      read_p95_ms: percentile(reads, 0.95),
      read_max_ms: percentile(reads, 1),
      stalled_ms: this.stallMs,
    };
  }
}

async function request(worker) {
  const base = {
    isolated: self.crossOriginIsolated === true,
    csp_violations: violations,
    user_agent: navigator.userAgent,
    capacity: capacity(),
  };
  const scenario = query.get("scenario");
  if (scenario) {
    const r = await fetch(`scenarios/${scenario}.toml`);
    if (!r.ok) throw new Error(`scenario ${scenario}: ${r.status}`);
    const text = await r.text();
    const names = [...text.matchAll(/^\[modules\.([A-Za-z0-9_-]+)\]/gm)].map((m) => m[1]);
    const modules = [];
    for (const name of names) {
      modules.push({ name, bytes: await fetchBytes(`scenarios/modules/${name}.wasm`) });
    }
    const seed = query.get("seed");
    // `resume=<path>`: a checkpoint file under the origin; `checkpoint-at=T`: a checkpoint
    // file out, in the result (RFC 009 rule 6: a checkpoint crosses the Hosts).
    const resumePath = query.get("resume");
    const resume = resumePath ? await fetchBytes(resumePath) : null;
    const checkpointAt = query.get("checkpoint-at");
    // `branch-at=T`: a branch at the tick (RFC 009 rule 10); both records in the result.
    const branchAt = query.get("branch-at");
    // `cost-model=N`: the version new runs admit under (RFC 010 rule 9).
    const costModel = query.get("cost-model");
    // `residency=swap`: swapping asked for (RFC 009 rule 14); `resident=N`: the pool's cap.
    const residency = query.get("residency");
    const resident = query.get("resident");
    let display = null;
    let reader = null;
    if (query.get("display") === "1") {
      reader = new Display(worker, parseInt(query.get("stall") || "0", 10));
      display = {
        ring: reader.ring,
        slice: parseInt(query.get("slice") || "100", 10),
        period_ms: parseInt(query.get("period") || "16", 10),
        pad: parseInt(query.get("pad") || "0", 10),
      };
    }
    return {
      message: {
        type: "scenario",
        ...base,
        name: scenario,
        text,
        modules,
        seed: seed === null ? null : seed,
        replay: query.get("replay") === "1",
        display,
        resume,
        checkpoint_at: checkpointAt === null ? null : checkpointAt,
        branch_at: branchAt === null ? null : branchAt,
        residency: residency === null ? null : residency,
        resident: resident === null ? null : parseInt(resident, 10),
        cost_model: costModel === null ? null : parseInt(costModel, 10),
      },
      transfer: [...modules.map((m) => m.bytes.buffer), ...(resume ? [resume.buffer] : [])],
      reader,
    };
  }
  const overflow = query.get("overflow");
  if (overflow) {
    // More echo actors than the renderer holds, with the capacity told to be ample: the Host
    // must end the run with `host.instance-failed` and the page must live (RFC 005 rule 10).
    const count = parseInt(overflow, 10);
    const echo = await fetchBytes("scenarios/modules/echo.wasm");
    const hash = await sha256Hex([echo]);
    let text = `florn = 1\nname = "overflow"\ndomains = ["core"]\n\n[time]\ntick = "1ns"\nend = "10ns"\n\n[order]\ntie = "by-identity"\n\n[seed]\nroot = 1\n\n[limits.default]\nmemory = "2MiB"\n\n[modules.echo]\nsha256 = "${hash}"\ncontract = "florn:core/guest@0.1.0"\n`;
    for (let id = 1; id <= count; id++) {
      text += `\n[[actors]]\nid = ${id}\nname = "a${id}"\nkind = "module"\nmodule = "echo"\nports = []\n`;
    }
    return {
      message: {
        type: "scenario",
        ...base,
        // Told ample, so that the estimate passes and the renderer's own ceiling is what fails.
        capacity: { instances: 100000, memory_bytes: 64 * 1024 * 1024 * 1024 },
        name: "overflow",
        text,
        modules: [{ name: "echo", bytes: echo }],
        seed: "1",
        replay: false,
        display: null,
        // `residency=swap` beside `overflow`: the same scenario runs through a pool (rule 13).
        residency: query.get("residency"),
        resident: query.get("resident") === null ? null : parseInt(query.get("resident"), 10),
      },
      transfer: [echo.buffer],
      reader: null,
    };
  }
  const bench = query.get("bench");
  const plain = (message, transfer) => ({ message: { type: "bench", ...base, ...message }, transfer, reader: null });
  if (bench === "crossing") {
    const bytes = await fetchBytes("scenarios/modules/kit-crossing.wasm");
    return plain({ kind: "crossing", bytes, scale: parseInt(query.get("scale") || "1", 10) }, [bytes.buffer]);
  }
  if (bench === "cost") {
    const guests = [];
    for (const name of ["bench_protocol", "bench_compute"]) {
      guests.push({ name, bytes: await fetchBytes(`scenarios/modules/${name}.wasm`) });
    }
    return plain({ kind: "cost", guests, activations: parseInt(query.get("activations") || "10000", 10), cost_model: query.get("cost-model") === null ? null : parseInt(query.get("cost-model"), 10) }, guests.map((g) => g.bytes.buffer));
  }
  if (bench === "hash") {
    return plain({ kind: "hash", scale: parseInt(query.get("scale") || "1", 10) }, []);
  }
  if (bench === "probe") {
    return plain({ kind: "probe", bound: parseInt(query.get("bound") || "5000", 10) }, []);
  }
  if (bench === "handle") {
    return plain({ kind: "handle" }, []);
  }
  return null;
}

const worker = new Worker("worker.js", { type: "module" });
let reader = null;
worker.onerror = (e) => show({ error: String(e.message || e) });
worker.onmessage = async (e) => {
  const m = e.data;
  if (m.type === "ready") {
    try {
      const req = await request(worker);
      if (!req) {
        show({ ready: true, isolated: self.crossOriginIsolated === true, hint: "add ?scenario=NAME or ?bench=crossing" });
        return;
      }
      reader = req.reader;
      worker.postMessage(req.message, req.transfer);
    } catch (err) {
      show({ error: String(err) });
    }
  } else if (m.type === "display") {
    if (reader) reader.push(m.buffer);
  } else if (m.type === "flush") {
    // Every batch before this one is pushed and its drops reported: say so.
    worker.postMessage({ type: "flushed" });
  } else if (m.type === "done") {
    const result = m.result;
    if (reader) {
      result.display = await reader.finish();
      reader = null;
    }
    show(result);
  } else if (m.type === "error") {
    show({ error: m.error });
  }
};
