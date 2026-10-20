# RFC 002 — Guest contract: artifact, ABI, activation, layering

**Status.** Accepted (2026-10-14) Rule 36 and the randomness row of section 10 amended 2026-10-20 by PR-14a.
**Tracks.** Requirements ARCH-001, ARCH-003, ARCH-004, ARCH-006, ARCH-010, ACT-001, ACT-002, ACT-003, ACT-005, ACT-009, BUS-002, BUS-005, OBS-008, SEC-002, SEC-003, MIG-001, MIG-008, NUM-006, PKG-003, PKG-004, PKG-006, PKG-007, AUTO-007, AC-005, AC-007. Requirement IDs refer to `docs/src/requirements.md`.
**Touches.** The form of a guest module, its self-description, the core contract every domain builds on, what happens during an activation, the guest SDK, and the guest half of the conformance kit.
**Depends on.** [RFC 001 — Determinism contract](./001-determinism-contract.md) for time, activation, effects, randomness, and faults. [RFC 004 — Core model](./004-core-model.md) for identities, the scenario's `config`, ports, and the canonical encoding.
**Informed by.** Stage 0 proofs of concept 1 (one artifact, two engines), 2 (deterministic limits), 3 (boundary crossing cost), and 4 (instance scaling), whose figures appear in `ROADMAP.md`.

## Summary

A guest is a core WebAssembly module that follows the canonical ABI of the Component Model for a declared **world**, carries its own **manifest**, and imports nothing but the functions of its **contract**. The Host lifts and lowers every value through one implementation shared by both Hosts. The guest acts by calling Host functions; the Host records those calls as effects and commits them when the activation returns normally.

This RFC fixes four things: the artifact and its ABI (§1, §2), the module manifest (§3), the core contract `florn:core` that every domain includes (§4, §5), and the rules of an activation as seen from the guest (§6). Domain contracts (§7) add interfaces; they cannot change these rules.

## Background

The specifications promise that one guest file runs unchanged under the browser Host and the native Host and produces the same trace. Three gaps stood in the way of that promise.

1. "The same `.wasm` on both Hosts" had no executable definition. Browsers execute core modules; they do not execute Components. The proof of concept compared both forms: a core module with the canonical ABI gave one digest on every engine under a strict Content Security Policy, with 29 lines of shared lifting code; a Component in the browser needed a 1.2 MB transpiler, a relaxed policy, two naming conventions, and ran 5–15× slower.
2. A module could not describe itself. Without a manifest, the Host cannot validate a scenario's configuration and the user interface cannot offer a form.
3. Guest time, guest randomness, and timer semantics were unspecified. RFC 001 now defines them for the logical world; this RFC gives them a shape at the boundary.

Two further results shaped the contract. The crossing between Host and guest costs 0.2–0.3 µs at its floor on every engine, and copying a payload is not the cost below a few kilobytes; therefore the contract makes one call per item and passes the current time as an argument rather than offering a call to read it. A proof-of-concept scenario that walked a controller through a circuit step showed that a device model is evaluated several times inside one solver step; the activation rules therefore allow a domain to declare a **compound activation** (§6.6).

## Terms

The terms of RFC 001 and RFC 004 apply. In addition:

| Term | Meaning |
|---|---|
| **World** | The WIT world a module implements: the interfaces it imports and exports. Named `<package>/<world>@<version>`. |
| **Contract** | A versioned WIT package. `florn:core` is the core contract; `florn:network`, `florn:circuit`, `florn:finance` are domain contracts. |
| **Module manifest** | The self-description a module carries in a custom section: its world, actor kind, port kinds, parameters, telemetry channels, and resource needs. Distinct from the scenario manifest of RFC 004. |
| **Actor kind** | What a module provides: its ports, parameters, and world. One module provides one actor kind. |
| **Port kind** | A declaration in the module manifest of a port type the module accepts and how many ports of it an actor may have. |
| **Parameter** | A configuration value the module declares, with a type and, where it is a quantity, a unit. The scenario's `config` supplies values for it. |
| **Channel** | A telemetry stream the module declares, with a fixed record layout. |
| **Compound activation** | An activation that consists of several calls into the guest, declared by a domain contract. One budget, one commit. |
| **Guest SDK** | The library and template with which an author writes a conforming guest. |

