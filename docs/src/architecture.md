# Architecture

One Host core compiled for the browser and for native targets; an engine adapter and a platform adapter per Host; front ends as equal clients of one control interface. The design is RFC 005; this page is the map.

| Crate | Role | RFC |
|---|---|---|
| `florn-kernel` | Logical time, the event queue and its order, activations with recorded effects, streams, the trace and its chain, replay comparison | 001 |
| `florn-abi` | Canonical ABI lifting and lowering over an abstract memory, strict and canonical | 002 §2 |
| `florn-model` | The scenario manifest, quantities, identities, the canonical encoding and hash, the package | 004 |
| `florn-admission` | Validation, the manifest sections, the estimate, the transformation with cost-model version 1, the state layout, the config encoder | 003 |
| `florn-control` | Commands, queries, subscriptions, notifications; the catalogue of codes; the encoding over every transport; the display stream's record and reduction | 005 §6, 005 rule 14, 007 |
| `florn-host` | The run loop, the instance pool, activation with the budget before lowering, classification, quotas, export and restore, the diagnostic log, the run identity and record, the controller | 005 §4 to §6 |
| `florn-host::domain` | The domain interface: the `Domain` trait, the registry of the Host's domains, the composed world that routes a send, a change, an intervention or an event to the domain owning the type, class or kind; the conformance kit's test domain implements it | 008 §2 |
| `florn-engine`, `florn-engine-wasmtime`, `florn-engine-web` | The engine adapter trait and its two implementations; the benchmark drivers | 005 §2 |
| `florn-platform`, `florn-platform-native`, `florn-platform-web` | The platform adapter trait and its two implementations: clock, entropy, capacity | 005 §3 |
| `florn-guest`, `florn-guest-network` | The guest SDK and the example guests; the router world's bindings and `router!` | 002 §8, 008 §6 |
| `florn-domain-network` | The network domain: links and lans over interfaces, frames with delay, serialisation, jitter and loss, the `link` and `node` interventions, the blackhole built-in, the codes, the decoders | 008 |
| `florn-cli`, `florn` | The headless runner as a library, and the binary | 005 rule 31 |
| `florn-ui-model`, `florn-ui-scene` | The view model (what is shown, over the control interface only: topology, timeline, inspectors, interventions, the authoring draft, the outbox) and the scene (what to draw: layout, items, flights in motion, hit testing); native, framework-free, tested over the real Host | 005 §8 |
| `florn-ui-web` | The Host's worker module, the display transport (the ring or messages) and the test page; the shell of 0.2 | 005 §7, §8 |
| `florn-conformance` | The kit: scenarios with digests, the RFC verification rows, the benchmarks | AUTO-007 |
| `florn-ui-model`, `florn-ui-scene` | Empty until 0.2 | 005 §8 |

A domain (RFC 005 rule 3, RFC 008 §2) is a crate of the Host core that implements the `Domain` trait: its connection and port types, its validation and units, its event classes, the world its guests need beyond the core, its delivery rule, its changes and intervention kinds, its built-ins, its decoders and telemetry layouts, its checkpoint state. The Host holds one value per domain a scenario declares, composed with the core into one world; the kernel never sees a domain. A world function beyond the core has a flat call shape from the table in `florn-abi`, which admission and both engine adapters share, so that a signature no Host can call is refused before any run. The first domain is the network of RFC 008, composed by both shells; the kit's test domain stays as the interface's regression test.

The dependency direction is checked in CI (`ci/check-deps.py`): the core crates contain no platform or engine crate; the Host depends on the adapter traits, never on an implementation. The contract of record is `contracts/florn-core/wit/core.wit`, kept equal to RFC 002's text by a test.

Where to read next: the handoff package `rfcs/handoffs/005-host-architecture/` (the implementation handoff, the pull-request plan with its progress table, the acceptance checklist), then the RFCs under `rfcs/done/`.
