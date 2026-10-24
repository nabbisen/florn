# RFC 008 — Network domain: how a domain enters the Host, links, frames, the router contract, the guest ladder

**Status.** Accepted (2026-10-24, the owner's word on the day it was proposed). The first domain RFC; answers the twelve design questions the audit before 0.1.0 listed for Phase 2. Implementation: Phase 2, release 0.2.0, by the handoff package `rfcs/handoffs/008-network-domain/`.

**Tracks.** Requirements NET-001 to NET-004, NET-006 to NET-012, BUS-001, BUS-003, BUS-008, BUS-009, OBS-003, OBS-004, OBS-006, OBS-008, OBS-009, ARCH-006, ARCH-010, AC-008 to AC-011. Requirement IDs refer to `docs/src/requirements.md`.

**Touches.** The mechanism by which any domain crate enters the Host (RFC 005 rule 3 named the duties, not the interface); the connection types, attributes, classes and delivery of the network domain; interventions beyond a message (RFC 001 rule 37); the `florn:network@0.1.0` contract; the validation, codes, telemetry layouts and decoders the network view needs; the built-in mechanism; the guest ladder and the OSPF subset.

## 1. Why a domain interface first

The core of 0.1 has the shape a network needs: connections of two or more ports with attributes the core leaves opaque (RFC 004 rule 19), a `World` that decides target, class and delay for every committed send (`florn-kernel`), scheduled changes addressed to connections (RFC 004 rule 20), and domain events with their own class. What it lacks is the way a domain crate supplies those pieces: the Host hard-wires the core world, drops domain and change events addressed to module actors, and knows one intervention kind, a message. Without the interface, every domain would be a patch of the Host. This RFC specifies the interface once (§2) and the network domain as its first user (§3 onwards). The circuit and finance domains reuse §2 unchanged; where they would need more, §10 says so.

## 2. The domain interface

**Rule 1.** A domain is a crate of the Host core (RFC 005 rule 3) that implements the Host's `Domain` trait. The Host holds one instance per domain the scenario declares (RFC 004 rule 5), composed into one world. The kernel knows no domain (ARCH-006); the Host routes to a domain by the types the scenario uses.

| Operation | What the domain does | When the Host calls it |
|---|---|---|
| `name` | Its prefix: `network`, `circuit`, `finance` | At composition |
| `connection-types`, `port-types`, `builtin-kinds` | The types it owns, each as `<domain>/<name>` | At validation, to route a type to its domain |
| `validate` | Checks every connection of its types (port count, port types, `attrs`, `attrs.per-port`), every actor of its port types (per-port fields), its top-level table (RFC 004 rule 22), and every built-in's ports and `config`; reports every error with a code in one pass (RFC 004 §7) | At scenario load, before the estimate |
| `units` | The units it registers for quantities in its attributes and in its guests' parameters (RFC 004 rules 23 to 25) | At validation and config encoding |
| `classes` | Its event classes with their names (RFC 001 rule 15) | At composition; the kernel's queue uses the numbers, the timeline the names |
| `exports`, `imports` | The signatures of its world beyond the core's, as the adapters need them (RFC 002 rule 46) | At admission and instantiation |
| `world` | Its `World`: for a committed `send` on a port of its types, the deliveries with target, class, delay and kind (§4) | Per committed send |
| `change` | A scheduled change or an intervention addressed to one of its connections, to the world, or an intervention kind of its own: the events it schedules and the state it changes | At the instant of the change (RFC 004 rule 20; RFC 001 rules 33 to 36) |
| `event` | A domain event of its classes whose target is not an actor (a connection, the world): what it records or schedules | When the kernel hands the event out |
| `answer` | An import of its world called by a guest, answered from the activation's starting state (RFC 002 rule 47) | Inside an activation |
| `builtin` | An actor the Host runs for a built-in kind: its activation function over the same effects a guest has | When a built-in actor is activated |
| `decode` | A payload's fields for the inspector, by the protocol a module declares (OBS-006) | On request from a front end; never on the hot path |
| `telemetry` | The channel names and layouts its views read (§7) | At validation of a module's manifest, as a warning when a declared channel's layout differs |
| `export`, `restore` | The domain's own state for a checkpoint (RFC 003 §6), if it has any | At checkpoint and restore |

**Rule 2.** The composed world dispatches a send by the port's type: a `core/port` goes to the core world of RFC 005 (every other port of the connection, class 2, delay 0); a port of a domain's type goes to that domain's world. A connection joins ports of one domain only, or of the core only; mixing is a validation error (`connection.mixed-types`).

**Rule 3.** A domain's randomness comes from streams the kernel derives for it (RFC 001 rules 28 to 31): the owner is the connection's or the world's identity, the purpose label names the use (`loss`, `jitter`). A domain never holds a generator of its own and never reads a clock. Its draws are a function of the run inputs and of the order of the sends that cause them, so the trace is unchanged in form: a dropped frame is simply never handled, and a delayed one is handled at its instant. For observation, not for determinism, a domain may record what it decided as an event of its own class addressed to the connection (rule 1, `event`), which appears in the timeline and the diagnostic log and never in a guest.

**Rule 4.** Interventions beyond a message. The control interface's `intervene` carries a canonical map with `kind` (RFC 005 rule 26); the core knows `message`; a domain declares its kinds and their fields in its RFC and decodes them in `change`. Every applied intervention is a run input (RFC 001 rule 35), whatever its kind. A change addressed to a connection and an intervention of the same content have the same effect; the difference is only when the request was made.

**Rule 5.** A fault's policy (RFC 001 rule 26) stays the scenario's: `drop` or `stop-run`. A domain may add a consequence for its own types; the network domain drops a frame addressed to a faulted actor at delivery and records a `frame-dropped` event with the reason `faulted`.

**Rule 6.** The adapters. A domain world's exports and imports beyond the core have signatures the engine adapters must call and bind: the Wasmtime adapter does so from the signature; the browser adapter's trampolines are a table of signatures that a domain extends at composition (RFC 005 rule 7; the `Sig` table of `florn-engine-web`). A signature outside the table is an admission refusal, `module.export-unsupported`, never a run-time surprise.

## 3. The network domain: types

**Rule 7.** Ports. One port type in 0.2: `network/interface`. A module declares its interface kinds with it (RFC 002 rule 15); a scenario places interfaces by it. Per-port fields of an actor's interface are given in the connection's `attrs.per-port` (rule 9), not on the actor.

**Rule 8.** Connections carry a `type` (an amendment of RFC 004 rule 19 this RFC requests: `type` is read by the core and routed to the domain; absent, `core/connection`). Two network types:

| Type | Ports | Meaning |
|---|---|---|
| `network/link` | Exactly two interfaces | A point-to-point link, full duplex; each direction has the same attributes |
| `network/lan` | Two or more interfaces | A shared medium: a frame sent by one interface reaches every other interface of the connection; each receiver's loss and jitter are drawn separately |

**Rule 9.** Attributes of a network connection, all optional, with their defaults and units; a scenario may set them in `attrs`, and `attrs.per-port` may override `cost` and `mtu` for one interface.

| Attribute | Type | Default | Meaning |
|---|---|---|---|
| `delay` | quantity, time | `0ns` | Propagation delay, whole ticks (RFC 004 rule 23) |
| `bandwidth` | quantity, bits per second (`bit/s`, `kbit/s`, `Mbit/s`, `Gbit/s`) | unlimited | Serialisation: a frame of `n` bytes takes `ceil(8n / bandwidth)` ticks, in addition to `delay` |
| `jitter` | quantity, time | `0ns` | Added delay drawn uniformly in `[0, jitter]`, whole ticks |
| `loss` | real in `[0, 1)` | `0` | The probability that a frame is dropped, per receiver |
| `mtu` | integer, bytes | `1500` | A frame above it is dropped at send with the reason `too-large` |
| `enabled` | bool | `true` | A disabled link drops at send with the reason `down` |
| `cost` (per port) | integer | `1` | The metric a routing guest reads for the interface through `link-info` (rule 15) |

The units `bit/s` and its multiples are registered by the domain (rule 1, `units`); `delay` and `jitter` are time quantities the core already converts to ticks.

## 4. Delivery

**Rule 10.** When an actor's activation commits a `send` on an interface attached to a connection, the network world decides for each other interface of the connection, in ascending order of the receivers' identities:

1. If the connection is not `enabled`, or the sender is faulted, or the frame is longer than the sender's `mtu`: no delivery; one `frame-dropped` event (class 5) is recorded for the connection with the reason (`down`, `too-large`), and the decision stops for every receiver.
2. Draw `u` from the connection's `loss` stream; if `u < loss`, no delivery to this receiver and a `frame-dropped` event with the reason `loss`.
3. Draw `j` from the connection's `jitter` stream when `jitter > 0`, uniform on `[0, jitter]` in whole ticks, by the SDK's algorithm for `below` (RFC 001 rule 30) so that every Host computes the same.
4. Schedule a `frame` event (class 4) for the receiver at `now + delay + serialisation + j`, with the payload unchanged, through the receiver's interface port.

A frame in flight when a link is disabled is delivered: it has left the sender. A frame addressed to an actor that faulted meanwhile is dropped at delivery with the reason `faulted` (rule 5).

**Rule 11.** Classes of the network domain (RFC 001 rule 15), beside the core's 0 (control), 1 (timer) and 2 (message):

| Class | Name | Target | Meaning |
|---|---|---|---|
| 3 | `link-state` | An interface's actor | The link of the interface went up or down; the guest's `on-link` is called |
| 4 | `frame` | An interface's actor | A frame arrives; the guest's `on-message` is called with the interface's port |
| 5 | `frame-dropped` | The connection | Observation: a frame was dropped, with the reason; no guest is called |

Within one instant the key of RFC 001 rule 14 orders them: `link-state` before `frame`, so that a guest learns a link is down before frames that were in flight arrive at the next microstep.

## 5. Changes and interventions

**Rule 12.** A scheduled change addressed to a network connection (RFC 004 rule 20) and an intervention of kind `link` carry the same payload: any of `enabled`, `delay`, `bandwidth`, `jitter`, `loss`, `mtu`, with the types of rule 9. At its instant the domain sets the attributes and, when `enabled` changed, schedules a `link-state` event for each attached interface at the next microstep.

**Rule 13.** An intervention of kind `node` carries `actor` and `up`. A node that is down has every attached link behave as disabled for it: its sends are dropped with the reason `down`, frames to it are dropped at delivery, and its interfaces receive `on-link(port, false)`; `up` restores the links and calls `on-link(port, true)`. The actor keeps running (timers fire, state persists); this is the outage of RFC 001 rule 37 as the network domain defines it. A faulted actor is not a node that is down: it handles nothing.

**Rule 14.** The core's `message` intervention stays, for an interface as for a core port: a frame delivered to the actor at the next microstep as if a peer had sent it.

## 6. The contract `florn:network@0.1.0`

**Rule 15.** The package, the contract of record once accepted (`contracts/florn-network/wit/network.wit`), with the equality test RFC 002 rule 19 prescribes:

```wit
package florn:network@0.1.0;

/// What a guest may ask about its interfaces, answered from the activation's starting state.
interface link {
    use florn:core/types@0.1.0.{port-id};

    record link-info {
        /// The link is enabled and the node is up.
        up: bool,
        /// The metric of this interface, from the connection's per-port attributes.
        cost: u32,
        /// The largest frame this interface sends, in bytes.
        mtu: u32,
        /// Bits per second; zero when unlimited.
        bandwidth: u64,
    }

    /// The state of one of this actor's interfaces.
    link-info: func(port: port-id) -> link-info;
}

/// Events of the network beyond the core's.
interface router-events {
    use florn:core/types@0.1.0.{tick, port-id};

    /// The link of `port` went up or down.
    on-link: func(now: tick, port: port-id, up: bool);
}

/// A router: the core world with the network's events.
world router {
    include florn:core/guest@0.1.0;
    import link;
    export router-events;
}
```

`link-info` is an import that answers from the activation's starting state (RFC 002 rule 47): a change applied at a later boundary is not visible before it. `on-link` is one activation like any other; a frame is `on-message` through the interface's port, its payload the bytes the sender gave `send`.

**Rule 16.** Frames are bytes. The domain defines no header: a protocol defines its own layout, and declares its name in the module manifest's `[network]` table, `protocol = "<name>"`, so that the inspector decodes the frames of that module with the domain's decoder for the protocol (OBS-006). The decoders of 0.2 are Host code in the domain crate for the ladder's protocols; a decoder as a sandboxed module is 0.3.

## 7. Telemetry the view reads

**Rule 17.** A router guest may report on two channels the network view knows by name; a guest that declares them with these layouts gets its neighbours and routes shown, and a guest that does not is shown by its events alone (NET-008, NET-009).

| Channel name | Fields | Meaning |
|---|---|---|
| `neighbour` | `port: u64`, `neighbour: u64`, `state: text` | A neighbour's state on an interface changed |
| `route` | `prefix: text`, `next-hop: u64`, `metric: u64`, `action: text` | A route was added, replaced or withdrawn |

Frames sent, delivered and dropped, and link state, come from the trace and the domain's events, not from guests (OBS-003: the Host reduces before display).

## 8. Validation and codes

**Rule 18.** The domain validates at scenario load (rule 1), each with a code added to RFC 007 section 4 in a new table 4.6:

| Code | Parameters | When |
|---|---|---|
| `network.link-ports` | `connection: name`, `found: count` | A `network/link` without exactly two ports |
| `network.port-type` | `connection: name`, `port: name`, `type: text` | A port of another type on a network connection |
| `network.attr-unknown` | `connection: name`, `key: name` | An attribute the domain does not know |
| `network.attr-type` | `connection: name`, `key: name`, `expected: text` | A wrong type, a bare number, a loss outside `[0, 1)` |
| `network.protocol-unknown` | `module: name`, `protocol: text` | A module declares a protocol the domain has no decoder for (a warning) |
| `connection.mixed-types` | `connection: name` | Ports of two domains on one connection (rule 2) |

The core's `module.port-type-unknown` and `module.port-count` stay the module-side checks.

## 9. Built-ins

**Rule 19.** The built-in mechanism (BUS-009; RFC 004 rule 18): an actor of kind `builtin = "network/<name>"` is run by the domain crate's `builtin` function over the same effects a guest has (send, timers, reports), with no instance, no budget and no quotas (it is Host code). Its activations are trace records like a guest's, with `metered = 0`. One built-in in 0.2: `network/blackhole`, which drops every frame it receives and reports the count on channel `dropped`, so that the mechanism is exercised by the kit before a lab needs a hub or a noise source.

## 10. The guest ladder and the OSPF subset

**Rule 20.** The ladder of NET-012, shipped as SDK examples and conformance fixtures with recorded digests:

| Guest | What it does | What it proves |
|---|---|---|
| `net-echo` | Echoes every frame through the interface it arrived on | Delivery with delay, serialisation, loss and `on-link` |
| `distance-vector` | Advertises its routes to every neighbour every `period` (default `30s`) with split horizon; counts to infinity at 16; a route unheard for `3 × period` is withdrawn; reports `neighbour` and `route` | Convergence after a link break (J3), the telemetry layouts, the decoder, the intervention kinds |

**Rule 21.** The OSPF subset of NET-004, a guest deliverable after the ladder: single area; `hello` and `dead` intervals; a two-state neighbour machine (down, full) over a point-to-point adjacency without designated-router election; a link-state database of router-LSAs only, flooded with sequence numbers and acknowledgements; Dijkstra on the database; a routing table of prefixes to next hops. Everything outside this list is out of the subset and refused by the guest with a logged reason, not silently approximated.

## 11. Consequences for other RFCs and the requirements

| RFC | Must provide |
|---|---|
| 001 | Classes 3 to 5 as rule 11 assigns them; rule 37's outage realised as the `node` intervention |
| 002 | The `[network]` table of the module manifest (`protocol`); the contract of record `florn:network@0.1.0` and its equality test |
| 004 | `type` on a connection (rule 8); the registered units `bit/s` and multiples |
| 005 | The `Domain` trait and the composite world (rule 1, 2); domain events routed to the domain; intervention kinds decoded by the domain (rule 4); the adapters' signature table (rule 6); the conformance kit's domain rows; `florn-domain-network` in the layout of rule 42 |
| 006 | The domain's decoders are Host code over bytes with the maxima of section 7; no new origin, no new capability |
| 007 | Table 4.6 with the codes of rule 18; enumerations `drop-reason` (`down`, `too-large`, `loss`, `faulted`) and `link-state` |
| Requirements | NET-004's subset is rule 21; BUS-003's jitter is uniform and bandwidth is serialisation time (rule 9) |

## 12. Verification

| Property | Test |
|---|---|
| Delay | Two nodes over a `20ms` link at a `1ms` tick: a frame sent at tick `t` arrives at `t + 20`; with `bandwidth = "1Mbit/s"` a 1,500-byte frame arrives 12 ticks later still |
| Loss | Over 10,000 frames at `loss = 0.1`, between 900 and 1,100 are dropped; the drops are identical under both Hosts and in replay |
| Jitter | Every arrival lies in `[delay, delay + jitter]`; the digest is identical under both Hosts |
| Link down | A change at `30s` disables the link: `on-link(false)` reaches both ends before any later frame; frames in flight arrive; sends after it are dropped with `down`; re-enabling calls `on-link(true)` |
| Node down | The `node` intervention: the node's timers keep firing, its frames are dropped, its neighbours see `on-link(false)` |
| Lan | Three interfaces: a frame reaches the two others, each with its own loss draw |
| Validation | Each code of rule 18 produced by a scenario built to provoke it, in one pass |
| Ladder | `net-echo` and `distance-vector` scenarios with recorded digests on every engine; the triangle converges, breaks and reconverges within the declared bound |
| Built-in | `network/blackhole` drops and reports; its records carry `metered = 0` |
| Decoder | The distance-vector frames decode to their fields in the inspector's query |
| Interface | A second domain implemented against §2 with no change to the Host (the circuit RFC's test; until then, a kit `test-domain` with one connection type exercises every operation of rule 1) |

## Alternatives considered

| Alternative | Reason for rejection |
|---|---|
| Patch the Host for the network domain and generalise later | Three domains are planned; the second would have to undo the patch. The interface costs one trait now. |
| Loss decided by the receiver's draw | The receiver's stream is the guest's (RFC 002 rule 30); a lossy link would change a guest's draws, breaking RFC 001 §14's streams property. The connection owns its draws. |
| Dropping frames in flight when a link goes down | Physically a frame already on the wire arrives; dropping it would also need the Host to search the queue, which the kernel does not offer. |
| A domain-defined frame header | Every protocol has its own; a header the domain imposes would be a second one. Decoders by declared protocol give the inspector what it needs. |
| The loss decision in the trace as a record | The trace changes form for every domain; determinism is already by construction (rule 3). Observation goes through the domain's events instead. |

## Open questions

| # | Question | Decided by |
|---|---|---|
| 1 | Whether `jitter` is uniform (this RFC) or offers a second distribution | The first lab that needs one |
| 2 | Whether the browser adapter's signature table is extended by the domain at composition or generated from the WIT at build | The first PR of the implementation handoff |
| 3 | The distance-vector guest's exact timers and infinity, before its digests are recorded | The ladder's PR, with the lab's story |
| 4 | Decoders as sandboxed modules (OBS-006's second form) | 0.3 |
