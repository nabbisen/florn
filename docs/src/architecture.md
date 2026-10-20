# Architecture

One Host core compiled for the browser and for native targets; an engine adapter and a platform adapter per Host; front ends as equal clients of one control interface. The design is RFC 005; this page is the map.

| Crate | Role | RFC |
|---|---|---|
| `florn-kernel` | Logical time, the event queue and its order, activations with recorded effects, streams, the trace and its chain, replay comparison | 001 |
| `florn-abi` | Canonical ABI lifting and lowering over an abstract memory, strict and canonical | 002 §2 |
| `florn-model` | The scenario manifest, quantities, identities, the canonical encoding and hash, the package | 004 |
| `florn-admission` | Validation, the manifest sections, the estimate, the transformation with cost-model version 1, the state layout, the config encoder | 003 |
| `florn-control` | Commands, queries, subscriptions, notifications; the catalogue of codes; the encoding over every transport | 005 §6, 007 |
| `florn-host` | The run loop, the instance pool, activation with the budget before lowering, classification, quotas, export and restore, the diagnostic log, the run identity and record, the controller | 005 §4 to §6 |
| `florn-engine`, `florn-engine-wasmtime`, `florn-engine-web` | The engine adapter trait and its two implementations; the benchmark drivers | 005 §2 |
| `florn-platform`, `florn-platform-native`, `florn-platform-web` | The platform adapter trait and its two implementations: clock, entropy, capacity | 005 §3 |
| `florn-guest` | The guest SDK and the example guests | 002 §8 |
| `florn-cli`, `florn` | The headless runner as a library, and the binary | 005 rule 31 |
| `florn-ui-web` | The Host's worker module and the test page; the shell of 0.2 | 005 §7, §8 |
| `florn-conformance` | The kit: scenarios with digests, the RFC verification rows, the benchmarks | AUTO-007 |
| `florn-ui-model`, `florn-ui-scene` | Empty until 0.2 | 005 §8 |

The dependency direction is checked in CI (`ci/check-deps.py`): the core crates contain no platform or engine crate; the Host depends on the adapter traits, never on an implementation. The contract of record is `contracts/florn-core/wit/core.wit`, kept equal to RFC 002's text by a test.

Where to read next: the handoff package `rfcs/handoffs/005-host-architecture/` (the implementation handoff, the pull-request plan with its progress table, the acceptance checklist), then the RFCs under `rfcs/done/`.
