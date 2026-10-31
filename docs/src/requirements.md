# Requirements Specification

| | |
|---|---|
| **Project** | florn |
| **Document** | Requirements specification, revision 3 |
| **Status** | Accepted by the owner on 2026-10-13 |
| **Supersedes** | Requirements baseline of 2026-09-28 (private draft) |
| **Owner** | Architect |
| **Date** | 2026-10-13 |
| **Governing RFCs** | [RFC 001 — Determinism contract](../../rfcs/done/001-determinism-contract.md), [RFC 004 — Core model](../../rfcs/done/004-core-model.md) |

florn is a local-first platform for executable testbeds. Isolated WebAssembly actors run the logic under test; a deterministic Host simulates the world around them. This document states what florn must do. How it does it is the subject of the RFCs.

## 0. How to read this document

### 0.1 Requirement records

Every requirement has a stable identifier, a statement, and three attributes.

| Attribute | Values | Meaning |
|---|---|---|
| **Release** | `0.1` … `0.6`, `1.0`, `later`, `—` | The release in which the requirement must be met. `later` means after 1.0 and not yet planned. `—` means the requirement is a constraint on every release rather than a deliverable. |
| **Source** | `study`, `derived` | `study`: traceable to the founding study. `derived`: added for product quality, security, or engineering; the rationale follows the statement in italics. |
| **Verification** | `test`, `demo`, `inspect`, `analyse` | How conformance is shown: an automated test; a demonstration accepted by the owner; inspection of code or documents; analysis or measurement. |

Releases are defined in `ROADMAP.md`. In short: `0.1` core on two Hosts, `0.2` network learning lab, `0.3` depth, `0.4` circuit and control, `0.5` finance, `0.6` native application, `1.0` stable contracts.

### 0.2 Words

| Word | Force |
|---|---|
| **shall**, **shall not** | Mandatory. Conformance requires it. |
| **should** | Recommended. A deviation is recorded with its reason. |
| **may** | Permitted. |

A requirement states one obligation. Where the previous revision stated the same obligation twice, one identifier was kept and the other retired (§23). Retired identifiers are never reused.

### 0.3 Identifiers

Identifiers are stable. An identifier is never renumbered, never reused, and never changes meaning. A requirement that changes substantially is retired and a new identifier is issued.

### 0.4 Relation to RFCs

An RFC records a decision and names the requirements it satisfies in its `Tracks` field. Where this document and an accepted RFC disagree, the RFC is corrected or this document is revised in the same change. Neither is left to drift.

## 1. Glossary

One word for one concept. These definitions bind the documentation, the user interface, and the public interfaces. RFC 001 defines the terms of the logical world in more detail; this table agrees with it.

| Term | Meaning |
|---|---|
| **Host** | The part of florn that owns time, events, communication, resources, and the domain world, and that executes guests. |
| **Browser Host** | The Host running inside a web browser, normally in a worker. Earlier drafts called this "Pattern 1". |
| **Native Host** | The Host running as a native program on a Wasmtime engine. Earlier drafts called this "Pattern 3". The headless form is a command-line runner; the graphical form is the native application. |
| **Guest** | A WebAssembly module supplied to the Host. Untrusted. |
| **Module** | The file that contains a guest. |
| **Actor** | One participant in a scenario with a stable identity. Backed by an instance of a guest, or a built-in of the Host. |
| **Built-in** | An actor implemented by the Host itself, for things that need no sandbox, such as a passive circuit element or a noise source. |
| **Domain** | A field of application with its own world, contracts, and views: network, circuit and control, finance. |
| **World** | The domain-specific environment that the Host simulates around the actors: links, a physical plant, a market. |
| **Contract** | A versioned interface between guest and Host, written in WIT. |
| **Core contract** | The interfaces every domain shares: lifecycle, time, timers, randomness, telemetry. |
| **Scenario** | The definition of a logical world before it runs: actors, connections, parameters, seed, scheduled changes. Stored in a manifest. |
| **Manifest** | The text form of a scenario. |
| **Package** | A manifest bundled with the modules it references, for sharing. |
| **Run** | One execution of a scenario. |
| **Intervention** | A change a user or script makes to the running world. Recorded as a run input. |
| **Trace** | The ordered record of what happened in the logical world during a run. |
| **Digest** | A hash over the trace, used to compare runs. |
| **Replay** | Executing a run again from its inputs. |
| **Playback** | Displaying a stored trace. Nothing executes. |
| **Checkpoint** | The stored state of the logical world between two instants. |
| **Tick** | The unit of logical time. Its length is declared per scenario. |
| **Admission** | The step that validates, reads, transforms, and instantiates a module before it can act. |
| **Control interface** | The interface through which any front end — browser UI, native UI, command line — commands and observes the Host. |
| **Conformance kit** | The test suite that checks a guest against its contract and a Host against RFC 001. |
| **Learning lab** | A scenario shipped with florn for the purpose of understanding a behaviour. |
| **Node** | A vertex of the topology graph. Every actor is a node. |
| **Port** | A named point of attachment on an actor, with a domain-defined type. |
| **Connection** | A set of two or more ports joined in the topology. A *link* (network) and a *net* (circuit) are connections; the core gives a connection no meaning of its own. |
| **Actor kind** | What a module or built-in provides: its ports, parameters, and contract. Declared by the module manifest, or by the domain for a built-in. |
| **Event** | Something scheduled to happen at one instant, addressed to one target. |
| **Activation** | One call from the Host into a guest-backed actor, to handle one event. |
| **Effect** | A request an actor makes during an activation: send, set or cancel a timer, report telemetry. Effects are committed when the activation returns normally. |
| **Origin** | Whatever causes an event: an actor, a connection, a world component, or the source of interventions. Every origin has a stable identity. |
| **Instant** | A tick count together with a microstep. |
| **Scheduled change** | A change to the world at a stated tick, written in the scenario. |
| **Fault** | The abnormal end of an activation, or of a run. |
| **Budget, limit, quota** | Bounds enforced per actor: *budget* for executed work, *limit* for stack and memory, *quota* for effects, payload, timers, and telemetry. |
| **Solver profile** | Domain-specific solver settings stored in the scenario and opaque to the core. |
| **Run identity** | The run inputs together with the environment that executed them. |
| **Diagnostic log** | The exportable record of faults, admission results, limit events, and interventions. |
| **Capability profile** | On the Native Host only: an explicit grant of a resource outside the virtual world. |
| **Engine adapter, platform adapter** | The only Host-specific code: the first binds the kernel to a WebAssembly engine, the second to transport, storage, and pacing. |
| **View model, scene, shell** | The three layers of the UI: what is shown; what to draw; the toolkit-specific surface. |
| **Device model, sampled controller** | The two kinds of guest-backed actor in the circuit domain: evaluated by the solver without side effects, or activated at sample instants. |
| **Venue** | The market world's exchange, with its order book. |

