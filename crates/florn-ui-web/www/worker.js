// The Host's worker (RFC 005 rules 6 to 8, 10, 32): instantiates the Host module and holds the
// guest instances for it. The Host reaches a guest through the imports under "florn" below and
// through its own exported function table, which this script grows with every guest export it
// resolves; a guest reaches the Host through the Host's exports bound as its imports; the
// budget global is shared by every guest and reached by the Host through the budget module.
"use strict";

// (module
//   (import "env" "gas_counter" (global $g (mut i64)))
//   (func (export "get") (result i64) global.get $g)
//   (func (export "set") (param i64) local.get 0 global.set $g))
const BUDGET_MODULE = new Uint8Array([0,97,115,109,1,0,0,0,1,9,2,96,0,1,126,96,1,126,0,2,20,1,3,101,110,118,11,103,97,115,95,99,111,117,110,116,101,114,3,126,1,3,3,2,0,1,7,13,2,3,103,101,116,0,0,3,115,101,116,0,1,10,13,2,4,0,35,0,11,6,0,32,0,36,0,11]);

const gasGlobal = new WebAssembly.Global({ value: "i64", mutable: true }, 0n);
const budget = new WebAssembly.Instance(new WebAssembly.Module(BUDGET_MODULE), {
  env: { gas_counter: gasGlobal },
});

let host = null;
let hostMemory = null;
let hostView = null;
let table = null;
const modules = [];
const instances = [];
const importObjects = new Map();
let lastError = "";
let lastErrorKind = 0;
// Views made over guest memories: one per instance, plus one per growth (RFC 005 rule 6).
let viewsCreated = 0;
const decoder = new TextDecoder();
const encoder = new TextEncoder();

function hostBytes() {
  if (hostView === null || hostView.buffer !== hostMemory.buffer) {
    hostView = new Uint8Array(hostMemory.buffer);
  }
  return hostView;
}

function text(ptr, len) {
  return decoder.decode(hostBytes().subarray(ptr, ptr + len));
}

function putText(s, dst, cap) {
  const b = encoder.encode(s);
  const n = Math.min(b.length, cap);
  hostBytes().set(b.subarray(0, n), dst);
  return n;
}

function fail(e, kind) {
  lastError = typeof e === "string" ? e : String((e && e.message) || e);
  lastErrorKind = kind || 0;
}

function isResource(e) {
  return e instanceof RangeError || typeof e === "string" || /out of memory|too many/i.test(String(e));
}

function inst(i) {
  const x = instances[i];
  if (!x) throw new Error("no instance " + i);
  return x;
}

function view(x) {
  if (x.view.buffer !== x.memory.buffer) {
    x.view = new Uint8Array(x.memory.buffer);
    viewsCreated += 1;
  }
  return x.view;
}

function family() {
  const ua = navigator.userAgent;
  let m;
  if ((m = /Firefox\/(\d+(?:\.\d+)?)/.exec(ua))) return "firefox " + m[1];
  if ((m = /(?:Chrome|Chromium|HeadlessChrome)\/(\d+(?:\.\d+)?)/.exec(ua))) return "chromium " + m[1];
  if ((m = /Version\/(\d+(?:\.\d+)?)/.exec(ua)) && /Safari/.test(ua)) return "webkit " + m[1];
  if ((m = /AppleWebKit\/(\d+(?:\.\d+)?)/.exec(ua))) return "webkit " + m[1];
  return "browser";
}

// The import object of a world, built once and shared by every instance of it (rule 10).
function importsFor(hostModule) {
  let obj = importObjects.get(hostModule);
  if (obj) return obj;
  obj = { env: { gas_counter: gasGlobal } };
  obj[hostModule] = {};
  for (const name of ["send", "set-timer", "cancel-timer", "random", "report"]) {
    const f = host.exports["florn_import_" + name.replaceAll("-", "_")];
    if (typeof f === "function") obj[hostModule][name] = f;
  }
  importObjects.set(hostModule, obj);
  return obj;
}

