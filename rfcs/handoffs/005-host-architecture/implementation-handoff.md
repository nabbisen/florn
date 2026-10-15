# Phase 1 — Implementation handoff

Entry point: [`README.md`](./README.md). This file says what to build, crate by crate, which RFC rules each crate implements, and what "done" means. The order of building is in [`task-breakdown-pr-plan.md`](./task-breakdown-pr-plan.md).

## 1. Scope of release 0.1.0

| In scope | Out of scope (later phases) |
|---|---|
| The kernel: time, instants, event order, activation, effects, randomness, interventions as recorded inputs, trace and digest, replay with divergence reporting (RFC 001) | Checkpoints taken and restored during a run, backward navigation, branching (0.3; the export capability itself is in scope) |
| The guest contract `florn:core@0.1.0`, the module manifest, the shared lifting crate, the guest SDK and the echo guest (RFC 002) | Any domain contract (0.2 network) |
| Admission: validation, both custom sections, estimate, the transformation with cost-model version 1, the quotas, capacity probing (RFC 003) | Cost-model version 2 (0.3) |
| The core model: manifest parsing, canonical encoding and hash, validation codes, the package reader (RFC 004) | Topology editing, scheduled changes of domains |
| The Host core over the engine and platform adapter traits; the Wasmtime adapter; the browser adapter with a test page; the control interface types and encoding; the run record; the headless runner; the diagnostic log (RFC 005) | The user interface (0.2), telemetry transport beyond what the test page needs, swapping (0.3) |
| The security baseline's controls that the above produce, and the CI gates (RFC 006) | The hosting of the application (0.2) |
| The notification type, the code enumeration, the fault codes; no catalogue yet (RFC 007) | Translation catalogues (0.2, with the shell) |
| The conformance kit: the RFC 001 §14 tests, the hostile guests, the admission tests, the crossing and cost benchmarks, under both Hosts | The UI tests of the kit (0.2) |

## 2. Toolchain, pinned

