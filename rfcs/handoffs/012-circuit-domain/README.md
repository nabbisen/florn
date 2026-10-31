# Phase 4 handoff package — circuit and control, release 0.4.0

Entry point for the implementation of RFC 012, 013 and 014 (as amended on 2026-10-31) and for the hardening that precedes them. Written 2026-10-31 on the owner's authorisation of the schedule. From Phase 4 the roles are held by separate sessions: the architect designs, plans and reviews; the development team implements from this package; a verifier session checks four units; the owner authorises and starts each job. The work is serial, on `main` alone, and the roles exchange files (OD-13; the rules and the authorisations are in the contributing chapter).

| Document | What it is |
|---|---|
| [`implementation-handoff.md`](./implementation-handoff.md) | What each area must do and when it is done; the answers to the questions the team put; the rules that hold in every unit |
| [`task-breakdown-pr-plan.md`](./task-breakdown-pr-plan.md) | Twenty-six units, PR-31 to PR-56, in the order they are done and grouped in milestones; the progress table |
| [`acceptance-qa-checklist.md`](./acceptance-qa-checklist.md) | The checks per unit, each beginning with the contract rules the unit touches; journey J7; the release reviews of 0.3.1 and 0.4.0 |
| [`verifier-brief.md`](./verifier-brief.md) | The brief for a verifier session, ready to paste, for the four units that have one |

## Start here: release 0.3.1 (PR-31 to PR-34)

This section is enough to do the first milestone; the other documents matter from PR-35. PR-31 to PR-33 are independent of each other; PR-34 follows them. None needs a verifier. Each unit begins its review request with the contract rules it touches, quoted from their RFC (RFC 000; RFC 004 rules 23 to 25; RFC 005 rule 31; RFC 006 rule 7; RFC 007 rule 17; RFC 008 rule 17).

| Unit | Do | Done when |
|---|---|---|
| PR-31 Gates that can fail | `ci/fuzz.sh` exits non-zero when a target crashes (today it pipes to `tail` and exits 0), uploads crash artifacts in CI, caches the corpus. `ci/check-rfc-index.sh` fails for an indexed file that is missing as well as for one not indexed. New `ci/check-rfc-invariants.sh`: status line equals folder, relative links resolve. The browser kit's page opener retries a navigation once with a fresh context on a timeout or a crashed page, counts and prints the retries, and the offline-start check stops spending two navigations of two minutes. Key-set test of `locales/en.json` and `locales/ja.json`. Untrack and ignore `ci/browser/*-many.florn-run`; the journeys write to a temporary directory. Remove the two private paths from the Phase 1 handoff package | Each control fails when a violation is planted in a scratch copy (a fake `cargo` exiting 77; an index line without a file; a doctored status line; a missing locale key) and passes on the tree; no digest changed |
| PR-32 Parser bound | A quantity with an enormous exponent (`end = "1e2000000000s"`) makes the runner allocate gigabytes. Bound `Decimal::shortest` and the exponent; refuse with `quantity.out-of-range` naming the field; add a dictionary of quantity shapes to the manifest fuzz target | The scenario above is refused under a 1 GB limit, as a regression test; the fuzz target runs 60 seconds clean |
| PR-33 Three small defects | The authoring draft keeps built-in actors (`Draft::from_scenario` skips them and `to_toml` writes `kind = "module"` only). The BGP lab's `route` layout is accepted in both forms and its log is asserted empty. The runner's exit code follows the end reason, also with `--to` | A draft with a built-in reads back to the same scenario hash; the BGP lab starts with an empty log; a faulted run stopped by `--to` exits 3 |
| PR-34 Release 0.3.1 | Version in the workspace and crates; `CHANGELOG.md` entry; release notes saying what changed and why; security review of the parser change; a dispatched CI run with the fuzz job. The owner approves the tag and publishes the crates | Every recorded digest unchanged; the dispatched run green with the fuzz job |

A red WebKit job on the macOS runner is re-run once and recorded in the review request (it has failed on documentation-only commits); any other red job is reported.

## What Phase 4 delivers

