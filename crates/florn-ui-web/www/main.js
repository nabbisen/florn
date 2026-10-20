// The test page's script (RFC 005 rules 32 to 34): no inline script, nothing from another
// origin. It registers the service worker, starts the Host's worker, waits for its readiness
// message, and then asks it to run one shipped scenario or one benchmark, as the query says.
// The result is written into #out as JSON for a person or the Playwright driver.
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
// as POC-4 did; a probe at start-up would use up what it measures, so the figure is assumed.
function capacity() {
  const ua = navigator.userAgent;
  const instances = /Firefox\//.test(ua) ? 990 : 120;
  return { instances, memory_bytes: 1024 * 1024 * 1024 };
}

async function fetchBytes(path) {
  const r = await fetch(path);
  if (!r.ok) throw new Error(`${path}: ${r.status}`);
  return new Uint8Array(await r.arrayBuffer());
}

async function request() {
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
    return {
      type: "scenario",
      ...base,
      name: scenario,
      text,
      modules,
      seed: seed === null ? null : seed,
      transfer: modules.map((m) => m.bytes.buffer),
    };
  }
  const bench = query.get("bench");
  if (bench === "crossing") {
    const bytes = await fetchBytes("scenarios/modules/kit-crossing.wasm");
    return { type: "bench", ...base, kind: "crossing", bytes, scale: parseInt(query.get("scale") || "1", 10), transfer: [bytes.buffer] };
  }
  if (bench === "cost") {
    const guests = [];
    for (const name of ["bench_protocol", "bench_compute"]) {
      guests.push({ name, bytes: await fetchBytes(`scenarios/modules/${name}.wasm`) });
    }
    return { type: "bench", ...base, kind: "cost", guests, activations: parseInt(query.get("activations") || "10000", 10), transfer: guests.map((g) => g.bytes.buffer) };
  }
  if (bench === "hash") {
    return { type: "bench", ...base, kind: "hash", scale: parseInt(query.get("scale") || "1", 10), transfer: [] };
  }
  if (bench === "probe") {
    return { type: "bench", ...base, kind: "probe", bound: parseInt(query.get("bound") || "5000", 10), transfer: [] };
  }
  if (bench === "handle") {
    return { type: "bench", ...base, kind: "handle", transfer: [] };
  }
  return null;
}

const worker = new Worker("worker.js");
worker.onerror = (e) => show({ error: String(e.message || e) });
worker.onmessage = async (e) => {
  const m = e.data;
  if (m.type === "ready") {
    try {
      const req = await request();
      if (!req) {
        show({ ready: true, isolated: self.crossOriginIsolated === true, hint: "add ?scenario=NAME or ?bench=crossing" });
        return;
      }
      const transfer = req.transfer;
      delete req.transfer;
      worker.postMessage(req, transfer);
    } catch (err) {
      show({ error: String(err) });
    }
  } else if (m.type === "done") {
    show(m.result);
  } else if (m.type === "error") {
    show({ error: m.error });
  }
};