| Tool | Version | Note |
|---|---|---|
| Rust | 1.99.0, edition 2024 | `rust-toolchain.toml` in PR-01; the CI uses it |
| Targets | `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `aarch64-apple-darwin`, `wasm32-unknown-unknown` | CI runs the three native ones |
| Wasmtime | 48.0.6 (the 48 long-term-support line; 48.0.2 was measured, 48.0.6 closes the advisories of the line), features `cranelift`, `runtime`, `std`; no `component-model`, no `threads`, no `gc` | RFC 005 rule 9 settings |
| `fvm-wasm-instrument` | 0.4.0 | Wrapped by `florn-admission`; limiter before gas (RFC 003 rule 9) |
| `wasmparser`, `wasm-encoder` | 0.261 | Validation and the two reencoding passes |
| `wit-bindgen` | 0.62 | Guest bindings in the SDK; the `component-type` section it emits is the contract identity |
| `wasm-bindgen`, `js-sys`, `web-sys` | 0.2.126, 0.3.103 | Browser adapter and test page only |
| `blake3`, `chacha20`-family generator | Current permissive releases, pinned in `Cargo.lock` | RFC 005 rule 22: `blake3`, `blake3-derive-key`, `chacha8` |
| `sha2` | 0.11 | Module and scenario identity (RFC 004) |
| `toml` | Current, pinned | Manifest and package metadata |
| Playwright | 1.64 (test only, not linked) | Browser matrix in CI from PR-12 |
| `cargo-deny` 0.20.2, `cargo-audit`, `cargo-fuzz`, `cargo-mutants` | Pinned in CI | RFC 006 rules 5 to 7 |

A version is changed by a PR that says why, and the run identity records the versions a build used (RFC 005 rule 44).

## 3. The crates

Dependency direction is RFC 005 rule 43; CI checks it. Every crate has unit tests; the conformance kit holds the cross-crate and cross-engine tests.

### 3.1 `florn-kernel`

Implements RFC 001 rules 1 to 7, 10 to 39, 43 to 50 and 55. No platform, no engine, no I/O.

| Provides | Rules |
|---|---|
| `Tick`, `Instant { tick, microstep }`, tick length as an enum of three | 1, 2, 10 |
| The event key and a priority queue ordered by it; `TiePolicy::{ByIdentity, BySeed}`; the keyed hash for `by-seed` | 14 to 19 |
| The microstep and events-per-instant limits with the fault that names the origins | 13 |
| Streams: `blake3-derive-key` from the root seed, owner identity, and purpose; ChaCha8 generator; `next_u64` only; position counted per activation for rollback | 27 to 32; RFC 002 rule 31 |
| The activation protocol as types: an `Activation` that records effects and can only be committed or discarded, never both; sequence numbers per origin at commit | 20 to 25; RFC 002 §6 |
| Interventions applied at instant boundaries, recorded as inputs; class 0 events | 33 to 38 |
| Trace records in the canonical encoding of RFC 005 rule 20 with payload digests (BLAKE3); the chain; stored chain values at the interval | 44 to 48; RFC 005 rules 20 to 23 |
| Replay comparison: equal at every stored point or the first divergent interval | 49, 50 |
| Per-actor chains on request | RFC 005 rule 24 |
| The advance units: one event, one instant, one tick, to a tick | §2.1 |

Done when: every row has a test; the RFC 001 §14 properties that need no engine (order, fairness of `by-seed` over 10,000 races, zero-delay fault, streams unchanged by a third consumer) pass natively.

### 3.2 `florn-abi`

Implements RFC 002 §2: lifting and lowering of the type subset (rule 7) with strict checks (rule 8) and canonical NaN (rule 9), over an abstract memory and call interface so that both adapters use it unchanged.

Done when: every type of rule 7 round-trips in a test against a hand-written core module; invalid UTF-8, out-of-range lists, and out-of-range discriminants produce the `contract` fault with the rule named; both adapters pass the same test file.

### 3.3 `florn-model`

Implements RFC 004: the manifest types, TOML parsing with the core sections closed and domain sections opaque, quantities with units (time and bytes known to the core; other units registered by domains — in 0.1 none), identities and references, the canonical encoding (rule 30) and the scenario hash (rule 31), the package reader with hash checks (rules 33 to 35), and every validation code of §7 reported in one pass with typed parameters (RFC 007 §4.1).

Done when: the verification table of RFC 004 §9 passes; a manifest declaring 10⁷ actors is refused in bounded memory; the worked example of RFC 004 §8 parses, hashes, and round-trips.

### 3.4 `florn-admission`

Implements RFC 003 §1 to §6 and §9, and RFC 002 §3.

| Provides | Rules |
|---|---|
| Validation with the exact feature set, import and export checks, the maxima, no start section, no table mutation | RFC 003 rule 5; RFC 006 §7 |
| Reading both custom sections; the florn manifest schema; port kinds, parameters, channels, needs | RFC 002 rules 11 to 17 |
| Estimate: needs against limits, capacity against the probe | RFC 003 rules 17, 20 |
| The transformation: cap memory, limiter, metering, export state, validate, hash; admission version 1; cost-model version 1 with its table | RFC 003 §3, §4, rule 14 |
| The state layout of an admitted module | RFC 003 §6 |
| The admission cache keyed by (source hash, admission version, cost-model version, class limits) | RFC 003 rule 3 |
| The config encoder: manifest `config` to the guest's byte record in declared order with defaults filled | RFC 002 rules 13, 14 |

Done when: the RFC 003 §10 tests that need no run pass (transformed bytes recorded per admission version; custom sections preserved; every refusal of §2 produces its code); the admitted hash is identical natively and in the browser for every kit module.

### 3.5 `florn-engine`, `florn-engine-wasmtime`, `florn-engine-web`

The trait of RFC 005 rule 5 and its two implementations, with rules 6 to 10: views not objects, typed cached calls, the budget global reached without JavaScript conversion, the Wasmtime settings, compiled-module caches in both, instantiation failures as Host faults.

Done when: the crossing benchmark of RFC 005 rule 11 runs under both adapters and reports within 1.5× of the Stage 0 baseline; the browser adapter has a test asserting no allocation per lowering; a restore never compiles (RFC 003 rule 23).

### 3.6 `florn-platform`, `florn-platform-native`, `florn-platform-web`

The trait of RFC 005 rule 12 with the native implementation (files, standard streams, the clock for pacing only, epoch watchdog, capacity probe, entropy for a new seed) and the web implementation needed by the test page (worker transport by messages, probe, watchdog, entropy). Telemetry's shared ring is 0.2.

Done when: the probe reports instances and memory on each platform; a wall clock reaches the kernel only as a recorded seed (RFC 005 rule 13, tested by searching the kernel's dependency tree and by the determinism tests).

### 3.7 `florn-host`

Composes kernel, admission, model, and the two traits into the Host: the run loop (RFC 005 rule 15), the activation with budget set before lowering (RFC 003 rule 11), fault classification by reading globals (RFC 003 rule 13), quotas in the import handlers (rule 17), the instance pool for resident actors (rule 17 of RFC 005, no swapping), actor export and checkpoint export (RFC 003 §6; restore only in tests, since 0.3 adds the feature), the diagnostic log (RFC 007 §5), the run identity and run inputs (RFC 001 §9), the run record writer and reader (RFC 005 rule 29).

Done when: AC-001 to AC-006 pass under the native Host with the kit's guests; the Host contains no platform or engine type (CI dependency check).

### 3.8 `florn-control`

The types and canonical encoding of RFC 005 §6 (commands, queries, subscriptions, notifications of rule 26 for release 0.1) and the notification shape, code enumeration, parameter types, and fault codes of RFC 007 rules 2 to 7 and §4. No text.

Done when: every code of RFC 007 §4 exists as a variant with its parameters typed; encoding round-trips; a command with an invalid parameter produces `control.invalid` and changes nothing (RFC 005 rule 27); version mismatch produces `control.version`.

### 3.9 `florn-cli`

The headless runner `florn` with `run`, `replay`, `admit`, `probe` (RFC 005 rule 31, external design §4): prints the digest and writes `.florn-run`; prints the comparison; prints the manifest and the admission result; prints the capacity. Text output is English, produced by the runner from codes with a minimal built-in English table (the catalogue mechanism arrives with the shell in 0.2; the runner's table is the English source of RFC 007 rule 17 and moves into the catalogue then).

Done when: AC-005's "parameters listed by the headless runner" holds; a script can run every kit scenario and compare digests (AUTO-001, AUTO-002).

### 3.10 `florn-guest`

The SDK of RFC 002 §8: bindings from the WIT, the declaration macro that writes the `florn.manifest` section and the config decoder, the distributions over `random`, telemetry encoders, allocator and panic handler, `std` and `no_std` profiles, `now()`. Plus the **echo guest** (`crates/florn-guest/examples/echo` or the kit's guests crate) and the kit's hostile guests, each a feature or an example.

Done when: a guest built from the template on a clean machine with only the Rust toolchain admits and runs under both Hosts (AC-007); the manifest and the decoder are generated from one declaration (RFC 002 §3.1).

### 3.11 `florn-conformance`

The kit (AUTO-007): guests (echo, the five hostile guests of RFC 003 §10, the probe guests, the benchmark guests B1 and B2), scenarios with recorded digests, the RFC 001 §14 tests, the RFC 002 §10 tests, the RFC 003 §10 tests, the RFC 005 §11 tests that apply in 0.1, the crossing and cost benchmarks with their published figures, runnable under the native Host directly and under the browser Host through the test page and Playwright.

Done when: the kit runs in CI on the three architectures natively and on Chromium, Firefox, and WebKit; AC-001 to AC-007 are its named tests.

## 4. Cross-cutting rules

| Rule | Source | How it is checked |
|---|---|---|
| The core reads no wall clock and uses no thread for execution; iteration over unordered collections never reaches the trace | RFC 001 rules 9, 19, §13.1 | Dependency check; the kit's Tier 1 test run twice |
| The Host never calls a guest for inspection | RFC 001 rule 39b | No such call exists; review |
| Every refusal and fault is a code with typed parameters and nothing else | RFC 007 rules 3 to 7 | The `Code` enumeration is the only way to produce one |
| No `unsafe` outside the adapters and the ABI crate, and each use has a comment stating the invariant | Project hygiene | `#![forbid(unsafe_code)]` in the other crates |
| No panic reaches an adapter boundary from the Host; errors are values | RFC 005 rule 39 (the same lesson for the Host) | The browser adapter test with a hostile guest |
| Every rule implemented has a test that fails without it | This package | Review of each PR against its RFC rules |
| Measured budgets are kit tests, not comments | NFR-013 | The benchmarks publish figures per release |

