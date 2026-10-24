# Phase 2 implementation handoff

Entry point: [`README.md`](./README.md). This document says what each crate must do for release 0.2.0 and when it is done. Rule numbers refer to RFC 008 unless another RFC is named.

## 1. Summary

Release 0.1.0 gave florn a deterministic core on two Hosts. Release 0.2.0 gives it its first world and its first users: a network domain, a router contract, a ladder of guests ending in an OSPF subset, and a browser interface in English and Japanese that shows a topology, a timeline and inspectors, lets a person build a topology and break a link, and replays what happened. The core does not change in meaning; it gains one interface (the domain trait) and the pieces RFC 005 reserved for 0.2 (the telemetry transport, the shell).

## 2. Toolchain additions, pinned

| Tool | Version | Note |
|---|---|---|
| `wasm-bindgen`, `js-sys`, `web-sys` | 0.2.126, 0.3.103 | The shell only (main thread); the worker module stays without them |
| Leptos | The current release at PR-20, pinned there with its reason | The shell framework (RFC 005 rule 38; POC-7) |
| `leptos_i18n` | Pinned with Leptos | The catalogues of RFC 007 rule 15 |
| Playwright | 1.64.0 (lock file) | The journeys J1 to J4 in CI, beside the kit's run |

Everything else is Phase 1's. A version is changed by a pull request that says why.

## 3. The crates

### 3.1 `florn-host`: the domain interface (RFC 008 §2)

The `Domain` trait of rule 1; a composite world dispatching by port type (rule 2); domain streams by connection or world identity (rule 3); intervention kinds decoded by the domain (rule 4); domain events routed to the domain's `event`; built-in actors run by the domain (rule 19) with trace records of `metered = 0`; the domain's state in checkpoints. `Run::new` takes the domains the scenario declares; the core world is the domain of `core/port`.

Done when: a kit `test-domain` with one connection type exercises every operation of rule 1 with no change to the Host; every 0.1 scenario gives its recorded digest unchanged; the dependency check still finds no adapter in the Host's tree.

### 3.2 `florn-model`: the connection type (RFC 004 amendment)

`type` on a connection, read by the core and routed to the domain; `core/connection` when absent; the registered units a domain adds for its quantities.

Done when: a scenario with a `network/link` parses, validates through the domain, and hashes canonically with the type included; a mixed connection is refused with `connection.mixed-types`.

### 3.3 `florn-engine-web`, `florn-engine-wasmtime`: domain signatures (rule 6)

The browser adapter's signature table takes a domain's exports and imports at composition; the Wasmtime adapter calls from the signature. A signature outside the table is `module.export-unsupported` at admission.

Done when: `on-link` and `link-info` are called under both adapters with one digest.

### 3.4 `florn-domain-network` and `contracts/florn-network/wit`

The domain of RFC 008 §3 to §9: the types, the attributes and their validation and units, the delivery of rule 10, the classes of rule 11, the `link` and `node` interventions, the codes of rule 18 (RFC 007 table 4.6), the `network/blackhole` built-in, the decoders for the ladder's protocols, the telemetry layouts the view reads. The contract of record `florn:network@0.1.0` with the equality test of RFC 002 rule 19; the SDK's bindings for the `router` world behind a feature.

Done when: every row of RFC 008 §12 but the ladder's passes natively and in the three browsers.

### 3.5 `florn-guest`: the ladder (rule 20) and the OSPF subset (rule 21)

`net-echo` and `distance-vector` as examples and kit fixtures with recorded digests; the OSPF subset as a guest with its declared limits, its decoder in the domain, and its scenarios.

Done when: the triangle converges, breaks and reconverges within the bound its scenario states, on every engine; the OSPF lab's story runs through J1 and J3.

### 3.6 `florn-host`, `florn-ui-web`: the telemetry transport (RFC 005 rule 14)

Sixteen-byte records; the ring over `SharedArrayBuffer` when the page is cross-origin isolated, one transferred message per Host tick otherwise; the reduction above the transport; `telemetry.transport` and `telemetry.dropped` as the catalogue says.

Done when: the two implementations deliver identical records for one run, measured at 2,000 records per frame with no frame lost (OBS-003, OBS-007).

### 3.7 `florn-ui-model`, `florn-ui-scene` (RFC 005 rule 37)

The view model over the control interface only: the topology, the run state, the timeline (OBS-004), the inspectors (OBS-005, OBS-006), the intervention list, validation by field; the scene: layout, the frames' animation without one update per event (NET-008), link state. Both compile and test natively with no framework.

Done when: the tests of RFC 005 rule 41 that need no browser pass natively; the dependency check finds no `web-sys`, `js-sys`, `wasm-bindgen` or framework crate in either.

### 3.8 `florn-ui-web`: the shell (RFC 005 rule 38; external design §3)

Leptos on the main thread; the worker and `florn_handle` as 0.1 left them, the only path to the Host. Run controls and pacing (§3.3), authoring with validation at the field (§3.4), interventions with the applied instant shown (§3.5), inspectors (§3.6), timeline (§3.7), faults and errors from the catalogue (§3.8), save, load, export, import of scenarios and run records (§3.9), replay and playback (§3.10), accessibility (§3.11, UI-008), English and Japanese (§3.12), performance transparency (§3.13). Versioned hosting path and the offline signal (RFC 005 rule 35).

Done when: the journeys J1 to J4 pass as Playwright scripts in Chromium, Firefox and WebKit in CI; the catalogue builds in both languages with every key present; `ci/forbidden-words.sh` passes over the catalogues; the network log holds one origin.

### 3.9 Release 0.2.0

Documentation (the lab's guide, the interface's guide in both languages, the network module author's chapter), the security review of RFC 006 rule 5 for the added data flows (file import and export in the shell, the shared ring), release notes with the figures, RFC 008 to `rfcs/done/`, the tag, the crates published.

## 4. Cross-cutting rules

| Rule | Source | How it is checked |
|---|---|---|
| The kernel knows no domain; a domain reaches no adapter | RFC 005 rule 3; ARCH-006 | `ci/check-deps.py` gains the domain crate's row |
| A domain's randomness is the kernel's streams; no clock, no generator | RFC 008 rule 3 | The determinism tests of RFC 001 §14 over the ladder |
| The shell reaches the Host through `florn_handle` only | RFC 005 rule 25; API-04 of the audit | A grep in CI for other worker exports in the shell's code |
| Every string a person sees is a catalogue key | RFC 007 rules 14 to 17 | The build fails on a missing key; `ci/forbidden-words.sh` |
| No framework below the shell | RFC 005 rules 37, 41 | `ci/check-deps.py` |
| The 0.1 digests do not change | RFC 001 | The kit's shipped scenarios, unchanged, in every CI run |