## 1. The artifact

**Rule 1.** A guest is one core WebAssembly module. It is identified by the SHA-256 of its bytes (RFC 004 rule 16). The same bytes are admitted by both Hosts.

**Rule 2.** The module follows the canonical ABI of the Component Model for its declared world, with these options: string encoding UTF-8; one exported linear memory named `memory`; an exported `cabi_realloc`; for every export that returns a list or a string, the corresponding `cabi_post_*` export. The Host lifts and lowers values itself. No binding code is generated at admission.

**Rule 3.** The module imports only the functions of its world, under the import module names the canonical ABI defines (`florn:core/host@0.1.0` and the domain interfaces). Every other import, including any WASI interface, is refused at admission (RFC 003). The imports that the admission transformation adds for metering and limits are added by the Host; a module that declares them itself is refused.

**Rule 4.** The module has no shared memory, no threads, no SIMD, and no relaxed SIMD. Bulk memory, reference types, sign extension, non-trapping float-to-int conversion, multi-value, and mutable globals are admitted. This is the feature set of Rust's default `wasm32-unknown-unknown` target; `wasm32v1-none` is admitted as well. The list is restated by RFC 003, which owns admission.

**Rule 5.** The guest may use `std` on `wasm32-unknown-unknown`. It is freestanding: no operating-system interface exists and none is emulated. A `std` function that needs one (a clock, a file, an environment variable) traps or returns an error inside the guest; the SDK's template warns about the ones a Rust author meets.

**Rule 6.** A WebAssembly Component is not an executable artifact of florn. A later release may accept a Component **on import** and reduce it to its core module; the module identity is then the hash of the reduced core module, and the user interface shows both hashes. This is not in release 0.1 and nothing in it may require generated script.

### 1.1 Why a core module

| Reason | Measured or decided where |
|---|---|
| One digest on every engine, including under a strict Content Security Policy | POC-1, every engine that could run |
| The lifting code is one small crate with one test suite, used by both Hosts; the full type subset of §2 is about 200 lines | POC-1 appendix |
| No script generated at run time, so a user-imported module cannot bring script with it | POC-1, M5 |
| The admission transformation (metering, limits, state export) works on one module | POC-2 |
| A Component can be produced from the module mechanically when generic tooling needs one | `wasm-tools component new`, POC-1 |

## 2. The ABI and its type subset

**Rule 7.** A contract uses only these WIT types.

| Kind | Types | Lifted and lowered as |
|---|---|---|
| Scalars | `bool`, `u8`, `u16`, `u32`, `u64`, `s8`, `s16`, `s32`, `s64`, `f32`, `f64` | Flat values |
| Text | `string` | Pointer and length in guest memory, UTF-8 |
| Bytes | `list<u8>` | Pointer and length in guest memory |
| Lists | `list<T>` for any `T` in this table | Pointer and length; elements in canonical layout |
| Enumerations | `enum` | Flat `u32` with a range check |
| Records | `record` | Flattened in declared order; spilled through memory above the canonical flat limit |
| Options and results | `option<T>`, `result<T, E>` | Discriminant and payload |
| Variants | `variant` | Discriminant and payload |

Not used: `resource`, `future`, `stream`, `async` functions, `flags`, `char`, `tuple`, `own`, `borrow`. A contract that needs one of these proposes an amendment to this RFC; the cost is a new lifting path on both Hosts.

**Rule 8.** Lifting is strict. A string whose bytes are not valid UTF-8, a list whose range lies outside the guest's memory, an enumeration or discriminant out of range: each is a contract violation and ends the activation with a fault (RFC 001 rule 24). Both Hosts reject the same input in the same way, because they share the implementation.