Words retired from normative use: *Pattern 1*, *Pattern 2*, *Pattern 3*, *testbed* (use scenario or learning lab), *agent* and *node* as synonyms of actor (a *node* is a vertex of the topology graph; an *agent* is a market actor in the finance domain only), *simulation* as a name for the whole product.

## 2. Purpose and vision

florn lets a person construct a virtual world, place isolated executable actors into it, connect them, run the world under deterministic logical time, inspect what happens, reproduce it exactly, and move the same actors from the browser to a native machine without changing them.

The organising idea is: **emulate the component; simulate the world.** The actor runs real code. The Host approximates everything around it.

### 2.1 Personas

| Persona | Who | Uses |
|---|---|---|
| **Learner-operator** | The primary user of the first releases. Wants to understand *why* a system behaves as it does by watching it and changing conditions. | The graphical UI |
| **Module author** | Writes a guest in Rust (later other languages) against a contract. | The SDK, the conformance kit, the contract documentation |
| **Automation user** | Runs scenarios without a UI, in tests or scripts, and compares traces. | The headless runner, manifests, digests |
| **Domain developer** | Adds a domain to florn. | The core APIs |
| **Newcomer** | A person who did not build florn and opens it for the first time. The design target for clarity, even while the primary user is the owner. | The graphical UI |

### 2.2 First release direction

The first user-facing release (`0.2`) is a **network learning lab**: a small set of router actors, virtual links, an OSPF single-area subset, and a UI in which the user runs, breaks, inspects, saves, and replays. Circuit and finance follow in later releases, each used as a test of whether the core is really domain-neutral.

## 3. Scope

### 3.1 In scope

An actor runtime; a deterministic event kernel; a topology and message bus; module admission with resource limits; telemetry transport; a scenario manifest and package; a Browser Host with a graphical UI; a Native Host, headless from the first release and graphical later; three domains in sequence; persistence and replay; a guest SDK and a conformance kit.

### 3.2 Out of scope

| Excluded | Reason |
|---|---|
| Raw sockets, TAP/TUN, or kernel integration from the browser | Browser security model. A Native Host capability, planned after `0.6`. |
| Workloads beyond browser memory or compute: large finite-element, fluid, terabyte replay, millisecond production risk | The Native Host is the target for these. The browser's limits are scope limits, not defects. |
| Multiple users, roles, shared workspaces, authentication | florn is a single-user local tool. A managed deployment is a separate future design. |
| Distribution of one run across several browsers | Not planned. |
| Module and scenario registries | Not planned before 1.0. |
| Hardware-in-the-loop | After the Native Host exists, by a separate decision. |

## 4. Architectural principles (ARCH)

| ID | Requirement | Release | Source | Verify |
|---|---|---|---|---|
| ARCH-001 | **Actor-centred execution.** Every independently executable component whose behaviour a user may supply shall be representable as an isolated WebAssembly actor. | — | study | inspect |
| ARCH-002 | **Host owns the world.** Guest code shall implement only actor-local logic. The Host shall own logical time, event ordering, communication, world state, resource control, and domain-wide solving. | — | study | inspect |
| ARCH-003 | **Pure guest boundary.** A guest shall depend on no browser, DOM, timing, or operating-system interface. It shall import only the functions of its contract. | — | study | test |
| ARCH-004 | **Contracts in WIT.** Every Host–guest boundary shall be defined by a versioned WIT contract. The WIT text is the contract of record. A Rust trait may mirror it for Host-side built-ins but is not the contract. | `0.1` | study | inspect |
| ARCH-005 | **Common core, separate domains.** florn shall consist of one domain-neutral core and independent domain modules. It shall not be a single three-domain application. | — | study | inspect |
| ARCH-006 | **Domain-neutral core.** The core shall contain only what every domain needs with identical meaning: actors, time, events, graph connectivity, messaging, resources, observation. Packets, matrix stamps, orders, routing, nodal analysis, constraint solving, and order matching shall remain domain-owned. An interface enters the core contract only when two domains need it with the same semantics. | — | study | inspect |
| ARCH-007 | **Browser first, two Hosts from the start.** The Browser Host is the first user-facing target. The Native Host shall exist in headless form from release `0.1` and shall run every guest and scenario that the Browser Host runs. | `0.1` | study | test |
| ARCH-008 | **Graphical primary UI.** The primary interactive interface shall be graphical. A command-line interface serves automation and diagnostics; it shall not be the primary surface for authoring, topology, animation, or dense visual analysis. | `0.2` | study | inspect |
| ARCH-009 | **One Host core.** The kernel, bus, admission, replay, and domain solvers shall be one codebase compiled for the browser and for native targets. Host-specific code shall be confined to an engine adapter and a platform adapter. *Derived: two kernels would diverge and destroy cross-host reproducibility by construction.* | `0.1` | derived | inspect |
| ARCH-010 | **Composition not precluded.** The core shall not preclude several domain worlds in one scenario on one clock, nor an actor that implements contracts of more than one domain. No cross-domain feature is required before 1.0. *Derived: florn is a cyber-physical-systems project; a controller on a network acting on a plant is its destination.* | — | derived | inspect |

## 5. Actor lifecycle and admission (ACT)

| ID | Requirement | Release | Source | Verify |
|---|---|---|---|---|
| ACT-001 | **Module loading.** The Host shall load a module and instantiate one or more independent actors from the same compiled module. | `0.1` | study | test |
| ACT-002 | **Initialisation.** Each actor shall receive its stable identity and its domain-specific configuration at initialisation. | `0.1` | study | test |
| ACT-003 | **Isolation.** Each actor shall have its own linear memory. No actor shall read or write another actor's memory, and no actor shall receive a reference into Host memory. | `0.1` | study | test |
| ACT-004 | **Lifecycle control.** The Host shall support creation, start, reset, stop, and destruction of an actor. Pausing one actor while time advances for others is not a core operation; a domain may define an outage as an intervention. | `0.1` | study | test |
| ACT-005 | **Fault containment.** A trap, contract violation, or exhaustion of any limit in one actor shall end that activation with a recorded fault and shall not corrupt or stop the Host, the UI, or any other actor. The effects of the faulted activation shall be discarded. | `0.1` | study | test |
| ACT-006 | **Execution budget.** The Host shall enforce a configurable budget of executed work per activation, counted by florn in the same way under every engine. Exhaustion is a fault. A wall-clock watchdog may exist only as a defence against defects in the Host itself and shall never be the mechanism that decides a fault. | `0.1` | study | test |
| ACT-007 | **Reuse of compiled modules.** Instances of one module should share its compiled form. | `0.1` | study | analyse |
| ACT-008 | **State export.** The Host shall be able to export the complete state of an actor between activations and restore it into a fresh instance, such that execution continues with the same trace. Checkpoints that use this are a `0.3` feature; the export capability is `0.1` so that no earlier decision precludes it. | `0.1` | study | test |
| ACT-009 | **Module manifest.** A module shall carry a manifest that states its contract and version, the kind of actor it provides, its ports, its configuration parameters with types and units, and its resource needs. The Host shall reject a module without a readable manifest. *Derived: without it the UI cannot offer a configuration form and the Host cannot validate.* | `0.1` | derived | test |
| ACT-010 | **Admission.** Every module shall pass one admission step, identical under both Hosts: validate the binary and its features, read the manifest, estimate resources, transform the module for metering and limits, instantiate with granted capabilities. The transformation shall have a version, and that version shall be part of the run identity. *Derived: limits, containment, and cross-host determinism all depend on this step.* | `0.1` | derived | test |
| ACT-011 | **Faulted-actor policy.** What happens to an event addressed to a faulted actor shall be defined by the domain and declared in the scenario. *Derived: undefined behaviour here would make replay ambiguous.* | `0.2` | derived | test |

