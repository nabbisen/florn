// The conformance kit under the browser Host (AC-001, AC-002, AC-007; RFC 005 section 11):
// every shipped scenario through the test page in each browser, compared with the recorded
// digests; the hostile scenarios' faults; the isolation headers, the policy and the network log
// (rules 33 and 34); the start from the service worker's cache (rules 32 and 35); the transport's
// refusal of bytes that are no message; the crossing and cost benchmarks' figures.
//
// Usage: node run.js --browsers chromium,firefox[,webkit] [--quick] [--port N]
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { readFile } from "node:fs/promises";
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

/// The digests file: `[modules]`, `[admitted]`, and one `[runs."name"]` table per scenario.
function parseDigests(text) {
  const d = { modules: {}, admitted: {}, runs: {} };
  let table = null;
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    let m;
    if ((m = /^\[runs\."([^"]+)"\]$/.exec(line))) {
      table = d.runs[m[1]] = {};
    } else if ((m = /^\[(\w+)\]$/.exec(line))) {
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
const { server, origin } = await startServer({
  www: join(root, "crates/florn-ui-web/www"),
  scenarios: join(root, "scenarios"),
  port: parseInt(arg("--port", "0"), 10),
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
    if (u.origin !== origin) foreign.add(u.origin);
  });
  const consoleErrors = [];
  const open = async (query, timeout = 900000) => {
    const page = await context.newPage();
    page.on("console", (m) => {
      if (m.type() === "error") consoleErrors.push(m.text());
    });
    page.on("pageerror", (e) => consoleErrors.push(String(e)));
    try {
      await page.goto(`${origin}/index.html?${query}`);
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
    const c = await open(`bench=cost&activations=${quick ? 200 : 10000}`);
    for (const f of c.figures || []) {
      check(f.status === 0, `${name}: the cost benchmark runs for ${f.guest}`);
      console.log(JSON.stringify({ figure: "cost", browser: name, engine: c.engine, ...f, ratio: f.metered_micros / f.unmetered_micros }));
    }
  } catch (err) {
    check(false, `${name}: cost benchmark`, { error: String(err).split("\n")[0] });
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

  // Rules 32 and 35: a start from the service worker's cache, offline, with the headers. The
  // last scenario's files were fetched under the service worker's control, so they are cached.
  try {
    await context.setOffline(true);
    const s = scenarios[scenarios.length - 1];
    const r = await open(`scenario=${s}&seed=${digests.runs[s].seed}`, 300000);
    check(r.chain === digests.runs[s].chain, `${name}: the page starts offline from the cache`, { chain: r.chain, error: r.error });
    check(r.isolated === true, `${name}: the cached responses carry the headers`);
  } catch (err) {
    check(false, `${name}: offline start`, { error: String(err).split("\n")[0] });
  } finally {
    await context.setOffline(false);
  }

  check(foreign.size === 0, `${name}: the network log holds only the page's origin`, { foreign: [...foreign] });
  check(consoleErrors.length === 0, `${name}: no error in the console`, { errors: consoleErrors.slice(0, 5) });
  await browser.close();
}
server.close();
console.log(JSON.stringify({ failures }));
process.exit(failures ? 1 : 0);
