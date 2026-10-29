# florn RFCs

Design decisions for florn are recorded as RFCs. An RFC states **what was decided and why**;
it stays in this directory permanently, including after the work ships.

The lifecycle, folder layout, naming, and numbering rules are defined in
[RFC 000 — RFC lifecycle policy](./done/000-rfc-lifecycle-policy.md).

## Layout

florn uses the **5-folder variant** of RFC 000, because design and implementation are
separate roles in this project.

| Folder | State | Meaning |
|---|---|---|
| `draft/` | Draft | Optional. Still being written. |
| `proposed/` | Proposed | Open for review. Do not implement yet. |
| `accepted/` | Accepted | Review complete. Implementation may start. |
| `done/` | Implemented | Shipped. Kept as the historical record. |
| `archive/` | Withdrawn / Superseded | Not pursued, or replaced. Kept as a record. |
| `handoffs/NNN-slug/` | — | Optional implementation companions. State is inherited from the RFC. |

The folder is the source of truth for an RFC's state. A folder is created when the first
RFC enters it.

## Conventions

- Files are named `NNN-slug.md`. New RFCs are numbered from `001`. Numbers are never
  reused or renumbered.
- The `Status` field inside an RFC mirrors its folder and is updated in the same commit
  that moves the file.
- Release tags have no `v` prefix, so an implemented RFC reads `Implemented (0.1.0)`.
- This index is updated in the same commit that adds or moves an RFC.
- Numbers 001 to 007 are the foundational RFCs named in `ROADMAP.md`; later numbers are taken in order.

## Index

### Proposed

| ID | Title | Proposed | For |
|----|-------|----------|-----|
| 009 | [Checkpoints, backward navigation, branching, actor swapping, and the application's depth](./proposed/009-checkpoints-navigation-branching.md) | 2026-10-29 | Release 0.3 |
| 010 | [Cost model version 2: florn's own metering pass](./proposed/010-cost-model-version-2.md) | 2026-10-29 | Release 0.3 |
| 011 | [The BGP subset and its learning lab](./proposed/011-bgp-subset.md) | 2026-10-29 | Release 0.3 |

### Accepted

_None at present._

### Implemented

| ID | Title | Shipped in |
|----|-------|------------|
| 000 | [RFC lifecycle policy](./done/000-rfc-lifecycle-policy.md) | Project start |
| 001 | [Determinism contract](./done/001-determinism-contract.md) | 0.1.0 (accepted 2026-10-13) |
| 002 | [Guest contract: artifact, ABI, activation, layering](./done/002-guest-contract.md) | 0.1.0 (accepted 2026-10-14) |
| 003 | [Module admission and resource limits](./done/003-module-admission-and-limits.md) | 0.1.0 (accepted 2026-10-14) |
| 004 | [Core model: graph, scenario manifest, package](./done/004-core-model.md) | 0.1.0 (accepted 2026-10-13) |
| 005 | [Host architecture: one core, adapters, control interface, repository layout](./done/005-host-architecture.md) | 0.1.0 (accepted 2026-10-14) |
| 006 | [Security baseline for the browser Host](./done/006-security-baseline-browser-host.md) | 0.1.0 (accepted 2026-10-14) |
| 007 | [Error model and internationalisation](./done/007-error-model-and-internationalisation.md) | 0.1.0 (accepted 2026-10-14) |
| 008 | [Network domain: how a domain enters the Host, links, frames, the router contract, the guest ladder](./done/008-network-domain.md) | 0.2.0 (accepted 2026-10-24) |

### Archive

_None yet._

### Handoffs

| Companion of | Package | Date |
|---|---|---|
| 005 | [Phase 1 handoff package — core on two engines, release 0.1.0](./handoffs/005-host-architecture/README.md): implementation handoff, task breakdown and PR plan, acceptance and QA checklist; implements RFC 001 to 007 | 2026-10-14 |
| 008 | [Phase 2 handoff package — network learning lab, release 0.2.0](./handoffs/008-network-domain/README.md): implementation handoff, task breakdown and PR plan, acceptance and QA checklist; implements RFC 008 and the 0.2 rules of RFC 005 | 2026-10-24 |
