# Phase 2 handoff package — network learning lab, release 0.2.0

Entry point for the implementation of RFC 008 and the 0.2 rules of RFC 005 (sections 7 and 8: the telemetry transport, the browser shell). Written 2026-10-24 on the acceptance of RFC 008; the architect and the developer are one person in Phase 2 as in Phase 1, and the owner verifies.

| Document | What it is |
|---|---|
| [`implementation-handoff.md`](./implementation-handoff.md) | What each crate must do and when it is done; the tooling added for the shell; the cross-cutting rules |
| [`task-breakdown-pr-plan.md`](./task-breakdown-pr-plan.md) | Eight pull requests in order, from the domain interface to the release; the progress table |
| [`acceptance-qa-checklist.md`](./acceptance-qa-checklist.md) | The owner's checks C15 to C22, the journeys J1 to J4 as procedures, and the release review |

## What Phase 2 delivers

The roadmap's Phase 2: the network domain, the first protocol guests, a browser interface in two languages, and a bundled learning lab. The exit criterion is the owner's acceptance of the journeys J1 to J4 of the external design (AC-008 to AC-011): open a lab and watch it converge; break a link during the run and replay it with the intervention; save, reload and replay; build a topology from nothing.

## Order of work and why

1. **The domain interface** (RFC 008 §2) comes first, because every later piece rests on it and because it is the one change that touches the Host's core.
2. **The network domain crate and its contract** follow, with the verification rows of RFC 008 §12 as tests.
3. **The guest ladder** proves the contract before any interface exists: digests on every engine.
4. **The telemetry transport** of RFC 005 rule 14 and the **view model and scene** are native code without a framework, tested without a browser.
5. **The browser shell** is the only framework code, kept to the main thread; the worker and `florn_handle` stay as 0.1 left them.
6. **The OSPF subset and the lab** arrive last, as content over a finished interface.
7. **Release 0.2.0** repeats the release discipline of 0.1.0.

## What stays out

Checkpoints and backward navigation (0.3), module import through the UI (0.3), BGP (0.3), a second domain (0.4), the native shell (0.6). Anything the owner did not decide is an open question in RFC 008 or in this package, never a quiet choice in code.
