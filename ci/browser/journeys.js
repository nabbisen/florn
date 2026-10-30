// The journeys of release 0.2 through the application (RFC 005 rules 38 to 41; the handoff's
// checklist C20; AC-008 to AC-011), driven as a person would drive them, in each browser:
//   J1 open a bundled lab, run it, watch the topology, the timeline and an inspector;
//   J3 take a link down during the run, see it listed with its instant, step, inspect a
//      frame, export the run record, replay it to a match;
//   J4 save the scenario, reload it, replay the saved run with its intervention: the same digest;
//   J2 author the triangle from an empty scenario with validation at the field, run it: the
//      digest the runner recorded for the shipped scenario.
// Release 0.3 (RFC 009 section 7; the handoff's checklist C28):
//   J5 import a module from a file, read both hashes, place it beside bundled routers, connect,
//      run, replay; the file is kept in this browser and exports byte for byte;
//   P1 go back past an intervention: the application says what it will discard, lists the
//      intervention as undone, re-applies it on request;
//   P2 branch from an instant: the parent named, its record kept, both records replaying natively;
//   P3 export a checkpoint, open the application again, load it, run to the lab's digest;
//   P4 a scenario beyond the browser's capacity: refused resident, run swapping, the status
//      line saying so, the digest the runner gives;
//   the installable manifest and its icons from the origin; the address after every journey.
// Then the rules of rule 41: switch the language mid-run and keep the typed text; reach the
// controls by keyboard with a bound on the presses between landmarks; start offline; and the
// measured frame budget (rule 40) and the network log of one origin (rule 34).
//
// Usage: node journeys.js --browsers chromium,firefox[,webkit] [--port N]
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { readFile, mkdtemp, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { chromium, firefox, webkit } from "playwright";
import { startServer } from "./serve.js";

const arg = (k, d) => {
  const i = process.argv.indexOf(k);
  return i >= 0 ? process.argv[i + 1] : d;
};
const browsers = arg("--browsers", "chromium").split(",");
const engines = { chromium, firefox, webkit };
const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function parseDigests(text) {
  const d = { runs: {} };
  let table = null;
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    let m;
    if ((m = /^\[runs\."([^"]+)"\]$/.exec(line))) table = d.runs[m[1]] = {};
    else if (/^\[/.test(line)) table = null;
    else if ((m = /^([\w-]+) = (.*)$/.exec(line)) && table) table[m[1]] = m[2].startsWith('"') ? m[2].slice(1, -1) : Number(m[2]);
  }
  return d;
}
const digests = parseDigests(await readFile(join(root, "scenarios/digests.toml"), "utf8"));
const triangle = digests.runs["dv-triangle"];
// The learning lab of NET-010: four OSPF routers in a ring with a chord (RFC 008 rule 21).
const LAB = "ospf-lab";
const lab = digests.runs[LAB];

const { server, origin } = await startServer({ www: join(root, "crates/florn-ui-web/www"), scenarios: join(root, "scenarios"), port: parseInt(arg("--port", "0"), 10) });
let failures = 0;
function check(ok, what, detail) {
  if (!ok) failures += 1;
  console.log(JSON.stringify({ check: what, ok: Boolean(ok), ...(detail || {}) }));
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const state = (page) => page.evaluate(() => window.florn && window.florn.state());
async function until(page, pred, timeout = 60000, what = "condition") {
  const t0 = Date.now();
  for (;;) {
    const s = await state(page);
    if (s && pred(s)) return s;
    if (Date.now() - t0 > timeout) {
      const brief = s && { state: s.state, instant: s.instant, records: s.records, chain: s.chain, replay: s.replay, applied: s.applied, notices: s.notices, codes: (s.codes || []).slice(-8) };
      throw new Error(`${what}: not within ${timeout} ms: ${JSON.stringify(brief)}`);
    }
    await sleep(100);
  }
}
// WebKit tabs through form fields only unless the person presses Option with Tab (the
// browser's own convention); the journeys press what a person would.
let tabKey = "Tab";
const violationsOf = (page) => page.evaluate(() => (window.florn && window.florn.violations) || []);
// WebKit refuses its own menu-list styling of a `<select>` under `style-src 'self'` and
// logs it as a stylesheet refusal, once per select, from `createElement` itself, with no
// sample: nothing of the application's. A diagnostic run recorded no style attribute, no
// CSSOM write and no markup injection. Those refusals are counted, not failed.
const ownRefusal = (text) => /Refused to apply a stylesheet because its hash, its nonce, or 'unsafe-inline'/.test(text);
const consoleCheck = async (page, errors, what) => {
  const ua = errors.filter(ownRefusal).length;
  const rest = errors.filter((e) => !ownRefusal(e));
  check(rest.length === 0, what, { errors: rest.slice(0, 5), browser_own_style_refusals: ua, violations: (await violationsOf(page)).slice(0, 3) });
};
async function openApp(context, query = "") {
  const page = await context.newPage({ viewport: { width: 1400, height: 900 } });
  const errors = [];
  page.on("console", (m) => {
    if (m.type() === "error") {
      const l = m.location() || {};
      errors.push(`${m.text()} (${l.url || ""}:${l.lineNumber || 0})`);
    }
  });
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.goto(`${origin}/index.html${query}`);
  await page.waitForFunction(() => window.florn && window.florn.state().admitted >= 2, null, { timeout: 60000 });
  return { page, errors };
}
async function openLab(page, name, nodes = 4) {
  await page.click(`button.lab[data-lab="${name}"]`);
  await until(page, (s) => s.state === "ready" && s.nodes === nodes, 30000, `lab ${name} ready`);
}
async function runToEnd(page, speed = "0") {
  await page.selectOption("#speed", speed);
  await page.click("#run");
  await until(page, (s) => s.state === "ended", 120000, "run ended");
}
// Run at the lab's pace to an instant and pause there.
async function runTo(page, tick, speed = "1000") {
  await page.selectOption("#speed", speed);
  await page.click("#run");
  await until(page, (x) => x.instant >= tick || x.state === "ended", 120000, `at ${tick}`);
  await page.click("#pause");
  await until(page, (x) => x.state === "paused" || x.state === "ended", 10000, "paused");
}
// Select a connection by clicking its middle on the canvas (between its two actors).
async function selectLink(page, a, b) {
  const box = await page.locator("#topology").boundingBox();
  const pos = (await state(page)).positions;
  const [x1, y1] = pos[String(a)], [x2, y2] = pos[String(b)];
  await page.mouse.click(box.x + (x1 + x2) / 2, box.y + (y1 + y2) / 2);
  await sleep(400);
  return /Connection|接続/.test(await page.textContent("#inspector-kind"));
}
// RFC 009 rule 22 (SEC-007): the address holds the origin, the path and at most a lab's name.
function addressClean(url) {
  const u = new URL(url);
  const keys = [...u.searchParams.keys()].filter((k) => k !== "lab");
  return u.origin === origin && keys.length === 0 && u.hash === "";
}
const runner = join(root, "target/release/florn");
const run = promisify(execFile);
const chainOf = (stdout) => (/^chain = (\S+)$/m.exec(stdout) || [])[1];

for (const name of browsers) {
  console.log(JSON.stringify({ browser: name }));
  let browser;
  try {
    browser = await engines[name].launch();
  } catch (err) {
    check(false, `${name}: launch`, { error: String(err).split("\n")[0] });
    continue;
  }
  const context = await browser.newContext({ acceptDownloads: true });
  tabKey = name === "webkit" ? "Alt+Tab" : "Tab";
  const foreign = new Set();
  context.on("request", (r) => {
    const u = new URL(r.url());
    if (u.origin !== origin) foreign.add(u.origin);
  });
  const dir = await mkdtemp(join(tmpdir(), "florn-journeys-"));

  // J1 (AC-008): the lab runs; neighbours form and routes converge in the views.
  let recordPath = null;
  let scenarioPath = null;
  let j1chain = null;
  try {
    const { page, errors } = await openApp(context);
    await openLab(page, LAB);
    let s = await state(page);
    const storyText = await page.textContent("#lab-story");
    check(s.nodes === 4 && s.links === 5 && s.timeline >= 2 && storyText.length > 200, `${name}: J1 the lab opens with its topology, its scheduled changes and its story`, { nodes: s.nodes, links: s.links, timeline: s.timeline, story: storyText.length });
    // Slowly first (a frame's flight is a few ticks: at 100 ticks per second it crosses a
    // batch boundary and is drawn in flight), then at the lab's pace.
    await page.selectOption("#speed", "100");
    await page.click("#run");
    s = await until(page, (x) => x.state === "running" && x.instant > 10, 30000, "running");
    check(s.state === "running", `${name}: J1 the run runs at the chosen speed`, { instant: s.instant });
    // A flight of two ticks lasts 20 ms at this speed: the shell counts every flight it was
    // shown, so the check does not depend on catching one between two looks.
    s = await until(page, (x) => x.flights_seen > 0 || x.state === "ended", 20000, "a flight");
    check(s.flights_seen > 0, `${name}: J1 frames move on the topology while it runs`, { flights_seen: s.flights_seen });
    await page.selectOption("#speed", "1000");
    s = await until(page, (x) => x.routes >= 4 && x.neighbours >= 2, 60000, "routes in the timeline");
    check(s.routes >= 4 && s.neighbours >= 2, `${name}: J1 neighbours form and routes converge in the timeline`, { routes: s.routes, neighbours: s.neighbours });
    await page.click('button.actor[data-actor="1"]');
    await sleep(600);
    const reports = await page.textContent('dd[data-fact="reports"]');
    check(reports.includes("route") && reports.includes("prefix="), `${name}: J1 the inspector shows the router's routes`, { reports: reports.slice(0, 120) });
    const label = await page.getAttribute("#topology", "aria-label");
    check(label.includes("4") && label.includes("5") && (await page.textContent("#topology-description")).length > 20, `${name}: J1 the canvas has its description and the actors list is its twin`, { label });
    s = await until(page, (x) => x.state === "ended", 180000, "ended");
    j1chain = s.chain;
    check(s.chain === lab.chain, `${name}: J1 the lab's run has the recorded digest`, { chain: s.chain, expected: lab.chain });
    check(addressClean(page.url()), `${name}: J1 the address holds nothing but the origin and the path (RFC 009 rule 22)`, { url: page.url() });
    await consoleCheck(page, errors, `${name}: J1 no error of the application in the console`);
    await page.close();
  } catch (err) {
    check(false, `${name}: J1`, { error: String(err).split("\n")[0] });
  }

  // J3 (AC-009): a link taken down during the run; stepping; a frame inspected; export; replay.
  try {
    const { page, errors } = await openApp(context);
    await openLab(page, LAB);
    await page.selectOption("#speed", "1000");
    await page.click("#run");
    await until(page, (x) => x.instant >= 600, 30000, "at 600");
    await page.click("#pause");
    await until(page, (x) => x.state === "paused", 10000, "paused");
    // Select the link r2-r3 from the timeline's scheduled change target? Select by the canvas's twin: the connection inspector through hit testing is the canvas's; the keyboard path is the actors list. The link is selected by clicking its middle on the canvas.
    // The link r2-r3 is where a person sees it: between the two nodes on the canvas.
    const box = await page.locator("#topology").boundingBox();
    const pos = (await state(page)).positions;
    const [x2, y2] = pos["2"], [x3, y3] = pos["3"];
    await page.mouse.click(box.x + (x2 + x3) / 2, box.y + (y2 + y3) / 2);
    await sleep(400);
    let selected = null;
    const kind = await page.textContent("#inspector-kind");
    if (/Connection|接続/.test(kind)) selected = await page.textContent('dd[data-fact="name"]');
    check(selected !== null, `${name}: J3 a link is selected on the canvas`, { selected });
    await page.click("#link-down");
    await page.click("#step");
    let s = await until(page, (x) => x.applied >= 1, 20000, "intervention applied");
    check(s.interventions === 1 && s.applied === 1 && s.links_down === 1, `${name}: J3 the intervention is listed with its instant and the link shows down`, { interventions: s.interventions, applied: s.applied, links_down: s.links_down });
    let applied = "";
    for (let i = 0; i < 30 && !/applied at tick|ティック .* に適用/.test(applied); i++) {
      applied = await page.textContent("#interventions");
      await sleep(100);
    }
    check(/applied at tick|ティック .* に適用/.test(applied), `${name}: J3 the list says where it applied`, { text: applied.slice(0, 80) });
    // Step through events: three steps advance the handled count.
    const before = (await state(page)).handled;
    for (let i = 0; i < 3; i++) await page.click("#step");
    s = await until(page, (x) => x.handled >= before + 3, 10000, "stepped");
    check(s.handled >= before + 3, `${name}: J3 stepping advances one event at a time`, { before, after: s.handled });
    // Inspect a frame: an activation of the frame class with a send, from the timeline.
    await page.click("#run");
    await until(page, (x) => x.instant >= 1200, 60000, "at 1200");
    await page.click("#pause");
    await until(page, (x) => x.state === "paused", 10000, "paused");
    await page.click('input.kind-filter[data-kind="activation"]');
    for (const k of ["fault", "intervention", "change", "event", "report", "notice"]) {
      const checked = await page.isChecked(`input.kind-filter[data-kind="${k}"]`);
      if (checked) await page.click(`input.kind-filter[data-kind="${k}"]`);
    }
    await page.click('input.kind-filter[data-kind="activation"]');
    await sleep(400);
    const rows = page.locator("#timeline li.row");
    const count = await rows.count();
    let decoded = "";
    for (let i = count - 1; i >= 0 && i > count - 40; i--) {
      await rows.nth(i).locator("button").first().click();
      await sleep(300);
      const kind = await page.textContent("#inspector-kind");
      if (/Message|メッセージ/.test(kind)) {
        for (let w = 0; w < 20; w++) {
          decoded = (await page.textContent('dd[data-fact="decoded"]')) || "";
          if (decoded.includes("sender=")) break;
          await sleep(100);
        }
        if (decoded.includes("sender=")) break;
      }
    }
    check(decoded.includes("sender="), `${name}: J3 a frame's bytes decode in the inspector`, { decoded: decoded.slice(0, 80) });
    // Export the run record, previewed first, then replay it.
    const [download] = await Promise.all([
      page.waitForEvent("download", { timeout: 20000 }),
      (async () => {
        await page.click("#export-record");
        await page.waitForSelector("#export-preview", { timeout: 10000 });
        await page.click("#export-confirm");
      })(),
    ]);
    recordPath = join(dir, `${name}-record.florn-run`);
    await download.saveAs(recordPath);
    check((await readFile(recordPath)).length > 100, `${name}: J3 the run record is exported after a preview`, { bytes: (await readFile(recordPath)).length });
    const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.click("#replay-record")]);
    await chooser.setFiles(recordPath);
    s = await until(page, (x) => x.state === "paused" && x.interventions === 0, 20000, "replay started");
    await page.click("#run");
    // The verdict arrives with the end; the chain the state shows settles a message later.
    s = await until(page, (x) => x.state === "ended" && x.replay, 120000, "replay ended");
    await sleep(500);
    s = await state(page);
    check(s.replay === "replay.matched" && s.applied === 1, `${name}: J3 the replay matches at every stored point and applies the intervention`, { replay: s.replay, applied: s.applied });
    // J4 (AC-010): save the scenario; reload it; replay the saved run: the same digest.
    const [save] = await Promise.all([page.waitForEvent("download", { timeout: 20000 }), page.click("#save-scenario")]);
    scenarioPath = join(dir, `${name}-scenario.toml`);
    await save.saveAs(scenarioPath);
    const j3chain = s.chain;
    const [chooser2] = await Promise.all([page.waitForEvent("filechooser"), page.click("#open-scenario")]);
    await chooser2.setFiles(scenarioPath);
    await until(page, (x) => x.state === "ready" && x.nodes === 4, 20000, "scenario reloaded");
    const [chooser3] = await Promise.all([page.waitForEvent("filechooser"), page.click("#replay-record")]);
    await chooser3.setFiles(recordPath);
    await until(page, (x) => x.state === "paused", 20000, "replay of the saved run started");
    await page.click("#run");
    s = await until(page, (x) => x.state === "ended" && x.replay, 120000, "replay ended");
    await sleep(500);
    s = await state(page);
    check(s.replay === "replay.matched" && s.chain === j3chain, `${name}: J4 the saved scenario reloads and replays the saved run with its intervention to the same digest`, { chain: s.chain, replay: s.replay, expected: j3chain });
    check(addressClean(page.url()), `${name}: J3/J4 the address holds nothing but the origin and the path (RFC 009 rule 22)`, { url: page.url() });
    await consoleCheck(page, errors, `${name}: J3/J4 no error of the application in the console`);
    await page.close();
  } catch (err) {
    check(false, `${name}: J3/J4`, { error: String(err).split("\n")[0] });
  }

  // J2 (AC-011): the triangle authored from an empty scenario, validation at the field, run.
  try {
    const { page, errors } = await openApp(context);
    await page.click("#authoring-title");
    await page.click("#new-draft");
    // RFC 006 C15: text typed into the scenario is rendered as text, never as markup or script.
    const markup = "<b>bold</b><script>alert(1)</script>";
    await page.fill("#draft-name", markup);
    await page.press("#draft-name", "Tab");
    await page.fill("#draft-end", "3000ns");
    await page.press("#draft-end", "Tab");
    await page.fill("#place-ports", "2");
    await page.click('button.place[data-module="distance-vector"]');
    await page.click("#apply-draft");
    await sleep(400);
    const shownName = await page.textContent("#scenario-name");
    const markupNodes = await page.locator("#scenario-name b, #scenario-name script, header b, header script").count();
    const violationsBefore = (await state(page)).violations;
    check(shownName === markup && markupNodes === 0, `${name}: J2 a scenario named with markup renders as text (RFC 006 C15)`, { shown: shownName, nodes: markupNodes, violations: violationsBefore });
    await page.click("#new-draft");
    await page.fill("#draft-name", "triangle");
    await page.press("#draft-name", "Tab");
    await page.fill("#draft-end", "3000");
    await page.press("#draft-end", "Tab");
    await sleep(300);
    let problems = await page.textContent("#draft-problems");
    const fieldError = await page.locator("#draft-end ~ ul.messages li").count();
    check(fieldError >= 1 && /^[1-9]/.test(problems.trim()), `${name}: J2 a bare number for the end is refused at the field`, { problems: problems.slice(0, 60) });
    await page.fill("#draft-end", "3000ns");
    await page.press("#draft-end", "Tab");
    await page.fill("#place-ports", "2");
    for (let i = 0; i < 3; i++) await page.click('button.place[data-module="distance-vector"]');
    await sleep(200);
    for (const id of [1, 2, 3]) {
      // The options appear as the placed actors reach the draft; WebKit has selected before
      // they did and waited for nothing.
      await page.waitForSelector(`#attr-target option[value="${id}"]`, { state: "attached" });
      await page.selectOption("#attr-target", String(id));
      await page.fill("#attr-key", "period");
      await page.fill("#attr-value", "100ns");
      await page.click("#attr-set");
    }
    const pairs = [["1.0", "2.0"], ["2.1", "3.0"], ["3.1", "1.1"]];
    for (const [a, b] of pairs) {
      await page.waitForSelector(`#connect-from option[value="${a}"]`, { state: "attached" });
      await page.waitForSelector(`#connect-to option[value="${b}"]`, { state: "attached" });
      await page.selectOption("#connect-from", a);
      await page.selectOption("#connect-to", b);
      await page.click("#connect");
    }
    await sleep(200);
    for (const id of [1000, 1001, 1002]) {
      await page.selectOption("#attr-target", String(id));
      await page.fill("#attr-key", "delay");
      await page.fill("#attr-value", "2ns");
      await page.click("#attr-set");
    }
    await page.selectOption("#attr-target", "1000");
    await page.fill("#attr-key", "loss");
    await page.fill("#attr-value", "0.0");
    await page.click("#attr-set");
    // A wrong dimension is refused by the Host, at the field.
    await page.selectOption("#attr-target", "1001");
    await page.fill("#attr-key", "delay");
    await page.fill("#attr-value", "2kg");
    await page.click("#attr-set");
    await page.click("#apply-draft");
    let s = await until(page, (x) => x.draft_errors > 0, 20000, "the Host refused");
    let atField = 0;
    for (let i = 0; i < 30 && atField === 0; i++) {
      atField = await page.locator('li.draft-connection[data-connection="1001"] ul.messages li').count();
      if (atField === 0) await sleep(100);
    }
    check(s.state === "invalid" && atField >= 1, `${name}: J2 the Host's refusal is shown at the field and the scenario cannot run`, { state: s.state, errors: s.draft_errors });
    await page.selectOption("#attr-target", "1001");
    await page.fill("#attr-key", "delay");
    await page.fill("#attr-value", "2ns");
    await page.click("#attr-set");
    for (const [at, value] of [["1000ns", "false"], ["2000ns", "true"]]) {
      await page.fill("#change-at", at);
      await page.selectOption("#change-target", "1000");
      await page.fill("#change-key", "enabled");
      await page.fill("#change-value", value);
      await page.click("#schedule");
    }
    await page.click("#apply-draft");
    s = await until(page, (x) => x.state === "ready" && x.nodes === 3, 20000, "the draft loaded");
    await runToEnd(page);
    s = await state(page);
    check(s.chain === triangle.chain, `${name}: J2 the authored scenario runs to the digest the runner recorded`, { chain: s.chain, expected: triangle.chain });
    check(addressClean(page.url()), `${name}: J2 the address holds nothing but the origin and the path (RFC 009 rule 22)`, { url: page.url() });
    await consoleCheck(page, errors, `${name}: J2 no error of the application in the console`);
    await page.close();
  } catch (err) {
    check(false, `${name}: J2`, { error: String(err).split("\n")[0] });
  }

  // J5 (RFC 009 rule 17): a module imported from a file is admitted with both hashes shown,
  // kept in this browser, placed from the palette beside two routers, connected, run to the
  // triangle's recorded digest, replayed; its export is byte for byte the imported file.
  try {
    const { page, errors } = await openApp(context);
    const modulePath = join(root, "scenarios/modules/distance_vector.wasm");
    const moduleBytes = await readFile(modulePath);
    const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.click("#import-module")]);
    await chooser.setFiles(modulePath);
    let s = await until(page, (x) => x.files.some((f) => f.kind === "module"), 20000, "the module kept");
    const hashes = await page.textContent('#palette li[data-module="distance-vector"] .hash');
    const hex = hashes.match(/[0-9a-f]{64}/g) || [];
    check(hex.length === 2 && hex[0] !== hex[1], `${name}: J5 the imported module shows its source and admitted hashes before it can be placed`, { hashes: hashes.slice(0, 160) });
    const kept = s.files.find((f) => f.kind === "module");
    check(kept && kept.name === "distance_vector.wasm" && kept.bytes === moduleBytes.length && s.storage_bytes >= kept.bytes, `${name}: J5 the module is kept in this browser under Files with its name and size`, { kept, storage_bytes: s.storage_bytes });
    const note = await page.textContent("#storage-note");
    check(/kept in this browser|このブラウザー/.test(note), `${name}: J5 the Files list says the cache's size and that the file is the durable form`, { note });
    // The triangle from the palette: the imported module beside two bundled routers.
    await page.click("#authoring-title");
    await page.click("#new-draft");
    await page.fill("#draft-name", "triangle-j5");
    await page.press("#draft-name", "Tab");
    await page.fill("#draft-end", "3000ns");
    await page.press("#draft-end", "Tab");
    await page.fill("#place-ports", "2");
    for (let i = 0; i < 3; i++) await page.click('button.place[data-module="distance-vector"]');
    await sleep(200);
    for (const id of [1, 2, 3]) {
      await page.waitForSelector(`#attr-target option[value="${id}"]`, { state: "attached" });
      await page.selectOption("#attr-target", String(id));
      await page.fill("#attr-key", "period");
      await page.fill("#attr-value", "100ns");
      await page.click("#attr-set");
    }
    for (const [a, b] of [["1.0", "2.0"], ["2.1", "3.0"], ["3.1", "1.1"]]) {
      await page.waitForSelector(`#connect-from option[value="${a}"]`, { state: "attached" });
      await page.selectOption("#connect-from", a);
      await page.selectOption("#connect-to", b);
      await page.click("#connect");
    }
    await sleep(200);
    for (const id of [1000, 1001, 1002]) {
      await page.selectOption("#attr-target", String(id));
      await page.fill("#attr-key", "delay");
      await page.fill("#attr-value", "2ns");
      await page.click("#attr-set");
    }
    await page.selectOption("#attr-target", "1000");
    await page.fill("#attr-key", "loss");
    await page.fill("#attr-value", "0.0");
    await page.click("#attr-set");
    for (const [at, value] of [["1000ns", "false"], ["2000ns", "true"]]) {
      await page.fill("#change-at", at);
      await page.selectOption("#change-target", "1000");
      await page.fill("#change-key", "enabled");
      await page.fill("#change-value", value);
      await page.click("#schedule");
    }
    await page.click("#apply-draft");
    await until(page, (x) => x.state === "ready" && x.nodes === 3, 20000, "the draft loaded");
    await runToEnd(page);
    s = await state(page);
    check(s.chain === triangle.chain, `${name}: J5 the scenario with the imported module runs to the recorded digest`, { chain: s.chain, expected: triangle.chain });
    const [download] = await Promise.all([
      page.waitForEvent("download", { timeout: 20000 }),
      (async () => {
        await page.click("#export-record");
        await page.waitForSelector("#export-preview", { timeout: 10000 });
        await page.click("#export-confirm");
      })(),
    ]);
    const j5record = join(dir, `${name}-j5.florn-run`);
    await download.saveAs(j5record);
    const [chooser2] = await Promise.all([page.waitForEvent("filechooser"), page.click("#replay-record")]);
    await chooser2.setFiles(j5record);
    await until(page, (x) => x.state === "paused", 20000, "replay started");
    await page.click("#run");
    s = await until(page, (x) => x.state === "ended" && x.replay, 120000, "replay ended");
    await sleep(500);
    s = await state(page);
    check(s.replay === "replay.matched", `${name}: J5 the replay matches`, { replay: s.replay });
    // The kept module exports byte for byte; the record exported is kept too.
    const [back] = await Promise.all([page.waitForEvent("download", { timeout: 20000 }), page.click('#files li[data-kind="module"] .export-stored')]);
    const exported = await readFile(await back.path());
    check(Buffer.compare(exported, moduleBytes) === 0, `${name}: J5 the exported module's bytes equal the imported file's`, { bytes: exported.length });
    s = await state(page);
    check(s.files.some((f) => f.kind === "run-record"), `${name}: J5 the exported run record is kept under Files too`, { files: s.files.map((f) => f.kind) });
    check(addressClean(page.url()), `${name}: J5 the address holds nothing but the origin and the path (RFC 009 rule 22)`, { url: page.url() });
    await consoleCheck(page, errors, `${name}: J5 no error of the application in the console`);
    await page.close();
  } catch (err) {
    check(false, `${name}: J5`, { error: String(err).split("\n")[0] });
  }

  // P1 (RFC 009 rules 19 and 8): going back past an intervention says what it discards, lists
  // the intervention as undone, and re-applies it on request.
  try {
    const { page, errors } = await openApp(context, `?lab=${LAB}`);
    await until(page, (s) => s.state === "ready" && s.nodes === 4, 30000, "lab ready");
    await runTo(page, 2000);
    check(await selectLink(page, 3, 4), `${name}: P1 the link r3-r4 is selected`);
    await page.click("#link-down");
    await page.click("#step");
    await until(page, (x) => x.applied >= 1, 20000, "intervention applied");
    await runTo(page, 2500);
    let s = await state(page);
    const rowsBefore = s.timeline;
    await page.fill("#goto-tick", "1200");
    await page.click("#goto");
    await page.waitForSelector("#go-back", { timeout: 10000 });
    const dialog = await page.textContent("#go-back");
    s = await state(page);
    check(s.go_back === 1200 && /1,200|1200/.test(dialog) && /link/.test(dialog), `${name}: P1 before going back the application says what it will discard`, { text: dialog.slice(0, 160) });
    await page.click("#go-back-confirm");
    s = await until(page, (x) => x.state === "paused" && x.now !== null && x.now <= 1200 && x.undone === 1, 60000, "went back");
    const undoneText = await page.textContent("#undone");
    check(s.interventions === 0 && s.undone === 1 && s.timeline < rowsBefore && /2,[0-9]{3}/.test(undoneText), `${name}: P1 the timeline ends at the instant and the intervention is listed as undone with its instant`, { now: s.now, timeline: s.timeline, rows_before: rowsBefore, undone: undoneText.slice(0, 120) });
    check(s.links_down === 0, `${name}: P1 the link is up again at the earlier instant`, { links_down: s.links_down });
    // "Step to here" on an early row goes back the same way.
    const firstTick = await page.locator("#timeline li.row .tick").first().textContent();
    const target = parseInt(firstTick.replace(/,/g, ""), 10) + 1;
    await page.locator("#timeline li.row .step-to").first().click();
    s = await until(page, (x) => x.state === "paused" && x.now !== null && x.now <= target, 60000, "stepped back");
    check(s.now <= target && s.undone === 1, `${name}: P1 "Step to here" moves back to the row's instant`, { now: s.now, row_tick: firstTick });
    // Nothing re-applied: the lab's digest.
    await runToEnd(page);
    s = await state(page);
    check(s.chain === lab.chain && s.undone === 1, `${name}: P1 the run continued to the end gives the lab's digest when nothing is re-applied`, { chain: s.chain, expected: lab.chain });
    // Back again and re-applied: a different digest, the intervention applied.
    await page.fill("#goto-tick", "1000");
    await page.click("#goto");
    await until(page, (x) => x.state === "paused" && x.now !== null && x.now <= 1000, 60000, "went back again");
    await page.click("#undone .reapply");
    await until(page, (x) => x.undone === 0 && x.interventions === 1, 10000, "re-applied");
    await runToEnd(page);
    s = await state(page);
    check(s.chain !== lab.chain && s.applied === 1, `${name}: P1 the re-applied intervention applies at its instant and changes the digest`, { chain: s.chain, applied: s.applied });
    check(addressClean(page.url()), `${name}: P1 the address holds the origin, the path and the lab's name only`, { url: page.url() });
    await consoleCheck(page, errors, `${name}: P1 no error of the application in the console`);
    await page.close();
  } catch (err) {
    check(false, `${name}: P1`, { error: String(err).split("\n")[0] });
  }

  // P2 (RFC 009 rules 10 to 12 and 19): a branch from an instant; the parent named and its
  // record kept; both records replay natively on their own.
  try {
    const { page, errors } = await openApp(context);
    await openLab(page, LAB);
    await runTo(page, 1200);
    let s = await state(page);
    const parentId = s.run_id;
    await page.click("#select-world");
    await sleep(300);
    await page.click("#branch");
    s = await until(page, (x) => x.parent && x.run_id !== parentId && x.files.some((f) => f.kind === "run-record"), 30000, "branched");
    // The panels re-render a few times a second (rule 39): the fact follows the state.
    await sleep(600);
    const parentFact = await page.textContent('dd[data-fact="parent"]');
    check(s.parent.run === parentId && parentFact.includes(parentId), `${name}: P2 the world inspector names the parent and the instant`, { parent: s.parent, fact: parentFact });
    const branchedRow = await page.locator('#timeline li.row[data-kind] .code', { hasText: "run.branched" }).count();
    check(branchedRow >= 1 && s.checkpoints.length === 1, `${name}: P2 the timeline marks the branch and the branch begins at its instant`, { rows: branchedRow, checkpoints: s.checkpoints });
    await runTo(page, 1300);
    check(await selectLink(page, 1, 3), `${name}: P2 the chord r1-r3 is selected`);
    await page.click("#link-down");
    await runToEnd(page);
    s = await state(page);
    check(s.chain !== lab.chain && s.applied === 1, `${name}: P2 the branch with the chord down ends with another digest`, { chain: s.chain });
    if (existsSync(runner)) {
      const [branchDl] = await Promise.all([
        page.waitForEvent("download", { timeout: 20000 }),
        (async () => {
          await page.click("#export-record");
          await page.waitForSelector("#export-preview", { timeout: 10000 });
          await page.click("#export-confirm");
        })(),
      ]);
      const branchPath = join(dir, `${name}-branch.florn-run`);
      await branchDl.saveAs(branchPath);
      const parentKey = s.files.find((f) => f.kind === "run-record" && f.name.startsWith(parentId)).key;
      const [parentDl] = await Promise.all([page.waitForEvent("download", { timeout: 20000 }), page.click(`#files li[data-key="${parentKey}"] .export-stored`)]);
      const parentPath = join(dir, `${name}-parent.florn-run`);
      await parentDl.saveAs(parentPath);
      for (const [which, file] of [["parent", parentPath], ["branch", branchPath]]) {
        const { stdout } = await run(runner, ["replay", file, join(root, `scenarios/${LAB}.toml`), "--modules", join(root, "scenarios/modules")]);
        check(/^replay = matched$/m.test(stdout), `${name}: P2 the ${which}'s record replays natively to a match on its own`, { tail: stdout.trim().split("\n").slice(-2) });
      }
    } else {
      console.log(JSON.stringify({ notice: `${name}: the runner is not built; P2's native replays are skipped` }));
    }
    check(addressClean(page.url()), `${name}: P2 the address holds nothing but the origin and the path`, { url: page.url() });
    await consoleCheck(page, errors, `${name}: P2 no error of the application in the console`);
    await page.close();
  } catch (err) {
    check(false, `${name}: P2`, { error: String(err).split("\n")[0] });
  }

  // P3 (RFC 009 rules 5 and 6): a checkpoint exported after a preview, the application opened
  // again, the checkpoint loaded, the run to the lab's digest; another scenario's is refused.
  try {
    const { page, errors } = await openApp(context);
    await openLab(page, LAB);
    await runTo(page, 2000);
    await page.click("#select-world");
    await sleep(300);
    await page.click("#take-checkpoint");
    let s = await until(page, (x) => x.checkpoints.length >= 2, 10000, "a checkpoint taken");
    const last = s.checkpoints.length - 1;
    const cpInstant = s.checkpoints[last];
    const [download] = await Promise.all([
      page.waitForEvent("download", { timeout: 20000 }),
      (async () => {
        await page.click(`#checkpoints li[data-index="${last}"] .export-checkpoint`);
        await page.waitForSelector("#export-preview", { timeout: 10000 });
        const preview = await page.textContent("#export-preview");
        check(/checkpoint|チェックポイント/.test(preview), `${name}: P3 the preview says what the file holds`, { preview: preview.slice(0, 120) });
        await page.click("#export-confirm");
      })(),
    ]);
    const cpPath = join(dir, `${name}-2000.florn-checkpoint`);
    await download.saveAs(cpPath);
    await page.close();
    const again = await openApp(context);
    await openLab(again.page, LAB);
    const [chooser] = await Promise.all([again.page.waitForEvent("filechooser"), again.page.click("#load-checkpoint")]);
    await chooser.setFiles(cpPath);
    s = await until(again.page, (x) => x.state === "paused" && x.now !== null && x.now >= 1500, 30000, "the checkpoint loaded");
    check(s.now === cpInstant && s.files.some((f) => f.kind === "checkpoint"), `${name}: P3 the checkpoint loads at its instant and is kept under Files`, { now: s.now, instant: cpInstant });
    await runToEnd(again.page);
    s = await state(again.page);
    check(s.chain === lab.chain, `${name}: P3 the resumed run ends with the lab's digest`, { chain: s.chain, expected: lab.chain });
    // Another scenario: refused with the field named.
    await openLab(again.page, "dv-triangle", 3);
    const [chooser2] = await Promise.all([again.page.waitForEvent("filechooser"), again.page.click("#load-checkpoint")]);
    await chooser2.setFiles(cpPath);
    await until(again.page, (x) => (x.codes || []).includes("checkpoint.mismatch"), 20000, "the mismatch said");
    await again.page.waitForSelector('li.notice[data-code="checkpoint.mismatch"]', { timeout: 10000 });
    const refusal = await again.page.textContent('li.notice[data-code="checkpoint.mismatch"]');
    check(/scenario/.test(refusal) || /シナリオ/.test(refusal), `${name}: P3 a checkpoint of another scenario is refused with the field named`, { refusal: refusal.slice(0, 120) });
    check(addressClean(again.page.url()), `${name}: P3 the address holds nothing but the origin and the path`, { url: again.page.url() });
    await consoleCheck(again.page, again.errors, `${name}: P3 no error of the application in the console`);
    await again.page.close();
  } catch (err) {
    check(false, `${name}: P3`, { error: String(err).split("\n")[0] });
  }

  // P4 (RFC 009 rules 14 and 21): a scenario of more actors than this browser's capacity is
  // refused resident with the numbers and runs swapping, the status line saying so, to the
  // digest the native runner gives.
  try {
    const { page, errors } = await openApp(context);
    const count = name === "chromium" ? 130 : 1000;
    const echoPath = join(root, "scenarios/modules/echo.wasm");
    const echo = await readFile(echoPath);
    const hash = (await import("node:crypto")).createHash("sha256").update(echo).digest("hex");
    let text = `florn = 1\nname = "many"\ndomains = ["core"]\n\n[time]\ntick = "1ns"\nend = "10ns"\n\n[order]\ntie = "by-identity"\n\n[seed]\nroot = 1\n\n[limits.default]\nmemory = "2MiB"\n\n[modules.echo]\nsha256 = "${hash}"\ncontract = "florn:core/guest@0.1.0"\n`;
    for (let id = 1; id <= count; id++) text += `\n[[actors]]\nid = ${id}\nname = "a${id}"\nkind = "module"\nmodule = "echo"\nports = []\n`;
    const manyPath = join(dir, `${name}-many.toml`);
    await writeFile(manyPath, text);
    const [c1] = await Promise.all([page.waitForEvent("filechooser"), page.click("#import-module")]);
    await c1.setFiles(echoPath);
    await until(page, (x) => x.files.some((f) => f.kind === "module"), 20000, "echo imported");
    const [c2] = await Promise.all([page.waitForEvent("filechooser"), page.click("#open-scenario")]);
    await c2.setFiles(manyPath);
    let s = await until(page, (x) => x.state === "ready" && x.nodes === count, 60000, "the scenario ready");
    await page.selectOption("#speed", "0");
    await page.click("#run");
    await page.waitForSelector('li.notice[data-code="host.capacity-exceeded"]', { timeout: 30000 });
    const refusal = await page.textContent('li.notice[data-code="host.capacity-exceeded"]');
    s = await state(page);
    check(s.state === "ready" && /\d/.test(refusal), `${name}: P4 resident, the scenario is refused with the need and the capacity`, { refusal: refusal.slice(0, 120), state: s.state });
    await page.selectOption("#residency", "swap");
    await page.click("#run");
    s = await until(page, (x) => x.state === "ended", 300000, "the swapping run ended");
    await sleep(600);
    const statusSwapping = await page.textContent("#status-swapping");
    check(s.swapping && s.swapping[1] === count && s.swapping[0] < count && /swapping|スワップ/.test(statusSwapping), `${name}: P4 the run swaps and the status line says so with the counts`, { swapping: s.swapping, status: statusSwapping });
    if (existsSync(runner)) {
      const { stdout } = await run(runner, ["run", manyPath, "--module", echoPath, "--seed", "1"]);
      const native = chainOf(stdout);
      check(native && s.chain === native, `${name}: P4 the swapping run's digest equals the native runner's`, { chain: s.chain, native });
    }
    check(addressClean(page.url()), `${name}: P4 the address holds nothing but the origin and the path`, { url: page.url() });
    await consoleCheck(page, errors, `${name}: P4 no error of the application in the console`);
    await page.close();
  } catch (err) {
    check(false, `${name}: P4`, { error: String(err).split("\n")[0] });
  }

  // RFC 011 rule 8 (checklist C29): the BGP lab opens from the list with its story, its peer
  // and route reports decode in the inspector, a frame decodes, and the run ends with the
  // recorded digest.
  try {
    const { page, errors } = await openApp(context);
    const bgp = digests.runs["bgp-lab"];
    await openLab(page, "bgp-lab");
    const story = await page.textContent("#lab-story");
    check(/BGP/.test(story) && story.length > 200, `${name}: the BGP lab opens with its story`, { story: story.length });
    await runTo(page, 300);
    await page.click('button.actor[data-actor="1"]');
    await sleep(600);
    const reports = await page.textContent('dd[data-fact="reports"]');
    check(/peer/.test(reports) && /established/.test(reports) && /route/.test(reports) && /10\.4\.0\.0\/16/.test(reports) && /local-pref=200/.test(reports), `${name}: the inspector shows r1's peer and route reports decoded`, { reports: reports.slice(0, 200) });
    let s = await state(page);
    check(s.routes >= 1 && s.timeline > 10, `${name}: the BGP lab's reports reach the timeline`, { routes: s.routes, timeline: s.timeline });
    // A frame of the lab decodes in the message inspector.
    await page.click('input.kind-filter[data-kind="activation"]');
    for (const k of ["fault", "intervention", "change", "event", "report", "notice"]) {
      if (await page.isChecked(`input.kind-filter[data-kind="${k}"]`)) await page.click(`input.kind-filter[data-kind="${k}"]`);
    }
    await page.click('input.kind-filter[data-kind="activation"]');
    await sleep(400);
    const rows = page.locator("#timeline li.row");
    const count = await rows.count();
    let decoded = "";
    for (let i = count - 1; i >= 0 && i > count - 40; i--) {
      await rows.nth(i).locator("button").first().click();
      await sleep(300);
      if (/Message|メッセージ/.test(await page.textContent("#inspector-kind"))) {
        for (let w = 0; w < 20; w++) {
          decoded = (await page.textContent('dd[data-fact="decoded"]')) || "";
          if (/type=/.test(decoded)) break;
          await sleep(100);
        }
        if (/type=/.test(decoded)) break;
      }
    }
    check(/type=(keepalive|update|open)/.test(decoded) && /sender-as=/.test(decoded), `${name}: a BGP frame decodes in the inspector`, { decoded: decoded.slice(0, 120) });
    await runToEnd(page);
    s = await state(page);
    check(bgp && s.chain === bgp.chain, `${name}: the BGP lab's run has the recorded digest`, { chain: s.chain, expected: bgp && bgp.chain });
    await consoleCheck(page, errors, `${name}: BGP lab no error of the application in the console`);
    await page.close();
  } catch (err) {
    check(false, `${name}: BGP lab`, { error: String(err).split("\n")[0] });
  }

  // RFC 009 rule 20 (WEB-004): the web application manifest and its icons, from the origin.
  try {
    const { page } = await openApp(context);
    const href = await page.getAttribute('link[rel="manifest"]', "href");
    const manifest = await page.evaluate(async (h) => {
      const r = await fetch(h);
      return { ok: r.ok, type: r.headers.get("content-type"), body: await r.json() };
    }, href);
    const iconsOk = await page.evaluate(async (icons) => {
      const out = [];
      for (const i of icons) {
        const r = await fetch(i.src);
        out.push(r.ok && (r.headers.get("content-type") || "").startsWith("image/png"));
      }
      return out.every(Boolean);
    }, manifest.body.icons);
    const sameOrigin = new URL(manifest.body.start_url, page.url()).origin === origin && manifest.body.icons.every((i) => new URL(i.src, page.url()).origin === origin);
    check(manifest.ok && manifest.body.name === "florn" && manifest.body.display === "standalone" && manifest.body.icons.length >= 2 && iconsOk && sameOrigin, `${name}: the application is installable from its own origin: the manifest and its icons`, { type: manifest.type, icons: manifest.body.icons.length, iconsOk, sameOrigin });
    // Rule 41 on the new panels: every new string has its Japanese.
    await page.click("#lang-ja");
    await sleep(300);
    const filesTitle = await page.textContent("#files-title");
    const residency = await page.textContent('#residency option[value="swap"]');
    const loadCp = await page.textContent("#load-checkpoint");
    check(filesTitle === "このブラウザーに保持" && residency === "容量を超えた分はスワップ" && loadCp === "チェックポイントを読み込む…", `${name}: the new panels are in the chosen language`, { filesTitle, residency, loadCp });
    await page.click("#lang-en");
    await page.close();
  } catch (err) {
    check(false, `${name}: installable manifest`, { error: String(err).split("\n")[0] });
  }

  // Rule 41: the language switches mid-run without a reload and keeps the typed text; every
  // control is reached by keyboard within a bound; the frame budget is measured.
  try {
    const { page, errors } = await openApp(context);
    await openLab(page, "dv-triangle", 3);
    await page.selectOption("#speed", "100");
    await page.click("#run");
    await until(page, (x) => x.state === "running", 10000, "running");
    await page.click('button.actor[data-actor="2"]');
    await sleep(400);
    await page.fill("#actor-period", "250n");
    await page.click("#lang-ja");
    await sleep(300);
    const typed = await page.inputValue("#actor-period");
    const title = await page.textContent("#actors-title");
    const htmlLang = await page.getAttribute("html", "lang");
    check(typed === "250n" && title === "アクター" && htmlLang === "ja", `${name}: the language switches mid-run and the typed text stays`, { typed, title, htmlLang });
    const sState = await page.textContent("#status-state");
    check(sState === "実行中", `${name}: the status line is in the chosen language`, { state: sState });
    await page.click("#lang-en");
    await sleep(200);
    check((await page.textContent("#actors-title")) === "Actors" && (await page.inputValue("#actor-period")) === "250n", `${name}: and back, with the text still there`);
    // RFC 006 C12: the status line shows the version and the origin the application was loaded from.
    const about = await page.textContent("#status-about");
    const version = (about.match(/florn (\d+\.\d+\.\d+)/) || [])[1];
    check(version !== undefined && about.includes(new URL(page.url()).host), `${name}: the status line shows the version and the origin (RFC 006 C12)`, { about, version });
    await page.click("#pause");
    await until(page, (x) => x.state === "paused", 10000, "paused");
    await sleep(500);
    // Keyboard: from the first control of the page, every landmark's first control within the
    // bound (WebKit does not wrap the Tab order around the page, so the start is explicit).
    await page.focus("#run");
    const landmarks = ["#run", "#lang-en", "button.lab", "#actors button", "#authoring-title", "#topology", "#inspector-fields input, #select-world", "#timeline button"];
    const reached = { "#run": 0 };
    let presses = 0;
    for (let i = 0; i < 160 && Object.keys(reached).length < landmarks.length; i++) {
      await page.keyboard.press(tabKey);
      presses += 1;
      for (const l of landmarks) {
        if (reached[l] !== undefined) continue;
        const hit = await page.evaluate((sel) => { const a = document.activeElement; return Boolean(a && [...document.querySelectorAll(sel)].includes(a)); }, l);
        if (hit) reached[l] = presses;
      }
    }
    const max = Math.max(...Object.values(reached), 0);
    // The bound grew by the controls 0.3 added before the timeline: the residency choice, the
    // checkpoint loader, and the Files list (one stop, its row's two buttons).
    check(Object.keys(reached).length === landmarks.length && max <= 48, `${name}: every landmark's control is reached by keyboard within 48 presses`, { reached, presses });
    // Arrow keys move inside the actors list without leaving it.
    await page.focus('button.actor[data-actor="1"]');
    await page.keyboard.press("ArrowDown");
    const active = await page.evaluate(() => document.activeElement && document.activeElement.getAttribute("data-actor"));
    check(active === "2", `${name}: arrow keys move inside the list`, { active });
    // The frame budget (rule 40): the draw time the status line measures while running.
    await page.selectOption("#speed", "100");
    await page.click("#run");
    await sleep(2500);
    const f = await state(page);
    console.log(JSON.stringify({ figure: "frame", browser: name, median_ms: f.frame_median_ms, p95_ms: f.frame_p95_ms, frames: f.frames }));
    check(f.frames > 60 && f.frame_p95_ms <= 16, `${name}: the frame loop runs and drawing stays within the budget`, { p95: f.frame_p95_ms, frames: f.frames });
    await consoleCheck(page, errors, `${name}: rule 41 no error of the application in the console`);
    await page.close();
  } catch (err) {
    check(false, `${name}: rule 41`, { error: String(err).split("\n")[0] });
  }

  // Rule 35: the application starts offline from the service worker's cache, isolated, and
  // says it is available offline.
  try {
    await context.setOffline(true);
    const { page } = await openApp(context);
    await openLab(page, LAB);
    const offline = await page.textContent("#status-offline");
    const s = await state(page);
    check(s.nodes === 4 && s.offline === 1 && /offline/i.test(offline), `${name}: the application starts offline and says so`, { offline, nodes: s.nodes });
    const isolated = await page.evaluate(() => self.crossOriginIsolated === true);
    check(isolated, `${name}: the cached application is cross-origin isolated`);
    await page.close();
  } catch (err) {
    check(false, `${name}: offline start`, { error: String(err).split("\n")[0] });
  } finally {
    await context.setOffline(false);
  }

  check(foreign.size === 0, `${name}: the network log holds the application's origin only`, { foreign: [...foreign] });
  await browser.close();
}
server.close();
console.log(JSON.stringify({ failures }));
process.exit(failures ? 1 : 0);
