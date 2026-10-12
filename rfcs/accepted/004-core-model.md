# RFC 004 — Core model: graph, scenario manifest, package

**Status.** Accepted (2026-10-13)
**Tracks.** Requirements ARCH-006, ARCH-010, BUS-001, BUS-004, BUS-005, BUS-009, DES-010, DES-013, DES-014, DATA-001, DATA-002, DATA-003, DATA-007, DATA-009, DATA-010, DATA-011, DATA-012, SEC-005, SEC-007, PKG-007. Requirement IDs refer to `docs/src/requirements.md`.
**Touches.** The core graph types, the scenario manifest, the package format, the canonical encoding used for hashing, and the validation errors the Host reports for a scenario.
**Depends on.** [RFC 001 — Determinism contract](./001-determinism-contract.md), for identities, time, ordering, and run inputs.

## Summary

This RFC defines the three things every domain shares and no domain owns: the **graph** in which actors are connected, the **manifest** in which a scenario is written, and the **package** in which a scenario travels with its modules. It also defines the **canonical encoding** from which a scenario's hash is computed.

The core reads a small, closed set of fields. Everything else in a manifest belongs to a domain and is opaque to the core. The boundary is drawn so that a circuit net with three pins, a market venue connected to many agents, and a network link all fit without the core knowing which is which.

## Background

The earlier draft of the external design gave every connection a source, a target, a latency, a jitter, a loss model, and a bandwidth. A circuit net has none of these. A market connection has latency and nothing else. The common model had taken the shape of the first domain, which is exactly what the architecture forbids.

Before this RFC was written, one network, one circuit, and one finance scenario were expressed in the model below on paper, and a controller was walked through a solver step. Three results from that exercise are built in here:

1. Identities must be explicit in the manifest. Deriving them from names or from position would change the order of events whenever a scenario is edited.
2. A port may belong to several connections, and a connection may carry attributes per port.
3. Unit checking is split: the core checks that every quantity carries a unit and that every duration is a whole number of ticks; the domain checks dimensions.

## Terms

The terms of RFC 001 and of the specification's glossary apply. In addition:

| Term | Meaning |
|---|---|
| **Core field** | A field of the manifest that the core reads and validates. Listed in §3. |
| **Domain field** | A field the core carries but does not read. Validated by the domain it belongs to. |
| **Quantity** | A number with a unit, written as one token: `5ms`, `1.2ohm`, `0.01USD`. |
| **Reference** | A field that names another element of the manifest: a module, an actor, a port, a connection. |

## 1. The graph

**Rule 1.** The core graph consists of **actors**, **ports**, and **connections**.

| Element | Has | Belongs to |
|---|---|---|
| Actor | An identity, a name, a kind, a class, zero or more ports | The scenario |
| Port | An identity, a name, a type | One actor |
| Connection | An identity, a name, two or more port references | The scenario |

**Rule 2.** A connection joins two or more ports. The core attaches no meaning to the number of ports, to their order, or to any attribute of the connection.

**Rule 3.** A port may belong to any number of connections, including none.

**Rule 4.** A connection may carry attributes for the connection as a whole and attributes for each of its ports. Both are domain fields.

**Rule 5.** A port has a type, written `<domain>/<name>`. The domain prefix must be one of the domains the scenario declares. The core reads the prefix only.

**Rule 6.** An actor is backed by a **module** or is a **built-in** of a domain. Both are referenced from the same place in the manifest (§3.6).

### 1.1 Why a hyperedge

A circuit net joins every pin that shares a voltage: two, three, or twenty. A mechanical shaft joins a motor, a load, and a sensor. A market venue serves many agents. A link in a network happens to join two interfaces. The general case is a set of ports; the pair is a special case the network domain checks for itself.

## 2. Identities

**Rule 7.** Every actor and every connection carries a 32-bit unsigned identity, written in the manifest by its author or by the user interface. Actors and connections share one identity space: no actor and no connection have the same identity.

**Rule 8.** Identities 0 and 4,026,531,840 (`0xF000_0000`) and above are reserved. The Host uses the reserved range for origins that are not in the manifest: the source of interventions, the scenario's schedule, and each domain world.

| Reserved identity | Origin |
|---|---|
| 0 | None. Never a valid origin. |
| `0xF000_0000` | Interventions |
| `0xF000_0001` | Scheduled changes |
| `0xF000_0010` + *n* | The world of the *n*-th declared domain |

**Rule 9.** A port carries a 32-bit unsigned identity unique within its actor. Ports are not origins.