## 5. Figures to meet or to establish

| Figure | Stage 0 baseline | Phase 1 rule |
|---|---|---|
| Crossing: call floor; 64 B; 64 KiB | 0.2–0.3 µs; 0.6 / 1.0 / 1.9 µs; 2.3 / 2.4 / 3.2 µs (Wasmtime / Chromium / Firefox) | Within 1.5× of baseline per engine (RFC 005 rule 11) |
| Metering cost, protocol-style and compute-style | 1.9–4.0× and 1.4–2.9× | Published per release; the plan's step 2 (RFC 003 §7) |
| Kernel path per activation at 1,500 B | ≈ 2× the crossing in the browser, with payload digests | Within 2× the crossing (RFC 005 §11) |
| Capacity probe | 124 instances in Chromium, 999 in Firefox, 10,000 per store natively | Reported at start-up |
| Export and restore | 0.04–0.31 ms and 0.12–0.32 ms per MB | Kit test; restore never compiles |
| Limits and maxima defaults | RFC 003 rule 19, RFC 006 §7 | Measured and fixed before 0.1.0 (RFC 003 open question 1) |
| Chain interval; BLAKE3 against SHA-256 in the browser | 10,000 provisional; expected faster | Measured before the trace format is frozen (RFC 005 open questions 1, 2) |

## 6. Decisions the developer must not take alone

| Topic | Who decides |
|---|---|
| Any change to a WIT file, a code, a record format, or a pass order | The architect, by RFC amendment |
| Any new dependency | The licence gate first; the architect if it is not permissive or not pinned |
| Any figure of §5 that cannot be met | The architect, with the measurement |
| Any test that cannot be made to pass under one engine | Reported with the first diverging record; never skipped per engine |
