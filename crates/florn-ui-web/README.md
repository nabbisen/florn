# florn-ui-web

The browser front end. In release 0.1 it is the Host's worker module and a test page; the shell of RFC 005 section 8 follows in 0.2.

## What is here

| Path | What |
|---|---|
| `src/worker.rs` | The worker module's exports: `florn_start`, `florn_load_scenario`, `florn_load_module`, `florn_admit`, `florn_admit_kit`, `florn_new_run`, `florn_advance`, `florn_pump`, `florn_subscribe_events`, `florn_run_state`, `florn_support`, `florn_handle` (one encoded control message in), `florn_subscribe_display`, `florn_take_display` (the display batch, through `florn_display_ptr`), `florn_display_dropped`, `florn_drain` (the log's lines since the last reply), `florn_bench_crossing`, `florn_bench_cost`; replies as JSON through `florn_out_ptr` and `florn_out_len` |
| `www/worker.js` | The worker's script (a module): instantiates the Host module, holds the guest instances, provides the imports of `florn-engine-web` and `florn-platform-web`, writes each Host tick's display batch to the transport |
| `www/display.js` | The display transport of RFC 005 rule 14: one reader interface over the ring in shared memory (where the page is cross-origin isolated) and over one transferred message per Host tick; the page reads it once per animation frame |
| `www/main.js`, `www/kit.html`, `www/page.css` | The kit's page: runs one shipped scenario or one benchmark as the query says and writes the result as JSON |
| `src/main.rs`, `src/shell/` | The shell (RFC 005 rule 38; feature `shell`): Leptos on the main thread over `florn-ui-model` and `florn-ui-scene`; `mod.rs` the bridge to the page and the frame loop, `panels.rs` the panels, `catalogue.rs` the codes' sentences, `draw.rs` the canvas back end; built by `ci/build-shell.sh` into `www/florn_shell.js` and `www/florn_shell_bg.wasm` |
| `locales/*.json`, `catalogue/*.json` | The catalogues: the interface's labels (checked at compile time) and the codes' sentences (checked by `tests/catalogue.rs`), English and Japanese |
| `www/app.js`, `www/index.html`, `www/shell.css`, `www/labs.txt`, `www/palette.txt` | The application: the loader, the page, the style, the bundled labs and the palette's modules |
| `www/sw.js` | The service worker: the page starts offline after one visit, with the headers of RFC 005 rule 33 on every cached response |
| `build.rs` | Exports the module's function table and lets it grow (RFC 005 rule 7) |

The module uses no `wasm-bindgen`: its surface is a few C functions, and the guests are reached without a JavaScript value in between (see `florn-engine-web`). The shell of 0.2 will use `wasm-bindgen` on the main thread.

## Running the page

```sh
cargo build --release --target wasm32-unknown-unknown -p florn-ui-web
cp target/wasm32-unknown-unknown/release/florn_ui_web.wasm crates/florn-ui-web/www/florn_host.wasm
cd ci/browser && npm ci && node serve.js
```

Then open `http://127.0.0.1:8765/index.html?scenario=echo-pair&seed=42`, or `?bench=crossing`, `?bench=cost`; `?scenario=dv-triangle&seed=1&display=1` runs in slices and reads the display stream once per frame, reporting what was read beside what was written (`&slice=`, `&period=`, `&pad=`, `&stall=` set the kit's transport cell). The server sets the isolation headers and the policy of RFC 005 rule 33; `scenarios/` is served under `/scenarios`.

The conformance kit's browser run, as CI does it (`npx playwright install` first):

```sh
cd ci/browser && node run.js --browsers chromium,firefox,webkit
```

It runs every shipped scenario, compares the digests and the admitted hashes with `scenarios/digests.toml`, checks the hostile scenarios' faults, the isolation, the policy, the network log, the start from the cache offline, the transport's refusal, the display stream over the ring and over messages (a second origin without the isolation headers), and prints the crossing, cost and display figures.
