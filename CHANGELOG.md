# Changelog

All notable changes to florn are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow semantic versioning; tags carry no `v` prefix.

## [Unreleased]

### Added

- RFC 000, the RFC lifecycle policy (5-folder variant).
- RFC 001, the determinism contract. Accepted 2026-10-13.
- RFC 004, the core model: graph, scenario manifest, package. Accepted 2026-10-13.
- RFC 002, the guest contract: artifact, ABI, activation, layering. Accepted 2026-10-14.
- RFC 003, module admission and resource limits, with the versioned cost model and the metering-cost plan. Accepted 2026-10-14.
- RFC 005, the Host architecture: one core, adapters, control interface, repository layout. Accepted 2026-10-14; amended the same day with the telemetry-transport and snapshot-and-restore results.
- RFC 006, the security baseline for the browser Host. Accepted 2026-10-14. Copyleft licences refused outright.
- RFC 007, the error model and internationalisation. Accepted 2026-10-14.
- Requirements specification, revision 3, under `docs/`. Accepted 2026-10-13.
- External design, revision 2, under `docs/`. Accepted 2026-10-14.
- Roadmap. Authorised 2026-10-13; Stage 0 results recorded through 2026-10-14.
- Repository skeleton: licence, notice, readme, this changelog.
- Phase 1, PR-01: the Cargo workspace of RFC 005 with eighteen empty crates, the pinned toolchain, the contract of record `contracts/florn-core/wit/core.wit` with the test that keeps it equal to RFC 002, and the dependency-direction check in CI.
- Phase 1, PR-02: `florn-model`, the core model of RFC 004: manifest parsing with every validation code reported in one pass with typed parameters, quantities converted exactly to ticks and bytes, the canonical encoding and the scenario hash, the package reader and writer. RFC 007 section 4.1 gains four codes.
- Phase 1, PR-03: `florn-kernel`, the logical world of RFC 001: ticks and instants, the event key with both tie policies, the zero-delay limits, seeded streams over an in-crate ChaCha8 derived with BLAKE3, activations that commit in order or discard everything, timers with replace and cancel, interventions as recorded and replayed inputs, trace records with payload digests, the BLAKE3 chain, run comparison, and the advance units.
- Phase 1, PR-04: `florn-abi`, the canonical ABI lifting and lowering of the type subset of RFC 002, shared by both engine adapters: layout and flat representation, the sixteen-flat spill, strict lifting that produces the contract fault, canonical NaN, and the core world's signatures checked against the contract of record.
- Phase 1, PR-05: `florn-engine`, the engine adapter trait of RFC 005: compile with a cache by bytes, instantiate with the world's imports and the Host-owned globals, typed cached calls with an import handler, memory views, globals and tables for state export, trap kinds; and the adapter conformance cases over a hand-written guest of the core world. `florn-engine-wasmtime`, the Wasmtime adapter with the deterministic settings, store limits, and the epoch as the Host's watchdog. Wasmtime is pinned at 48.0.6, the patch of the long-term-support line that closes its advisories.
- Phase 1, PR-06: `florn-admission`, the admission step of RFC 003: validation with the exact feature set and a code for every refusal; both custom sections, the contract identity decoded by its interfaces, the manifest schema of RFC 002; the estimate of needs against limits and of instances and memory against capacity before anything exists; the transformation in its fixed order, with custom sections and exported global indices corrected around the library; the state layout; the admission cache; the config encoder. The admitted hash of the kit guest is recorded for admission version 1. RFC 007 section 4.2 gains two codes; RFC 003 rule 9 records the library behaviours the wrapper corrects. `zip` moved to 9.0.4 for the advisories of the 8 line.
- Phase 1, PR-07: `florn-host`, the Host core over the engine and platform traits: the run loop of RFC 005 rule 15; activations with the budget in place before lowering, fault classification by globals, the quotas in the import handlers; the instance pool with the estimate before anything exists; actor export and restore and checkpoints with one canonical encoding; the diagnostic log; the run identity and run inputs; the run record (`.florn-run`) written and read; replay with the recorded interventions and the comparison at every stored point. With it: the notification types of RFC 007 rule 3 in `florn-control`, the platform trait with the probe, the seed and the wall clock in `florn-platform`, kernel and trace snapshots, and the engine's name and version for the identity. AC-002, AC-003, AC-004 and AC-006 pass natively with hand-written guests. RFC 007 gains `run.limit-exceeded`; a duplicate `module.contract-mismatch` row that PR-06 had added is removed (the code carries the module's name, as section 4.1 says).
- Continuous integration on GitHub Actions: x86-64 Linux, AArch64 Linux, and Apple Silicon macOS; the licence and advisory gate of RFC 006; the book build; the RFC index and catalogue checks.

### Stage 0

- Seven proofs of concept reported and reviewed (one artifact on two engines; deterministic limits; boundary crossing cost; instance scaling; telemetry transport; snapshot and restore; UI shell). Their figures are recorded in the roadmap and the RFCs.
- Stage 0 exit review of 2026-10-14: every criterion met except continuous integration and the Cargo workspace, both scheduled before Phase 1.
