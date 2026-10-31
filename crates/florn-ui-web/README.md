# florn-ui-web

The browser front end: the Host's worker module, the application (the shell of RFC 005 section 8, since 0.2.0) and the kit's page.

## What is here

| Path | What |
|---|---|
| `src/worker.rs` | The worker module's exports, C functions over the Host (`cargo doc` or the file lists their contracts): the Host's life (`florn_start`, `florn_alloc`, `florn_support`, `florn_run_state`), loading and admission (`florn_load_scenario`, `florn_load_module`, `florn_admit`, `florn_admit_kit`), the run (`florn_new_run`, `florn_advance`, `florn_pump`, `florn_pump_bytes`, `florn_set_cost_model`, `florn_set_resident_limit`), the control interface (`florn_handle`, `florn_handle_bytes`: one encoded message in; `florn_out_ptr`, `florn_out_len`), events and the diagnostic log (`florn_subscribe_events`, `florn_drain`, `florn_drain_bytes`), the display stream (`florn_subscribe_display`, `florn_take_display`, `florn_display_ptr`, `florn_display_dropped`), checkpoints and branching (`florn_checkpoint`, `florn_load_checkpoint`, `florn_branch`, `florn_export_parent`, `florn_export_record`, `florn_replay`), the benchmarks (`florn_bench_crossing`, `florn_bench_cost`, `florn_bench_hash`) |
| `www/worker.js` | The worker's script (a module): instantiates the Host module, holds the guest instances, provides the imports of `florn-engine-web` and `florn-platform-web`, writes each Host tick's display batch to the transport |
| `www/display.js` | The display transport of RFC 005 rule 14: one reader interface over the ring in shared memory (where the page is cross-origin isolated) and over one transferred message per Host tick; the page reads it once per animation frame |
| `www/main.js`, `www/kit.html`, `www/page.css` | The kit's page: runs one shipped scenario or one benchmark as the query says and writes the result as JSON |
| `src/main.rs`, `src/shell/` | The shell (RFC 005 rule 38; feature `shell`): Leptos on the main thread over `florn-ui-model` and `florn-ui-scene`; `mod.rs` the bridge to the page and the frame loop, `panels.rs` the panels, `catalogue.rs` the codes' sentences, `draw.rs` the canvas back end; built by `ci/build-shell.sh` into `www/florn_shell.js` and `www/florn_shell_bg.wasm` |
| `locales/*.json`, `catalogue/*.json` | The catalogues: the interface's labels (checked at compile time) and the codes' sentences (checked by `tests/catalogue.rs`), English and Japanese |
| `www/app.js`, `www/index.html`, `www/shell.css`, `www/labs.txt`, `www/palette.txt`, `www/manifest.webmanifest`, `www/icon-192.png`, `www/icon-512.png` | The application: the loader (with the origin storage of RFC 009 section 6), the page, the style, the bundled labs, the palette's modules, the web application manifest and its two icons (installable since 0.3.0) |
| `www/sw.js` | The service worker: the page starts offline after one visit, with the headers of RFC 005 rule 33 on every cached response |
| `build.rs` | Exports the module's function table and lets it grow (RFC 005 rule 7) |

The module uses no `wasm-bindgen`: its surface is a few C functions, and the guests are reached without a JavaScript value in between (see `florn-engine-web`). The shell uses `wasm-bindgen` on the main thread and reaches the Host through `florn_handle` only.

## Running the page

```sh
cargo install --locked wasm-bindgen-cli --version 0.2.126   # once
ci/build-shell.sh
cd ci/browser && npm ci && node serve.js
```

The application is `http://127.0.0.1:8765/`; the kit's page is `kit.html`:

Open `http://127.0.0.1:8765/kit.html?scenario=echo-pair&seed=42`, or `?bench=crossing`, `?bench=cost`, `?bench=hash`; the 0.3 parameters `&cost-model=`, `&residency=swap&resident=N`, `&checkpoint-at=TICK`, `&resume=<file>`, `&branch-at=TICK` and `&replay=<file>` run the checkpoint, swapping, branching and replay checks the kit's `run.js` drives; `?scenario=dv-triangle&seed=1&display=1` runs in slices and reads the display stream once per frame, reporting what was read beside what was written (`&slice=`, `&period=`, `&pad=`, `&stall=` set the kit's transport cell). The server sets the isolation headers and the policy of RFC 005 rule 33; `scenarios/` is served under `/scenarios`.

The conformance kit's browser run, as CI does it (`npx playwright install` first):

```sh
cd ci/browser && node run.js --browsers chromium,firefox,webkit
```

It runs every shipped scenario, compares the digests and the admitted hashes with `scenarios/digests.toml`, checks the hostile scenarios' faults, the isolation, the policy, the network log, the start from the cache offline, the transport's refusal, the display stream over the ring and over messages (a second origin without the isolation headers), and prints the crossing, cost and display figures.