const florn = {
  engine_compile(ptr, len) {
    try {
      modules.push(new WebAssembly.Module(hostBytes().subarray(ptr, ptr + len)));
      return modules.length - 1;
    } catch (e) {
      fail(e, 0);
      return -1;
    }
  },
  engine_instantiate(m, namePtr, nameLen) {
    const module = modules[m];
    if (!module) {
      fail("no module " + m, 0);
      return -1;
    }
    const hostModule = text(namePtr, nameLen);
    try {
      const instance = new WebAssembly.Instance(module, importsFor(hostModule));
      const memory = instance.exports.memory;
      if (!(memory instanceof WebAssembly.Memory)) {
        fail("no memory export", 0);
        return -1;
      }
      instances.push({ instance, memory, exports: instance.exports, view: new Uint8Array(memory.buffer) });
      viewsCreated += 1;
      return instances.length - 1;
    } catch (e) {
      fail(e, isResource(e) ? 1 : 0);
      return -1;
    }
  },
  engine_drop(i) {
    instances[i] = null;
  },
  engine_func(i, namePtr, nameLen) {
    const f = inst(i).exports[text(namePtr, nameLen)];
    if (typeof f !== "function") return -1;
    const index = table.grow(1);
    table.set(index, f);
    return index;
  },
  engine_call(i, sig, func, args, results) {
    try {
      host.exports.florn_call(sig, func, args, results);
      return 0;
    } catch (e) {
      fail(e, 0);
      if (e instanceof RangeError) return 2;
      if (e instanceof WebAssembly.RuntimeError) return 1;
      return 3;
    }
  },
  engine_memory_size(i) {
    return inst(i).memory.buffer.byteLength;
  },
  engine_memory_read(i, addr, len, dst) {
    const v = view(inst(i));
    if (addr + len > v.length) return -1;
    hostBytes().set(v.subarray(addr, addr + len), dst);
    return 0;
  },
  engine_memory_write(i, addr, src, len) {
    const v = view(inst(i));
    if (addr + len > v.length) return -1;
    v.set(hostBytes().subarray(src, src + len), addr);
    return 0;
  },
  engine_memory_grow_to(i, bytes) {
    const x = inst(i);
    const have = x.memory.buffer.byteLength;
    if (bytes > have) {
      try {
        x.memory.grow(Math.ceil((bytes - have) / 65536));
      } catch (e) {
        fail(e, 0);
        return -1;
      }
    }
    return 0;
  },
  engine_global_get(i, namePtr, nameLen, tag, out) {
    const g = inst(i).exports[text(namePtr, nameLen)];
    if (!(g instanceof WebAssembly.Global)) return -1;
    const buf = hostBytes().buffer;
    const v = g.value;
    if (tag === 1) new Int32Array(buf, out, 1)[0] = v;
    else if (tag === 2) new BigInt64Array(buf, out, 1)[0] = v;
    else if (tag === 3) new Float32Array(buf, out, 1)[0] = v;
    else new Float64Array(buf, out, 1)[0] = v;
    return 0;
  },
  engine_global_set(i, namePtr, nameLen, tag, bits) {
    const g = inst(i).exports[text(namePtr, nameLen)];
    if (!(g instanceof WebAssembly.Global)) return -1;
    const buf = hostBytes().buffer;
    try {
      if (tag === 1) g.value = new Int32Array(buf, bits, 1)[0];
      else if (tag === 2) g.value = new BigInt64Array(buf, bits, 1)[0];
      else if (tag === 3) g.value = new Float32Array(buf, bits, 1)[0];
      else g.value = new Float64Array(buf, bits, 1)[0];
      return 0;
    } catch (e) {
      fail(e, 0);
      return -1;
    }
  },
  engine_table_size(i, namePtr, nameLen) {
    const t = inst(i).exports[text(namePtr, nameLen)];
    return t instanceof WebAssembly.Table ? t.length : -1;
  },
  engine_last_error(dst, cap) {
    return putText(lastError, dst, cap);
  },
  engine_last_error_kind() {
    return lastErrorKind;
  },
  engine_info(dst, cap) {
    return putText(family(), dst, cap);
  },
  budget_get: budget.exports.get,
  budget_set: budget.exports.set,
  wall_clock_ms: () => Date.now(),
  entropy: () => crypto.getRandomValues(new Uint32Array(1))[0],
  perf_now: () => performance.now() / 1000,
};

function out() {
  const s = text(host.exports.florn_out_ptr(), host.exports.florn_out_len());
  return s.length ? JSON.parse(s) : null;
}

// Hand bytes to the Host: a buffer of its allocator, which the call takes back.
function give(bytes) {
  const p = host.exports.florn_alloc(bytes.length);
  hostBytes().set(bytes, p);
  return p;
}

function words(n) {
  const b = BigInt(n);
  return [Number(b & 0xffffffffn), Number(b >> 32n)];
}

