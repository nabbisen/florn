// The conformance kit under the browser Host (AC-001, AC-002, AC-007; RFC 005 section 11):
// every shipped scenario through the test page in each browser, compared with the recorded
// digests; the hostile scenarios' faults; the isolation headers, the policy and the network log
// (rules 33 and 34); the start from the service worker's cache (rules 32 and 35); the transport's
// refusal of bytes that are no message; the display stream over the ring and over messages
// (rule 14; checklist C18): identical records, the transport's cell at 2,000 records per frame,
// the drops reported; the crossing and cost benchmarks' figures.
//
// Usage: node run.js --browsers chromium,firefox[,webkit] [--quick] [--port N]
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { existsSync } from "node:fs";
import { chromium, firefox, webkit } from "playwright";
import { startServer } from "./serve.js";

const arg = (k, d) => {
  const i = process.argv.indexOf(k);
  return i >= 0 ? process.argv[i + 1] : d;
};
const browsers = arg("--browsers", "chromium").split(",");
const quick = process.argv.includes("--quick");
const engines = { chromium, firefox, webkit };
const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

/// The digests file: `[modules]`, `[admitted]`, `[admitted-2]`, and one `[runs."name"]` table
/// per scenario.
function parseDigests(text) {
  const d = { modules: {}, admitted: {}, "admitted-2": {}, runs: {} };
  let table = null;
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    let m;
    if ((m = /^\[runs\."([^"]+)"\]$/.exec(line))) {
      table = d.runs[m[1]] = {};
    } else if ((m = /^\[([\w-]+)\]$/.exec(line))) {
      table = d[m[1]] = d[m[1]] || {};
    } else if ((m = /^([\w-]+) = (.*)$/.exec(line)) && table) {
      const v = m[2];
      table[m[1]] = v.startsWith('"') ? v.slice(1, -1) : Number(v);
    }
  }
  return d;
}

const digests = parseDigests(await readFile(join(root, "scenarios/digests.toml"), "utf8"));
const scenarios = Object.keys(digests.runs);
const digests_of = (s) => digests.runs[s];
const { server, origin } = await startServer({
  www: join(root, "crates/florn-ui-web/www"),
  scenarios: join(root, "scenarios"),
  port: parseInt(arg("--port", "0"), 10),
});
// A second origin without the isolation headers, as a host that cannot set them: the page
// there reads the display stream from messages (rule 14).
const plain = await startServer({
  www: join(root, "crates/florn-ui-web/www"),
  scenarios: join(root, "scenarios"),
  port: 0,
  isolated: false,
});

let failures = 0;
function check(ok, what, detail) {
  if (!ok) failures += 1;
  console.log(JSON.stringify({ check: what, ok: Boolean(ok), ...(detail || {}) }));
}
function faultOf(result) {
  const e = (result.events || []).find((x) => x.code === "actor.faulted");
  if (!e) return null;
  return { actor: e.params.actor.value, fault: e.params.fault.value, metered: e.params.metered.value };
}

