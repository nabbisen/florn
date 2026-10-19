# Phase 1 — Task breakdown and pull-request plan

Entry point: [`README.md`](./README.md). One pull request per task, in this order unless marked independent. Each PR names its task number, the RFC rules it implements, the tests it adds, and the checklist items the verifier runs. The progress table at the end is updated with every merge.

| # | Task | Implements | Tests added | Verifier checks | Size |
|---|---|---|---|---|---|
| PR-01 | **Workspace.** The Cargo workspace of RFC 005 §9 with every crate as an empty library or binary; `rust-toolchain.toml`; `contracts/florn-core/wit/` copied from RFC 002 §4; the test that compares the WIT files with the RFC's text block; the dependency-direction check in CI; `#![forbid(unsafe_code)]` where §4 says | RFC 005 rules 42 to 44; RFC 002 rule 19 | WIT equality; dependency direction | CI green on three architectures; `cargo deny` green | S |
| PR-02 | **Model.** `florn-model`: manifest types, parsing, quantities, identities, canonical encoding, scenario hash, validation codes, package reader | RFC 004 §1 to §7; RFC 007 §4.1 | RFC 004 §9 table; bounded-memory refusal; worked example | AC-005 part (manifest refusals); checklist C2 | L |
| PR-03 | **Kernel.** `florn-kernel`: time, instants, key and queue, tie policies, limits of rule 13, streams, activation types, interventions, trace records with payload digests, chain, replay comparison, advance units | RFC 001 §2 to §11; RFC 005 §5 | RFC 001 §14 engine-free properties; encoding round-trips; chain interval | Checklist C3 | L |
| PR-04 | **ABI.** `florn-abi`: the type subset, strict lifting, canonical NaN, over abstract memory and calls | RFC 002 §2 | Round-trip of every type; the three contract faults | Checklist C4 | M |
| PR-05 | **Engine trait and Wasmtime.** `florn-engine`, `florn-engine-wasmtime` with the settings, cache, typed calls, globals and tables | RFC 005 rules 5 to 9; RFC 003 rule 11 | Adapter conformance test file; restore never compiles | Checklist C5 (native half) | M |
| PR-06 | **Admission.** `florn-admission`: validation, manifest sections, estimate, transformation with the fixed order, cost-model version 1 table, layout, cache, config encoder | RFC 003 §1 to §4, §6; RFC 002 §3 | RFC 003 §10 run-free rows; identical admitted hash on both Hosts (completed in PR-12) | AC-005; checklist C6 | L |
| PR-07 | **Host.** `florn-host`: run loop, activation with budget before lowering, classification, quotas, instance pool, actor and checkpoint export, diagnostic log, run identity, run record | RFC 005 §4, rules 15 to 19, 29; RFC 003 rules 12, 13, 17, 20; RFC 001 §9 | AC-002, AC-003, AC-004, AC-006 natively with the kit's guests | Checklist C7 | L |
| PR-08 | **Control.** `florn-control`: commands, queries, subscriptions, notifications; code enumeration with typed parameters; fault codes | RFC 005 §6; RFC 007 rules 2 to 7, §4 | Encoding round-trip; `control.invalid`, `control.version` | Checklist C8 | M |
| PR-09 | **Headless runner.** `florn-cli`: `run`, `replay`, `admit`, `probe`; the English table | RFC 005 rule 31; AUTO-001, AUTO-002 | Every kit scenario by script; digest comparison | AC-005 (parameters listed); checklist C9 | M |
| PR-10 | **Guest SDK and guests.** `florn-guest`: bindings, declaration macro and manifest section, distributions, telemetry encoders, profiles; the echo guest; the hostile and probe guests | RFC 002 §8; RFC 003 §10 guests | Template builds on a clean machine; manifest from one declaration | AC-007 half; checklist C10 | L |
| PR-11 | **Conformance kit, native.** `florn-conformance`: scenarios with digests, the RFC 001 §14 tests, RFC 002 §10, RFC 003 §10, the crossing and cost benchmarks, under the native Host, in CI on three architectures | AUTO-003, AUTO-005, AUTO-007; RFC 003 §7 step 2; RFC 005 rule 11 | The kit itself | AC-001 (native, two architectures), AC-002, AC-003, AC-004, AC-006, AC-007 (native); checklist C11 | L |
| PR-12 | **Browser Host.** `florn-engine-web`, `florn-platform-web`, a test page and worker, the Playwright driver in CI for Chromium, Firefox, and WebKit on macOS; the kit under the browser Host | RFC 005 rules 10, 12, 32 to 35 (test page under the headers); RFC 002 rule 10; RFC 003 rule 11 (budget global without JavaScript) | Identical digests and admitted hashes across engines; no allocation per lowering; the crossing benchmark in the browser | AC-001 (three browsers), AC-002 (both Hosts), AC-007 (both Hosts); checklist C12 | L |
| PR-13 | **Figures and formats fixed.** The measured defaults and maxima (RFC 003 rule 19, RFC 006 §7), the chain interval, BLAKE3 against SHA-256 in the browser; the trace and run-record formats frozen for 0.1 | RFC 003 open question 1; RFC 005 open questions 1, 2 | The benchmarks that produced the figures | Checklist C13; the architect amends the RFCs with the figures | M |
| PR-14 | **Release 0.1.0.** Documentation chapters for module authors and automation users; `CHANGELOG.md`; the security review of RFC 006 §8; release notes with file hashes; the tag; RFC 001 to 007 moved to `rfcs/done/` | RFC 006 rule 5; RFC 000; roadmap release checklist | The release checklist | The owner accepts the phase against its exit criteria | M |

Independent of the order above: PR-04 may precede PR-03; PR-08 may follow PR-03 directly; PR-10's guests are needed by PR-07's tests and may be built as a fixtures crate first.

## Progress

| PR | State | Merged | Verifier record |
|---|---|---|---|
| PR-01 | **merged** | `64cd19c`, 2026-10-14; CI run 38571954065 green on x86-64, AArch64, Apple Silicon | C1: pending |
| PR-02 | **merged** | `d9f1422` + `fbe172a`, 2026-10-14; CI green on three architectures | C2: pending |
| PR-03 | **merged** | `1dbf805`, 2026-10-14; CI green on three architectures | C3: pending |
| PR-04 | **merged** | `3b37564`, 2026-10-14; CI green on three architectures | C4: pending |
| PR-05 | **merged** | `0d95214`, 2026-10-15; CI green on three architectures | C5 (native half): pending |
| PR-06 | **merged** | `b4469fe`, 2026-10-17; CI green on three architectures | C6 (native half): pending; AC-005 with PR-09 |
| PR-07 | **merged** | `fa75d7a`, 2026-10-19; CI green on three architectures | C7: pending |
| PR-08 | **merged** | `7969acf`, 2026-10-19; CI green on three architectures | C8: pending |
| PR-09 | not started | | |
| PR-10 | not started | | |
| PR-11 | not started | | |
| PR-12 | not started | | |
| PR-13 | not started | | |
| PR-14 | not started | | |
