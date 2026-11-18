# Phase 4 handoff package — circuit and control, release 0.4.0

Entry point for the implementation of RFC 012, 013 and 014 (as amended on 2026-10-31) and for the hardening that precedes them. Written 2026-10-31 on the owner's authorisation of the schedule. From Phase 4 the roles are held by separate sessions: the architect designs, plans and reviews; the development team implements from this package; a verifier session checks four units; the owner authorises and starts each job. The work is serial, on `main` alone, and the roles exchange files (OD-13; the rules and the authorisations are in the contributing chapter).

| Document | What it is |
|---|---|
| [`implementation-handoff.md`](./implementation-handoff.md) | What each area must do and when it is done; the answers to the questions the team put; the rules that hold in every unit |
| [`task-breakdown-pr-plan.md`](./task-breakdown-pr-plan.md) | Twenty-six units, PR-31 to PR-56, in the order they are done and grouped in milestones; the progress table |
| [`acceptance-qa-checklist.md`](./acceptance-qa-checklist.md) | The checks per unit, each beginning with the contract rules the unit touches; journey J7; the release reviews of 0.3.1 and 0.4.0 |
| [`verifier-brief.md`](./verifier-brief.md) | The brief for a verifier session, ready to paste, for the four units that have one |

## Start here: milestone M2, the core prepared (from PR-35)

Milestone M1 is done: PR-31 to PR-34a are merged and release 0.3.1 is published. This section is enough to start M2; for each unit from PR-37 on, read its row in `task-breakdown-pr-plan.md`, the matching section of `implementation-handoff.md` and its row in `acceptance-qa-checklist.md` (B-nn) before you write anything. Each unit begins its review request with the contract rules it touches, quoted from their own RFC. A unit is reviewed from its review request after the push; push with `git push upstream main`; stage files by name. M2 changes the core only where RFC 012 section 11 says so, each change with a test in the kit's test domain, before any circuit code exists; a change the core needs that the table lacks is a new finding to the architect.

| Unit | Do | Done when |
|---|---|---|
| PR-35 Tests out of the implementation files | **Done** (`049475a`, 2026-11-07): tests in `tests.rs` files, `ci/check-test-layout.sh` in the `docs` job; new tests are written in the file form, which the gate enforces | — |
| PR-36 Split the Host's run file | **Done** (`8fb341c`, `a6d3c9b`, 2026-11-09): `run.rs` is 590 lines and `run/` holds `start`, `activation`, `actions`, `end`, `checkpoint`. `panels.rs`, `controller.rs` and `message.rs` are split by the first unit that touches each heavily, in its first commit | — |
| PR-37 Floats and lists across the ABI | **Done** (`ceb6d49`, 2026-11-10): rows 12 to 16, `Ty::import_signature` and `Ty::export_signature`, the post-return required at admission and called by `florn_engine::call_lifting`; a result that holds a list or a string needs its post-return too | — |
| PR-38 The kernel's discard and the call-in handle (V) | **Done** (`c45a08c` with `887c1fd`, `25cea3b`, `f8c59ef`; verified 2026-11-18, accepted 2026-11-18): `Kernel::discard`, `CallIn` with the domain lent out, calls through `call_lifting` with the length bound, fault parameter 12019, the Host fault's reason from RFC 007's set | — |

PR-39 and PR-40 are done (`bdebadb`; `e8bf591` to `bd571cd`, 2026-11-19): several and composed worlds; imports that record effects, the start hook with refusal, a domain's end request, the end reasons `non-convergence` and `actor-fault`, the faulted-actor policy from the domains.

