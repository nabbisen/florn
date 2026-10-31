# florn Roadmap

**Status.** Authorised by the owner on 2026-10-13.
**Last updated.** 2026-10-31

florn is planned by milestones, not by dates. A release is made when its exit criteria are met.

## Principles

1. **Decisions that are expensive to reverse come first.** Contracts, time, and ordering are settled before code depends on them.
2. **Claims are proven before they are built on.** A proof of concept answers one question with a measurement.
3. **Two engines from the first day.** Every guest runs under the browser Host and under the native Host, and must produce the same trace.
4. **One domain at a time.** A domain is added after the core has been proven by the one before it.
5. **Every phase ends with something that can be shown.**

## Overview

| Stage | Name | Result | Release |
|---|---|---|---|
| Stage 0 | Foundation | Decisions, proofs of concept, foundational RFCs, revised specifications. No product code. | — |
| Phase 1 | Core on two engines | Deterministic kernel, module admission, guest SDK, headless runner. One guest with an identical trace on both engines. **Released 2026-10-20.** | 0.1.0 |
| Phase 2 | Network learning lab | Network domain, first protocol guests, browser UI in two languages. **Released 2026-10-29.** | 0.2.0 |
| Phase 3 | Depth | Checkpoints, backward navigation, branching, import of user modules, a second protocol family. **Released 2026-10-31.** | 0.3.0 |
| Phase 4 | Circuit and control | Second domain. Tests whether the core is neutral. | 0.4.0 |
| Phase 5 | Finance | Third domain. Tests the core under high event volume. | 0.5.0 |
| Phase 6 | Native application | Native GUI on the native Host. | 0.6.0 |
| — | Stable contracts | Compatibility policy in force. | 1.0.0 |

## Stage 0 — Foundation

| ID | Item | Depends on | State |
|---|---|---|---|
| S0-01 | Owner decisions | — | Done 2026-09-30 |
| S0-02 | RFC 001 — Determinism contract | S0-01 | Accepted 2026-10-13 |
| S0-03 | POC-1 — One artifact, two engines | S0-01 | Reported 2026-10-13: Go for AD-01. WebKit and AArch64 wait for S0-19. |
| S0-04 | POC-2 — Deterministic limits and containment | S0-01 | Reported 2026-10-13: Go for AD-03. Measured metering cost 2–4× accepted by the owner for now; the optimisation is planned in Phase 3 (see "Metering cost" below). |
| S0-05 | POC-3 — Cost of a boundary crossing | S0-03 | Reported 2026-10-13: 0.6–1.9 µs per activation at 64 bytes across Wasmtime, Chromium, Firefox; one call per item is affordable. The trace encoding must hash payloads once (decided in RFC 005). |
| S0-06 | POC-4 — Instance scaling in the browser | S0-01 | Reported 2026-10-13: 100 instances in 2 ms; the ceiling is 124 instances per page in Chromium and 999 in Firefox, independent of guest settings. The browser scale is restated as at least 100 resident actors; more by swapping actor state (Phase 3). |
| S0-07 | POC-7 — UI shell | S0-01 | Reported 2026-10-13: Go. Leptos confirmed for the browser shell; view model and scene are framework-free with native tests; 500 nodes drawn in 1–3 ms per frame; language switch, keyboard operation, and offline start pass. Input-method check done 2026-10-14 (fcitx5 + mozc, both browsers). |
| S0-08 | RFC 002 — Guest contract | S0-02, S0-03, S0-05 | Accepted 2026-10-14 |
| S0-09 | RFC 003 — Module admission and resource limits | S0-02, S0-04 | Accepted 2026-10-14. Specifies the versioned cost model and restates the metering-cost plan. |
| S0-10 | RFC 004 — Core model: graph, scenario manifest, package | S0-02 | Accepted 2026-10-13 |
| S0-11 | RFC 005 — Host architecture and control interface | S0-02, S0-07 | Accepted 2026-10-14 |
| S0-12 | RFC 006 — Security baseline for the browser Host | S0-09 | Accepted 2026-10-14. Copyleft licences refused outright. |
| S0-13 | RFC 007 — Error model and internationalisation | S0-11 | Accepted 2026-10-14. Unit symbols are not localised. |
| S0-14 | POC-5 — Telemetry transport and isolation headers | S0-07 | Reported 2026-10-14: Go. Shared memory under the isolation headers in both browsers; the message fallback delivers every record at 2,000 per frame with no measurable cost; offline start keeps isolation; no request leaves the origin. Hosting requirement: COOP same-origin, COEP require-corp, the policy, versioned paths. |
| S0-15 | POC-6 — Snapshot and restore | S0-04 report | Reported 2026-10-14: Go. Memory, globals, and tables export and restore into a fresh instance with an identical trace under Wasmtime, Chromium, and Firefox, across a restart and backward in time; 0.04–0.3 ms per MB; the pass costs nothing measurable. Swapping measured at 50–60 µs per actor at 131 KB of state. |
| S0-16 | Requirements, revised: priorities, one statement per obligation | S0-01 | Accepted 2026-10-13, `docs/src/requirements.md` |
| S0-17 | External design, revised: personas, UI, control interface, glossary | S0-11, S0-16 | Accepted 2026-10-14, `docs/src/external-design.md` (revision 2) |
| S0-18 | Repository skeleton: `LICENSE`, `NOTICE`, `README.md`, `CHANGELOG.md`, workspace | S0-11 | Files in place 2026-10-13. The Cargo workspace is laid out with RFC 005, when the first crate exists. |
| S0-19 | Hosted repository and continuous integration, with a macOS runner for Safari and an AArch64 runner | Owner | Done 2026-10-14: `github.com/nabbisen/florn`, GitHub Actions. First run green on x86-64 Linux, AArch64 Linux, and Apple Silicon macOS (`.github/workflows/ci.yml`): check and test on the three architectures, the licence and advisory gate of RFC 006, the book build, the RFC index check. Safari's WebKit joins the matrix with the conformance kit in Phase 1; the Stage 0 PoCs are private and are not re-run in CI, the kit covers their cases. |