**Rule 10.** An identity is never reused within a scenario's history. An editor that deletes an actor does not give its identity to another. The manifest may record the next free identity for this purpose.

**Rule 11.** Names are for people. An actor's name is unique in the scenario; a port's name is unique in its actor; a connection's name is unique in the scenario. References in the text form of the manifest use names (§3.7), because a person reads them. The canonical encoding (§5) uses identities only.

### 2.1 Why explicit identities

RFC 001 orders equal-time events by origin (rule 16). If identities were assigned from the order of appearance in the file, inserting one actor above another would reorder every race between them. If they were derived from names, a rename would do the same. A stable identity written in the file is the only form that survives editing.

## 3. The manifest

**Rule 12.** A scenario is written in one TOML document. The document has the sections below. Unknown keys inside a core section are a validation error. Keys inside a domain section are passed to the domain.

### 3.1 Why TOML

| Candidate | Reason for or against |
|---|---|
| **TOML** | Unambiguous grammar, comments, readable diffs, a maintained parser in the Rust ecosystem under a permissive licence. Chosen. |
| YAML | Implicit typing turns `no` into `false` and `1e3` into a string or a number depending on the parser. A format that can mislead a reader is excluded by the project's principles. |
| JSON | No comments. Numbers lose units unless quoted. Diffs are noisy. |
| KDL, RON | Clear grammars, small ecosystems. Not excluded for later; not chosen now. |

### 3.2 Header

```toml
florn = 1                          # manifest schema version, integer
name = "ospf-triangle"             # scenario name, for people
domains = ["network"]              # one or more; order defines the world identities of rule 8
description = "Three routers in a triangle; link r3-r1 fails at 30 s."   # optional
next-identity = 200                # optional; see rule 10
```

**Rule 13.** `florn` is the schema version. A Host that reads a manifest with a newer major version refuses it with a coded error. A Host that reads an older version migrates it, and says so, or refuses with a coded error. It never guesses.

### 3.3 Time, order, seed

```toml
[time]
tick = "1ns"                       # "1ns", "1ps", or "1fs"
end = "60s"                        # a duration; the first event at or after it is not handled

[order]
tie = "by-identity"                # or "by-seed"
microsteps-per-tick = 1000         # optional; may only lower the Host maximum
events-per-instant = 100000        # optional; may only lower the Host maximum

[seed]
root = 42                          # unsigned 64-bit
```

**Rule 14.** `tick` and `end` are core fields, and both are mandatory. `end` must be a whole number of ticks. Every scenario is therefore a bounded experiment: the user interface shows where a run ends before it starts, memory and trace are bounded by construction, and "the run" has one meaning. A user who wants to go on past `end` does so by an intervention that sets a later end; it is recorded like any other intervention, so the longer run is reproducible too. A missing `end` is the validation error `manifest.missing-field`.

### 3.4 Limits

```toml
[limits.default]                   # applies to every class without its own table
budget = 10000000                  # executed work per activation
stack = 1024
memory = "4MiB"
effects-per-activation = 1000
payload = "64KiB"
pending-timers = 64
telemetry-per-activation = "16KiB"

[limits.router]                    # a class named by actors
budget = 50000000
```

**Rule 15.** A limit in a manifest may only lower the Host's maximum. A value above it is a validation error, not a silent clamp.

### 3.5 Modules

```toml
[modules.ospf]
sha256 = "9f2c…"                   # 64 hex digits; the identity of the module
contract = "florn:network/router@0.1.0"   # package and version the module must implement
```

**Rule 16.** A module is identified by the SHA-256 of its bytes. The Host refuses a module whose bytes do not hash to the declared value, and a module whose manifest declares a contract other than the one named here.

### 3.6 Actors

```toml
[[actors]]
id = 1
name = "r1"
kind = "module"                    # "module" or "builtin"
module = "ospf"                    # when kind = "module": a key of [modules]
class = "router"                   # optional; a key of [limits]; default "default"
ports = [
  { id = 0, name = "eth0", type = "network/ethernet" },
  { id = 1, name = "eth1", type = "network/ethernet" },
]

[actors.config]                    # domain field, opaque to the core
router-id = "10.0.0.1"
hello = "10s"
```

```toml
[[actors]]
id = 3
name = "motor"
kind = "builtin"
builtin = "circuit/dc-motor"       # when kind = "builtin": "<domain>/<name>"
ports = [
  { id = 0, name = "a+", type = "circuit/pin" },
  { id = 1, name = "a-", type = "circuit/pin" },
  { id = 2, name = "shaft", type = "circuit/shaft" },
]

[actors.config]
R = "1.2ohm"
L = "0.5mH"
```