**Rule 9.** A floating-point value that crosses the contract in either direction is made canonical: every NaN becomes one fixed NaN (RFC 001 rules 47 and 59). A guest that inspects NaN bit patterns inside its own memory may see values that differ by CPU architecture; this is outside the promise and the SDK says so.

**Rule 10.** The Host calls an export through a typed, cached function reference, never by name lookup per call, and lowers and lifts through views over the guest's memory, never through an intermediate object per crossing. These are Host rules (RFC 005); they are stated here because the contract's granularity (§6.1) assumes them.

## 3. The module manifest

**Rule 11.** A module carries two custom sections. Both are read at admission. A module without either is refused with the coded error `module.manifest-missing`.

| Section | Name | Content | Written by |
|---|---|---|---|
| Contract identity | `component-type:…` | The encoded WIT world the module implements, as the Component Model toolchain emits it | The bindings generator |
| florn manifest | `florn.manifest` | A TOML document, UTF-8, with the fields of rule 12 | The guest SDK, from the author's declaration |

The world named in the florn manifest must equal the world in the contract-identity section; a difference is `module.contract-mismatch`.

**Rule 12.** The florn manifest has these fields. Unknown keys are a refusal, `module.manifest-schema`.

```toml
florn-module = 1                                   # manifest schema version
world = "florn:network/router@0.1.0"               # the world; "florn:core/guest@0.1.0" for a core-only guest
kind = "router"                                    # actor kind, for people and for the UI
description = "OSPF single-area router"            # optional
sdk = "florn-guest 0.1.0; rustc 1.99.0"            # optional, free text; not part of any identity

[[ports]]                                          # port kinds; at most one per port type
kind = "eth"
type = "network/ethernet"
min = 1
max = 8

[[params]]                                         # in this order the guest receives them
name = "router-id"
type = "text"
required = true

[[params]]
name = "hello"
type = "ticks"                                     # written as a duration in the scenario, delivered in ticks
default = "10s"

[[params]]
name = "cost"
type = "u64"
default = 10

[[telemetry]]
channel = 1
name = "neighbour-state"
fields = [{ name = "neighbour", type = "u64" }, { name = "state", type = "u64" }]

[resources]                                        # what the module needs; a scenario limit below this is refused
memory = "4MiB"
pending-timers = 16
```

**Rule 13.** Parameter types.

| Type | Written in the scenario as | Delivered to the guest as |
|---|---|---|
| `bool` | `true` / `false` | 1 byte |
| `u64`, `s64` | Integer | 8 bytes, little-endian |
| `f64` | Number | 8 bytes, little-endian, canonical NaN |
| `text` | String | `u32` length, then UTF-8 bytes |
| `bytes` | Hexadecimal string | `u32` length, then the bytes |
| `ticks` | Duration with unit (`"10s"`) | `u64` ticks; a value that is not a whole number of ticks is `quantity.not-whole-ticks` (RFC 004) |
| `integer` with `unit` | Quantity with unit (`"12.34USD"`) | `s64` in the declared delivery unit (for example `0.01USD`), converted with exact decimal arithmetic; a value that is not whole in that unit is `quantity.not-whole-unit` |
| `real` with `unit` | Quantity with unit (`"1.2ohm"`) | `f64` in the SI unit the domain registers for the dimension |

A unit on an `integer` or `real` parameter must be registered by a domain the module's world includes (RFC 004 rule 27).

**Rule 14.** Every declared parameter reaches the guest, in declaration order, encoded as in rule 13. The Host fills defaults. A required parameter without a value in the scenario is the validation error `config.missing-parameter`; a value for an undeclared parameter is `config.unknown-parameter`; a value of the wrong type is `config.type`. The scenario's `config` (RFC 004 rule 17) is validated against this declaration by the Host before any instance exists.

**Rule 15.** Port kinds. A scenario port is matched to the module's port kind of the same type. A port whose type matches no kind is `module.port-type-unknown`; fewer than `min` or more than `max` ports of a kind is `module.port-count`. Because a module declares at most one kind per type, the match is unambiguous.

