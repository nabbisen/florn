# florn

florn is a local-first platform for executable testbeds. Isolated WebAssembly actors run the logic under test; a deterministic Host simulates the world around them — a network, a circuit and its controller, a market.

**Emulate the component; simulate the world.**

This book is organised by reader.

| Reader | Start with |
|---|---|
| Module authors | [For module authors](./module-authors.md): write a guest in Rust, admit it, run it |
| Automation users | [For automation users](./automation.md): the runner, run records, digests, replay |
| Everyone | [Compatibility and versions](./compatibility.md), the [release notes](./releases/0.1.0.md) |
| Maintainers and contributors | [Architecture](./architecture.md), [Contributing](./contributing.md), the [requirements specification](./requirements.md), the [external design](./external-design.md), then the RFCs in `rfcs/` |

## Status

Release 0.1.0 is the core on two Hosts: a deterministic kernel, module admission with limits, a guest SDK, a headless runner, a browser test page, and a conformance kit whose scenarios give one digest on x86-64, AArch64, Apple Silicon, Chromium, Firefox and WebKit. The next release, 0.2.0, is a network learning lab with a browser interface. See `ROADMAP.md` in the repository.
