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
- Continuous integration on GitHub Actions: x86-64 Linux, AArch64 Linux, and Apple Silicon macOS; the licence and advisory gate of RFC 006; the book build; the RFC index and catalogue checks.

### Stage 0

- Seven proofs of concept reported and reviewed (one artifact on two engines; deterministic limits; boundary crossing cost; instance scaling; telemetry transport; snapshot and restore; UI shell). Their figures are recorded in the roadmap and the RFCs.
- Stage 0 exit review of 2026-10-14: every criterion met except continuous integration and the Cargo workspace, both scheduled before Phase 1.