**Rule 17.** The core reads `id`, `name`, `kind`, `module` or `builtin`, `class`, and the `id`, `name`, and `type` of each port. `config` is a domain field. The domain validates it against the module's manifest or the built-in's definition.

**Rule 18.** A built-in is named `<domain>/<name>`. The domain prefix must be a declared domain. The domain defines which built-ins exist and what ports they have; the manifest states the ports it uses, and the domain checks them.

### 3.7 Connections

```toml
[[connections]]
id = 100
name = "l31"
ports = ["r3.eth1", "r1.eth1"]    # "<actor name>.<port name>"

[connections.attrs]                # domain field, for the connection
delay = "20ms"
loss = 0.001
cost = 40
```

```toml
[[connections]]
id = 104
name = "shaft"
ports = ["motor.shaft", "load.shaft", "encoder.shaft"]

[[connections]]
id = 100
name = "c-all"
ports = ["venue.gw", "mm1.gw", "mm2.gw"]

[connections.attrs.per-port]       # domain field, keyed by port reference
"mm1.gw" = { delay = "10us" }
"mm2.gw" = { delay = "50us" }
```

**Rule 19.** The core reads `id`, `name`, and `ports`. Every port reference must resolve. A connection with fewer than two ports is a validation error. `attrs` and `attrs.per-port` are domain fields.

### 3.8 Scheduled changes

```toml
[[changes]]
at = "30s"
target = "l31"                     # an actor name, a connection name, or "world:<domain>"

[changes.payload]                  # domain field
enabled = false
```

**Rule 20.** The core reads `at` and `target`. `at` must be a whole number of ticks. The target must resolve. The payload is a domain field. The change becomes an event of class 0 at the named tick, microstep zero, with the origin of rule 8.

### 3.9 Views

```toml
[views]                            # opaque to the core; used by the user interface
layout = "triangle"
[views.positions]
r1 = [100, 100]
```

**Rule 21.** `views` is not read by the core and is excluded from the scenario hash (§5).

### 3.10 Domain sections

```toml
[network]                          # one table per declared domain; opaque to the core
solver = { }                       # the domain's solver profile, if it has one
```

**Rule 22.** Each declared domain may have one top-level table named after it. The core passes it to the domain unread.

## 4. Quantities and units

**Rule 23.** A quantity is one token: a number followed immediately by a unit. No space. A number is a decimal integer or a decimal fraction, with an optional exponent. Examples: `5ms`, `0.5mH`, `1e-3`, `24V`, `0.01USD`.

**Rule 24.** Every field the manifest schema declares as a quantity must carry a unit. A bare number in such a field is a validation error with a coded message that names the field and the expected dimension.

**Rule 25.** The core knows the dimension **time**. Its units are `s`, `ms`, `us` (alias `µs`), `ns`, `ps`, `fs`. A time quantity is converted to ticks with exact decimal arithmetic. A value that is not a whole number of ticks is a validation error that names the field, the value, and the smallest tick length that would make it whole.

**Rule 26.** The core knows the dimension **bytes**, used in `[limits]`: units `B`, `KiB`, `MiB`. Decimal multiples are not accepted.

**Rule 27.** Every other unit belongs to a domain. A domain registers its units with their dimension. The core parses the number and the unit text and hands both to the domain, which checks the dimension.

**Rule 28.** Dimensionless numbers are written without a unit, in fields the schema declares as numbers (for example `loss = 0.001`). Whether such a field is a probability, a count, or a ratio is the domain's definition.

### 4.1 Why one token

`delay = 5` tells the reader nothing. `delay = "5ms"` tells the reader and the Host the same thing. A field that holds a number and a separate field that holds its unit invites the two to disagree.

## 5. Canonical encoding and the scenario hash

**Rule 29.** The scenario hash is computed over the canonical encoding of the parsed manifest, never over its text. Formatting, comments, key order, and `views` do not change the hash.

**Rule 30.** The canonical encoding is defined by these rules, applied to the parsed model.

