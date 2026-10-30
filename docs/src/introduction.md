# florn

florn is a local-first platform for executable testbeds. Isolated WebAssembly actors run the logic under test; a deterministic Host simulates the world around them — a network, a circuit and its controller, a market.

**Emulate the component; simulate the world.**

This book is organised by reader.

| Reader | Start with |
|---|---|
| Module authors | [For module authors](./module-authors.md): write a guest in Rust, admit it, run it |
| Automation users | [For automation users](./automation.md): the runner, run records, digests, replay |
| People at the application | [The browser interface](./interface.md) and [the learning lab](./lab.md); 日本語では[ブラウザーインターフェース](./ja/interface.md)と[学習ラボ](./ja/lab.md) |
| Everyone | [Compatibility and versions](./compatibility.md), the [release notes](./releases/0.3.0.md) |
| Maintainers and contributors | [Architecture](./architecture.md), [Contributing](./contributing.md), the [requirements specification](./requirements.md), the [external design](./external-design.md), then the RFCs in `rfcs/` |

## Status

Release 0.3.0 is depth: on the network learning lab of 0.2.0 (a deterministic kernel, module admission with limits, the guest SDKs, a headless runner, a conformance kit whose scenarios give one digest on x86-64, AArch64, Apple Silicon, Chromium, Firefox and WebKit, the network domain with its ladder of guests, and a browser application in English and Japanese), checkpoints, moving back and branching in a run, florn's own metering pass as cost-model version 2, actor swapping beyond a browser's instance ceiling, modules imported from files, an installable application, and the BGP subset with its lab. The next release, 0.4.0, is the circuit and control domain. See `ROADMAP.md` in the repository.