function runScenario(m) {
  host.exports.florn_start(m.capacity.instances, m.capacity.memory_bytes);
  const events = [];
  const collect = (r) => {
    for (const x of r || []) if (x.code) events.push(x);
    return r || [];
  };
  const scenario = encoder.encode(m.text);
  host.exports.florn_load_scenario(give(scenario), scenario.length);
  collect(out());
  const admitted = {};
  for (const mod of m.modules) {
    host.exports.florn_admit_kit(give(mod.bytes), mod.bytes.length);
    const a = (out() || []).find((x) => x.kind === "admitted");
    if (a) admitted[mod.name] = a.admitted;
    host.exports.florn_load_module(give(mod.bytes), mod.bytes.length);
    collect(out());
  }
  host.exports.florn_subscribe_events();
  const [lo, hi] = m.seed === null ? [0, 0] : words(m.seed);
  host.exports.florn_new_run(lo, hi, m.seed === null ? 0 : 1);
  collect(out());
  const t0 = performance.now();
  host.exports.florn_advance(0xffffffff, 0xffffffff);
  const ms = performance.now() - t0;
  collect(out());
  host.exports.florn_run_state();
  const state = (out() || []).find((x) => x.kind === "run-state") || {};
  host.exports.florn_support();
  const support = (out() || []).find((x) => x.kind === "support") || {};
  return { ...state, admitted, events, ms, views_created: viewsCreated, instances: instances.filter(Boolean).length, support };
}

function runBench(m) {
  if (m.kind === "crossing") {
    const r = host.exports.florn_bench_crossing(give(m.bytes), m.bytes.length, m.scale);
    return { kind: "crossing", status: r, figures: out(), views_created: viewsCreated, engine: family() };
  }
  if (m.kind === "cost") {
    const figures = [];
    for (const g of m.guests) {
      const name = encoder.encode(g.name);
      const r = host.exports.florn_bench_cost(give(g.bytes), g.bytes.length, give(name), name.length, m.activations);
      figures.push({ status: r, ...out() });
    }
    return { kind: "cost", figures, engine: family() };
  }
  if (m.kind === "hash") {
    const r = host.exports.florn_bench_hash(m.scale);
    return { kind: "hash", status: r, figures: out(), engine: family() };
  }
  if (m.kind === "probe") {
    // The capacity probe (RFC 003 rule 20): instances of a one-page module until the renderer
    // refuses, up to the bound given; then the largest memory one instance may reserve.
    // The memory first, while the address space is free; the instances after, since what the
    // probe makes stays until the page goes.
    let memoryPages = 0;
    for (const pages of [1024, 4096, 16384, 32768, 65536]) {
      try {
        new WebAssembly.Memory({ initial: pages });
        memoryPages = pages;
      } catch (e) {
        break;
      }
    }
    const tiny = new WebAssembly.Module(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 5, 3, 1, 0, 1]));
    const kept = [];
    let error = null;
    const t0 = performance.now();
    while (kept.length < m.bound) {
      try {
        kept.push(new WebAssembly.Instance(tiny, {}));
      } catch (e) {
        error = String((e && e.message) || e);
        break;
      }
    }
    const perInstance = (performance.now() - t0) * 1000 / Math.max(1, kept.length);
    return { kind: "probe", instances: kept.length, stopped_by: error, micros_per_instance: perInstance, memory_pages: memoryPages, engine: family() };
  }
  if (m.kind === "handle") {
    // The transport's form with bytes that are no control message: the coded refusal.
    host.exports.florn_start(m.capacity.instances, m.capacity.memory_bytes);
    const junk = new Uint8Array([1, 2, 3]);
    host.exports.florn_handle(give(junk), junk.length);
    return { kind: "handle", replies: out() };
  }
  throw new Error("unknown benchmark " + m.kind);
}

self.onmessage = (e) => {
  const m = e.data;
  try {
    const result = m.type === "scenario" ? runScenario(m) : runBench(m);
    self.postMessage({ type: "done", result });
  } catch (err) {
    self.postMessage({ type: "error", error: String((err && err.stack) || err) });
  }
};

(async () => {
  const bytes = await (await fetch("florn_host.wasm")).arrayBuffer();
  const { instance } = await WebAssembly.instantiate(bytes, { florn });
  host = instance;
  hostMemory = host.exports.memory;
  table = host.exports.__indirect_function_table;
  // Rule 32: nothing is posted to this worker before it says it is ready.
  self.postMessage({ type: "ready" });
})().catch((err) => self.postMessage({ type: "error", error: String((err && err.stack) || err) }));