**Rule 16.** Telemetry channels. Channel 0 is reserved for text (§5.4). Every other channel the guest reports on must be declared, with a record of fields. Field types are `bool`, `u64`, `s64`, `f64`, `text`, `bytes`, each encoded as in rule 13. The Host does not decode telemetry on the hot path; the display path decodes it with the manifest.

**Rule 17.** Resources. The module declares the memory, pending timers, and any other quota of RFC 003 it needs. At admission, a scenario limit below a declared need is `limit.below-need`, which names the actor, the limit, the need, and the value. This is how the Host estimates before it instantiates (SEC-005).

**Rule 18.** The manifest is part of the module bytes and therefore of its identity. It is not separately signed, versioned, or trusted; provenance is displayed, not enforced (SEC-010).

### 3.1 Why two sections

The Component Model toolchain already leaves the encoded world in the module, so the contract identity costs the author nothing and cannot disagree with the bindings. What the toolchain cannot know — parameters, port kinds, channels, needs — is written by the SDK from one declaration in the author's source, so the manifest and the decoder in the guest are generated from the same text and cannot disagree either.

## 4. The core contract

**Rule 19.** The package `florn:core` holds what every domain needs with identical meaning: lifecycle, logical time, messages through ports, timers with cancel, randomness, and telemetry. An interface enters `florn:core` only when two domains need it with identical semantics (ARCH-006). The WIT text below is the contract of record (ARCH-004). The files under `contracts/` in the repository must be identical to it; a test checks that.

```wit
package florn:core@0.1.0;

/// Types shared by every contract.
interface types {
    /// Logical time in ticks (RFC 001 rule 1).
    type tick = u64;
    /// The identity of an actor, as written in the scenario (RFC 004 rule 7).
    type actor-id = u32;
    /// The identity of a port, unique within its actor (RFC 004 rule 9).
    type port-id = u32;
    /// A timer identity chosen by the guest. One timer per identity.
    type timer-id = u32;
    /// A telemetry channel declared in the module manifest. Channel 0 is text.
    type channel-id = u32;

    /// One port of this actor, as the scenario declared it.
    record port {
        id: port-id,
        /// Index of the port kind in the module manifest.
        kind: u32,
        /// The port's name in the scenario, for telemetry and messages to people.
        name: string,
    }
}

/// Functions the Host provides. Each is recorded as an effect and committed
/// only if the activation returns normally (RFC 001 rules 22 to 24).
interface host {
    use types.{tick, port-id, timer-id, channel-id};

    /// Ask for delivery of `payload` through `port` of this actor.
    /// The domain's world decides what delivery means.
    send: func(port: port-id, payload: list<u8>);

    /// Ask for `on-timer(timer)` to be called `delay` ticks from the current instant.
    /// A pending timer with the same identity is replaced.
    set-timer: func(timer: timer-id, delay: tick);

    /// Cancel a pending timer. Cancelling a timer that is not pending has no effect.
    cancel-timer: func(timer: timer-id);

    /// Draw the next value of this actor's random stream (RFC 001 rules 28 to 31).
    random: func() -> u64;

    /// Report telemetry on a declared channel, in the channel's record layout.
    report: func(channel: channel-id, payload: list<u8>);
}

/// Functions the guest provides. Each call is one activation (§6), except
/// where a domain contract declares a compound activation.
interface actor {
    use types.{tick, actor-id, port-id, timer-id, port};

    /// The first activation of the actor. `now` is tick 0 for an actor in the
    /// scenario, or the current instant for an actor created by an intervention.
    /// `config` holds the declared parameters in order (rule 14).
    init: func(now: tick, actor: actor-id, ports: list<port>, config: list<u8>);

    /// A message arrived through `port`.
    on-message: func(now: tick, port: port-id, payload: list<u8>);

    /// The timer `timer` expired.
    on-timer: func(now: tick, timer: timer-id);
}

/// The world of a guest that uses the core contract only, such as the echo guest.
/// Domain worlds include this one.
world guest {
    import host;
    export actor;
}
```

**Rule 20.** The export names in the module are the canonical ABI's mangled names, for example `florn:core/actor@0.1.0#on-message`; the import module name is `florn:core/host@0.1.0`. The Host resolves them once, at instantiation.