## 6. Deterministic kernel (DES)

RFC 001 is the normative definition of determinism. The requirements below state what the kernel must provide; the RFC states how.

| ID | Requirement | Release | Source | Verify |
|---|---|---|---|---|
| DES-001 | **Logical clock.** The Host shall maintain a logical clock independent of any wall clock. Protocol timers, solver steps, and market time all advance on it. | `0.1` | study | test |
| DES-002 | **Event queue.** The kernel shall handle events in a total order defined by RFC 001, independent of arrival order, thread, or wall clock. | `0.1` | study | test |
| DES-003 | **Event kinds.** The kernel shall support message delivery, timer expiry, scheduled changes, interventions, and domain-defined events, each with a class that participates in ordering. | `0.1` | study | test |
| DES-004 | **Time control.** The Host shall advance a run by one event, one instant, one tick, or to a named tick, and shall support pacing: pause, run at a chosen speed, run as fast as possible. Pacing shall not influence the trace. | `0.1` | study | test |
| DES-005 | **Ordering of equal-time events.** Events at one instant shall be ordered by the key of RFC 001. The scenario shall select the tie policy. | `0.1` | study | test |
| DES-006 | **Reproducibility tiers.** florn shall meet the three tiers of RFC 001: identical traces for the same build; identical traces across Hosts for discrete domains and agreement within a documented tolerance for continuous domains; and, before 1.0, availability of every released version so that a run can be reproduced under the version that produced it. | `0.1` | study | test |
| DES-007 | **Replay.** The Host shall re-execute a run from its inputs and report whether the digest matches the stored one, naming the first interval of divergence if not. Playback of a stored trace is a separate display feature and shall not be called replay. | `0.1` | study | test |
| DES-008 | **Branching.** The Host should allow a new run to begin from a checkpoint of another run, with its own inputs from that instant, and record the parent in its identity. | `0.3` | study | test |
| DES-009 | **No browser timing.** No browser timer, animation frame, or event-loop ordering shall influence the logical world. | — | study | test |
| DES-010 | **Time representation.** Logical time shall be an unsigned 64-bit tick count. The scenario declares the tick length (1 ns, 1 ps, or 1 fs). No floating-point value shall represent time in the kernel or cross a contract as a time. | `0.1` | study | test |
| DES-011 | **Interventions are inputs.** Every change a user or script makes to a running world shall be applied at an instant boundary, recorded with its instant and content as a run input, and applied again on replay. *Derived: a learning lab is built on breaking things during a run; such a run must be reproducible.* | `0.2` | derived | test |
| DES-012 | **Observation without effect.** Inspecting an actor, a message, or the world shall not change the trace. The Host shall not call into a live guest for inspection. *Derived: an inspector that consumes a guest's budget would make the UI part of the simulation.* | `0.1` | derived | test |
| DES-013 | **Seeded randomness.** A run shall have one root seed. Every consumer — actor, connection, world component — shall draw from a stream derived from the seed and its own stable identity, so that adding an actor leaves other streams unchanged. Guests shall have no other source of randomness. | `0.1` | study | test |
| DES-014 | **Zero-delay budget.** The number of microsteps per tick and of events per instant shall be limited. Exceeding a limit ends the run with a fault that names the origins involved. *Derived: an endless zero-delay chain would otherwise stop time without a reason.* | `0.1` | derived | test |

## 7. Topology and messaging (BUS)

| ID | Requirement | Release | Source | Verify |
|---|---|---|---|---|
| BUS-001 | **Graph model.** The core shall represent nodes, ports, and connections. A connection shall join two or more ports. Direction, latency, loss, bandwidth, and every other attribute of a connection shall be defined by the domain and opaque to the core. | `0.1` | study | inspect |
| BUS-002 | **Mediated delivery.** All actor-to-actor communication shall be event-driven through Host queues. Guests shall never share memory. | `0.1` | study | test |
| BUS-003 | **Connection effects.** A domain shall be able to model at least delay and loss on a connection, and should be able to model jitter and bandwidth where meaningful. | `0.2` | study | test |
| BUS-004 | **Message identity.** Every message visible to observation shall have an identifier derived from its origin and sequence number, stable across replays. *Derived: tracing a message from send to receive needs a stable name.* | `0.1` | derived | test |
| BUS-005 | **Opaque payloads.** The core shall carry domain payloads without interpreting them. | `0.1` | study | inspect |
| BUS-006 | **Host mediation.** Every interaction that crosses a connection shall pass through the Host. | `0.1` | study | test |
| BUS-007 | **Deterministic delivery.** Given the same run inputs, delivery shall be identical. | `0.1` | study | test |
| BUS-008 | **Topology editing.** The UI shall let a user create, delete, connect, disconnect, and configure nodes and connections appropriate to the domain, before a run and — as interventions — during one. | `0.2` | study | demo |
| BUS-009 | **Built-in actors.** A domain may provide actors implemented by the Host. A scenario shall be able to reference a built-in by name in the same place it references a module. *Derived: passive elements and noise sources need no sandbox and would cost a boundary crossing each.* | `0.2` | derived | test |

## 8. Observation and telemetry (OBS)

