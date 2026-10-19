//! The echo guest (RFC 002 §10, AC-001): echoes every message through the port it arrived on;
//! on its timer, reports one random value on channel 1 and re-arms. The kit's reference guest.
// The example guests are WebAssembly modules; on another target the file is empty.
#![cfg(target_arch = "wasm32")]
#![cfg_attr(target_arch = "wasm32", no_std)]

use florn_guest::{Actor, ActorId, Port, PortId, Random, TimerId, host, now};

florn_guest::declare! {
    world: "florn:core/guest@0.1.0",
    kind: "echo",
    description: "Echoes every message; reports a random value on its timer",
    ports: [("p", "core/port", 0, 8)],
    params: [
        (period, "period", u64, false, default = 10),
        (label, "label", text, false, default = "echo"),
    ],
    telemetry: [(1, "draw", Draw, [(value, "value", u64), (at, "at", u64)])],
    resources: [("memory", "1MiB"), ("pending-timers", 4)],
}

struct Echo {
    period: u64,
    random: Random,
    echoed: u64,
}

impl Actor for Echo {
    fn init(_: ActorId, _: &[Port], config: &[u8]) -> Self {
        let c = Config::decode(config);
        host::set_timer(0, c.period);
        Self {
            period: c.period,
            random: Random::host(),
            echoed: 0,
        }
    }
    fn on_message(&mut self, port: PortId, payload: &[u8]) {
        self.echoed += 1;
        host::send(port, payload);
    }
    fn on_timer(&mut self, _: TimerId) {
        Draw {
            value: self.random.u64(),
            at: now(),
        }
        .report();
        host::set_timer(0, self.period);
    }
}

florn_guest::actor!(Echo);
