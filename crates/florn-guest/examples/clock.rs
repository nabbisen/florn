//! The clock guest (RFC 002 §10, the no-ambient-access row): a `std` guest that reads the system
//! clock. The target has no clock, so the read panics inside the guest, the panic aborts, and
//! the Host records a plain trap. Nothing is imported: the guest cannot reach the machine.
// The example guests are WebAssembly modules; on another target the file is empty.
#![cfg(target_arch = "wasm32")]

use florn_guest::{Actor, ActorId, Port, PortId, TimerId, host};

florn_guest::declare! {
    world: "florn:core/guest@0.1.0",
    kind: "clock",
    description: "Reads the system clock, which the target does not have",
    ports: [],
    params: [],
    telemetry: [(1, "elapsed", Elapsed, [(nanos, "nanos", u64)])],
    resources: [("memory", "1MiB"), ("pending-timers", 1)],
}

struct Clock;

impl Actor for Clock {
    fn init(_: ActorId, _: &[Port], _: &[u8]) -> Self {
        host::set_timer(0, 5);
        Clock
    }
    fn on_message(&mut self, _: PortId, _: &[u8]) {}
    fn on_timer(&mut self, _: TimerId) {
        let start = std::time::SystemTime::now();
        let nanos = start
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        Elapsed { nanos }.report();
    }
}

florn_guest::actor!(Clock);