| ID | Requirement | Release | Source | Verify |
|---|---|---|---|---|
| OBS-001 | **Off the main thread.** In the Browser Host, the kernel and the guests shall run outside the main thread. | `0.1` | study | test |
| OBS-002 | **Efficient transport.** Where the browser permits shared memory, the Browser Host shall use a shared ring buffer for high-rate telemetry. Where it does not, a message-passing path shall work without any change in the core. | `0.2` | study | test |
| OBS-003 | **Reduce before display.** The Host shall aggregate, sample, or thin high-rate data before it reaches the display path. Loss on the display path shall never touch the trace. | `0.2` | study | test |
| OBS-004 | **Timeline.** The user shall be able to inspect an ordered timeline of events: actor, message, timer, fault, intervention, and domain events. | `0.2` | study | demo |
| OBS-005 | **Actor inspection.** The user shall be able to select an actor and see its configuration, status, faults, and the telemetry it has reported. | `0.2` | study | demo |
| OBS-006 | **Message inspection.** Where a domain defines structured payloads, the user shall be able to inspect the raw bytes and the decoded fields. Decoders are domain-provided Host code or sandboxed modules; a guest's memory is never read for this. | `0.2` | study | demo |
| OBS-007 | **Display rate.** Display shall target the refresh rate of the screen, at least 60 frames per second, under the reference workload of NFR-013. | `0.2` | study | analyse |
| OBS-008 | **Binary hot paths.** High-rate telemetry shall use a binary representation. No text serialisation on a hot path. | `0.2` | study | inspect |
| OBS-009 | **Domain views.** Each domain shall provide its own views rather than a generic one. | `0.2` | study | inspect |
| OBS-010 | **Detail by replay.** Detail for a window of time should be produced by replaying that window with recording on, rather than by storing the full trace. *Derived: determinism makes storage of every record unnecessary and keeps the browser within memory.* | `0.3` | derived | test |

## 9. Browser Host (WEB)

| ID | Requirement | Release | Source | Verify |
|---|---|---|---|---|
| WEB-001 | **Zero install.** The Browser Host shall be usable by opening a URL. | `0.2` | study | demo |
| WEB-002 | **Local execution.** All simulation shall execute locally. No scenario, module, or result shall leave the machine unless the user exports it. | `0.2` | study | test |
| WEB-003 | **Offline.** After one visit, the application shall start and run without network access. | `0.2` | study | test |
| WEB-004 | **Installable.** The application should be installable as a progressive web application. | `0.3` | study | demo |
| WEB-005 | **Import of modules.** The user shall be able to import a module from a local file. The module is admitted (ACT-010) before it can act; its manifest and hash are shown. | `0.3` | study | demo |
| WEB-006 | **Sandboxed execution.** An imported module shall receive only the functions of its contract. No browser or system privilege. | `0.1` | study | test |
| WEB-007 | **Limits made visible.** When a scenario exceeds the practical limits of the browser, the UI shall say so, with the limit that was reached, before or when it happens. It shall not fail silently or ambiguously. | `0.2` | study | demo |
| WEB-008 | **Browser is one target.** The browser's capabilities shall not define florn. Workloads beyond them belong to the Native Host, not to workarounds. | — | study | inspect |
| WEB-009 | **No third-party requests.** The application shall make no request to any origin other than its own, and shall contain no analytics or telemetry about its use. A Content Security Policy shall enforce this. *Derived: makes the local-first promise technically enforced and testable.* | `0.2` | derived | test |
| WEB-010 | **Version pinning.** Each released version shall remain available at an immutable address, so that a run recorded under one version can be reproduced under it. *Derived: a self-updating application would otherwise silently change the Host under an old run.* | `0.2` | derived | inspect |
| WEB-011 | **Support matrix.** The project shall publish which browsers and versions are supported, which features are required, and which are optional with their fallbacks. *Derived: shared memory, GPU access, and 64-bit memory differ by engine.* | `0.2` | derived | inspect |
| WEB-012 | **Durable storage is the file.** Scenarios and results stored inside the browser are a cache. Export to a file is the durable path, and the UI shall make the difference visible. *Derived: browser origin storage is evictable.* | `0.2` | derived | demo |

## 10. Native Host (MIG)

| ID | Requirement | Release | Source | Verify |
|---|---|---|---|---|
| MIG-001 | **Same module, both Hosts.** A module that conforms to its contract shall run under the Browser Host and the Native Host without any change, and shall produce the same trace within the tier that applies. | `0.1` | study | test |
| MIG-002 | **Wasmtime engine.** The Native Host shall run guests on Wasmtime, with the same admission step as the Browser Host. | `0.1` | study | test |
| MIG-003 | **Adapter substitution.** The two Hosts shall differ only in their engine adapter and platform adapter (ARCH-009). | `0.1` | study | inspect |
| MIG-004 | **No serialisation in-process.** In the native application, the Host, solvers, telemetry, and UI should share one process without a network or serialisation boundary. | `0.6` | study | inspect |
| MIG-005 | **Native graphical UI.** The native application should use a Rust GPU-rendered toolkit. The choice is made when `0.6` is designed. | `0.6` | study | demo |
| MIG-006 | **Explicit native capabilities.** The Native Host may offer capabilities the browser cannot — files, network interfaces, large datasets — only through explicit, separately granted capability profiles. None is implied by running natively. | `later` | study | test |
| MIG-007 | **Scale target.** The Native Host is the planned target when a workload exceeds browser limits. | — | study | inspect |
| MIG-008 | **No native-only assumptions in contracts.** No contract shall assume a native capability. | — | study | inspect |

## 11. Control interface (CTL)

The control interface is the fourth contract, between the Host and any front end.

| ID | Requirement | Release | Source | Verify |
|---|---|---|---|---|
| CTL-001 | **One interface for every front end.** The Host shall expose one transport-independent interface of commands, queries, and telemetry subscriptions. The browser UI, the native UI, and the headless runner shall be equal clients of it. *Derived: without it each front end couples to Host internals and the UI is written twice.* | `0.1` | derived | inspect |
| CTL-002 | **Validation.** The Host shall validate every command. A command that changes the logical world is an intervention (DES-011). | `0.1` | derived | test |
| CTL-003 | **Messages as codes.** Every message the Host produces for a user — error, warning, fault, limit — shall be a stable code with typed parameters. Text is produced by the front end from a translation catalogue. *Derived: required by internationalisation; English sentences from the Host could not be translated.* | `0.1` | derived | test |
| CTL-004 | **Three error families.** The design shall distinguish errors returned to a guest, guest faults observed by the Host, and errors presented to a user, and shall not conflate them. | `0.1` | derived | inspect |

## 12. Network domain (NET)