| Construct | Encoding |
|---|---|
| Unsigned integer | Little-endian, fixed width as declared (8, 32, or 64 bits) |
| Byte string, text | Length as unsigned 32-bit, then the bytes; text as UTF-8 |
| Quantity | The number in its shortest decimal form as text, then the unit as text |
| List | Length as unsigned 32-bit, then the elements |
| Map (domain field) | Length as unsigned 32-bit, then the entries sorted by key as UTF-8 bytes, each as key then value |
| Record (core structure) | Fields in the order this RFC lists them, no tags |
| Reference | The identity of the referenced element, never its name |
| Absent optional field | A single zero byte; a present one is a single one byte followed by the value |

**Rule 31.** The hash is SHA-256 over the encoding. It is the `scenario_hash` of RFC 001 rule 40.

**Rule 32.** Domain fields are encoded as maps of text keys to values, where a value is an integer, a decimal written as shortest text, a text, a quantity, a boolean, a list, or a map. A domain that needs another type encodes it as text.

### 5.1 Why the parsed model and not the text

Two files that differ only in a comment describe the same world. Two people who format TOML differently must get the same hash, or they cannot compare runs. Hashing the text would make the hash a property of the editor.

## 6. The package

**Rule 33.** A package is a ZIP archive with the extension `.florn`. It contains:

| Path in the archive | Content | Required |
|---|---|---|
| `manifest.toml` | The scenario | Yes |
| `modules/<sha256>.wasm` | One file per module the manifest references, named by its hash | Yes for every referenced module, unless the package is declared partial |
| `package.toml` | Package metadata: `florn-package = 1`, `partial = false`, `created-by`, free-text `notes` | Yes |
| `README.md` | For people | No |

**Rule 34.** On import, the Host hashes every module file and refuses the package if any hash differs from its name or from the manifest's declaration.

**Rule 35.** A partial package omits some modules. On import the Host lists the missing hashes and lets the user supply them. A partial package is never run as is.

**Rule 36.** A package contains no run record, no trace, no checkpoint, and no secret. Results travel in a separate file defined with the control interface.

**Rule 37.** Export is explicit. The user interface shows what a package will contain before it is written, including every module by name and hash.

### 6.1 Why ZIP

It is readable by every operating system and by the browser without a native helper. It allows a person to inspect a package with ordinary tools. Nothing in the package needs compression to be useful, and nothing in it needs a format the user cannot open.

## 7. Validation

**Rule 38.** Validation reports every error it can find in one pass, each as a stable code with typed parameters, as required by the control interface. The core reports the errors in the table below; a domain reports its own.

| Code | Meaning |
|---|---|
| `manifest.schema-version` | Unsupported `florn` value |
| `manifest.unknown-key` | Unknown key in a core section |
| `manifest.missing-field` | Required core field absent |
| `identity.duplicate` | Two actors or connections share an identity |
| `identity.reserved` | An identity in the reserved range |
| `identity.port-duplicate` | Two ports of one actor share an identity |
| `name.duplicate` | Two elements share a name where names must be unique |
| `reference.unresolved` | A module, actor, port, connection, or domain reference does not resolve |
| `domain.undeclared` | A port type, built-in, or world refers to a domain not in `domains` |
| `module.hash-mismatch` | Module bytes do not hash to the declared value |
| `module.contract-mismatch` | Module's own manifest names another contract |
| `connection.too-few-ports` | Fewer than two ports |
| `quantity.missing-unit` | A quantity field without a unit |
| `quantity.not-whole-ticks` | A duration that is not a whole number of ticks |
| `quantity.unknown-unit` | A unit neither the core nor the domain knows |
| `limit.above-maximum` | A limit above the Host's maximum |
| `time.end-before-change` | A scheduled change at or after `end` |

**Rule 39.** Validation allocates nothing proportional to a declared count before the declaration itself has been validated. A manifest that declares a million actors is refused by the limits before a million of anything exists.

## 8. Worked example: the network scenario

