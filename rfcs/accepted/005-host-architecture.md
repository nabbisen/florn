# RFC 005 — Host architecture: one core, adapters, control interface, repository layout

**Status.** Accepted (2026-10-14). Amended 2026-10-19 by PR-08: rule 26 gains `load-module`, the command that supplies a module a manifest references by hash (a package carries its own). Amended 2026-10-14 with the results of the proof of concept on telemetry transport: rule 14 (record size, ring, queue), rule 32 (worker readiness), rule 33 (hosting requirement), open question 3. Amended 2026-10-14 with the results of the proof of concept on snapshot and restore: rule 10 (compiled-module cache in the browser adapter), rule 17 (swapping cost and policy). Rule 8's choice recorded 2026-10-20 by PR-12: the budget is one Host-owned global shared by every guest instance, reached by the Host through the exports of a six-instruction module that imports the same global, a WebAssembly-to-WebAssembly call; the cost of the two accesses per activation is the metered-minus-unmetered difference of the call floor in the conformance kit's browser figures. The same path serves the calls into a guest (an index of the Host module's exported function table) and out of it (the Host's exports bound as the guest's imports). Open questions 1 and 2 resolved 2026-10-20 by PR-13 (rules 22 and 23); the formats of 0.1 are frozen the same day: the trace records of rule 20, the control messages of rule 25, `run.toml` and the checkpoint of rule 29, each with a version of 1, their encodings recorded by the conformance kit's format tests. Rules 20 and 29 amended 2026-10-20 by PR-14a (the trace's version is the record's; `diagnostic.bin` listed).
**Tracks.** Requirements ARCH-007, ARCH-008, ARCH-009, ACT-007, DES-004, DES-007, DES-011, DES-012, OBS-001, OBS-002, OBS-003, OBS-008, WEB-001, WEB-002, WEB-003, WEB-006, WEB-009, WEB-010, WEB-012, MIG-001 to MIG-005, CTL-001 to CTL-004, SEC-008, DATA-006, DATA-012, UI-007, UI-011, UI-013, AUTO-001, AUTO-002, AUTO-006, PKG-001, PKG-007, NFR-003, NFR-005, NFR-009, NFR-013. Requirement IDs refer to `docs/src/requirements.md`.
**Touches.** The structure of the Host and its two adapters; the engine adapter's rules; the trace encoding and the digest; the control interface and the run record; the browser Host's hosting rules; the three-layer user interface; the repository layout.
**Depends on.** [RFC 001](./001-determinism-contract.md) rules 33 to 55; [RFC 002](./002-guest-contract.md) for the ABI and effects; [RFC 003](./003-module-admission-and-limits.md) for admission, capacity, and state export; [RFC 004](./004-core-model.md) for the manifest, the package, the canonical encoding, and the coded errors.
**Informed by.** Stage 0 proofs of concept 1, 3, 4, and 7; the proof of concept on telemetry transport (5) and on snapshot and restore (6) refine §4 and §6 before Phase 1 ends.

## Summary

One Host core — kernel, admission, domains, control interface — is one Rust codebase compiled for the browser and for native targets. Everything host-specific lives in two adapters: an **engine adapter** that binds the kernel to a WebAssembly engine, and a **platform adapter** that binds it to transport, storage, pacing, and the machine. Every front end — the browser UI, the native UI, the headless runner — is an equal client of one transport-independent **control interface**; a command that changes the logical world is an intervention and is recorded.

This RFC also fixes what the proofs of concept taught about the Host side of the boundary: lower and lift through views, call through typed references, keep the budget counter out of JavaScript, hash a payload once, probe capacity before instantiating, and build the user interface in three layers so that the framework is the smallest replaceable part.

## Background

Two Hosts with two kernels would diverge, and divergence would destroy cross-host reproducibility by construction. The decision to have one core was taken before any code existed; the proofs of concept then showed what the one core must look like to be fast enough and what the adapters must not do.

| Finding | Consequence here |
|---|---|
| One kernel crate ran unchanged under four engine adapters with one digest (POC-1). | §1: the structure holds. The kernel is the point of it. |
| The native runner compiled the module on every start and was slower than the browser for that reason alone. | Rule 9: the native Host caches compiled modules. |
| A JavaScript allocation per lowering cost 8× at 64 KiB and made Firefox's tail 10× its median; a decimal-string read of the budget global cost 0.5–1 µs per activation (POC-3). | Rules 6 to 8: views, typed calls, no JavaScript on the budget path. Both are rules because both are easy to make again. |
| Hashing payloads twice — into the run chain and the actor chain — was 231 µs at 64 KiB in the browser, 3–5× the crossing even at 1,500 bytes (POC-3). | §5: a record carries payload digests computed once. |
| A browser renderer process holds about 125 (Chromium) or 1,000 (Firefox) instances regardless of their size; instantiation failures arrive as a `RangeError` or as a bare string (POC-4). | §4: an instance pool with probed capacity; failures are Host faults with a clear message. |
| Three UI layers held without effort; the framework-specific part is about 300 lines; two defects were found only by tests that behave like a person (POC-7). | §8: the layers and the rules. |

## Terms

The terms of RFC 001 to RFC 004 apply. In addition:

| Term | Meaning |
|---|---|
| **Host core** | The code compiled for both targets: kernel, admission, domains, control interface, model. |
| **Engine adapter** | The code that binds the kernel to one WebAssembly engine: the browser's, or Wasmtime. |
| **Platform adapter** | The code that binds the Host to transport, storage, pacing, watchdog, and capacity probing on one platform. |
| **Front end** | A client of the control interface: the browser UI, the native UI, the headless runner, a script. |
| **Run record** | The file that holds a run's identity, inputs, digests, and optionally its trace and diagnostic log. |
| **Instance pool** | The set of resident guest instances and the Host's accounting of them. |
| **View model, scene, shell** | The three layers of the user interface (UI-013). |

## 1. Structure

```text
+---------------------------------------------------------------------------+
|  Front ends            browser UI (Leptos)   native UI (Phase 6)   florn CLI |
+---------------------------------------------------------------------------+
|  Control interface     commands, queries, subscriptions; one definition     |
+---------------------------------------------------------------------------+
|  Host core             kernel   admission   model   domains   trace/digest  |
|  (one codebase,        time, queue, order, activation, effects, replay,     |
|   both targets)        instance pool, diagnostic log                         |
+-------------------------------------+-------------------------------------+
|  Engine adapter                     |  Platform adapter                    |
|  browser WebAssembly  |  Wasmtime   |  web (worker, storage, SAB/messages) |
|                       |             |  native (files, channels, clock)     |
+-------------------------------------+-------------------------------------+
```

**Rule 1.** The Host core contains no browser interface and no operating-system interface. It compiles for `wasm32-unknown-unknown` and for the native targets, and a test in continuous integration proves it: the core crates' dependency trees contain none of `web-sys`, `js-sys`, `wasm-bindgen`, `std::fs`, `std::net`, `std::time::SystemTime`, or a threading primitive used for execution. The core reads no wall clock (RFC 001 rule 9).

**Rule 2.** The two Hosts differ only in the engine adapter and the platform adapter (MIG-003). A feature that needs host-specific code is designed as an operation of one adapter trait, implemented twice, and tested once through the core.

**Rule 3.** Domains are crates of the Host core. A domain provides its world's built-ins, its solver, its message layouts, its validation of domain fields, its registered units, and its views' data. A domain depends on the kernel; the kernel depends on no domain (ARCH-006). A domain never reaches an adapter directly.

**Rule 4.** The kernel executes on one thread, one event at a time (RFC 001 rule 21). Parallel execution, if ever introduced, is an adapter concern that must leave the trace identical (NUM-002).

## 2. The engine adapter

**Rule 5.** The engine adapter offers the kernel these operations and nothing else. Every operation that takes an address or a length checks it against the memory's current size and returns an error, never panics.

| Operation | Meaning |
|---|---|
| Compile | Admitted bytes to a compiled module, shared by every instance of it (ACT-007) |
| Instantiate | Compiled module and the import table of its world to an instance; supplies the Host-owned globals of RFC 003 |
| Call | A typed, cached reference to an export; flat arguments to flat results, or a trap with its kind |
| Read memory, write memory | Through a view over the instance's memory taken after the call that may have grown it |
| Read global, write global | The budget and height globals; the exported mutable globals for state export |
| Read table, write table | For state export and restore |
| Drop | Release an instance; the engine's resources are returned |

**Rule 6.** Lowering and lifting go through views over existing memories. The adapter never creates an intermediate object per crossing: no `Uint8Array` copy of a payload, no per-call closure, no string conversion of a number. Measured: one allocation per lowering cost 8× at 64 KiB in Chromium and made Firefox's tail 10× its median.

**Rule 7.** Exports are resolved once at instantiation into typed function references, by the canonical ABI names of RFC 002 rule 20, and called through those references. No name lookup and no reflection per call.

**Rule 8.** The remaining-budget global is set and read without a JavaScript value conversion (RFC 003 rule 11). The browser engine adapter reaches it from the Host's own WebAssembly code, through a global shared at instantiation or through a WebAssembly-to-WebAssembly call; the choice and its measured saving are recorded by the conformance kit's crossing benchmark.

**Rule 9.** The Wasmtime adapter configures the engine deterministically and the same way in every build: threads off, relaxed SIMD off, fuel off (florn meters), NaN canonicalisation off (RFC 003 §9), epoch interruption on only as the Host's watchdog against its own defects (ACT-006), store limits set from the capacity probe. Compiled modules are cached on disk keyed by admitted hash and engine version, so that a run does not pay for compilation twice.

**Rule 10.** The browser engine adapter runs inside a worker (OBS-001), instantiates synchronously, binds the import table once per world, and surfaces every instantiation failure — `RangeError`, a bare string, an out-of-memory condition — as a Host fault with the coded message `host.instance-failed` naming the actor and the engine's text. It never lets such a failure escape as an exception. It caches compiled modules by admitted hash, as the native adapter does (rule 9): a restore or a swap instantiates and never compiles.

**Rule 11.** The conformance kit carries a crossing benchmark: the call floor, a 64-byte crossing, a 64 KiB crossing, each with metering off and on, on every supported engine. The Stage 0 figures are the baseline: call floor 0.2–0.3 µs everywhere; 64 bytes 0.6 µs native, 1.0 µs Chromium, 1.9 µs Firefox; 64 KiB 2.3, 2.4, 3.2 µs. The figures appear in the notes of every minor release next to the cost benchmark of RFC 003.

## 3. The platform adapter

**Rule 12.** The platform adapter offers the Host core these operations.

| Operation | Browser | Native |
|---|---|---|
| Transport to front ends | `postMessage` with transferred buffers; a shared ring over `SharedArrayBuffer` for high-rate telemetry where the page is cross-origin isolated | In-process channels for the native UI (MIG-004); standard streams for the CLI |
| Storage | Origin storage as a cache; the file system through the user's explicit import and export (WEB-012) | Files |
| Pacing clock | The wall clock, used only to decide when the Host works (RFC 001 rule 9) | The same |
| Watchdog | The page may terminate the worker | Epoch interruption; process supervision |
| Capacity probe | Instances and memory the renderer process will hold (RFC 003 rule 20) | Instances per store and memory |
| New-run seed | A source of entropy for the root seed of a *new* run, which then becomes a recorded input | The same |

**Rule 13.** The platform adapter is the only place where a wall clock or a source of entropy exists. Both reach the kernel only as recorded inputs: pacing decisions never enter the logical world, and a seed is an input the moment it is chosen (RFC 001 rules 9 and 27).

**Rule 14.** Telemetry transport. The Host writes high-rate telemetry as fixed-size binary records of 16 bytes (OBS-008) into one interface with two implementations: a single-producer single-consumer ring in shared memory (1 MiB, 65,536 records, with a drop counter) when the page is cross-origin isolated, and one transferred message per Host tick, queued on the display side with a bound of 64 buffers, otherwise. Both implementations deliver every record; which records are shown is a reduction decided above the transport, the same for both. The reader on the display side does not know which it reads from. The Host reduces before it writes (OBS-003): the display path may drop or thin; the trace never changes. Measured in Stage 0 in Chromium and Firefox: reading costs at most 0.1 ms per frame at the 95th percentile at 20,000 records per frame on either path, with no record dropped; at 2,000 records per frame both paths hold the display's cadence; drawing 20,000 objects per frame halves it in both browsers, so a view reduces to the order of 2,000 moving objects before drawing. Without shared memory the user sees one line stating that messages are in use.

## 4. The kernel's execution model and the instance pool

**Rule 15.** The run loop is: take the next event in key order (RFC 001 rule 14); if its target is a guest-backed actor, set the budget, call the export, read the globals, classify (RFC 003 rule 13); commit or discard the recorded effects; append the record and extend the digest chain; apply pending interventions at the instant boundary (RFC 001 rule 33); repeat until the unit of advance (RFC 001 §2.1) is complete or `end` is reached. Pacing is the platform adapter deciding when to call "advance", never what it computes.

**Rule 16.** The Host records, per activation, the effects with their payload digests (§5), the number of random draws (RFC 002 rule 31), and the metered work. A faulted activation restores the random stream, discards the effects, and appends the fault record (RFC 001 rule 24).

**Rule 17.** The instance pool holds the resident instances. Its capacity comes from the probe of RFC 003 rule 20. In release 0.1 and 0.2 every guest-backed actor is resident for the whole run, and a scenario beyond capacity is refused before instantiation with the numbers. From release 0.3 the pool may hold more actors than instances: an actor that is not resident is held as exported state (RFC 003 §6) and restored into a fresh instance before its activation; the policy for choosing victims is the pool's and never affects the trace: the least recently activated actor is swapped out. Measured in Stage 0: one swap — restore into a fresh instance, one activation, export — costs 50–60 µs at 131 KB of state in Wasmtime, Chromium, and Firefox, about 0.4 µs per KB, linear in state size; 1,000 actors are served through one resident instance at about 100 ms per round in which each is activated once. The capacity probe reports the measured cost per KB, and the user interface shows when a run is swapping (NFR-004).

**Rule 18.** The Host counts memory: the sum of the instances' memory limits, the kernel's queue, and the trace buffers, against the probed capacity. When a limit is approached the control interface reports it (`host.capacity-warning`) before it is hit (WEB-007, NUM-004).

**Rule 19.** Inspection never calls a live guest (RFC 001 rule 39b, DES-012). An inspector that needs guest code runs it on a copy restored from exported state.

## 5. Trace, encoding, digest

**Rule 20.** The trace consists of the records of RFC 001 rule 44 in the canonical encoding of RFC 004 §5, applied to these record formats. The trace format carries no version of its own: its version is the run record's (rule 29).

| Record | Fields, in order |
|---|---|
| Handled event | Kind (u8); key (tick u64, microstep u32, class u8, tie value u64, origin u32, sequence u64); target (u32); metered work (u64); random draws (u32); committed effects as a list |
| Effect | Kind (u8: send, set-timer, cancel-timer, report); sequence number (u64); for send: port (u32), payload length (u32), payload digest (32 bytes); for timers: identity (u32) and delay (u64); for report: channel (u32), payload length (u32), payload digest (32 bytes) |
| Fault | Kind (u8); key; actor (u32); fault kind (u8) and its parameter (u32); metered work at the fault (u64) |
| Intervention | Kind (u8); key; content as a canonical map (RFC 004 rule 32) |

**Rule 21.** A payload is hashed once, when the effect is recorded; the record carries the digest and the length, never the bytes. The chain of RFC 001 rule 48 hashes records. The digest is a function of the same information as before — a changed byte changes the payload digest and therefore the record — but a 64 KiB payload is hashed once instead of twice, and the record stays small. Payload bytes are kept for display and export separately from the chain (rule 26).

**Rule 22.** The functions. Module and scenario identities remain SHA-256 (RFC 004). The payload digests and the trace chain use BLAKE3; stream keys are derived with BLAKE3's key derivation from the root seed, the owner's identity, and the purpose label (RFC 001 rule 29); the generator is ChaCha with 8 rounds (RFC 001 rule 32). The three names — `blake3`, `blake3-derive-key`, `chacha8` — are recorded in the run identity (RFC 001 rule 40, item 7). This answers RFC 001's open question 2. BLAKE3 is chosen for the chain because it is faster than SHA-256 in WebAssembly without SIMD, which is the browser Host's situation; the Phase 1 benchmark confirmed it before the trace format was frozen (open question 1, resolved 2026-10-20): per digest, BLAKE3 against SHA-256 is 0.10 against 1.48 µs at 64 bytes, 5.4 against 15.5 µs at 1,500 bytes and 81 against 228 µs at 64 KiB in Chromium, and 0.06 against 0.39, 1.3 against 3.6 and 59 against 146 µs in Firefox, and 0.16 against 0.91, 4.4 against 9.1 and 223 against 484 µs in WebKit; natively, where SHA-256 has hardware support, 0.07 against 0.06, 1.6 against 0.6 and 5.1 against 24.6 µs.

**Rule 23.** The chain is stored at fixed intervals of records and at the end (RFC 001 rule 48). The interval is a parameter of the run record, 1,000 records by default (RFC 001 open question 4; RFC 005 open question 2, resolved 2026-10-20): a stored point is 40 bytes, so the default costs 40 KiB per million records, and a divergence is localised within about ten milliseconds of the kernel's work at the measured 6 to 11 µs per activation. Replay compares at every stored point and reports the first interval of divergence (RFC 001 rule 50, DES-007).

**Rule 24.** Diagnostics for a divergence (NFR-009): on request, the Host computes per-actor chains from a trace, so that two divergent runs can be narrowed to the first actor and record that differ; the per-actor chain is derived from the trace, not part of it.

## 6. The control interface

**Rule 25.** One definition of commands, queries, subscriptions, and notifications, as Rust types in the Host core with the canonical binary encoding over every transport (CTL-001). Every front end uses it and nothing else; no front end reaches the kernel or an adapter directly. The definition has a version; a front end and a Host of different versions refuse each other with `control.version`.

**Rule 26.** The messages, release 0.1 unless marked.

| Kind | Name | Effect on the logical world |
|---|---|---|
| Command | `load-scenario` (manifest or package bytes), `load-module` (module bytes, held by hash for a manifest that references them), `admit` (validate and admit without running) | None |
| Command | `new-run` (scenario, seed or "choose"), `end-run` | Creates or ends a run; the seed becomes an input |
| Command | `advance` (one event, one instant, one tick, to a tick), `pause`, `set-pacing` (speed or "as fast as possible") | None: pacing and stepping change when the Host works (RFC 001 rule 9) |
| Command | `intervene` (target, payload per domain, optional tick) | **An intervention**: validated, applied at the next boundary, recorded as an input (RFC 001 rules 33 to 38) |
| Command | `export` (run record, trace with or without payloads, diagnostic log, package; with a preview first) | None |
| Command | `replay` (run record) | A new run whose inputs are the record's; reports the comparison |
| Command, 0.3 | `checkpoint`, `restore`, `branch` | Per RFC 001 rules 51 to 54 |
| Query | `scenario`, `run-state`, `actor` (configuration, status, faults, recent telemetry), `trace-window`, `capacity`, `support` (which features this Host build has) | None |
| Subscription | `telemetry` (channels, reduction), `events` (faults, limits, interventions applied), `progress` | None |
| Notification | Every coded message of the Host to a user: errors, warnings, faults, limits, capacity | None |

**Rule 27.** The Host validates every command before it acts (CTL-002). A command that fails validation produces a coded notification and changes nothing. A command that changes the logical world is an intervention, and only `intervene` (and, from 0.3, `restore` and `branch`) does. Topology editing during a run (BUS-008) is a sequence of `intervene` commands whose payloads the domain defines.

**Rule 28.** Three error families, kept apart in the types (CTL-004): a `result` a guest receives from a domain function is a value of the contract; a fault is a trace record; an error a user sees is a notification with a stable code and typed parameters, and its text comes from the front end's catalogue (CTL-003, RFC 007).

**Rule 29.** The run record is a ZIP archive with the extension `.florn-run` (RFC 004 rule 36).

| Path | Content | Required |
|---|---|---|
| `run.toml` | Run identity (RFC 001 rule 41): florn version, Host build, engine and version, CPU architecture; the inputs: scenario hash, module hashes, admission and cost-model versions with the cost table, contract versions, root seed, the function names of rule 22, the interventions in order; the stored chain values with their record indices; wall-clock metadata, marked as such | Yes |
| `trace/<n>.bin` | The trace in chunks of the interval of rule 23 | Optional |
| `payloads/<digest>` | Payload bytes by digest, when the export includes them | Optional |
| `diagnostic.log` | One coded record per line: admission results, faults, limit events, interventions, capacity warnings (SEC-008) | Yes |
| `diagnostic.bin` | The same log in the canonical encoding of RFC 007 rule 12, the form a front end re-renders in any language | Yes |
| `README.md` | For people | No |

A run record contains no scenario text and no module bytes; it references them by hash. A package (RFC 004 §6) and a run record are different files for different questions: what world, and what happened in it.

**Rule 30.** Export is explicit and previewed (RFC 004 rule 37): before any file is written, the front end shows what it will contain — every module by name and hash, whether payload bytes are included, the size — and the user confirms. Nothing leaves the machine otherwise (WEB-002, SEC-006).

**Rule 31.** The headless runner (`florn` on the command line) is a front end like the others: `florn run <scenario> [--seed N]` prints the digest and writes a run record; `florn replay <run>` prints the comparison; `florn admit <module>` prints the manifest and the admission result; `florn probe` prints the capacity. Automation needs nothing else in release 0.1 (AUTO-001, AUTO-002); a text form of the whole control interface on standard streams is open question 4.

## 7. The browser Host

**Rule 32.** The Host core and the engine adapter run in a dedicated worker. The main thread holds the shell and reads telemetry once per animation frame (UI-007, NFR-003). The worker posts a readiness message once its handlers exist, and the page posts nothing to it before: a message delivered after the worker's initial script and before its WebAssembly has initialised is dropped silently, which a start from a cached page exposed in Stage 0. The conformance kit starts the Host from a cache-served page as one of its cases.

**Rule 33.** The application is served with the isolation headers and the Content Security Policy below, on every path including those a service worker answers from its cache. This is the hosting requirement. Shared memory is used only when the page reports cross-origin isolation; otherwise the message transport applies without a change in the core (OBS-002), and a host that cannot set headers can still serve the application at the measured cost of nothing at 2,000 records per frame. `Cross-Origin-Embedder-Policy: credentialless` is not used: the application loads nothing cross-origin. A service worker that injects the headers on a host that cannot set them is a documented fallback, florn's own code and never a third-party script; it costs a second load on the first visit and is not recommended.

| Header | Value |
|---|---|
| `Cross-Origin-Opener-Policy` | `same-origin` |
| `Cross-Origin-Embedder-Policy` | `require-corp` |
| `Content-Security-Policy` | `default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'; style-src 'self'; img-src 'self'` |

**Rule 34.** No inline script and no script generated at run time: the loader is a file, every module is a core module (RFC 002 rule 6), and the policy has no `unsafe-inline`, no `blob:`, and no other origin. The application makes no request to any origin but its own and contains no analytics (WEB-009); the conformance kit's browser run asserts that the network log holds no other origin.

**Rule 35.** After one visit the application starts offline: a service worker caches the page, the scripts, the stylesheets, and the Host's WebAssembly, and serves them with the headers of rule 33 (WEB-003). Each released version lives at an immutable versioned path and never updates itself under a user (WEB-010); the user chooses a version to open.

**Rule 36.** Origin storage holds scenarios and run records as a cache; the file is the durable form, and the shell shows the difference (WEB-012). A module is imported from a local file, hashed in the worker, admitted, and its manifest and hash shown before it can act (WEB-005, from 0.3).

## 8. The user interface

**Rule 37.** The user interface has three layers (UI-013), each a crate, with dependencies in one direction.

| Layer | Holds | Depends on | Written |
|---|---|---|---|
| View model | What is shown: selection, visible area, zoom, time window, filters, the fields of the inspectors, language-independent state | The control interface types only | Once |
| Scene | What to draw: lists of lines, circles, labels, waveform segments, each with position and style; the layout of the topology | The view model only | Once |
| Shell | Panels, forms, menus, translations, accessibility, the drawing back end; the framework | The scene, the view model, the framework | Once per pattern |

The view model and the scene compile and test natively, with no dependency on the framework, `web-sys`, `js-sys`, or `wasm-bindgen`; continuous integration checks the dependency trees as in rule 1.

**Rule 38.** The browser shell is Leptos, client-side rendered, drawing the scene on a 2D canvas (which draws text with the system's fonts, including Japanese, without shipping font files). The native shell is decided in Phase 6; candidates are a GPU-rendered Rust toolkit and the browser shell attached to a local native Host. The decision is reversible because of rule 37: the framework-specific part of the proof-of-concept shell was about 300 of 1,000 lines.

**Rule 39.** Rules for every shell, each learned from a defect or a measurement in the proof of concept.

| Rule | Why |
|---|---|
| The 60 Hz data path bypasses the reactive graph: telemetry goes from the transport to the drawing code directly; reactive state holds only what a person changes | A reactive framework would re-run work on every message |
| The shell never writes back into a field while the user types; a field's value is set only when the selection changes, and the view model is updated on input outside composition and on composition end | Input methods compose text in the field; a write-back destroys the composition. Verified with a Japanese input method in both browsers |
| No panic may escape an event handler; the view model is reached through `try_borrow` or a single-entry update function | Under `panic = "abort"` a trapped panic left the view model borrowed and the page kept animating while silently refusing every edit |
| Every list is one Tab stop (roving tabindex); arrow keys move inside it; every scrollable region has an accessible name | A 500-item list was 500 Tab stops; the automated test, allowed 600 presses, did not notice, and a person would |
| Canvas content has an accessible twin: a list or table of the same elements, selectable by keyboard, with the canvas's own description in `aria-label` | Text on a canvas is invisible to assistive technology (UI-008) |
| Every user-visible string comes from a catalogue; keys are checked at compile time; the language switches without a reload and keeps typed text (UI-011) | Verified; a missing key is a build error |
| The loader is a file; the service worker serves cached responses with the headers of rule 33 | Rule 34, rule 35 |
| Status is never colour alone; the focused element is always visible | UI-008 |

**Rule 40.** The frame budget of the shell is measured, not assumed: draw time at the 95th percentile at most 8 ms, and at most 1% of frames longer than twice the browser's cadence, with 500 nodes, their labels, and 200 moving messages (OBS-007, NFR-013). The proof of concept measured 1 ms in Chromium and 2.6 ms in Firefox; the interval between frames is the browser's cadence and is not a measure of florn.

**Rule 41.** The user interface tests of the conformance kit are those a person would perform: type into every field after selecting; switch the language and check the typed text; reach every control by keyboard with a bound on the presses between landmarks; start offline; and the Japanese-input script, which runs against a real input method when the machine has one.

## 9. Repository layout

**Rule 42.** One Cargo workspace. Crate names are prefixed `florn-`; WIT packages live under `florn:` (PKG-007). The tree shows the split of §1.

```text
florn/
  Cargo.toml                   workspace
  contracts/
    florn-core/wit/            florn:core (RFC 002 §4; a test checks equality with the RFC)
    florn-network/wit/         florn:network (Phase 2)
  crates/
    florn/                     the crate that carries the name: the binary `florn` (the runner's entry point); its library re-exports nothing before 1.0
    florn-kernel/              time, queue, order, activation, effects, trace, digest, replay
    florn-abi/                 canonical ABI lifting and lowering (RFC 002 §2), shared by both engine adapters
    florn-model/               manifest, package, canonical encoding, validation (RFC 004)
    florn-admission/           the admission step (RFC 003)
    florn-control/             the control interface types and encoding (§6)
    florn-host/                the Host: kernel + admission + model + domains + control, over the two adapter traits
    florn-engine/              the engine adapter trait (§2)
    florn-engine-wasmtime/     Wasmtime adapter
    florn-engine-web/          browser adapter
    florn-platform/            the platform adapter trait (§3)
    florn-platform-native/
    florn-platform-web/
    florn-domain-network/      Phase 2; later florn-domain-circuit, florn-domain-finance
    florn-guest/               the guest SDK (RFC 002 §8)
    florn-conformance/         the conformance kit: guests, scenarios, benchmarks, Host tests
    florn-ui-model/            view model (§8)
    florn-ui-scene/            scene (§8)
    florn-ui-web/              browser shell (Leptos); later florn-ui-native
    florn-cli/                 the headless runner as a library (§6, rule 31); its tests live with the binary in `florn/`
  scenarios/                   example scenarios with recorded digests (PKG-005, AUTO-003)
  docs/                        the book: requirements, guides
  rfcs/                        this directory
```

**Rule 43.** Dependency direction, checked in continuous integration: `florn-kernel` and `florn-abi` depend on no other florn crate; `florn-host` depends on the core crates and the two adapter *traits*, never on an adapter implementation; adapters depend on their trait and on `florn-abi`; `florn-ui-model` depends on `florn-control` only; `florn-ui-scene` on `florn-ui-model` only; front ends depend on `florn-control` and, for the browser, on `florn-ui-*`; domains on `florn-kernel` and `florn-model`.

**Rule 44.** Toolchain versions — Rust, the WebAssembly target, the engine, the transformation library — are pinned in the repository, and the run identity records the versions a build used. Dependencies carry permissive licences and are checked automatically (SEC-012).

## 10. Phasing

| Release | This RFC delivers |
|---|---|
| 0.1.0 | Rules 1 to 13, 15, 16, 20 to 31 (without 0.3 items), 42 to 44; the headless runner; a test page as the browser front end |
| 0.2.0 | Rules 14, 32 to 41; the browser shell; the network domain's views |
| 0.3.0 | Swapping (rule 17), checkpoints in the control interface, module import (rule 36) |
| 0.6.0 | The native shell (rule 38) |

## 11. Verification

| Property | Test |
|---|---|
| One core | The core and UI inner crates' dependency trees contain no platform or framework crate; `cargo check` passes for both targets (rule 1, rule 37). |
| Tiers | RFC 001 §14's tests, driven through the control interface under both Hosts; the browser run under Chromium, Firefox, and WebKit; the native run on x86-64 and AArch64 (AC-001). |
| Engine adapter rules | The crossing benchmark (rule 11) within 1.5× of the Stage 0 baseline per engine; a test asserts no allocation per lowering in the browser adapter. |
| Payload digests | A 64 KiB payload is hashed once per effect; the kernel-path figure of the benchmark is within 2× the crossing at 1,500 bytes. |
| Replay | A run record replays to the same chain at every stored point; a record with one altered intervention reports the first divergent interval (AC-003). |
| Interventions | A run with interventions through the control interface, replayed from its record: equal digests. |
| Control interface | Every command is validated: an invalid one changes nothing and produces its code; version mismatch refuses with `control.version`. |
| Run record | Exported, re-imported, replayed; contains no module bytes and no scenario text; the preview matches the file. |
| Capacity | `florn probe` and the browser Host report the probed capacity; a scenario beyond it is refused before instantiation; a warning precedes a limit. |
| Instance failure | An instantiation failure in the browser arrives as `host.instance-failed`, the run ends, the page does not. |
| Browser hosting | Under the headers of rule 33: cross-origin isolated, no policy violation, no request to another origin, offline start after one visit, cached responses carry the headers. |
| UI rules | The tests of rule 41 in both browsers. |
| Native cache | The second start of the same scenario does not compile the module (rule 9). |

## Alternatives considered

| Alternative | Reason for rejection |
|---|---|
| Two Hosts, two kernels | They diverge, and divergence breaks Tier 2 by construction (ARCH-009). |
| A Host written in JavaScript for the browser | Not one codebase; the lifting would not be shared; measured Host-side mistakes in JavaScript cost more than the engine. |
| Payload bytes inline in trace records | Hashed twice, 231 µs at 64 KiB in the browser; large records. |
| SHA-256 for the chain | Slower than BLAKE3 in WebAssembly without SIMD; SHA-256 stays where a person verifies an identity with ordinary tools. |
| An HTTP or REST control interface | A network boundary inside a local tool; serialisation in the native application (MIG-004). The interface is transport-independent instead; HTTP could be one transport later. |
| One framework for both the browser and the native shell | The two patterns have opposite strengths (text and input methods against GPU rendering in one process); the three layers make the framework the small part instead. |
| A web view for the native application | MIG-004: no web view, no serialisation boundary in the native application. |
| Telemetry over the reactive graph | Re-runs work per message; the measured shell keeps the 60 Hz path outside it. |
| Letting the pool's swapping policy depend on the run | The policy must never reach the trace; it is an adapter-side optimisation. |

## Prior art

| Topic | Precedent |
|---|---|
| Kernel with pluggable engine and platform layers | Simulation frameworks that separate model, scheduler, and environment |
| Transport-independent control of a local engine | Debugger adapter protocols and language-server protocols, which define messages once and carry them over any transport |
| Hash chains with content digests | Content-addressed storage and append-only logs |
| Three-layer user interface | Model–view separation as practised in retained-mode GUIs; the proof of concept confirmed it for this project |

## Consequences for other RFCs

| RFC | Must provide |
|---|---|
| 001 | Open question 2 answered (rule 22); open question 4 provisional (rule 23). |
| 003 | The instance pool and probe are consumed here; the budget-global access decided by the benchmark (rule 8). |
| 006 — Security baseline | The threat model of the browser Host cites rules 32 to 36 as the hosting boundary and §6 as the only interface. |
| 007 — Error model | Codes for `control.*`, `host.*`, and every notification of rule 26; the catalogue mechanism of rule 39. |
| Network domain RFC | Its views' data for the scene; its intervention payloads; its telemetry channels. |
| External design (S0-17) | Patterns from the proof of concept: list as the accessible twin of the canvas; inspector fields enabled only with a selection; language switch as a button group; offline status in the header; export preview. |
| Requirements | NFR-013 receives the Stage 0 figures of rules 11 and 40; WEB-011's support matrix lists shared memory as optional with the message fallback. |

## Open questions

| # | Question | Decided by |
|---|---|---|
| 1 | ~~BLAKE3's measured advantage over SHA-256 for the chain in the browser Host (rule 22), before the trace format is frozen.~~ Resolved 2026-10-20 by PR-13; the figures are in rule 22. | — |
| 2 | ~~The chain interval (rule 23).~~ Resolved 2026-10-20 by PR-13: 1,000 records; the reasoning is in rule 23. | — |
| 3 | ~~The telemetry record size and the fallback's cost and notice (rule 14).~~ Resolved 2026-10-14 by the proof of concept on telemetry transport; see rule 14. | — |
| 4 | A text form of the control interface on standard streams for automation beyond the four commands of rule 31. | Phase 2, with the first script that needs it. |
| 5 | The swapping policy and whether a scenario beyond capacity is refused or swapped (rule 17). | Phase 3, with the proof of concept on snapshot and restore and the owner. |
| 6 | The native shell (rule 38). | Phase 6. |