| ID | Requirement | Release | Source | Verify |
|---|---|---|---|---|
| NET-001 | **Router actors.** A network node shall be an actor with isolated routing state. | `0.2` | study | test |
| NET-002 | **Network contract.** The network contract shall provide initialisation, packet reception, timer expiry with cancel, packet transmission, and structured telemetry. It shall provide no periodic tick. | `0.2` | study | test |
| NET-003 | **Interfaces.** A node shall expose one or more virtual interfaces that connect through the topology. | `0.2` | study | test |
| NET-004 | **OSPF subset.** The first release shall ship an OSPF guest implementing a declared single-area subset: neighbour state machine, link-state database, shortest-path calculation, routing table. The subset is declared in the network domain RFC; it is a guest deliverable, not a platform feature. | `0.2` | study | demo |
| NET-005 | **BGP subset.** A later release shall ship a BGP guest with a declared subset: peer state machine, Adj-RIB-In, Loc-RIB, Adj-RIB-Out, and path selection by local preference, AS path, and MED. | `0.3` | study | demo |
| NET-006 | **Link effects.** Links shall support configurable delay and loss, and should support jitter and bandwidth. | `0.2` | study | test |
| NET-007 | **Packet inspection.** The user shall inspect raw and decoded packets, including OSPF link-state advertisements and BGP UPDATE attributes where those guests exist. | `0.2` | study | demo |
| NET-008 | **Packet animation.** The UI shall show packets moving on links and link state changes, without one display update per kernel event. | `0.2` | study | demo |
| NET-009 | **Protocol timeline.** Neighbour and peer state changes, sends and receives, route updates, and timer expiries shall appear in the timeline. | `0.2` | study | demo |
| NET-010 | **Learning labs.** The network domain shall ship at least one learning lab: a topology, a story of what to watch, and the interventions to try. | `0.2` | study | demo |
| NET-011 | **No real network from the browser.** Access to physical routers, kernels, raw sockets, or TAP/TUN is not a Browser Host feature. | — | study | inspect |
| NET-012 | **Guest ladder.** Before OSPF, the domain shall ship an echo guest and a distance-vector guest, used as SDK examples and conformance fixtures. *Derived: the first guests must be small enough to validate the contract before protocol complexity arrives.* | `0.2` | derived | test |

## 13. Circuit and control domain (PHY)

| ID | Requirement | Release | Source | Verify |
|---|---|---|---|---|
| PHY-001 | **Component actors.** Controllers, behavioural sub-circuits, sensors, and actuators shall be representable as actors. Passive elements shall be built-ins (BUS-009). | `0.4` | study | test |
| PHY-002 | **Two kinds of actor.** The circuit contract shall distinguish a *device model*, which the solver evaluates at trial operating points without side effects and with commit and rollback, from a *sampled controller*, which runs at sample instants only and is never called inside an iteration. *Derived: a controller called inside nonlinear iteration is corrupted; a device model that keeps state across trials diverges.* | `0.4` | derived | test |
| PHY-003 | **Nodal analysis.** The circuit solver shall use modified nodal analysis as its baseline formulation. | `0.4` | study | test |
| PHY-004 | **Nonlinear iteration.** The solver shall provide Newton-type iteration with convergence control. | `0.4` | study | test |
| PHY-005 | **Transient integration.** The solver shall support trapezoidal and Gear-type integration. | `0.4` | study | test |
| PHY-006 | **Sparse solving.** Large systems shall be stored sparsely and solved with sparse direct or iterative methods, using permissively licensed pure-Rust libraries. | `0.4` | study | analyse |
| PHY-007 | **Mechanical coupling.** The domain should support rigid-body or constraint-based coupling. | `later` | study | test |
| PHY-008 | **Controller software-in-the-loop.** A controller guest shall receive sampled inputs from the world and return outputs that the Host applies to the world. | `0.4` | study | test |
| PHY-009 | **Time coupling.** The circuit domain RFC shall define how solver steps and the event queue advance together: sample instants, solver-detected conditions as events, and step boundaries on ticks. *Derived: discrete and continuous time were never reconciled.* | `0.4` | derived | test |
| PHY-010 | **Sub-stepping.** The solver shall step independently of display and shall support fixed and adaptive step policies. | `0.4` | study | test |
| PHY-011 | **Waveforms.** The user shall inspect time series of voltages, currents, positions, forces, and controller outputs. | `0.4` | study | demo |
| PHY-012 | **Tolerance.** The circuit domain is continuous. Its RFC shall state the tolerance and comparison method for Tier 2 of RFC 001. | `0.4` | derived | inspect |

## 14. Finance domain (FIN)

| ID | Requirement | Release | Source | Verify |
|---|---|---|---|---|
| FIN-001 | **Agent actors.** Strategies, market makers, flow models, and pricing or risk components shall be actors. | `0.5` | study | test |
| FIN-002 | **Market contract.** The market contract shall provide initialisation, market data, execution reports, order submission and cancellation, and structured telemetry. Prices and quantities shall be integers in declared units. | `0.5` | study | test |
| FIN-003 | **Venue.** The Host shall provide a venue with a central limit order book. | `0.5` | study | test |
| FIN-004 | **Matching.** Matching shall be price–time priority, deterministic, with the seeded tie policy of RFC 001 available for equal-time orders. | `0.5` | study | test |
| FIN-005 | **Latency.** Agent-to-venue latency shall be configurable per agent. | `0.5` | study | test |
| FIN-006 | **Exogenous events.** The world should generate seeded noise flow and scheduled shocks. | `0.5` | study | test |
| FIN-007 | **Book and tape views.** The user shall inspect depth, executions, price history, and per-agent profit and loss. | `0.5` | study | demo |
| FIN-008 | **Aggregation.** High-volume trade data shall be aggregated before display. | `0.5` | study | test |
| FIN-009 | **Pricing actors.** The architecture should allow a pricing or risk actor to compute internally and report summaries. | `later` | study | test |
| FIN-010 | **Statistical reduction.** Large trial output should be reduced in the actor or the Host, never shipped raw to the display. | `later` | study | test |
| FIN-011 | **Replay readiness.** The agent contract shall permit later replacement of the simulated venue by replay of recorded market data on the Native Host. | `0.5` | study | inspect |
| FIN-012 | **Discrete domain.** The finance domain is discrete under RFC 001: no floating point in any Host computation that reaches the trace. | `0.5` | derived | test |

## 15. Numerics (NUM)

| ID | Requirement | Release | Source | Verify |
|---|---|---|---|---|
| NUM-001 | **Vectorisation.** Host numerics may use fixed-width SIMD where it does not change results within the domain's tier. | `later` | study | analyse |
| NUM-002 | **Single-threaded semantics.** The kernel's single-threaded order is normative. Parallel execution may be introduced later only if it produces the identical trace. | — | study | test |
| NUM-003 | **Memory-aware solvers.** Browser-side solvers shall be designed for the browser memory ceiling and shall estimate their need at admission. | `0.4` | study | analyse |
| NUM-004 | **Graceful degradation.** Near a resource ceiling the application should reduce display detail and telemetry before touching correctness. | `0.2` | derived | demo |
| NUM-005 | **Explicit step control.** Solvers shall expose step and convergence parameters; the display rate is never the step. | `0.4` | study | inspect |
| NUM-006 | **Numeric policy.** Money and quantities cross contracts as integers in declared units; physical values as 64-bit floats in SI units; one mathematical library on every target; NaN canonical at contracts and in the trace (RFC 001 §12). | `0.1` | derived | test |

## 16. Security (SEC)

