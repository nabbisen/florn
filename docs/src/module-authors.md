# For module authors

A guest is one core WebAssembly module that implements the `florn:core/guest` world and carries its manifest in a custom section. florn runs it as an actor: the Host calls it when a message arrives or a timer fires, the guest answers by calling the Host's functions, and every effect is recorded. This chapter takes a Rust author from an empty directory to a running actor. The contract itself is RFC 002; the limits are RFC 003.

## What you need

The Rust toolchain with the guest target, and the `florn` runner:

```sh
rustup target add wasm32-unknown-unknown
cargo install florn                    # from crates.io, or
cargo install --git https://github.com/nabbisen/florn --tag 0.2.0 florn
cargo install --path crates/florn      # from a checkout of florn
```

`florn help` prints the commands and the exit codes.

## The template

The SDK's readme (`crates/florn-guest/README.md`) carries a complete template: a `Cargo.toml` of a `cdylib` crate depending on `florn-guest`, and a `src/lib.rs` with one declaration and one `impl Actor`. The dependency is written in one of two forms:

```toml
florn-guest = "0.2.0"                                                         # from crates.io
florn-guest = { git = "https://github.com/nabbisen/florn", tag = "0.2.0" }   # pinned to the tag
florn-guest = { path = "../florn/crates/florn-guest" }                        # from a checkout
```

Build, then admit:

```sh
cargo build --release --target wasm32-unknown-unknown
florn admit target/wasm32-unknown-unknown/release/my_guest.wasm
```

`admit` runs the admission step of RFC 003 and prints what the Host learnt from the manifest: the contract, the kind, the port kinds, every parameter with its type, unit and default, the telemetry channels, the needs, and the admitted hash. A refusal is a coded message, such as `module.import-refused` for an import outside the world.

## One declaration, two artefacts

`florn_guest::declare!` writes the manifest section and generates the `Config` type that decodes the `config` argument of `init`, so that the two cannot disagree. It declares:

| Part | What it says | Example |
|---|---|---|
| `world` | The contract | `"florn:core/guest@0.1.0"` |
| `kind` | The module's kind, as scenarios and people name it | `"echo"` |
| `ports` | The port kinds with a type and a count range | `("p", "core/port", 0, 8)` |
| `params` | The parameters in order, each `(ident, "name", type, required)` with an optional default and unit | `(period, "period", u64, false, default = 10)` |
| `telemetry` | One record type per channel, with its fields | `(1, "draw", Draw, [(value, "value", u64)])` |
| `resources` | What the guest needs of each limit | `("memory", "1MiB")`, `("pending-timers", 4)` |

The scenario gives a value for every required parameter and may give one for the others; the Host fills the defaults and hands `init` the values in declaration order. `Config::decode` reads them. A parameter of type `ticks` is written in the scenario as a duration (`"5ms"`) and arrives as a count of ticks; one that is not whole in ticks is refused.

## What the Host calls, what the guest may call

| The Host calls | When |
|---|---|
| `init(actor, ports, config)` | Once, before any other call; the ports as the scenario declared them |
| `on_message(port, payload)` | A payload arrived through one of the actor's ports |
| `on_timer(timer)` | A timer the guest set expired |

| The guest calls | What it does |
|---|---|
| `host::send(port, payload)` | Delivers the payload through the port to every other port of the connection |
| `host::set_timer(timer, delay)` | Asks for `on_timer(timer)` `delay` ticks later; the same identity replaces a pending timer |
| `host::cancel_timer(timer)` | Drops a pending timer |
| `host::random()` and `Random` | The actor's seeded stream and the distributions over it |
| `host::report(channel, payload)` and `Record::report()` | Telemetry on a declared channel |
| `host::log(text)` | Text for you, on channel 0; it appears in the diagnostic log |
| `now()` | The current tick |

Each call is one activation. The effects of an activation are committed together when it returns normally; an activation that ends in a fault commits nothing (RFC 001 rule 24), and the random stream stands where the activation began.

## Faults and limits

An actor that faults handles nothing further in that run; the run continues under the scenario's policy, and the diagnostic log says what happened. The kinds:

| Fault | What causes it |
|---|---|
| `fault.trap` | The guest trapped: a panic, `unreachable`, an access out of bounds, a division by zero, a `std` call the target cannot serve (the clock) |
| `fault.budget` | The activation exceeded its budget of cost-model units |
| `fault.stack` | The call stack exceeded its height |
| `fault.quota` | A quota was exceeded; the message names which: effects per activation, payload size, pending timers, telemetry bytes per activation |
| `fault.contract` | A contract violation: an undeclared port or channel, a pointer outside the memory, a call out of sequence; the message names the rule |

The defaults, which a scenario may lower and the Host's maxima bound (RFC 003 rule 19): budget 100,000,000 units per activation (a 64×64 matrix product costs about ten million; a protocol-style activation about a hundred thousand), stack 1,024, memory 4 MiB, 1,000 effects per activation, 64 KiB per payload, 64 pending timers, 16 KiB of telemetry per activation. A guest declares what it needs in `resources`; a scenario that gives less is refused before the run starts, so that a fault is never a surprise of configuration.