### Exit criteria

1. POC-1 to POC-4 and POC-7 have reports.
2. RFC 001 to RFC 005 are accepted.
3. Requirements and external design are revised and published under `docs/`.
4. The repository has `LICENSE`, `NOTICE`, `README.md`, `CHANGELOG.md`, `docs/`, a Cargo workspace, and CI.
5. No critical finding of the startup review is open.

**Exit review, 2026-10-14.** Criteria 1, 2, 3, and 5 are met; 1 and 2 are exceeded (seven PoCs, seven RFCs). Criterion 4 was open on two items; CI exists since 2026-10-14 (S0-19), and the Cargo workspace is the first commit of Phase 1, laid out per RFC 005. Phase 1 may start; its handoff package comes first.

## Phase 1 — Core on two engines (0.1.0)

| | |
|---|---|
| **Scope** | Kernel: time, events, ordering, activation. Module admission with limits. Engine adapters for the browser and for Wasmtime. Guest SDK for Rust. Headless runner. Replay from inputs. Trace digest. Conformance kit. One guest. |
| **Not in scope** | Any domain. Any UI beyond a test page. Checkpoints. |
| **Exit criteria** | One guest module, unchanged, produces the same trace digest under both Hosts. A hostile guest faults at the same point under both Hosts, and the Host and the other actors continue. A run replays from its inputs with the same digest. |
| **RFCs** | 001 to 007 |
| **Handoff package** | `rfcs/handoffs/005-host-architecture/` (2026-10-14): implementation handoff, fourteen pull requests in order, acceptance checklist for the verifier. PR-01 is the Cargo workspace. |

## Phase 2 — Network learning lab (0.2.0)

| | |
|---|---|
| **Scope** | Network domain contract and link model. Guests in a ladder: echo, a distance-vector protocol, an OSPF single-area subset. Browser UI: topology, run controls, timeline, packet inspector, breaking a link during a run, save, load, replay. English and Japanese. Hosting with versioned paths. |
| **Not in scope** | BGP. Checkpoints. The native GUI. |
| **Exit criteria** | The first journeys work and are accepted by the owner: run a bundled example; break a link and follow the reconvergence; save, reload, and replay the same run. The application makes no request to any other origin. |
| **RFCs** | 008 (accepted 2026-10-24); the 0.2 rules of RFC 005. The UI's external design is `docs/src/external-design.md`. |
| **Handoff package** | `rfcs/handoffs/008-network-domain/` (2026-10-24): PR-15 to PR-22. |
| **Released** | 0.2.0 on 2026-10-29; the evidence in `docs/src/releases/0.2.0.md`. |