| ID | Requirement | Release | Source | Verify |
|---|---|---|---|---|
| SEC-001 | **Guests are untrusted.** Every guest shall be treated as hostile code. | — | study | inspect |
| SEC-002 | **Capability by contract.** A guest shall receive only the functions of its contract, bound at admission. Nothing is ambient. | `0.1` | study | test |
| SEC-003 | **No ambient I/O.** No guest shall have network, file, DOM, clock, or process access in any Host. The Native Host grants extra capabilities only under MIG-006. | — | study | test |
| SEC-004 | **Quotas.** Memory, execution budget, stack depth, effects per activation, payload size, pending timers, and telemetry volume shall be limited per actor or actor class, with defaults set by the Host and tightenable by the scenario. | `0.1` | study | test |
| SEC-005 | **Malformed input.** The Host shall validate module binaries, manifests, scenario files, payload sizes, port identifiers, and resource requests before use, and shall bound every allocation sized by untrusted input. Admission shall estimate resources before instantiating anything. | `0.1` | derived | test |
| SEC-006 | **Confidential by default.** Nothing leaves the machine unless the user exports it (WEB-002, WEB-009). | `0.2` | study | test |
| SEC-007 | **Safe sharing.** A shared package or URL shall never embed scenario contents or results without the user's explicit action and a visible indication of what is included. | `0.3` | study | demo |
| SEC-008 | **Diagnostic log.** Faults, admission results, limit events, and interventions shall be recorded in a diagnostic log that is exportable. | `0.1` | derived | test |
| SEC-009 | **Native boundary.** Any Native Host capability that touches real resources shall be explicit, constrained to a profile, and separate from the baseline model. | `later` | study | inspect |
| SEC-010 | **Single local user.** florn is a single-user local tool. It shall have no authentication, roles, or shared workspaces. Trust states of a module are limited to *validated* and *rejected*; provenance is displayed, not enforced. A managed multi-user deployment is a separate future design. *Derived: a role model without an identity to bind to is a claim the product cannot honour.* | — | derived | inspect |
| SEC-011 | **Threat model maintained.** The project shall keep a threat model scoped to what is built, and shall update it in any release that adds a data flow, an external integration, or a capability. | `0.1` | derived | inspect |
| SEC-012 | **Dependency policy.** All linked dependencies shall carry permissive licences, checked automatically, and advisories shall be checked on every build. | `0.1` | derived | test |

## 17. Data and persistence (DATA)

| ID | Requirement | Release | Source | Verify |
|---|---|---|---|---|
| DATA-001 | **Manifest.** A scenario shall be stored as a versioned text manifest, readable in a diff, containing: domain(s), actors with module or built-in references, configuration, ports, connections and their domain attributes, tick length, tie policy, limits, seed, scheduled changes, solver profile, view metadata. | `0.1` | study | test |
| DATA-002 | **Package.** A package shall bundle a manifest with the module bytes it references, for sharing. A manifest alone references modules by hash. | `0.3` | study | test |
| DATA-003 | **Module identity.** Every module shall be identified by the hash of its bytes. A run records the hash of every module and the admission version. | `0.1` | study | test |
| DATA-004 | **Save and load.** The user shall save a scenario to a file and load it later. | `0.2` | study | demo |
| DATA-005 | **Checkpoints.** The Host shall capture and restore checkpoints between instants, validated against scenario revision, module hashes, and admission version before restore. | `0.3` | study | test |
| DATA-006 | **Trace export.** Traces and the diagnostic log shall be exportable independently of display. | `0.1` | study | test |
| DATA-007 | **Schema versions.** Manifest, trace, and package formats shall carry a version. A newer florn shall detect an older format and migrate or refuse explicitly. | `0.1` | derived | test |
| DATA-008 | **Large output.** Results shall stream, aggregate, or chunk; the browser shall never be required to hold all raw results at once. | `0.3` | study | analyse |
| DATA-009 | **Canonical form.** Manifests shall have one canonical serialisation for hashing. | `0.1` | derived | test |
| DATA-010 | **Units.** Every quantity a user or author writes — time, money, physical value — shall carry an explicit unit. A unitless quantity is a validation error. | `0.1` | derived | test |
| DATA-011 | **Derived identifiers.** Identifiers inside the logical world shall be derived from origin and sequence, never random or clock-based. Wall-clock data is metadata and excluded from comparison. | `0.1` | derived | test |
| DATA-012 | **Static and runtime state.** Scenario state, run state, event state, actor state, and observation state shall be distinct in the design; a saved scenario shall contain no runtime internals. | `0.1` | derived | inspect |

## 18. User interface (UI)

| ID | Requirement | Release | Source | Verify |
|---|---|---|---|---|
| UI-001 | **Domain workspace.** Each domain shall have a workspace optimised for it, sharing common navigation and run controls. | `0.2` | study | demo |
| UI-002 | **Graphical authoring.** The user shall construct and edit the world graphically. | `0.2` | study | demo |
| UI-003 | **Run controls.** Run, pause, step (with the unit shown), speed, reset, go to time. Each control shall say what it will do. | `0.2` | study | demo |
| UI-004 | **Inspectors.** Contextual inspection of actors, connections, messages, solver state, and telemetry. | `0.2` | study | demo |
| UI-005 | **Multi-panel layout.** Topology or scene, timeline, inspector, and plots should be visible together. | `0.2` | study | demo |
| UI-006 | — retired, see §23 — | | | |
| UI-007 | **Display independent of stepping.** Rendering shall never block on, nor be blocked by, the kernel. | `0.2` | study | test |
| UI-008 | **Accessibility baseline.** Keyboard operability of every control, visible focus, text alternatives for critical information, status never encoded by colour alone. Canvas content shall have an accessible counterpart. The exact conformance target is decided in the UI external design. | `0.2` | derived | demo |
| UI-009 | **Actionable errors.** Module faults, limit events, invalid scenarios, and non-convergence shall be presented as states with a cause and a next step, never as a generic failure. | `0.2` | derived | demo |
| UI-010 | — retired, see §23 — | | | |
| UI-011 | **Internationalisation.** The UI shall support multiple languages. Every user-visible string comes from a translation catalogue. The language switches without reload. First release: English and Japanese. Text input shall work with input methods. *Derived: required by the project rules; absent from revision 1.* | `0.2` | derived | demo |
| UI-012 | **Less is more.** The default view shall show the minimum needed for the current task; advanced information is reachable on request. The newcomer persona is the design target. | `0.2` | derived | demo |
| UI-013 | **Three-layer UI.** The UI shall separate a view model and a scene (framework-independent, compiled for browser and native) from a shell that depends on the toolkit. *Derived: no toolkit serves browser and native equally; the costly parts are written once.* | `0.2` | derived | inspect |
| UI-014 | **Limits of emulation stated.** Where a result is shown, the UI and documentation shall state that guests run under zero-time execution without an operating system, and what that does and does not prove. | `0.2` | derived | inspect |

