# florn-guest

The guest SDK (RFC 002 section 8). A guest is one core WebAssembly module that implements the
`florn:core/guest` world and carries its manifest. The SDK generates the bindings, writes the
manifest section and the config decoder from one declaration, and provides the distributions,
the telemetry encoders, `now()`, and the `std` and `no_std` profiles. It needs nothing beyond
the Rust toolchain with the `wasm32-unknown-unknown` target.

The SDK is not the contract (rule 51): a guest written without it that implements the world and
carries the manifest conforms.

## The template

`Cargo.toml`:

```toml
[package]
name = "my-guest"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
florn-guest = { git = "https://github.com/nabbisen/florn", tag = "0.1.0" }
# or, from a checkout beside your crate:
# florn-guest = { path = "../florn/crates/florn-guest" }

[profile.release]
opt-level = "s"
lto = true
panic = "abort"
```

`src/lib.rs`:

```rust
#![no_std]

use florn_guest::{Actor, ActorId, Port, PortId, Random, TimerId, host, now};

florn_guest::declare! {
    world: "florn:core/guest@0.1.0",
    kind: "my-guest",
    description: "What it does, for people",
    ports: [("p", "core/port", 0, 8)],
    params: [
        (period, "period", u64, false, default = 10),
        (label, "label", text, true),
    ],
    telemetry: [(1, "draw", Draw, [(value, "value", u64)])],
    resources: [("memory", "1MiB"), ("pending-timers", 4)],
}

struct MyGuest {
    period: u64,
    random: Random,
}

impl Actor for MyGuest {
    fn init(_: ActorId, _: &[Port], config: &[u8]) -> Self {
        let c = Config::decode(config);
        host::set_timer(0, c.period);
        Self { period: c.period, random: Random::host() }
    }
    fn on_message(&mut self, port: PortId, payload: &[u8]) {
        host::send(port, payload);
    }
    fn on_timer(&mut self, _: TimerId) {
        Draw { value: self.random.u64() }.report();
        host::log("tick");
        let _ = now();
        host::set_timer(0, self.period);
    }
}

florn_guest::actor!(MyGuest);
```

Build and admit (the runner comes from `cargo install --path crates/florn` in a checkout of
florn, or `cargo install --git https://github.com/nabbisen/florn --tag 0.1.0 florn`):

```sh
rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown
florn admit target/wasm32-unknown-unknown/release/my_guest.wasm
```

Running it needs a scenario that names the module by its hash; the book's chapter for module
authors (`docs/src/module-authors.md`) carries a one-actor scenario and the `florn run` line.

`florn admit` prints the contract, the kind, the port kinds, the parameters with their types,
units and defaults, the channels, the needs, and the admitted hash.

## The declaration

| Part | Form | Manifest |
|---|---|---|
| Port kinds | `("kind", "domain/type", min, max)` | `[[ports]]` |
| Parameters | `(ident, "name", type, required)`, then `, default = <TOML literal>` for one with a default, and `, unit = "..."` for `integer` (the delivery unit, a quantity such as `"0.01USD"`) and `real` (the unit symbol) | `[[params]]` |
| Telemetry | `(channel, "name", RecordType, [(ident, "name", type), ...])` | `[[telemetry]]`; `RecordType::report()` encodes and reports |
| Needs | `("limit", value)` | `[resources]` |

Parameter types and their Rust types: `bool`, `u64`, `s64` (`i64`), `f64`, `text` (`String`),
`bytes` (`Vec<u8>`), `ticks` (`u64`), `integer` (`i64` in the delivery unit), `real` (`f64` in
the SI unit of the dimension). `Config::decode` reads the `config` argument of `init`: every
declared parameter in order, defaults filled by the Host (rule 14).

## Randomness

`Random::host()` draws from the actor's stream. `below`, `range`, `f64`, `chance`,
`exponential`, `normal`, and `choose` are computed by the algorithms the module documentation
fixes, the same in every guest (RFC 001 rule 30). Every draw belongs to the activation: a fault
restores the stream (RFC 002 rule 31).

## Profiles

The default profile uses `std` (the standard library's allocator; a panic aborts, which traps).
The `no_std` profile, `--no-default-features --features no-std`, brings the SDK's allocator and
a panic handler that traps, for size. The `#![no_std]` attribute of your own crate is a separate
choice: it keeps `std` out of your code and works under both profiles.

## Examples

`cargo build --release --target wasm32-unknown-unknown --examples` builds the fifteen example
guests into `target/wasm32-unknown-unknown/release/examples/`: the echo guest, the NaN probe,
the six hostile guests of RFC 003 section 10 (loop, recurse, grow, flood, bad pointer, a WASI
import), and the conformance kit's guests (pinger, timers, chatter, heap, clock, and the
benchmark guests B1 and B2). The kit's readme (`crates/florn-conformance/README.md`) says what
each is for.
