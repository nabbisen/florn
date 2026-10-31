// The rule of open.js, with fake pages and no browser: a navigation timeout or a crash is
// retried once in a fresh context and counted; a second failure, a failure of the work and a
// call that asked for no retry are not retried. Usage: node test-open.mjs
import { makeOpener, isTransient } from "./open.js";

let failures = 0;
const check = (ok, what) => {
  if (!ok) failures += 1;
  console.log(JSON.stringify({ check: what, ok: Boolean(ok) }));
};

class Timeout extends Error {
  constructor(m) {
    super(m);
    this.name = "TimeoutError";
  }
}

// A context whose pages behave as `script` says, one entry per page opened in it.
function context(script, opened) {
  let n = 0;
  return {
    closed: false,
    async newPage() {
      const step = script[n++] || "ok";
      opened.push(step);
      const handlers = {};
      return {
        on: (e, f) => (handlers[e] = f),
        async goto() {
          if (step === "goto-timeout") throw new Timeout("page.goto: Timeout 120000ms exceeded.");
          if (step === "crash") {
            handlers.crash && handlers.crash();
            throw new Error("page.goto: Page crashed");
          }
        },
        async waitForSelector() {
          if (step === "work-timeout") throw new Timeout("page.waitForSelector: Timeout 900000ms exceeded.");
        },
        async textContent() {
          return JSON.stringify({ chain: "abc" });
        },
        async close() {},
      };
    },
    async close() {
      this.closed = true;
    },
  };
}

async function scenario(first, second) {
  const opened = [];
  const logs = [];
  const fresh = [];
  const firstContext = context(first, opened);
  const { open, stats } = makeOpener({
    firstContext,
    newContext: async () => {
      const c = context(second, opened);
      fresh.push(c);
      return c;
    },
    origin: "http://x",
    log: (o) => logs.push(o),
  });
  return { open, stats, opened, logs, fresh };
}

{
  const s = await scenario(["goto-timeout"], ["ok"]);
  const r = await s.open("q").catch(() => ({}));
  check(r.chain === "abc" && s.stats.retries === 1 && s.opened.length === 2, "a navigation timeout is retried once and succeeds");
  check(s.logs.length === 1 && s.logs[0] && s.logs[0].reason === "navigation timed out", "the retry is printed with its reason");
  check(s.fresh.length === 1 && s.fresh[0].closed, "the retry runs in a fresh context that is closed after");
}
{
  const s = await scenario(["crash"], ["ok"]);
  await s.open("q").catch(() => {});
  check(s.stats.retries === 1 && s.logs[0] && s.logs[0].reason === "page crashed", "a crashed page is retried once");
}
{
  const s = await scenario(["goto-timeout"], ["goto-timeout"]);
  let thrown = null;
  await s.open("q").catch((e) => (thrown = e));
  check(thrown && s.stats.retries === 1 && s.opened.length === 2, "a second failure is thrown, not retried again");
}
{
  const s = await scenario(["work-timeout"], ["ok"]);
  let thrown = null;
  await s.open("q").catch((e) => (thrown = e));
  check(thrown && s.stats.retries === 0 && s.opened.length === 1, "a page that loads and does not finish is not retried");
}
{
  const s = await scenario(["goto-timeout"], ["ok"]);
  let thrown = null;
  await s.open("q", 1000, "http://x", { retry: false }).catch((e) => (thrown = e));
  check(thrown && s.stats.retries === 0 && s.opened.length === 1, "retry: false is honoured");
}
check(!isTransient(new Error("some other failure")), "an unrelated error is not transient");

console.log(JSON.stringify({ failures }));
process.exit(failures ? 1 : 0);