## 19. Headless operation and automation (AUTO)

| ID | Requirement | Release | Source | Verify |
|---|---|---|---|---|
| AUTO-001 | **Headless runner.** The Native Host shall run any scenario from the command line without a UI and print its digest. | `0.1` | study | test |
| AUTO-002 | **Scriptable scenarios.** Manifests shall be runnable in automated tests. | `0.1` | study | test |
| AUTO-003 | **Regression scenarios.** The project shall keep deterministic regression scenarios for the core and each domain, with recorded digests. | `0.1` | derived | test |
| AUTO-004 | **Golden results.** Continuous domains shall compare against recorded results within their tolerance. | `0.4` | derived | test |
| AUTO-005 | **Hostile cases.** Tests shall cover traps, budget exhaustion, stack and memory limits, malformed payloads, invalid manifests, bad pointers, and non-convergence. | `0.1` | derived | test |
| AUTO-006 | **Cross-host validation.** Every regression scenario shall run under both Hosts in continuous integration and compare digests. | `0.1` | study | test |
| AUTO-007 | **Conformance kit.** The project shall ship a kit that checks a guest against its contract and a Host against RFC 001, usable by a module author without the rest of florn. | `0.1` | derived | test |

## 20. Packaging (PKG)

| ID | Requirement | Release | Source | Verify |
|---|---|---|---|---|
| PKG-001 | **Core packaging.** The core shall be consumable by domain modules without duplication, as crates in one workspace. | `0.1` | study | inspect |
| PKG-002 | **Domain modules.** Each domain shall be a separately maintainable set of crates and WIT packages. | `0.2` | study | inspect |
| PKG-003 | **Contract versioning.** Every WIT package shall be versioned and tested as an API. Before 1.0 contracts are unstable and say so. | `0.1` | derived | test |
| PKG-004 | **Guest SDK.** florn shall provide a Rust SDK and template for writing a conforming guest with no browser or system dependency, from release `0.1`. | `0.1` | study | demo |
| PKG-005 | **Example scenarios.** Each domain shall ship at least one reproducible example scenario with its recorded digest. | `0.2` | derived | test |
| PKG-006 | **Compatibility policy.** The project shall publish which interfaces are stable from which version and for how long old guests, manifests, and traces remain readable. | `0.1` | derived | inspect |
| PKG-007 | **Naming.** Crates are prefixed `florn-`; WIT packages live under the `florn:` namespace. | `0.1` | derived | inspect |

## 21. Quality attributes (NFR)

| ID | Requirement | Release | Source | Verify |
|---|---|---|---|---|
| NFR-001 | — retired, see §23 — | | | |
| NFR-002 | — retired, see §23 — | | | |
| NFR-003 | **Responsiveness.** The browser main thread shall remain interactive during any run. | `0.2` | study | demo |
| NFR-004 | **Performance transparency.** The UI shall show whether a run is limited by actor execution, solver cost, telemetry, display, or memory. | `0.3` | derived | demo |
| NFR-005 | **Browser scale.** The Browser Host shall support the actor count measured in Stage 0 with margin; the figure is recorded in NFR-013. Dozens to hundreds of actors is the expectation; it is not an API limit. | `0.2` | study | analyse |
| NFR-006 | **Native scale.** The Native Host shall have a credible path to thousands of actors without unsafe techniques. | `0.6` | study | analyse |
| NFR-007 | — retired, see §23 — | | | |
| NFR-008 | **Maintainability.** The core shall remain free of domain assumptions; each new domain is a test of this. | — | study | inspect |
| NFR-009 | **Observability.** Every execution layer shall provide enough diagnostics to investigate a determinism discrepancy. | `0.1` | study | inspect |
| NFR-010 | — retired, see §23 — | | | |
| NFR-011 | **Run identity.** Every run shall record florn version, Host build, engine, CPU architecture, module hashes, admission version, contract versions, scenario hash, seed, and interventions. | `0.1` | derived | test |
| NFR-012 | **Upgrade safety.** Changes to core interfaces shall include compatibility tests against representative domain modules and guests. | `0.2` | derived | test |
| NFR-013 | **Performance budgets.** The project shall record measured budgets — events per second, activations per millisecond, actors per browser, memory per actor, cold start, frame time — set from Stage 0 measurements and checked in continuous integration. | `0.1` | derived | analyse |
| NFR-014 | **Zero-time execution stated.** The model that guest execution takes zero logical time shall be documented as an assumption, with the metered work recorded per activation for a possible later execution-time model. | `0.1` | derived | inspect |

## 22. Acceptance criteria by release

Each criterion names the requirements it demonstrates. A release is accepted when every criterion of its row is shown.

### 22.1 Release 0.1 — Core on two Hosts

| ID | Criterion | Demonstrates |
|---|---|---|
| AC-001 | One guest module, unchanged, produces the same digest under the Browser Host (Chromium, Firefox, WebKit) and the Native Host (x86-64, AArch64). | MIG-001, MIG-002, DES-006, AUTO-006 |
| AC-002 | A guest that loops, recurses, grows memory, floods, or passes a bad pointer faults at the same metered point under every engine; the Host and other actors continue; its effects are discarded. | ACT-005, ACT-006, SEC-004, SEC-005 |
| AC-003 | A run is replayed from its inputs with an equal digest; a deliberately altered module is detected by hash. | DES-007, DATA-003, NFR-011 |
| AC-004 | Two actors produce equal-time events; the order follows the tie policy and is unchanged when the actors are created in reverse order. | DES-005 |
| AC-005 | A module without a manifest is rejected with a coded message; a module with one is admitted and its parameters are listed by the headless runner. | ACT-009, ACT-010, CTL-003 |
| AC-006 | An actor's state is exported between activations and restored into a fresh instance; the digest equals an uninterrupted run. | ACT-008 |
| AC-007 | The conformance kit runs against the echo guest and against both Hosts and passes. | AUTO-007, PKG-004 |

### 22.2 Release 0.2 — Network learning lab

| ID | Criterion | Demonstrates |
|---|---|---|
| AC-008 | **Journey 1.** The user opens a bundled learning lab, runs it, and watches neighbours form and routes converge in the topology, timeline, and inspector. | NET-004, NET-008, NET-009, OBS-004, OBS-005, UI-003 |
| AC-009 | **Journey 3.** The user breaks a link during the run, watches reconvergence, steps through events, and inspects a packet. The run with the intervention replays with an equal digest. | DES-011, BUS-008, NET-007, DES-004 |
| AC-010 | **Journey 4.** The user saves the scenario to a file, reloads it, and replays the same run including the intervention. | DATA-004, WEB-012 |
| AC-011 | **Journey 2.** The user builds a topology from nothing: places routers, connects them, sets link delay and loss. | UI-002, BUS-008, NET-006 |
| AC-012 | The UI switches between English and Japanese without reload; Japanese text is entered through an input method into a form field and shown on the canvas. | UI-011, CTL-003 |
| AC-013 | With the network inspector open, the application makes no request to any other origin; the Content Security Policy is verified. | WEB-009, WEB-002, SEC-006 |
| AC-014 | The application starts offline after one visit. | WEB-003 |
| AC-015 | Every control is reachable and operable by keyboard; a node can be selected without a pointer. | UI-008 |
| AC-016 | The owner completes journeys 1 to 4 and confirms that each control did what its label led them to expect. | UI-012, UI-009 |