**Since your last unit** (PR-38's second follow-up, `f8c59ef`), read this first. Another session built the units in between; you continue from `e63c80f`.

- **PR-38 accepted after its verifier** (a fresh session that had not read the code: 38 checks, 20 planted defects, no defect found). Its checks are now `crates/florn-kernel/tests/compound_discard.rs` and `crates/florn-conformance/tests/compound_handle.rs`.
- **RFC 012 rules 19 and 20, amended:**
  - `send`, `set-timer` and `cancel-timer` in a device are all 12019.
  - The length bound is inclusive.
  - After a refused list result, only a fault closes the activation (12020); `commit`, `discard` and `call` are refused (`3ed5c80`).
- **The example guests of `crates/florn/tests/guest_sdk.rs` are built once per test binary** (`aff50a2`, the macOS failure), and `zip` is pinned at 9.0.5 for RUSTSEC-2026-0548 (`bc2fe2b`).
- **PR-39** (`bdebadb`):
  - `Domain::world` is replaced by `Domain::worlds`, and `Domains::with_composed` registers a world of several domains.
  - Every module's domains must be declared in the scenario, or the run fails with `domain.undeclared`.
  - A contract of record may be several WIT blocks joined by one blank line (RFC 012 rule 22).
  - `Controller::new` panics on a composition whose domains it holds but cannot build (`e8bf591`).
- **PR-40** (`ac81d92`, `497e9cc`, `bd571cd`):
  - A world's `records` imports are recorded as a `send` of eight canonical bytes.
  - `Domain::start_calling` is the start hook, and it may refuse the run.
  - A domain ends a run with `Action::EndRun` or `Domain::end_requested`, which the Host reads after every event.
  - The end reasons gain `non-convergence` and `actor-fault`; a `stop-run` fault now ends as `actor-fault`.
  - `Domain::faulted_policy` gives an actor's policy, the most severe of its domains' and the Host's.
- **A rule that changed for you:** a unit may now record its own **Done** notes in RFC 012 section 11, and the amendments to older RFCs that RFC 012's table lists, in that table's words (section 4 of `implementation-handoff.md`). Anything else in an RFC is still the architect's.

**Next: PR-41, units with dimensions.** Read its row in `task-breakdown-pr-plan.md`, section 3.4 of `implementation-handoff.md` and B-41 first; it starts from RFC 012 finding F-13 and rule 5, and RFC 004 rules 23 to 28. Read the code of PR-39 and PR-40 that it builds on (`Domain::units` and the config encoder) before you change it.

## What Phase 4 delivers

The roadmap's Phase 4, the second domain: a controller guest and a Host-side plant run in closed loop, and the results under both Hosts agree within the documented tolerance. The plant is a circuit solved by the Host (modified nodal analysis, Newton iteration, trapezoidal and Gear integration, an adaptive step on ticks); the controller is a guest that runs at sample instants and sees the plant through held signals; a device model is the second kind of guest, called by the solver inside a step; a detector turns a solver-found condition into an event. The application gains the waveform view, a schematic of the plant, inspectors for the solver, the learning lab `motor-lab` and journey J7.

The purpose beyond the feature is to test the core. Where the core could not carry the domain, RFC 012 section 11 records a finding against it, and the units of milestone M2 make those changes before any circuit code is written. A change the core needs that is not in that table is a new finding: stop, write it to the architect, and the RFC is amended first.

## The milestones and the order

Planned by evidence, not by date. Each ends with a report from the architect to the owner.

| M | Name | Units | Ends with |
|---|---|---|---|
| M0 | The package | (the architect's) | This package, the amended RFCs, external design revision 3 |
| M1 | Hardening, release 0.3.1 | PR-31 to PR-34 | A dispatched CI run green with a fuzz job that can fail; every recorded digest unchanged; 0.3.1 tagged by the owner's approval and published by the owner |
| M2 | The core prepared | PR-35 to PR-43 | The kit's test domain exercises every new mechanism natively, and what the engines do differently (floats, lists, the post-return, the budget across the calls of one activation) has an adapter case run in three browsers; the handle itself reaches the browsers with the circuit domain (PR-47, PR-49); all 27 shipped scenarios keep their digests; a dispatched run green. No circuit code yet |
| M3 | The closed loop, headless | PR-44 to PR-48 | `motor-loop` runs natively and, through the kit, in three browsers; the comparison is `matched` or `within-tolerance` on all five engines; `rc-step` is within its analytic bound; the cross-engine table is printed. **This is the roadmap's exit criterion** |
| M4 | The second kind of actor | PR-49 to PR-51 | `guest-diode` agrees with the built-in diode; `cps-loop` works; `comparator-latch` passes; the hostile scenarios fault the same way on every engine |
| M5 | The application | PR-52 to PR-54 | J7 and `motor-lab` pass in Chromium, Firefox and WebKit; the owner's hands-on pass at their own pace |
| M6 | Release 0.4.0 | PR-55, PR-56 | The release notes carry the evidence for each exit criterion and the cross-engine table; the owner approves and publishes |

The critical path is PR-37 (float call shapes and the post-return call) to PR-38 (the kernel's discard and the call-in handle) to PR-40 (the start hook and the effect-recording imports) to PR-46 (the on-demand advance) to PR-47 (the controller) to PR-48 (the cross-engine results). Everything else is placed in the order that serves it. A milestone does not wait for the owner's pending hands-on checks of earlier releases.

## How a unit goes

1. The owner starts the job. One unit is in flight at a time and nobody else pushes meanwhile.
2. Read the unit's row in the plan, the RFC rules it cites **in their own RFC**, and the code it touches. The first thing in the review request is the list of contract rules the unit touches, each quoted from its RFC and not from this package's summary of it.
3. Implement; the routine of the contributing chapter green locally; the digests of `scenarios/` unchanged unless the unit says why; a commit message under 400 characters; stage files by name.
4. Push (`git push upstream main`: the remote is named `upstream`), wait for CI. A red WebKit job on the macOS runner is re-run once (it has failed on documentation-only commits); a second failure, or any other red job, is reported. Write the review request, with the evidence, in the review-request folder of the team handover.
5. For the four units marked **V** in the plan a verifier session runs its checks, which it wrote before reading the implementation, and reports in a file beside the review request. The architect reviews the unit and, where there is one, the verifier's report, and records the outcome and the unit's state in the progress table.
6. A question, a defect found in the design, or a finding against the core is a file to the architect and not a decision in code.

## What stays out

Two or more guest device models in one scenario (RFC 012 finding F-8, deferred: the kernel keeps its one open activation); a reactive guest device (a later contract version); a sparse solver (RFC 013 rule 22, deferred until a plant exceeds the Host maximum); mechanical ports (PHY-007, later); graphical authoring of a plant (RFC 014 rule 16); the native shell (0.6); a text form of the control interface. Anything the owner did not decide is an open question in an RFC or in this package, never a quiet choice in code.