```toml
florn = 1
name = "ospf-triangle"
domains = ["network"]

[time]
tick = "1ns"
end = "60s"

[order]
tie = "by-identity"

[seed]
root = 42

[modules.ospf]
sha256 = "9f2c4e…"
contract = "florn:network/router@0.1.0"

[[actors]]
id = 1
name = "r1"
kind = "module"
module = "ospf"
class = "router"
ports = [{ id = 0, name = "eth0", type = "network/ethernet" },
         { id = 1, name = "eth1", type = "network/ethernet" }]
[actors.config]
router-id = "10.0.0.1"

[[actors]]
id = 2
name = "r2"
kind = "module"
module = "ospf"
class = "router"
ports = [{ id = 0, name = "eth0", type = "network/ethernet" },
         { id = 1, name = "eth1", type = "network/ethernet" }]
[actors.config]
router-id = "10.0.0.2"

[[actors]]
id = 3
name = "r3"
kind = "module"
module = "ospf"
class = "router"
ports = [{ id = 0, name = "eth0", type = "network/ethernet" },
         { id = 1, name = "eth1", type = "network/ethernet" }]
[actors.config]
router-id = "10.0.0.3"

[[connections]]
id = 100
name = "l12"
ports = ["r1.eth0", "r2.eth0"]
[connections.attrs]
delay = "5ms"
loss = 0
cost = 10

[[connections]]
id = 101
name = "l23"
ports = ["r2.eth1", "r3.eth0"]
[connections.attrs]
delay = "5ms"
loss = 0
cost = 10

[[connections]]
id = 102
name = "l31"
ports = ["r3.eth1", "r1.eth1"]
[connections.attrs]
delay = "20ms"
loss = 0.001
cost = 40

[[changes]]
at = "30s"
target = "l31"
[changes.payload]
enabled = false

[limits.router]
budget = 50000000

[views.positions]
r1 = [100, 100]
r2 = [300, 100]
r3 = [200, 260]
```

The circuit and finance scenarios of the paper test are expressed in the same grammar with no additional core field.

## 9. Verification

| Property | Test |
|---|---|
| Hash is independent of formatting | Two manifests differing in comments, key order, whitespace, and `views` have the same hash. |
| Hash depends on content | Changing one identity, one quantity, or one domain field changes the hash. |
| Identities are stable | Inserting an actor above another leaves the other's identity and all event keys unchanged. |
| Hyperedge | A three-port connection validates and the domain receives all three ports. |
| Multi-membership | One port in two connections validates. |
| Units | `delay = 5` fails with `quantity.missing-unit`; `delay = "1.5ns"` with `tick = "1ns"` fails with `quantity.not-whole-ticks` naming `1ps`. |
| Limits | A manifest limit above the Host maximum fails with `limit.above-maximum`. |
| Package | A package with a renamed module file fails with `module.hash-mismatch`. A partial package lists its missing hashes. |
| No pre-allocation | A manifest declaring 10⁷ actors is refused in bounded memory. |
| Schema | A manifest with `florn = 2` is refused with `manifest.schema-version`. |

## Alternatives considered

| Alternative | Reason for rejection |
|---|---|
| Two-ended links with latency and loss in the core | A circuit net and a market connection do not fit. |
| Identities derived from names or from position | Editing would reorder events. |
| Hashing the manifest text | The hash would depend on formatting. |
| YAML | Implicit typing misleads readers. |
| JSON | No comments; noisy diffs; units need quoting anyway. |
| Separate number and unit fields | They can disagree. |
| A tar archive | Not readable in the browser without a helper; less familiar to users. |
| Run records inside the package | A package describes a world, not an execution. Mixing them confuses what a hash identifies. |

## Consequences for other RFCs

| RFC | Must provide |
|---|---|
| 001 | Rule 45's canonical encoding is the one defined in §5 here, applied to trace records. |
| 002 — Guest contract | The module manifest format: contract, actor kind, ports, parameters with types and units, resource needs. The relation between a manifest's `config` and the module's declared parameters. |
| 003 — Module admission | Reading `[modules]` and `[limits]`; the admission version in the run identity. |
| 005 — Host architecture | The run record and result file (rule 36); the coded-message catalogue for §7; what the UI shows before export (rule 37). |
| Domain RFCs | Registered units; built-in names and ports; which port types exist; validation of `config`, `attrs`, `per-port`, `payload`, and the domain table. |

## Open questions

Resolved at acceptance on 2026-10-13, except where marked deferred.

| # | Question | Resolution |
|---|---|---|
| 1 | Should `end` be mandatory? | **Yes** (rule 14). Reviewed against the project's principles: a bounded experiment is the safe default, the UI can always say when a run ends, and nothing runs forever by omission. Continuation is an explicit, recorded intervention. |
| 2 | SHA-256 for both module identity and scenario hash? | **Yes.** Both are identities a person may verify with ordinary tools. The trace digest of RFC 001 is an internal, performance-sensitive chain and may use a different function; that choice stays with RFC 001. |
| 3 | A `revision` counter in the manifest? | Deferred to the UI external design. Nothing in this RFC depends on it. |
| 4 | Maximum sizes: actors, connections, ports per actor, manifest bytes. | Deferred to measurement in release 0.1 (NFR-013). The limits exist from the first release; their values come from measurement. |