### 22.3 Later releases

| Release | Criterion |
|---|---|
| `0.3` | A run restored from a checkpoint continues with the digest of an uninterrupted run. A user imports a module through the UI and runs it. A BGP guest exchanges routes. |
| `0.4` | A controller guest and a Host-side plant run in closed loop; results under both Hosts agree within the declared tolerance; no change to the core was needed, or each change is recorded as a finding. |
| `0.5` | Several agent guests trade against the venue; the digest is identical under both Hosts; no floating point reaches the trace. |
| `0.6` | Every scenario of earlier releases runs unchanged in the native application. |

## 23. Retired identifiers

Retired identifiers are kept here so that references in older material still resolve. They are never reused.

| Retired | Merged into | Reason |
|---|---|---|
| UI-006 | OBS-006, NET-007 | Duplicate statement of inspection |
| UI-010 | ARCH-008 | Duplicate statement of GUI primacy |
| NFR-001 | DES-006 | Determinism is defined once, by the tiers |
| NFR-002 | ACT-005 | Duplicate statement of fault containment |
| NFR-007 | MIG-001 | Duplicate statement of portability |
| NFR-010 | WEB-002, WEB-003 | Duplicate statement of offline capability |
| SEC-005 (rev. 1) | ACT-005 | Duplicate; the SEC-005 identifier now carries *Malformed input*, which was SEC-006 in revision 1 |
| SEC-006 to SEC-010 (rev. 1) | SEC-006 to SEC-009 (rev. 2), MIG-006 | Renumbered within the group during the merge; see the mapping note |
| PHY-012 (rev. 1) | ACT-005 | Duplicate of fault containment; the identifier now carries *Tolerance* |
| NET-007 (rev. 1) | DES-001 | Duplicate of logical clock; NET-007 to NET-012 shifted by one |
| FIN-006 (rev. 1) | DES-013 | Randomness is a core requirement; FIN-006 to FIN-012 shifted by one |
| NUM-006 (rev. 1) | NUM-006 (rev. 2) | Rewritten as the numeric policy of RFC 001 |

**Note on renumbering.** Revision 1 was a private draft that no code, RFC, or test cited. Renumbering inside a group was therefore done once, here, to remove duplicates without leaving gaps. From this revision on, identifiers are frozen (§0.3) and gaps are left where identifiers retire.

### 23.1 Identifiers whose meaning changed between revisions

| Identifier | Revision 1 | Revision 2 |
|---|---|---|
| ACT-008 | Snapshot/reset "where feasible" | State export, mandatory in `0.1`; checkpoints in DATA-005 |
| DATA-002 | Separation of static and runtime state | Package; the separation moved to DATA-012 |
| PHY-002 | Physical/circuit WIT | Two kinds of actor (device model, sampled controller) |
| PHY-009 | Sensor/actuator loop | Time coupling; the loop is in PHY-008 |
| PHY-012 | Fault containment | Tolerance for Tier 2 |
| SEC-005 … SEC-009 | Crash containment, malformed input, confidentiality, safe sharing, auditability | Shifted up by one after SEC-005 retired: malformed input, confidentiality, safe sharing, diagnostic log, native boundary |
| NET-007 … NET-011 | Logical time, packet inspection, animation, timeline, learning use case | Shifted up by one after NET-007 retired: packet inspection, animation, timeline, learning labs, no real network |
| FIN-006 … FIN-011 | Deterministic randomness, noise, book view, aggregation, pricing, statistical reduction | Shifted up by one after FIN-006 retired: exogenous events, book and tape, aggregation, pricing, statistical reduction, replay readiness |
| NFR-011 | Reproducible distribution | Run identity (same intent, precise content) |
| NUM-006 | Reproducible floating-point policy | Numeric policy of RFC 001 §12 |

## 24. Open items

Where each item left open at acceptance was decided; items 3, 6 and 7 wait for the domain RFCs and the native shell.

| # | Item | Decided in |
|---|---|---|
| 1 | Exact WIT types, error model, and core contract | RFC 002 and RFC 007, implemented in 0.1.0 |
| 2 | Browser support matrix and required features | Chromium, Firefox and WebKit, checked in CI from 0.1.0; the isolation headers and the message fallback settled by the transport PoC (RFC 005 rule 33); WEB-011 |
| 3 | Numerical libraries for circuit and finance | Domain RFCs, under SEC-012 |
| 4 | Manifest and package format | RFC 004, implemented in 0.1.0 |
| 5 | UI toolkit for the browser | Leptos, confirmed by the UI shell PoC; RFC 005 rule 38; shipped in 0.2.0 |
| 6 | UI toolkit for the native application | `0.6` design |
| 7 | Tolerances per continuous domain | Domain RFCs |
| 8 | Default limits and budgets | Measured and fixed in 0.1.0 (RFC 001 and RFC 003 open questions; the budget default 100,000,000); NFR-013 |
| 9 | Accessibility conformance target | WCAG 2.2 AA, external design §10 |
| 10 | Whether metered work is compared across Hosts | Yes: the count is identical under every engine by construction (RFC 003's note on RFC 001 open question 5) |
| 11 | Native capabilities, TAP/TUN, hardware-in-the-loop | After `0.6` |

## 25. Change history

| Revision | Date | Change |
|---|---|---|
| 1 | 2026-09-28 | Baseline derived from the founding study. Private draft. |
| 3 | 2026-10-13 | Accepted by the owner. Governing RFCs 001 and 004 accepted the same day. No normative change. |
| 2 | 2026-10-03 | Startup review applied. Release, source, and verification attributes added to every requirement. Duplicates merged (§23). Determinism delegated to RFC 001. New groups CTL. New requirements for admission, manifest, interventions, observation, randomness, zero-delay budget, built-ins, single Host core, composition, no third-party requests, version pinning, support matrix, durable storage, internationalisation, three-layer UI, conformance kit, compatibility policy, performance budgets, single local user, threat model, dependency policy, units, derived identifiers, canonical form, faulted-actor policy, two kinds of circuit actor, time coupling, discrete finance, guest ladder. Acceptance criteria restated per release. Glossary added; "Pattern 1/3" replaced by Browser Host and Native Host. |
