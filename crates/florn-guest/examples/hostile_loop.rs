//! A hostile guest of RFC 003 §10: loops for ever (the budget fault).
// The example guests are WebAssembly modules; on another target the file is empty.
#![cfg(target_arch = "wasm32")]
#![cfg_attr(target_arch = "wasm32", no_std)]

use florn_guest::{Actor, ActorId, Port, PortId, TimerId, host};

florn_guest::declare! {
    world: "florn:core/guest@0.1.0",
    kind: "hostile-loop",
    description: "loops for ever (the budget fault).",
    ports: [("p", "core/port", 0, 8)],
    params: [],
    telemetry: [],
    resources: [("memory", "1MiB"), ("pending-timers", 1)],
}

struct Hostile {
    counter: u64,
}

impl Actor for Hostile {
    fn init(_: ActorId, _: &[Port], _: &[u8]) -> Self {
        host::set_timer(0, 5);
        Hostile { counter: 0 }
    }
    fn on_message(&mut self, _: PortId, _: &[u8]) {}
    fn on_timer(&mut self, _: TimerId) {
        loop {
            self.counter = core::hint::black_box(self.counter.wrapping_add(1));
        }
    }
}

florn_guest::actor!(Hostile);
