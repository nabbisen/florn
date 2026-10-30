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

Release 0.3.0: depth. On the network learning lab of 0.2.0 (the deterministic kernel, module admission with limits, the guest SDKs, the headless runner, the conformance kit whose scenarios give one digest on x86-64, AArch64, Apple Silicon, Chromium, Firefox and WebKit, the network domain, the ladder of guests and the browser application in English and Japanese), checkpoints as files and in memory, moving back in a run with what it undoes said first, branching, a cost model of florn's own pass at a fraction of the former cost, actor swapping beyond a browser's instance ceiling, modules imported from files with both hashes shown, an installable application, and a BGP subset with its learning lab. The next release, 0.4.0, is the circuit and control domain. See [ROADMAP.md](./ROADMAP.md) and the [release notes](./docs/src/releases/0.3.0.md).

## Quick start

```sh
rustup target add wasm32-unknown-unknown
cargo install florn
florn run scenarios/echo-pair.toml --modules scenarios/modules --seed 42
```

The `chain` line is the digest of the run; `scenarios/digests.toml` records what it must be. `cargo test -p florn-conformance` runs the whole kit. To write a guest of your own, start with the [module author's chapter](./docs/src/module-authors.md) and the SDK's template in [`crates/florn-guest`](./crates/florn-guest/README.md); to script runs, the [automation chapter](./docs/src/automation.md).

The browser application is the release's `florn-app-0.3.0.tar.gz`, served under the headers the [interface chapter](./docs/src/interface.md) names, or built from a checkout with `ci/build-shell.sh`; the [learning lab](./docs/src/lab.md) is the place to start. 日本語の手引きは[こちら](./docs/src/ja/interface.md)。

## Design notes

- Three layers: a domain-neutral **core** (actors, time, events, connections, resources, observation), **domain worlds** (network, circuit and control, finance), and **front ends** that are equal clients of one control interface.
- One Host core compiled for the browser and for native targets; only an engine adapter and a platform adapter differ.
- Contracts are written in WIT. The WIT text is the contract of record.
- Decisions are recorded as RFCs and never deleted.

## More

- [The book](./docs/src/SUMMARY.md): module authors, automation, compatibility, architecture, contributing
- [Requirements specification](./docs/src/requirements.md) and [external design](./docs/src/external-design.md)
- [RFCs](./rfcs/README.md) — start with [RFC 001, the determinism contract](./rfcs/done/001-determinism-contract.md)
- [The conformance kit](./crates/florn-conformance/README.md) and the [shipped scenarios](./scenarios/README.md)
- [Roadmap](./ROADMAP.md), [Changelog](./CHANGELOG.md), [Contributing](./CONTRIBUTING.md)

## License

Apache License 2.0. See [LICENSE](./LICENSE) and [NOTICE](./NOTICE).