Memory growth past the limit is not a fault: `memory.grow` returns −1 and the guest decides. The SDK's own sharp edges are traps by design: `below(0)`, an empty range, a config shorter than declared, a report on an undeclared channel, a panic under either profile. Use `host::log` before a risky step while developing.

The budget is counted by florn's metering pass (RFC 003 section 4; RFC 010): one unit per instruction. Under cost-model version 2 the pass checks the budget before every call, before a loop's back-edge, at every return and at the end, so a function is refused at admission when its length exceeds the class's budget (`module.segment-too-long`): the budget must exceed the longest function, which `florn admit` reports.

## Running your guest

A scenario names your module by its hash and places actors over it. For one actor with a timer, this is enough:

```toml
florn = 1
name = "mine"
domains = ["core"]

[time]
tick = "1ns"
end = "1000ns"

[order]
tie = "by-identity"

[seed]
root = 1

[modules.mine]
sha256 = "<the source hash florn admit printed>"
contract = "florn:core/guest@0.1.0"

[[actors]]
id = 1
name = "a1"
kind = "module"
module = "mine"
ports = []
[actors.config]
label = "hello"
```

```sh
florn run mine.toml --module target/wasm32-unknown-unknown/release/my_guest.wasm --seed 1 --trace --payloads
```

The runner prints the chain (the digest of the trace), writes `mine.florn-run`, and reports every notification on standard error. Two actors talk through a connection:

```toml
[[actors]]
id = 2
name = "a2"
kind = "module"
module = "mine"
ports = [{ id = 0, name = "p0", type = "core/port" }]

[[connections]]
id = 100
name = "c0"
ports = ["a1.p0", "a2.p0"]
```

The scenario format is RFC 004; the shipped scenarios under `scenarios/` are worked examples over the SDK's guests.

## Profiles and size

The default profile uses `std`: the standard library's allocator, and a panic aborts, which traps. `--no-default-features --features no-std` selects the SDK's allocator and a panic handler that traps, for a smaller module. The `#![no_std]` attribute in your own crate is a separate choice: the template uses it so that nothing of `std` is pulled in by accident, and it works under both profiles.

## Network guests

A guest of the network domain (RFC 008) speaks the router world, `florn:network/router@0.1.0`: the core world plus one import, `link-info`, and one export, `on-link`. It depends on `florn-guest` and on `florn-guest-network`, declares its interfaces with the port type `network/interface`, implements `florn_guest::Actor` and `florn_guest_network::Router`, and exports with `florn_guest_network::router!` instead of `actor!`:

```toml
[dependencies]
florn-guest = "0.2.0"
florn-guest-network = "0.2.0"
```

```rust
florn_guest::declare! {
    world: "florn:network/router@0.1.0",
    kind: "net-echo",
    description: "Echoes every frame through the interface it arrived on",
    ports: [("eth", "network/interface", 1, 8)],
    params: [],
    telemetry: [],
    resources: [("memory", "1MiB"), ("pending-timers", 1)],
    tables: [("network", [("protocol", "net-echo")])],
}

impl florn_guest_network::Router for NetEcho {
    fn on_link(&mut self, port: PortId, up: bool) { /* the link of `port` went up or down */ }
}

florn_guest_network::router!(NetEcho);
```

A frame is `on_message` through the interface's port, its payload the bytes the sender gave `send`. `link_info(port)` answers the state of one interface as it was when the activation began: whether the link is up, the per-port cost, the mtu, the bandwidth. The optional `tables` entry names the protocol the frames follow, so that the inspector decodes them with the domain's decoder for it; a protocol without a decoder is a warning at admission and the frames are shown as bytes. A guest that reports on channels named `neighbour` and `route` with the layouts of RFC 008 rule 17 gets its neighbours and routes shown by the network view. The examples in `crates/florn-guest-network/examples` are the ladder of RFC 008 rule 20: `net_echo`, which echoes every frame, and `distance_vector`, a router that advertises its routes with split horizon, counts to infinity, withdraws what it has not heard for three periods and reports `neighbour` and `route`; the kit's network scenarios are built on them.

The four network examples under `crates/florn-guest-network/examples/` are the ladder: `net_echo`, `distance_vector`, `ospf` and `bgp`. The last two show the pattern for a protocol that ships a declared subset: everything outside it is refused with a line on channel 0, which the application and the diagnostic log show, and the router keeps running.

## Where to look next

The fifteen example guests under `crates/florn-guest/examples/` show every feature in use: `echo` is the reference, `chatter` exercises parameters, telemetry and faults, `timers` the timer rules, `heap` state that grows, and the `hostile_*` guests each one fault kind. The conformance kit's readme maps every rule of RFC 002 and RFC 003 to the test that proves it.