## Phase 3 — Depth (0.3.0)

| | |
|---|---|
| **Scope** | Checkpoints, backward navigation, branching. Import of user modules through the UI. BGP guest. Usability test with people who did not build florn. **Metering optimisation:** florn's own metering pass, released as cost-model version 2 (see "Metering cost" below). **Actor swapping in the browser:** actors beyond the browser's instance ceiling are held as exported state and re-instantiated on activation. |
| **Exit criteria** | A run restored from a checkpoint continues with the same digest as an uninterrupted run. A user imports a module and runs it in a topology. The cost benchmark shows at most 1.5× on the protocol-style guest and 2× on the compute-style guest under cost-model version 2, or the owner has decided again on the measured figures. Runs recorded under version 1 still replay. |
| **RFCs** | 009, 010, 011 (accepted 2026-10-29); the 0.3 rules of RFC 001, 003 and 005. |
| **Handoff package** | `rfcs/handoffs/009-depth/` (2026-10-29): PR-23 to PR-30. |
| **Released** | 0.3.0 on 2026-10-31: every criterion has its evidence in `docs/src/releases/0.3.0.md` (the kit's checkpoint tests across the two Hosts; journey J5; the cost figures of both versions, the owner's decision of 2026-10-29 on the one engine over the compute-style target; a 0.1.0 and a 0.2.0 record replayed). The usability test of the handoff's checklist runs at the owner's pace; its findings become issues for 0.3.x or Phase 4. **Accepted by the owner on 2026-10-31.** |

## Phase 4 — Circuit and control (0.4.0)