**Rule 21.** Versioning. The package version follows semantic versioning. Before 1.0.0 the contracts are unstable: a Host release admits exactly the contract versions it lists, and a module built against another version is refused with `module.contract-unsupported`, which names both versions. The compatibility policy (PKG-006) states, from release 0.1, for how many minor releases a contract version remains admitted.

## 5. Semantics of the core contract

### 5.1 Time

**Rule 22.** Every export receives the current instant's tick as `now`. The microstep is not visible to a guest. There is no Host function to read the time: within an activation the time does not change (RFC 001 rule 7), so the argument is the only value the guest could receive.

### 5.2 Messages

**Rule 23.** `send` records an effect: the payload bytes and the port. The port must be one of the actor's ports as given at `init`; another value is a contract violation. The payload must not exceed the `payload` quota; exceeding it is a quota fault. The committed effect carries the origin's sequence number (RFC 001 rule 17) and is the message's identity (BUS-004).

**Rule 24.** What a committed `send` causes is defined by the domain whose world owns the port's type: the connection's attributes, delay, loss, delivery to one or many ports, or refusal with a diagnostic. The core attaches no meaning to the bytes (BUS-005) and does not itself deliver anything.

**Rule 25.** `on-message` delivers the payload the domain decided to deliver. The bytes are the guest's own to interpret; the domain's RFC and its SDK crate define their layout.

### 5.3 Timers

**Rule 26.** `set-timer(t, d)` records an effect. On commit, a timer event for this actor is scheduled at the current tick plus `d`, microstep zero for `d > 0`, or the next microstep for `d = 0` (RFC 001 rule 11). A `d` that places the event beyond the largest tick is a contract violation (RFC 001 rule 6).

**Rule 27.** One timer exists per identity. Setting a pending identity replaces it: the earlier scheduled event is dropped. The number of pending timers after commit must not exceed the `pending-timers` quota; exceeding it is a quota fault of the activation that set the timer.

**Rule 28.** `cancel-timer(t)` records an effect. On commit, a pending timer `t` is dropped. If none is pending, nothing happens. Because effects commit in order, a `set-timer` followed by `cancel-timer` of the same identity in one activation leaves no timer.

**Rule 29.** `on-timer` is called with the identity. There is no periodic timer; a guest that wants one sets the next timer in `on-timer`. The class of a timer event in the order key is assigned by the domain (RFC 001 rule 15).

### 5.4 Randomness

**Rule 30.** `random` returns the next value of the actor's stream, derived from the root seed and the actor's identity with the purpose label `actor` (RFC 001 rule 29). A guest has no other source of randomness (RFC 001 rule 31); the SDK provides the distributions florn specifies on top of this one function (RFC 001 rule 30).

**Rule 31.** A draw is not an effect in the trace, but it belongs to the activation: when an activation ends in a fault, the stream is restored to its position at the start of the activation (RFC 001 rule 24). The Host records the number of draws per activation for this purpose.

### 5.5 Telemetry

**Rule 32.** `report` records an effect: the channel and the payload. The channel must be declared (rule 16) or be channel 0; another value is a contract violation. The sum of payload sizes in one activation must not exceed the `telemetry-per-activation` quota; exceeding it is a quota fault.

**Rule 33.** Channel 0 carries UTF-8 text, for messages to the author during development. It has no declared layout. The display path may drop or thin any channel (RFC 001 rule 39c); the trace records every committed report.

**Rule 34.** Telemetry is committed and traced like every other effect (RFC 001 rule 39a). Adding a report changes the module's bytes and therefore its identity; it does not make two runs of one module differ.

## 6. Activation, as the guest sees it

### 6.1 One call, one activation

**Rule 35.** One event addressed to the actor causes one call into the guest (RFC 001 rule 20). The Host does not batch events into one call, and the contract does not expose a list of events. Measured on every engine, the crossing costs 0.2–0.3 µs at its floor and the copy is not the cost below a few kilobytes; batching is an optimisation a specific contract may propose later with measurements, not a principle of the design.

