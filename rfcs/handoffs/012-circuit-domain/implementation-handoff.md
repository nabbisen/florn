# Phase 4 implementation handoff

Entry point: [`README.md`](./README.md). This document says what each area must do for release 0.3.1 and release 0.4.0 and when it is done, and it answers the questions the team put on reading the RFCs. Rule numbers refer to RFC 012 unless another RFC is named. The RFCs are the authority: where this text and an RFC differ, the RFC wins and the difference is reported to the architect.

## 1. Summary

Release 0.3.0 finished the first domain. Phase 4 adds a second whose time is continuous, and it is built to find out what the core cannot carry. The rule of the phase: **the core changes only where RFC 012 section 11 says so**, in the units of milestone M2, each with a test in the kit's own test domain, before any circuit code exists. The circuit domain itself is a new crate that implements the `Domain` trait and nothing else of the Host; if a unit of M3 or later needs a change to the kernel, the ABI, an engine adapter or the control interface that M2 did not make, that is a new finding (stop, write to the architect). The kernel's time stays whole ticks and no floating-point value reaches an event key; the floating-point plant lives behind the domain.

## 2. Toolchain and dependencies

One dependency is added in the phase: the crate `libm` (licence MIT, on the allow-list of `deny.toml`), the newest 0.2 release on crates.io at the time of PR-44, **pinned exactly** (`=0.2.x`), the same pin in `florn-domain-circuit` and in the device SDK `florn-guest-device` (the guest's `Trial` helper carries the same `pnjlim` and must compute the same bits). RFC 013 names it; it is added in the change that needs it, with `cargo deny` green. No other dependency is named by this package: any other is a change request to the architect first, and the dense LU is written in the crate (about two hundred lines). The toolchain, `wasm-bindgen` and the test tools are those of the contributing chapter; nothing is installed beyond them.

## 3. The areas

### 3.1 Hardening (PR-31 to PR-34)

Gates that can fail, a parser that cannot be made to allocate without bound, a draft that keeps built-ins, the runner's exit codes by end reason, and the release 0.3.1 that ships them (PR-31 to PR-33 are independent of each other). The WebKit job of the browser kit has failed on documentation-only commits with a navigation timeout and with a crashed page; the opener retries once with a fresh context and **counts** what it retried, so that a flake is told from a regression and its rate is visible in the log.

Done when: each control fails when a violation is planted in a scratch copy and passes on the tree; no digest of `scenarios/` changed; the dispatched run of PR-34 is green with the fuzz job; the release notes of 0.3.1 say what changed and why.

### 3.2 The test layout and the Host's run file (PR-35, PR-36)

PR-35 moves the 23 inline test modules into `tests.rs` files, renames `shell/mod.rs` and adds `ci/check-test-layout.sh`; it changes no behaviour and no test. PR-36 splits `crates/florn-host/src/run.rs` by the areas the findings land in, behaviour preserved. In `florn-host` the type `Controller` is the control interface's handler and has nothing to do with the circuit domain's sampled controller: keep the name, and say "the control-interface Controller" or "the sampled controller" in reviews.

Done when: the test count before and after is the same (231 passed, 1 ignored at the time of writing) and every digest is unchanged; the gate fails on a planted inline module; no file of PR-36 is above 1,500 lines.

### 3.3 `florn-abi`, `florn-admission`, the engine adapters: floats, lists, post-return (PR-37)

RFC 012 findings F-1 and F-16. `f64` joins the flat types; the five shapes are new rows at the end of the table (rows are never renumbered) and the browser adapter gets the trampolines for them; `flat_signature` follows the canonical ABI for an export whose result does not fit one flat value (the export returns one pointer) and for an import (the return pointer is the last parameter), which today it applies to both. A list is lifted with its length checked **before** the lift (SEC-005). After the Host has lifted a list or string result of an export it calls the module's `cabi_post_<export>` once with the result pointer, inside the same activation and so under its budget; admission requires that export, of type `(i32) -> ()`, wherever an export returns a list or a string. Read RFC 002 rule 2 and the admission table of RFC 003 first.

Done when: the conformance case with floats and lists passes on Wasmtime and in the three browsers; a list-returning export called a thousand times leaves the guest's memory the size it was; a missing post-return is `module.export-missing`; a shape outside the table is `module.export-unsupported`.

### 3.4 `florn-kernel`, `florn-host`: discard, the handle, worlds, imports, the start hook (PR-38 to PR-41)

F-2, F-3, F-9, F-7, F-11, F-14, F-15, F-13. The kernel's one-open-activation assertion **stays** (F-8 is deferred); the new outcome `discard` records nothing, restores the stream and leaves the actor alive. The call-in handle is the one new way for a domain to run a guest: it is given while the domain handles an event or the start, it commits with a world that refuses sends (a device sends nothing), and it never outlives the call. The domain's request to end a run reaches the Host as an `Action` for what the domain handles and as a flag read after every commit for what `deliver` finds, because a delivery returns no action. Units are registered with their dimension and converted once. The kit's test domain, not a circuit, proves each of these.

Done when: every new mechanism has a test in the kit's test domain natively and in three browsers; all 27 shipped scenarios keep their recorded digests; PR-38 passes its verifier.

### 3.5 `florn-host`, `florn-control`, the runner: the comparison and what the application reads (PR-42, PR-43)

RFC 013 rules 20 and 21. The comparison of Tier 2 is a function of two runs' environments: the same florn version with a different Host, engine or architecture gets the tolerance; the same build is compared exactly. Read RFC 001 section 1.1 and rules 41 and 62 before writing it. The four `replay` codes and their two enumerations are in RFC 012 finding F-5; the sentences are in both languages. Additions to an answer of control version 1 are a trailing optional section (RFC 012 finding F-12): the format tests record the old and the new form, and an old front end ignores what it does not know.

Done when: the outcomes of the table in RFC 013 section 11 (Comparison) are produced by tests; the display kit reads `sample` records on both paths; a reader of a newer stream version refuses it before reading a record.

### 3.6 `florn-domain-circuit` (PR-44 to PR-46, PR-51)

A new crate under the five-crate burst of crates.io. It implements the `Domain` trait of RFC 008 rule 1 and takes its Host maxima (`max-unknowns`, `max-breakpoints`) at construction. The numbers of RFC 013 are normative: the formulas of rule 6, the start-up of rule 7, the Newton criteria and the limiter of rule 8 (the formula of `pnjlim` is written out there), the initial solve and its stages of rule 9, the estimate of rule 11 (the Gear 2 constant is `(1+ω)²/(ω(1+2ω))`), the limits of rule 13. The arithmetic rules of RFC 013 rule 19 hold from the first line: `libm` for every function IEEE does not round, no `mul_add`, no SIMD, the crate's own `fmin` and `fmax`, reductions in the fixed order of rule 5. The channel numbers of the built-ins are fixed (`probe` 1, `condition` 2) and frozen by a format test. `ci/check-deps.py` is extended in PR-44, with the test of the control, to list the new crates and to check external direct dependencies.

Done when: the verification tables of RFC 012 section 10 and RFC 013 section 11 pass for the rows the unit covers; `rc-step` is within `4·ltol·V` under both methods and the order test shows 3.8 to 4.2 at every halving; PR-45 passes its verifier.

### 3.7 The guests (PR-47, PR-49, PR-50)

One crate per world (`florn-guest-circuit`, `florn-guest-device`, `florn-guest-cps`); the examples `pid`, `guest-diode`, `bad-device`, `networked-pid`. The shipped modules and digests are regenerated and committed with the change (`cargo run --release -p florn-conformance -- scenarios`); even a formatting change to a guest changes its bytes.

Done when: the closed loop settles to the set point under both Hosts and through the kit in three browsers; a guest and a built-in diode agree within tolerance on the voltmeters' samples; each mode of `bad-device` faults with the code of its row.

### 3.8 `florn-ui-model`, `florn-ui-scene`, `florn-ui-web` (PR-52 to PR-54)

RFC 014. The view model and the scene are tested natively; the browser journeys prove the rest. The evidence of a unit that has pictures (the story's screenshots, the plots) goes in the unit's review-request folder. No part of the Iced, niri or xdotool guidance in the repository's GUI rules file applies to this browser application; the controls are the journeys, the draw budget of RFC 005 rule 40, the accessibility rules of the external design (section 3.11) and RFC 007 for the languages.

Done when: J7 passes in Chromium, Firefox and WebKit by `ci/browser/journeys.js`; the key sets of the two label files are equal; the draw budget holds as documented.

## 4. Rules that hold in every unit

- **Contract rules first.** The review request begins with the contract rules the unit touches, each quoted from its own RFC. The post-return call was missed by everyone who read the new RFCs and found by the team that read RFC 002.
- **Digests.** The 27 shipped scenarios keep their digests unless the unit says why and the review request shows the old and new value. A format or an encoding change needs a new version number and a new recorded value in the kit's format tests.
- **Dependencies.** A dependency this package or an RFC names is added in the change that needs it, permissive and exactly pinned, with `cargo deny` green. Any other dependency is a change request first.
- **File size.** A new file is at most 1,000 lines, tests included; a file a unit would take past that is split in the same change by the area it holds. Existing larger files are split by PR-36 or when a unit touches them heavily; no unit carries a repository-wide move.
- **Tests.** New unit tests go in `src/<mod>/tests.rs` from the first unit; tests validate the design specification and not the written code.
- **Findings and questions.** A defect in the design, a finding against the core that the table lacks, or a question is a file to the architect. A unit never changes an RFC, a contract, a file format, the roadmap or a dependency on its own.
- **Reviews.** The architect reviews each unit from its review request after the push (the work is serial, on `main` alone, OD-13). A red WebKit job on the macOS runner is re-run once and the flake recorded; any other red job is reported.
- **Public files cite no private path and no private finding.** The review requests live in a private folder; the repository's files say what a rule is and where it is written, not where the architect's working papers are.
