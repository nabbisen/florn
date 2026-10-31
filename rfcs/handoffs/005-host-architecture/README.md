# Phase 1 handoff package — Core on two engines (release 0.1.0)

| | |
|---|---|
| **Companion of** | RFC 005 — Host architecture, whose §9 fixes the crates this package builds. It implements RFC 001 to 007 together; the state of this package follows RFC 005 (RFC 000, companion handoffs). |
| **From** | Architect |
| **Date** | 2026-10-14 |
| **For** | The developer (implementation), the verifier (acceptance), the owner (progress and decisions) |
| **Phase** | Phase 1 of `ROADMAP.md`: kernel, admission, engine adapters for the browser and for Wasmtime, guest SDK for Rust, headless runner, replay from inputs, trace digest, conformance kit, one guest. Release 0.1.0. |

## Read in this order

| Reader | Start with | Then |
|---|---|---|
| Developer | [`implementation-handoff.md`](./implementation-handoff.md) — what each crate is, which rules it implements, what "done" means | [`task-breakdown-pr-plan.md`](./task-breakdown-pr-plan.md) — the order of pull requests |
| Verifier | [`acceptance-qa-checklist.md`](./acceptance-qa-checklist.md) — what to run for each acceptance criterion, and how to record it | The PR plan's "verifier checks" column |
| Owner | This file | The PR plan's progress table, updated with every merged PR |

The RFCs are the specification; this package does not restate them. Where this package and an RFC disagree, the RFC wins and the package is corrected.

## The phase's exit criteria (from the roadmap)

1. One guest module, unchanged, produces the same trace digest under both Hosts.
2. A hostile guest faults at the same point under both Hosts, and the Host and the other actors continue.
3. A run replays from its inputs with the same digest.

The acceptance criteria that demonstrate them are AC-001 to AC-007 of the requirements specification §22.1; the checklist turns each into a procedure.

## Rules of work in Phase 1

| Rule | Detail |
|---|---|
| One pull request per task of the plan, in the plan's order unless the plan says tasks are independent. | A PR names its task, the RFC rules it implements, and the tests it adds. |
| CI must be green before a PR is merged: the three architectures, the licence and advisory gate, the docs build. | `.github/workflows/ci.yml` |
| Every rule a PR implements has a test that would fail without it. | The conformance kit grows with every PR; the PR plan names the tests. |
| The verifier accepts a task by running the checklist's procedure for it and recording the result. | Results are recorded in the place the checklist names. |
| A contradiction or a defect found in an RFC is reported to the architect as a question, not redesigned in code. | The report cites the rule. The architect amends the RFC, and the amendment is the same commit as its reason. |
| No product code depends on anything the browser or the operating system provides outside the two adapter crates. | CI checks the dependency trees (RFC 005 rule 1). |
| Nothing is measured by hand that the kit can measure. | The crossing and cost benchmarks are kit tests with published figures. |

## What exists already

| From | What | Use |
|---|---|---|
| Stage 0 PoCs | A kernel with the RFC 001 order key, activation with recorded effects, SHA-256 trace chain; lowering of `list<u8>`; two engine adapters; an admission step with the fvm passes in the right order, memory cap preserving custom sections, the state-export pass; checkpoint export and restore; a Playwright harness for Chromium and Firefox | Reference for shape and for the numbers. The PoC code is private and is not copied as is: the product's crates are written against the RFCs, and a PoC's trick is kept only when the RFC says so. |
| RFC 002 §4 | The `florn:core@0.1.0` WIT | Copied verbatim to `contracts/florn-core/wit/`; a test keeps them equal |
| RFC 003 rule 19 | Provisional defaults and maxima | The Host's constants until Phase 1 measures |
| RFC 005 §9 | The crate list and the dependency direction | The workspace of PR-01 |
| RFC 007 §4 | The catalogue of codes | The `Code` enumeration of `florn-control` |
| CI | Three architectures, licence gate, docs | Extended with the browser matrix in PR-12 |

## Where things go

| What | Where |
|---|---|
| Code | `crates/<name>/` per RFC 005 rule 42 |
| Contracts | `contracts/florn-core/wit/` |
| Example scenarios with recorded digests | `scenarios/` |
| Documentation for module authors and automation users | `docs/src/` (new chapters in PR-14) |
| The verifier's acceptance records | The verifier's private records, one folder per task (not in the repository, as in Stage 0) |
| Questions to the architect | A file to the architect, one folder per task in the private review-request area, with the RFC rule cited |
