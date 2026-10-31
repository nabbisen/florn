// The kit's page opener, with its one retry. A page that takes longer than a navigation should
// (WebKit on the shared macOS runner has failed documentation-only commits this way) or that
// crashes is opened again, once, in a fresh browser context; the retry is counted and printed,
// so that a flake is told from a regression and its rate is visible in the log. A page that
// loads and then does not finish its work is not retried: that is a failing check.
//
// The module holds no browser: `firstContext` and `newContext` are whatever has a `newPage`
// and a `close`, so that `test-open.mjs` exercises the rule with fakes.

/// Whether an error is the kind a fresh context may cure: the navigation timed out, or the page
/// crashed (`crashed` is set from the page's own `crash` event).
export function isTransient(err, crashed = false) {
  if (crashed) return true;
  const message = String((err && err.message) || err);
  if (err && err.name === "TimeoutError" && /page\.goto/.test(message)) return true;
  return /Page crashed|Target crashed/.test(message);
}

export function makeOpener({ firstContext, newContext, origin, attachPage, attachContext, log = (o) => console.log(JSON.stringify(o)) }) {
  const stats = { opens: 0, retries: 0 };

  async function once(context, query, timeout, at, navTimeout) {
    const page = await context.newPage();
    let crashed = false;
    page.on("crash", () => {
      crashed = true;
    });
    if (attachPage) attachPage(page);
    try {
      // Navigation under a loaded runner gets `navTimeout`; the work has `timeout`.
      await page.goto(`${at}/kit.html?${query}`, { timeout: navTimeout });
      await page.waitForSelector("#out[data-done='1']", { timeout });
      return JSON.parse(await page.textContent("#out"));
    } catch (err) {
      err.pageCrashed = crashed;
      throw err;
    } finally {
      await page.close().catch(() => {});
    }
  }

  /// Open the kit's page with `query`. `retry: false` is for a check whose point is the
  /// context it runs in (the offline start needs the context that holds the cache).
  async function open(query, timeout = 900000, at = origin, { retry = true, navTimeout = 120000 } = {}) {
    stats.opens += 1;
    try {
      return await once(firstContext, query, timeout, at, navTimeout);
    } catch (err) {
      if (!retry || !isTransient(err, err.pageCrashed)) throw err;
      stats.retries += 1;
      log({ retry: stats.retries, query, reason: err.pageCrashed ? "page crashed" : "navigation timed out", error: String(err.message || err).split("\n")[0] });
      const fresh = await newContext();
      if (attachContext) attachContext(fresh);
      try {
        return await once(fresh, query, timeout, at, navTimeout);
      } finally {
        await fresh.close().catch(() => {});
      }
    }
  }

  return { open, stats };
}
