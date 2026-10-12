# florn

[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](./LICENSE)

**Emulate the component; simulate the world.**

## Overview

florn is a local-first platform for executable testbeds. The logic under test — a router, a controller, a trading strategy — runs as an isolated WebAssembly actor. A deterministic Host simulates the world around it: a network, a circuit and its plant, a market. Every run is reproducible from its inputs, in the browser and on a native machine alike.

## Why

- **Real code, not a model of it.** The actor is the compiled logic. The Host approximates only what surrounds it.
- **Reproducible by construction.** Logical time, a total order of events, seeded randomness, and recorded interventions. Two runs with the same inputs have the same trace.
- **Safe to experiment.** Untrusted modules run with no ambient access and with limits counted the same way under every engine.
- **Local first.** Nothing leaves the machine unless you export it.

## Status

Stage 0: foundational decisions, proofs of concept, and RFCs. No release exists yet. See [ROADMAP.md](./ROADMAP.md).

## Quick start

There is nothing to run yet. The first release, `0.1`, is the core on two Hosts; the first user-facing release, `0.2`, is a network learning lab.

## Design notes

- Three layers: a domain-neutral **core** (actors, time, events, connections, resources, observation), **domain worlds** (network, circuit and control, finance), and **front ends** that are equal clients of one control interface.
- One Host core compiled for the browser and for native targets; only an engine adapter and a platform adapter differ.
- Contracts are written in WIT. The WIT text is the contract of record.
- Decisions are recorded as RFCs and never deleted.

## More

- [Requirements specification](./docs/src/requirements.md)
- [RFCs](./rfcs/README.md) — start with [RFC 001, the determinism contract](./rfcs/accepted/001-determinism-contract.md)
- [Roadmap](./ROADMAP.md)
- [Changelog](./CHANGELOG.md)

## License

Apache License 2.0. See [LICENSE](./LICENSE) and [NOTICE](./NOTICE).
