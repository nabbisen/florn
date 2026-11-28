//! A peer for `cps-link` (RFC 012 rule 27): it sends one frame, at the tick its `at` parameter
//! names, through its first interface: eight bytes, the little-endian `f64` of `value`, the new set
//! point a `networked_pid` reads (`florn-guest-cps`'s example). A scenario file cannot carry the
//! `message` intervention of `cps-loop`, so this guest is what makes the composed actor's
//! `on-message` run on every engine. It is not one of the ladder's guests.
// The example guests are WebAssembly modules; on another target the file is empty.
#![cfg(target_arch = "wasm32")]
#![cfg_attr(target_arch = "wasm32", no_std)]

extern crate alloc;

use florn_guest::{Actor, ActorId, Port, PortId, TimerId, host};
use florn_guest_network::Router;

florn_guest::declare! {
    world: "florn:network/router@0.1.0",
    kind: "set-point-source",
    description: "Sends one frame of eight bytes, a set point, at a chosen tick",
    ports: [("eth", "network/interface", 1, 1)],
    params: [
        (at, "at", ticks, true),
        (value, "value", f64, false, default = 40.0),
    ],
    telemetry: [],
    resources: [("memory", "1MiB"), ("pending-timers", 1)],
}

struct Source {
    value: f64,
    port: PortId,
}

impl Actor for Source {
    fn init(_: ActorId, ports: &[Port], config: &[u8]) -> Self {
        let c = Config::decode(config);
        host::set_timer(0, c.at);
        Self {
            value: c.value,
            port: ports.first().map_or(0, |p| p.id),
        }
    }

    // A source of frames takes none.
    fn on_message(&mut self, _port: PortId, _payload: &[u8]) {}

    fn on_timer(&mut self, _: TimerId) {
        host::send(self.port, &self.value.to_le_bytes());
    }
}

impl Router for Source {
    fn on_link(&mut self, _port: PortId, _up: bool) {}
}

florn_guest_network::router!(Source);
