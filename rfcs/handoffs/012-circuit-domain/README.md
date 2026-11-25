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

PR-39, PR-40 and PR-41 are done (`bdebadb`; `e8bf591` to `bd571cd`; `91666f5`, 2026-11-19): several and composed worlds; recorded imports, the start hook, a domain's end request and the faulted-actor policy; units with dimensions, converted exactly and rounded once, and `quantity.dimension`.

PR-41's follow-up is done (`07687ed`): a `real` quantity may be negative (F-17), the Host's `quantity_si` checks a domain's attribute, a core unit on a `real` is `quantity.dimension`, and one unit symbol in two domains stops the controller.

**PR-42 is in review** (`b043712`, 2026-11-22): the comparison of Tier 2, the replay codes, the results file. Its design is accepted, including the three `Domain` methods (`tolerance`, `continuous_actor`, `numeric_fields`), which are F-5's change: only the domain knows its payloads.

Milestone M2 is complete (PR-35 to PR-43). **M3 has begun:** PR-44 is done (`c1ffd81`, `01b2296`, 2026-11-25): `florn-domain-circuit` with validation, the passive built-ins and sources, the DC operating point (MNA, dense LU, Newton with the diode's limiter), the initial solve and its stages.

**Next: PR-45, transient integration (V).** Read its row in `task-breakdown-pr-plan.md`, section 3.6 of `implementation-handoff.md` and B-45 first; it starts from RFC 013 rules 6, 7 and 10 to 14 and section 11. A verifier session follows its push.

**Its first commit, from PR-44's review: finding F-18 and three corrections.** F-18 is the only change to the core that M3 makes; it is in RFC 012 section 11, written before the code.

1. **F-18 (a): only an error of `Domain::validate` refuses a run.** A warning is kept, logged at start, and returned with the validation a front end asks for. Move `circuit.signal-unconnected` and `circuit.floating-node` back to `validate` from the start hook's `Action::Notify`. Test both: the run starts, and the warning is in the validation and in the log.
2. **F-18 (b): the start hook reads each guest actor's admitted module manifest**, read-only (its world, its declared channels). The shape is yours. With it:
   - `circuit.device-limit` is refused from the start hook, before any event, with a test of two device actors (B-49).
   - The domain keeps the channel layouts for a guest's `display_series` index; PR-47 uses it.
3. **A dimensionless consumer on a signal with a unit is `circuit.signal-dimension`**, and so is the reverse, with the new dimension `dimensionless` (RFC 012 rule 4, RFC 007 amended). Both catalogues get the text.
4. **The motor's pin is ASCII `a-`** (RFC 012 rule 23 amended, RFC 004's example corrected); U+2212 is not accepted.

The rest of PR-44's readings stand as they are; the review lists them. The two mutants that survived have checks in B-45 (the limiter's flag in convergence) and B-47 (a guest with two channels).

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