The roadmap's Phase 4, the second domain: a controller guest and a Host-side plant run in closed loop, and the results under both Hosts agree within the documented tolerance. The plant is a circuit solved by the Host (modified nodal analysis, Newton iteration, trapezoidal and Gear integration, an adaptive step on ticks); the controller is a guest that runs at sample instants and sees the plant through held signals; a device model is the second kind of guest, called by the solver inside a step; a detector turns a solver-found condition into an event. The application gains the waveform view, a schematic of the plant, inspectors for the solver, the learning lab `motor-lab` and journey J7.

The purpose beyond the feature is to test the core. Where the core could not carry the domain, RFC 012 section 11 records a finding against it, and the units of milestone M2 make those changes before any circuit code is written. A change the core needs that is not in that table is a new finding: stop, write it to the architect, and the RFC is amended first.

## The milestones and the order

Planned by evidence, not by date. Each ends with a report from the architect to the owner.

| M | Name | Units | Ends with |
|---|---|---|---|
| M0 | The package | (the architect's) | This package, the amended RFCs, external design revision 3 |
| M1 | Hardening, release 0.3.1 | PR-31 to PR-34 | A dispatched CI run green with a fuzz job that can fail; every recorded digest unchanged; 0.3.1 tagged by the owner's approval and published by the owner |
| M2 | The core prepared | PR-35 to PR-43 | The kit's test domain exercises every new mechanism natively and in three browsers; all 27 shipped scenarios keep their digests; a dispatched run green. No circuit code yet |
| M3 | The closed loop, headless | PR-44 to PR-48 | `motor-loop` runs natively and, through the kit, in three browsers; the comparison is `matched` or `within-tolerance` on all five engines; `rc-step` is within its analytic bound; the cross-engine table is printed. **This is the roadmap's exit criterion** |
| M4 | The second kind of actor | PR-49 to PR-51 | `guest-diode` agrees with the built-in diode; `cps-loop` works; `comparator-latch` passes; the hostile scenarios fault the same way on every engine |
| M5 | The application | PR-52 to PR-54 | J7 and `motor-lab` pass in Chromium, Firefox and WebKit; the owner's hands-on pass at their own pace |
| M6 | Release 0.4.0 | PR-55, PR-56 | The release notes carry the evidence for each exit criterion and the cross-engine table; the owner approves and publishes |

The critical path is PR-37 (float call shapes and the post-return call) to PR-38 (the kernel's discard and the call-in handle) to PR-40 (the start hook and the effect-recording imports) to PR-46 (the on-demand advance) to PR-47 (the controller) to PR-48 (the cross-engine results). Everything else is placed in the order that serves it. A milestone does not wait for the owner's pending hands-on checks of earlier releases.

## How a unit goes

1. The owner starts the job. One unit is in flight at a time and nobody else pushes meanwhile.
2. Read the unit's row in the plan, the RFC rules it cites **in their own RFC**, and the code it touches. The first thing in the review request is the list of contract rules the unit touches, each quoted from its RFC and not from this package's summary of it.
3. Implement; the routine of the contributing chapter green locally; the digests of `scenarios/` unchanged unless the unit says why; a commit message under 400 characters; stage files by name.
4. Push, wait for CI. A red WebKit job on the macOS runner is re-run once (it has failed on documentation-only commits); a second failure, or any other red job, is reported. Write the review request, with the evidence, in the review-request folder of the team handover.
5. For the four units marked **V** in the plan a verifier session runs its checks, which it wrote before reading the implementation, and reports in a file beside the review request. The architect reviews the unit and, where there is one, the verifier's report, and records the outcome and the unit's state in the progress table.
6. A question, a defect found in the design, or a finding against the core is a file to the architect and not a decision in code.

## What stays out

Two or more guest device models in one scenario (RFC 012 finding F-8, deferred: the kernel keeps its one open activation); a reactive guest device (a later contract version); a sparse solver (RFC 013 rule 22, deferred until a plant exceeds the Host maximum); mechanical ports (PHY-007, later); graphical authoring of a plant (RFC 014 rule 16); the native shell (0.6); a text form of the control interface. Anything the owner did not decide is an open question in an RFC or in this package, never a quiet choice in code.