| | |
|---|---|
| **Scope** | Circuit domain contract: device models and sampled controllers as separate kinds. Nodal analysis with nonlinear iteration and transient integration. One controller guest against one plant. Waveform view. |
| **Purpose beyond the feature** | To test the core. Any change the core needs for this domain is a finding against the core. |
| **Exit criteria** | A controller guest and a Host-side plant run in closed loop. Results under both Hosts agree within the documented tolerance. |
| **RFCs** | 012, 013, 014 (accepted 2026-10-31 and amended the same day on the owner's decisions D1, D3 and D4, AD-25). |
| **Handoff package** | `rfcs/handoffs/012-circuit-domain/` (2026-10-31): PR-31 to PR-56, in the milestones below. |
| **Milestones** | Planned by evidence, not by date; each ends with a report to the owner. **M0** the package: amended RFCs, external design revision 3, the handoff package. **M1** hardening and the patch release 0.3.1: gates that can fail, a bounded parser, the runner's exit codes, the authoring draft that keeps built-ins. **M2** the core prepared: the test layout, the split of the Host's run file, float call shapes and the post-return call, the kernel's discard and the call-in handle, several and composed worlds, effect-recording imports and the start hook, units with dimensions, the comparison outcomes, the display kind and the inspect query; no circuit code. **M3** the closed loop, headless: the domain, the integration, the on-demand advance, the controller contract and the cross-engine results; this is the exit criterion. **M4** the second kind of actor: the device contract, the composed world, the detector. **M5** the application: series and plots, the schematic and the inspectors, the lab and journey J7. **M6** release 0.4.0. |
| **Verification** | A development-team session in the verifier role, never the implementer of the unit, checks four units: the kernel's discard (PR-38), the comparison (PR-42), the integration and its goldens (PR-45) and the cross-engine results with their controls (PR-48). In every other unit the controls are the developer's tests, CI and the architect's review. |

## Phase 5 — Finance (0.5.0)

| | |
|---|---|
| **Scope** | Market domain contract. Order book with price–time priority. Latency model. Seeded exogenous events. Order book and execution views. |
| **Exit criteria** | Several agent guests trade against the matching engine. The trace is identical under both Hosts. |

## Phase 6 — Native application (0.6.0)

| | |
|---|---|
| **Scope** | Native GUI on the native Host. Workloads beyond the browser's limits. **Scheduled first, before any implementation:** the native shell is chosen by an evaluation and a one-screen spike, as the browser shell was. The control interface's text form on standard streams is implemented with the native shell's in-process transport. |
| **Exit criteria** | The scenarios of every earlier phase run unchanged in the native application. |

## Metering cost — plan recorded 2026-10-13

POC-2 measured the cost of deterministic metering at 2–4× on guest execution, against the 30% that was hoped for. No library and no engine-native mechanism does materially better. The owner accepted the measured cost for the first releases on condition that the optimisation is planned and cannot be missed. The plan:

| Step | When | What |
|---|---|---|
| 1 | RFC 003, Stage 0 | The admission step and a versioned cost model are specified. The cost-model version and its table are part of the run identity. |
| 2 | Phase 1, 0.1.0 | The conformance kit carries a cost benchmark (protocol-style and compute-style guests, metered against unmetered, on every supported engine). Its figures appear in the notes of every minor release. |
| 3 | Phase 3, 0.3.0 | florn's own metering pass, charging per function entry and loop back-edge, replaces the library pass as cost-model version 2. Target: at most 1.5× protocol-style, 2× compute-style. Earlier runs replay under their recorded version. **Implemented 2026-10-29 (PR-24)**; the figures of every engine decide step 4. |
| 4 | Phase 3 review | If the target is missed, the owner decides again with the measured figures. **Decided 2026-10-29**: met on every engine but Chromium's compute-style guest (2.99×, down from 8.31×); version 2 is the default everywhere. |

## 1.0.0 — Stable contracts

Guest contracts, the scenario format, and the Host control interface are declared stable. The compatibility policy takes effect.

## Staffing

| Role | Sessions | From | Trigger to add |
|---|---|---|---|
| Developer — runtime | 1 | Stage 0 | — |
| Developer — UI | +1 | Stage 0, at POC-7 | When the runtime developer has reported POC-1 and POC-2, or earlier if the owner wants the UI answer sooner |
| Verifier | +1 | Phase 1, from its first handoff | Phase 1 start |
| Developer — domain | +1 | Phase 4 | If the circuit domain and the UI are to proceed in parallel; otherwise the runtime developer takes it |

The team grows only when two lanes of work are independent and both are ready. Every added session adds handoffs and reviews that pass through the owner.

Phases 1 to 3 were implemented by the architect on the owner's instruction of 2026-10-13, with the roles above held by one session. From Phase 4 (2026-10-31) the roles are held by separate sessions: the architect designs, plans and reviews; the development team implements and tests from the handoff packages; the owner authorises. The rules between the roles and the authorisations are in `docs/src/contributing.md`.

## Release cycle

| Topic | Rule |
|---|---|
| Versioning | Semantic versioning. Before 1.0.0, a minor version may change a contract. |
| Tags | No prefix: `0.1.0`. |
| Cadence | A release is made when the exit criteria of its phase are met. There are no time-boxed releases. Patch releases correct defects: only one that accepts hostile input unsafely, gives a wrong result or digest, or loses data, found by the owner's checks, the usability test or the team; the architect reports it with a recommendation and the owner authorises. Release 0.3.1 (authorised 2026-10-31) corrects the parser that a hostile scenario can make allocate without bound and makes the gates able to fail. |
| Change record | `CHANGELOG.md`. Implemented RFCs move to `rfcs/done/` with the release in their status. |

### Release checklist

1. Acceptance and conformance tests pass under both Hosts.
2. Security review is done. The threat model is updated when the release adds a data flow, an external integration, or a capability.
3. Documentation matches the released behaviour.
4. Dependency licences and advisories are checked.
5. `CHANGELOG.md` has an entry.
6. The RFC index is current.

## Not on this roadmap

| Topic | State |
|---|---|
| Distribution of one run across several browsers | Not planned |
| Multiple users, roles, shared workspaces | Deferred to a separate RFC for managed deployments |
| Module and scenario registries | Deferred |
| Connection to real networks and hardware | After Phase 6, by a separate RFC |
| Composition of several domains in one scenario | The core must not preclude it. No feature is planned before 1.0.0. |

## Changing this roadmap

The architect proposes a change. The owner authorises it. The change is made in this file in the same commit as its reason.