**Rule 36.** `init` is the first activation of every actor. For actors of the scenario it happens at tick 0, before any event, in ascending order of identity. Its effects are committed and traced like any other activation's; a timer set in `init` with delay `d` fires at tick `d`. An actor created by an intervention is initialised at the instant the intervention is applied. An `init` that faults is a fault at tick 0: under the `drop` policy the actor handles nothing for the whole run, and the run continues (amended 2026-10-20, PR-14a).

### 6.2 Host functions during an activation

**Rule 37.** A Host function only appends to the record of effects of the current activation, or reads from the state of the logical world at the start of the activation and from those effects (RFC 001 rule 25). It never calls into a guest. A guest is never on the stack twice (RFC 001 rule 21).

**Rule 38.** Nothing is applied while the guest runs. A message sent during an activation is not delivered to anyone, and not even to the sender, before the activation has returned normally.

### 6.3 Return

**Rule 39.** When the export returns, the activation ends normally and its effects are committed in the order they were made (RFC 001 rule 23). The guest's memory and globals are its state until the next activation; the Host does not read them for any purpose but state export (RFC 003) and never writes them.

### 6.4 Faults

**Rule 40.** An activation ends in a fault when the guest traps, when the execution budget, the stack limit, or the memory limit is exhausted (RFC 003), when a quota of §5 is exceeded, or when the contract is violated (rules 8, 23, 26, 32). Every effect of the activation is discarded, the random stream is restored, and the fault is traced (RFC 001 rule 24). The actor handles no further event until it is reset or restored; what happens to its events is the domain's policy (ACT-011).

