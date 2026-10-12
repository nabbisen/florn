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
- Numbers 002, 003, 005, 006, and 007 are reserved by `ROADMAP.md` for the foundational RFCs named there, so that documents can refer to them before they are written.

## Index

### Proposed

_None at present._

### Accepted

| ID | Title | Accepted | Implemented by |
|----|-------|----------|----------------|
| 001 | [Determinism contract](./accepted/001-determinism-contract.md) | 2026-10-13 | Release 0.1 |
| 004 | [Core model: graph, scenario manifest, package](./accepted/004-core-model.md) | 2026-10-13 | Release 0.1 |

### Implemented

| ID | Title | Shipped in |
|----|-------|------------|
| 000 | [RFC lifecycle policy](./done/000-rfc-lifecycle-policy.md) | Project start |

### Archive

_None yet._