for (const name of browsers) {
  console.log(JSON.stringify({ browser: name }));
  let browser;
  try {
    browser = await engines[name].launch();
  } catch (err) {
    check(false, `${name}: launch`, { error: String(err).split("\n")[0] });
    continue;
  }
  const context = await browser.newContext();
  const foreign = new Set();
  context.on("request", (r) => {
    const u = new URL(r.url());
    if (u.origin !== origin && u.origin !== plain.origin) foreign.add(u.origin);
  });
  const consoleErrors = [];
  const open = async (query, timeout = 900000, at = origin) => {
    const page = await context.newPage();
    page.on("console", (m) => {
      if (m.type() === "error") consoleErrors.push(m.text());
    });
    page.on("pageerror", (e) => consoleErrors.push(String(e)));
    try {
      await page.goto(`${at}/kit.html?${query}`);
      await page.waitForSelector("#out[data-done='1']", { timeout });
      return JSON.parse(await page.textContent("#out"));
    } finally {
      await page.close();
    }
  };
  const t = (ms) => ({ ms: Math.round(ms) });

  // Every shipped scenario: the recorded chain, record count and end; the admitted hashes.
  let first = null;
  for (const s of scenarios) {
    const rec = digests.runs[s];
    let r;
    try {
      r = await open(`scenario=${s}&seed=${rec.seed}`);
    } catch (err) {
      check(false, `${name}: ${s}`, { error: String(err).split("\n")[0] });
      continue;
    }
    first = first || r;
    const same = r.chain === rec.chain && r.records === rec.records && r.ended === rec.ended;
    check(same, `${name}: ${s} gives the recorded digest`, { chain: r.chain, records: r.records, ended: r.ended, ...t(r.ms), error: r.error });
    for (const [mod, hash] of Object.entries(r.admitted || {})) {
      check(hash === digests.admitted[mod], `${name}: ${s}: admitted hash of ${mod}`, { hash });
    }
    const fault = faultOf(r);
    const expected = { "hostile-loop": "fault.budget", "hostile-recurse": "fault.stack", "hostile-grow": "fault.trap", "hostile-flood": "fault.quota", "hostile-bad-pointer": "fault.contract" }[s];
    if (expected) {
      check(fault && fault.fault === expected && fault.actor === 1 && fault.metered > 0, `${name}: ${s} faults with ${expected}`, fault || {});
    } else if (s !== "zero-delay") {
      check(!fault, `${name}: ${s} has no fault`, fault || {});
    }
  }
  if (first) {
    check(first.isolated === true, `${name}: the page is cross-origin isolated`, { user_agent: first.user_agent });
    check(first.csp_violations === 0, `${name}: no policy violation`);
    check(first.support && first.support.host_build === "browser", `${name}: the Host says it is the browser build`, first.support);
  }

  // Replay under the browser Host (RFC 005 section 11, replay; AC-003's browser half): the
  // record exported by the worker replays to a match in the same worker.
  try {
    const s = "echo-pair";
    const r = await open(`scenario=${s}&seed=${digests.runs[s].seed}&replay=1`);
    check(r.replay && r.replay.outcome === "replay.matched" && r.replay.chain === digests.runs[s].chain, `${name}: the record replays to a match in the browser`, r.replay || {});
  } catch (err) {
    check(false, `${name}: replay`, { error: String(err).split("\n")[0] });
  }

  // Instance failure (RFC 005 rule 10): more actors than the renderer holds arrive as
  // `host.instance-failed`, the run ends, the page lives.
  if (name !== "webkit") {
    try {
      const r = await open("overflow=1500", 600000);
      // The run never starts: the failure is reported with the actor and the engine's text, and
      // the Host is ready for another run (the engine came back).
      const failed = (r.events || []).find((x) => x.code === "host.instance-failed");
      check(Boolean(failed) && r.state === "ready", `${name}: an instance failure is reported as host.instance-failed and the Host lives`, { state: r.state, code: failed ? failed.code : null, text: failed ? String(failed.params.text.value).slice(0, 60) : null, error: r.error });
    } catch (err) {
      check(false, `${name}: instance failure`, { error: String(err).split("\n")[0] });
    }
  }

  // The display stream (rule 14; C18). One scenario in slices of 50 ticks, read once per frame
  // from the ring on the isolated origin and from messages on the plain one: the same digest
  // as recorded, every record written received, the two paths' records identical, the log's
  // one line naming the transport, and no drop.
  const displayOf = (r) => ({ ...(r.display || {}), produced: r.written && r.written.produced, produced_digest: r.written && r.written.produced_digest, batches: r.written && r.written.batches });
  const transportOf = (r) => ((r.events || []).find((x) => x.code === "telemetry.transport") || { params: { transport: {} } }).params.transport.value;
  const droppedOf = (r) => (r.events || []).filter((x) => x.code === "telemetry.dropped").map((x) => ({ records: x.params.records.value, path: x.params.path.value }));
  const paths = [["ring", origin, "sw=1"], ["messages", plain.origin, "sw=0"]];
  const received = {};
  for (const [path, at, sw] of paths) {
    const s = "dv-triangle";
    try {
      const r = await open(`scenario=${s}&seed=${digests_of(s).seed}&display=1&slice=50&period=4&${sw}`, 600000, at);
      const d = displayOf(r);
      received[path] = d.received_digest;
      check(r.chain === digests_of(s).chain && r.ended === digests_of(s).ended, `${name}: ${s} with the display read from the ${path} gives the recorded digest`, { chain: r.chain, ended: r.ended, ...t(r.ms), error: r.error });
      check(d.path === path && transportOf(r) === (path === "ring" ? "shared" : "messages"), `${name}: the ${path} is in use and the log says so`, { path: d.path, transport: transportOf(r), isolated: r.isolated });
      check(d.received_digest === d.produced_digest && d.records === d.produced && d.records > 0, `${name}: every record written to the ${path} was read`, { records: d.records, produced: d.produced, batches: d.batches, instants: d.instants, flights: d.flights, activity: d.activity, unknown: d.unknown });
      check(d.dropped === 0 && droppedOf(r).length === 0, `${name}: nothing dropped on the ${path}`, { dropped: d.dropped, reported: droppedOf(r) });
      console.log(JSON.stringify({ figure: "display", browser: name, path, records: d.records, frames: d.frames, interval_p95_ms: d.interval_p95_ms, read_p95_ms: d.read_p95_ms, read_max_ms: d.read_max_ms }));
    } catch (err) {
      check(false, `${name}: display over the ${path}`, { error: String(err).split("\n")[0] });
    }
  }
  check(received.ring && received.ring === received.messages, `${name}: the ring and the messages deliver identical records`, received);
  // The transport's cell: each batch padded to 2,000 records at the display's cadence for the
  // run's length, on both paths; every record read, the read cost a figure. Then the same
  // with the page not reading for two seconds (batches every 8 ms, so that the queue's bound
  // of 64 is passed even where the worker's timers run slow, as WebKit's do): the transport
  // drops, counts, and the run's log has `telemetry.dropped` with the path and the count.
  for (const [path, at, sw] of paths) {
    const s = "dv-triangle";
    try {
      const r = await open(`scenario=${s}&seed=${digests_of(s).seed}&display=1&slice=20&period=16&pad=2000&${sw}`, 600000, at);
      const d = displayOf(r);
      check(d.received_digest === d.produced_digest && d.records === d.produced && d.records >= 2000 * 100 && d.dropped === 0, `${name}: 2,000 records per frame over the ${path}, none dropped`, { records: d.records, produced: d.produced, batches: d.batches, frames: d.frames, dropped: d.dropped });
      check(d.read_p95_ms !== null && d.read_p95_ms <= 2, `${name}: reading the ${path} costs at most 2 ms per frame at the 95th percentile`, { read_p95_ms: d.read_p95_ms, read_max_ms: d.read_max_ms });
      console.log(JSON.stringify({ figure: "display-cell", browser: name, path, records_per_batch: 2000, records: d.records, frames: d.frames, interval_p95_ms: d.interval_p95_ms, interval_max_ms: d.interval_max_ms, read_p95_ms: d.read_p95_ms, read_max_ms: d.read_max_ms }));
      const stalled = await open(`scenario=${s}&seed=${digests_of(s).seed}&display=1&slice=20&period=8&pad=2000&stall=2000&${sw}`, 600000, at);
      const sd = displayOf(stalled);
      const reported = droppedOf(stalled).reduce((n, x) => n + x.records, 0);
      check(sd.dropped > 0 && reported === sd.dropped && droppedOf(stalled).every((x) => x.path === path), `${name}: a stalled page's drops on the ${path} are counted and in the log`, { dropped: sd.dropped, reported: droppedOf(stalled) });
      check(sd.records + sd.dropped === sd.produced, `${name}: on the ${path} every record was either read or counted as dropped`, { records: sd.records, dropped: sd.dropped, produced: sd.produced });
    } catch (err) {
      check(false, `${name}: the display cell over the ${path}`, { error: String(err).split("\n")[0] });
    }
  }

  // The transport's form refuses bytes that are no message with its code.
  try {
    const h = await open("bench=handle");
    const reply = (h.replies || [])[0] || {};
    check(reply.code === "control.invalid", `${name}: the transport refuses bytes that are no message`, { code: reply.code });
  } catch (err) {
    check(false, `${name}: transport`, { error: String(err).split("\n")[0] });
  }

  // The benchmarks (RFC 005 rule 11; RFC 003 section 7, step 2).
  try {
    const c = await open(`bench=crossing&scale=${quick ? 20 : 1}`);
    check(c.status === 0 && Array.isArray(c.figures), `${name}: the crossing benchmark runs`, { error: c.error });
    for (const f of c.figures || []) console.log(JSON.stringify({ figure: "crossing", browser: name, engine: c.engine, ...f }));
    // Rule 6: no object per crossing. Two instances and the growths of their memories account
    // for every view the worker made over 100,000 lowerings.
    check(c.views_created <= 8, `${name}: no allocation per lowering`, { views_created: c.views_created });
  } catch (err) {
    check(false, `${name}: crossing benchmark`, { error: String(err).split("\n")[0] });
  }
  try {
    // Both cost-model versions (RFC 010 rule 9): the figures the owner decides on.
    for (const version of [1, 2]) {
      const c = await open(`bench=cost&activations=${quick ? 200 : 10000}&cost-model=${version}`);
      for (const f of c.figures || []) {
        check(f.status === 0 && f.cost_model === version, `${name}: the cost benchmark runs for ${f.guest} under cost-model ${version}`);
        console.log(JSON.stringify({ figure: "cost", browser: name, engine: c.engine, ...f, ratio: f.metered_micros / f.unmetered_micros }));
      }
    }
  } catch (err) {
    check(false, `${name}: cost benchmark`, { error: String(err).split("\n")[0] });
  }

  // RFC 010 rule 1 (checklist C24): under cost-model version 2 a scenario without a trap gives
  // its recorded digest (a budget fault is metered the whole budget under both versions; a
  // stack fault or a trap is metered without the local's remainder, so those scenarios differ),
  // and its modules admit to the recorded version-2 hashes.
  for (const s of ["echo-pair", "hostile-loop", "ospf-lab", "net-lan"].filter((x) => digests.runs[x])) {
    const rec = digests.runs[s];
    try {
      const r = await open(`scenario=${s}&seed=${rec.seed}&cost-model=2`);
      check(r.chain === rec.chain && r.records === rec.records, `${name}: ${s} under cost-model 2 gives the recorded digest`, { chain: r.chain, records: r.records });
      for (const [mod, hash] of Object.entries(r.admitted || {})) {
        check(hash === digests["admitted-2"][mod], `${name}: ${s}: admitted hash of ${mod} under cost-model 2`, { hash });
      }
    } catch (err) {
      check(false, `${name}: ${s} under cost-model 2`, { error: String(err).split("\n")[0] });
    }
  }

  // RFC 009 rule 6 (checklist C23): a checkpoint crosses the Hosts both ways. The native
  // runner writes one at the lab's half and the page resumes it to the recorded digest; the
  // page writes one and the runner resumes it. Skipped with a notice where the runner is not
  // built (ci/build-shell.sh builds the worker only; CI builds the runner beside it).
  const runner = join(root, "target/release/florn");
  const checkpoints = join(root, "crates/florn-ui-web/www/checkpoints");
  if (existsSync(runner)) {
    const run = promisify(execFile);
    await mkdir(checkpoints, { recursive: true });
    for (const s of ["ospf-lab", "net-lan"].filter((x) => digests.runs[x])) {
      const rec = digests.runs[s];
      // The scenario's end, from its manifest: the checkpoint is taken at its half.
      const end = parseInt((/^end = "(\d+)/m.exec(await readFile(join(root, `scenarios/${s}.toml`), "utf8")) || [])[1] || "0", 10);
      const at = Math.floor(end / 2);
      try {
        // Native to browser.
        const file = `${s}-${name}-native.florn-checkpoint`;
        await run(runner, ["run", join(root, `scenarios/${s}.toml`), "--modules", join(root, "scenarios/modules"), "--seed", String(rec.seed), "--checkpoint-at", String(at), "--checkpoint-out", join(checkpoints, file), "--out", join(checkpoints, `${s}-${name}.florn-run`)]);
        const r = await open(`scenario=${s}&seed=${rec.seed}&resume=checkpoints/${file}`);
        check(r.chain === rec.chain && r.records === rec.records && at > 0, `${name}: a checkpoint of ${s} taken natively resumes in the browser to the recorded digest`, { chain: r.chain, records: r.records, at, expected: rec.chain });
        // Browser to native.
        const b = await open(`scenario=${s}&seed=${rec.seed}&checkpoint-at=${at}`);
        check(b.checkpoint && b.checkpoint.bytes > 0 && b.chain === rec.chain, `${name}: the page writes a checkpoint of ${s} and runs on to the recorded digest`, { bytes: b.checkpoint && b.checkpoint.bytes, chain: b.chain });
        const back = `${s}-${name}-browser.florn-checkpoint`;
        await writeFile(join(checkpoints, back), Buffer.from(b.checkpoint.base64, "base64"));
        const { stdout } = await run(runner, ["resume", join(checkpoints, back), join(root, `scenarios/${s}.toml`), "--modules", join(root, "scenarios/modules"), "--out", join(checkpoints, `${s}-${name}-resumed.florn-run`)]);
        const chain = (/^chain = (\S+)$/m.exec(stdout) || [])[1];
        check(chain === rec.chain, `${name}: a checkpoint of ${s} taken in the browser resumes natively to the recorded digest`, { chain, expected: rec.chain });
      } catch (err) {
        check(false, `${name}: checkpoint across the Hosts (${s})`, { error: String(err).split("\n")[0] });
      }
    }
    await rm(checkpoints, { recursive: true, force: true });
  } else {
    console.log(JSON.stringify({ notice: `${name}: the runner is not built; the cross-Host checkpoint check is skipped` }));
  }

  // Rules 32 and 35: a start from the service worker's cache, offline, with the headers. The
  // last scenario's files were fetched under the service worker's control, so they are cached.
  // The check runs before the capacity probe, whose thousands of reserved memories put WebKit
  // under pressure, and it is attempted twice: WebKit on a loaded runner has served a cache miss
  // offline right after the page saw the files in the cache; a second attempt that still misses
  // is a failure, a first miss is reported as a notice.
  const s = scenarios[scenarios.length - 1];
  const warmCache = async () => {
    const warm = await context.newPage();
    try {
      await warm.goto(`${origin}/kit.html`);
      await warm.evaluate(async () => {
        await navigator.serviceWorker.ready;
      });
      if (!(await warm.evaluate(() => navigator.serviceWorker.controller !== null))) await warm.reload();
      const need = await warm.evaluate(async (s) => {
        const need = ["/florn_host.wasm", `/scenarios/${s}.toml`];
        const text = await (await fetch(`scenarios/${s}.toml`)).text();
        for (const m of text.matchAll(/^\[modules\.([A-Za-z0-9_-]+)\]/gm)) need.push(`/scenarios/modules/${m[1]}.wasm`);
        for (const f of need) await fetch(f.slice(1));
        return need;
      }, s);
      await warm.waitForFunction(async (need) => {
        if (!navigator.serviceWorker.controller) return false;
        const have = new Set();
        for (const n of await caches.keys()) {
          for (const k of await (await caches.open(n)).keys()) have.add(new URL(k.url).pathname);
        }
        return ["/kit.html", "/main.js", "/page.css", "/worker.js", "/display.js", ...need].every((f) => have.has(f));
      }, need, { timeout: 60000, polling: 250 });
    } finally {
      await warm.close();
    }
  };
  const offlineStart = async () => {
    await warmCache();
    await context.setOffline(true);
    try {
      return await open(`scenario=${s}&seed=${digests.runs[s].seed}`, 120000);
    } finally {
      await context.setOffline(false);
    }
  };
  try {
    let r;
    try {
      r = await offlineStart();
    } catch (err) {
      console.log(JSON.stringify({ notice: `${name}: offline start, second attempt`, error: String(err).split("\n")[0] }));
      consoleErrors.length = 0;
      r = await offlineStart();
    }
    check(r.chain === digests.runs[s].chain, `${name}: the page starts offline from the cache`, { chain: r.chain, error: r.error });
    check(r.isolated === true, `${name}: the cached responses carry the headers`);
  } catch (err) {
    check(false, `${name}: offline start`, { error: String(err).split("\n")[0] });
  }

  // The hash figures (RFC 005 rule 22) and the capacity probe (RFC 003 rule 20).
  try {
    const h = await open(`bench=hash&scale=${quick ? 20 : 1}`);
    check(h.status === 0 && Array.isArray(h.figures), `${name}: the hash benchmark runs`, { error: h.error });
    for (const f of h.figures || []) console.log(JSON.stringify({ figure: "hash", browser: name, engine: h.engine, ...f }));
    const p = await open("bench=probe&bound=5000");
    console.log(JSON.stringify({ figure: "probe", browser: name, engine: p.engine, instances: p.instances, stopped_by: p.stopped_by, micros_per_instance: p.micros_per_instance, memory_pages: p.memory_pages }));
    check(p.instances >= 100, `${name}: the renderer holds at least 100 instances`, { instances: p.instances });
  } catch (err) {
    check(false, `${name}: hash or probe`, { error: String(err).split("\n")[0] });
  }

  check(foreign.size === 0, `${name}: the network log holds only the page's origins`, { foreign: [...foreign] });
  check(consoleErrors.length === 0, `${name}: no error in the console`, { errors: consoleErrors.slice(0, 5) });
  await browser.close();
}
server.close();
plain.server.close();
console.log(JSON.stringify({ failures }));
process.exit(failures ? 1 : 0);