**Rule 41.** The kinds of fault are a closed enumeration shared by the trace, the diagnostic log, and the user interface: `trap`, `budget`, `stack`, `memory`, `quota` (with the quota's name), `contract` (with the rule violated). RFC 007 gives them codes and text.

**Rule 42.** A guest has no way to end an activation abnormally other than trapping, and no way to learn that an earlier activation faulted. There are no error returns in the core contract: everything a guest could do wrong is a fault. A domain contract may define functions that return `result`, for conditions that are part of the domain's model (an order the venue refuses, a sensor that is not connected); those are messages to the guest, not errors of the Host (CTL-004).

### 6.5 What the guest does not see

**Rule 43.** The guest sees no microstep, no event key, no sequence number, no wall clock, no identity of other actors except what arrives in payloads, and no state of the world except through messages and the domain's functions. It cannot tell replay from a first run, playback does not involve it, and observation of it never calls it (RFC 001 rule 39b).

### 6.6 Compound activation

**Rule 44.** A domain contract may declare that a sequence of calls forms one activation. The declaration names the opening call, the calls permitted between, and the closing calls, of which one commits and one discards. Within a compound activation: one budget and one set of quotas apply to the whole; effects are recorded across the calls and committed at the committing close or discarded at the discarding close; a fault in any call ends the whole activation as a fault; no event is handled and no other activation of the actor starts before it closes. A call out of sequence is a contract violation.

**Rule 45.** The circuit contract uses this for a device model: `begin-step` opens, `evaluate` may be called any number of times, `accept-step` commits and `reject-step` discards. A rejected solver step therefore leaves no state in any guest, which is what the paper test of the core model required. A sampled controller uses plain activations.

## 7. Domain contracts

**Rule 46.** A domain contract is a WIT package `florn:<domain>@<version>` with its own interfaces and worlds. A domain world includes `florn:core/guest` and adds the domain's imports and exports. Example, for illustration only (the network RFC owns it):

```wit
package florn:network@0.1.0;

interface router-events {
    use florn:core/types@0.1.0.{tick, port-id};
    /// The link of `port` changed state.
    on-link: func(now: tick, port: port-id, up: bool);
}

world router {
    include florn:core/guest@0.1.0;
    export router-events;
}
```

**Rule 47.** A domain contract may not redefine, shadow, or bypass a core function, add a source of time or randomness, or give a guest a reference into Host memory. It may define messages (layouts for `send` and `on-message` payloads), additional exports (further kinds of event), additional imports that record effects or answer from the activation's starting state, compound activations, and `result`-returning functions for conditions of its model.

**Rule 48.** A module's world may include the interfaces of more than one domain package. The module manifest names the world; the scenario must declare every domain the world uses (RFC 004 rule 5). A controller with an Ethernet port and a signal input is one actor with one world that includes `florn:network` and `florn:circuit` interfaces. The core prevents nothing here (ARCH-010); each domain validates the ports of its own types.

**Rule 49.** Identifiers in a contract are the glossary's words in kebab-case. A domain contract's RFC lists its interfaces, its message layouts, its event classes (RFC 001 rule 15), and its compound activations, if any.

## 8. The guest SDK

**Rule 50.** From release 0.1 florn provides a Rust SDK and a template (PKG-004). Writing a guest with it requires no browser, no operating-system interface, and no tool beyond the Rust toolchain with the `wasm32-unknown-unknown` target. The SDK provides:

| Provided | Purpose |
|---|---|
| Bindings generated from the WIT of §4 and of each domain | The author implements a trait; the SDK owns the canonical ABI |
| One declaration of parameters, port kinds, channels, and needs | The SDK writes the `florn.manifest` section and the config decoder from it (rule 12) |
| The distributions of RFC 001 rule 30 over `random` | Ranges, floats, named distributions, computed by florn's algorithms |
| Encoders for declared telemetry channels | Binary records in the layout of rule 16 |
| An allocator, a panic handler that traps, and `std` and `no_std` profiles | A guest builds without further choices; the `no_std` profile for size |
| A `now()` accessor | The SDK stores the `now` argument of the current activation for library code |

**Rule 51.** The SDK is not the contract. A guest written without it, in Rust, C, C++, or Zig, that implements the world and carries the manifest is a conforming guest. The conformance kit checks the module, not the toolchain.

## 9. Phasing

| Release | Contracts | Guests |
|---|---|---|
| 0.1.0 | `florn:core@0.1.0` | The echo guest of the conformance kit; the hostile guests of the kit |
| 0.2.0 | `florn:network@0.1.0` | Echo, distance-vector, and the OSPF subset (NET-012, NET-004) |
| Later | `florn:circuit`, `florn:finance` | Per their domain RFCs |

## 10. Verification

Each property has a test in the conformance kit (AUTO-007). The guest half runs against a module; the Host half runs under both Hosts.

| Property | Test |
|---|---|
| One artifact | The echo guest's bytes are admitted by both Hosts and produce one digest (AC-001). |
| Strict lifting | A guest that returns a string with invalid UTF-8, a list outside its memory, and an enumeration out of range: each faults as `contract`, identically under both Hosts. |
| Manifest | A module without `florn.manifest` is refused with `module.manifest-missing`; a module whose two sections disagree with `module.contract-mismatch`; the headless runner lists the parameters of a conforming module (AC-005). |
| Parameters | A scenario with a missing required parameter, an unknown parameter, and a duration not whole in ticks is refused with the three named codes; a conforming scenario's values reach the guest in order with defaults filled. |
| Port kinds | A scenario with a port of an undeclared type, and one with nine ports of a kind whose `max` is 8, are refused with the named codes. |
| Init | A timer set in `init` with delay 10 fires at tick 10; actors are initialised in ascending identity order; an actor created by an intervention is initialised at that instant. |
| Timers | Set then cancel in one activation leaves no timer; set twice with one identity fires once; the 17th pending timer under a quota of 16 faults as `quota`. |
| Effects | An actor sends and then traps: the trace holds the fault and no message (RFC 001). |
| Randomness | An actor draws twice and traps; its stream stands where the activation began, so that after a restore (release 0.3) it draws the same two values. A third actor's presence changes no draw of the first two. |
| Telemetry | A report on an undeclared channel faults as `contract`; reports above the quota fault as `quota`; channel 0 text appears in the diagnostic log. |
| Compound activation | A device-model guest that mutates its state in `evaluate` and receives `reject-step` shows the state of the last `accept-step`; a call out of sequence faults as `contract`. |
| Two domains | A world including interfaces of two domain packages admits, and each domain validates its own ports. |
| No ambient access | The hostile guest that imports a WASI function is refused; a `std` guest that reads the clock traps inside the guest and the Host records a `trap`. |
| SDK | The template builds a conforming guest from a clean machine with the Rust toolchain only. |

## Alternatives considered

| Alternative | Reason for rejection |
|---|---|
| A WebAssembly Component as the executable artifact | In the browser: a 1.2 MB transpiler, generated script, a relaxed policy, two naming conventions, 5–15× slower. Kept as a possible import form (rule 6). |
| WASI, or a virtualised deterministic subset of it | Contradicts SEC-003 for the baseline. A virtualised subset remains a possible later extension for other languages; it would be a domain-neutral package built on this contract, not a replacement of it. |
| The guest returns its effects as a list instead of calling Host functions | Authors who port real code expect to call functions; a list forces every effect through one return value and makes a mid-activation fault ambiguous. The recorded-call style gives the same all-or-nothing property. |
| A Host function to read the time | One crossing per read, for a value that cannot change during an activation. |
| Configuration delivered as TOML text | Every guest would carry a parser; the manifest already declares the types, so a binary record is both smaller and checked. |
| Telemetry as text or JSON | OBS-008. A declared binary layout costs nothing on the hot path and the display path decodes it with the manifest. |
| Error returns from core Host functions | A guest cannot ignore an error it never receives; every misuse is a fault, which is simpler to specify, to test, and to explain. |
| Periodic timers | A second mechanism for what one timer and `on-timer` already do. |
| Port kinds matched by name pattern | Matching by type is unambiguous once a module declares one kind per type. |
| Batching several events into one call | The crossing floor is 0.2–0.3 µs; one call per item keeps the contract and the trace simple. May be revisited per contract with measurements. |

## Prior art

| Topic | Precedent |
|---|---|
| Canonical ABI and WIT | The WebAssembly Component Model, whose canonical ABI this RFC adopts for core modules |
| Recorded effects committed on normal return | Transactional actors in discrete-event simulators; the activation rules of RFC 001 |
| Self-describing modules | The `component-type` custom section of `wit-bindgen`; custom sections in general |
| Compound activation | The evaluate–accept–reject protocol of circuit simulators' device models |

## Consequences for other RFCs

| RFC | Must provide |
|---|---|
| 003 — Module admission | Reading both custom sections; refusing every import outside the world and every feature outside rule 4; the quotas of §5 with their defaults and codes; `limit.below-need`; state export without guest cooperation; the metering imports a module may not declare itself. |
| 005 — Host architecture | The shared lifting crate and the typed call path of the engine adapter; effect recording per activation, including the random-draw count; the config encoder and the telemetry decoder; the conformance kit's structure; `contracts/` layout and the equality test with this RFC. |
| 007 — Error model | Codes and text for the fault kinds of rule 41 and the refusals of §3. |
| Domain RFCs | The package, its worlds, message layouts, event classes, compound activations, registered units, and the semantics of `send` on their port types. |
| Requirements | ACT-009's manifest is defined here; NET-002's "no other access" is rule 3; AC-005 and AC-007 test this RFC. |

## Open questions

| # | Question | Decided by |
|---|---|---|
| 1 | Labelled random streams for a guest (several independent streams per actor, RFC 001 rule 28) — needed by any 0.2 guest? | The network RFC; until then one stream per actor. |
| 2 | The exact revision of the Component Model canonical ABI the shared lifting crate implements, and whether the flat-value limit is pinned by florn or followed from the specification. | RFC 005, when the crate is written. |
| 3 | The default quota values of §5 (`payload`, `pending-timers`, `telemetry-per-activation`, `effects-per-activation`). | RFC 003, from measurement in Phase 1. |
| 4 | Whether a Component is accepted on import in 0.3 (rule 6) or not at all. | The owner, with WEB-005. |
