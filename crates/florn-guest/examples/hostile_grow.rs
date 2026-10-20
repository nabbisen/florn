//! A hostile guest of RFC 003 §10: grows its memory until the limit refuses, then traps at that point.
// The example guests are WebAssembly modules; on another target the file is empty.
#![cfg(target_arch = "wasm32")]
#![cfg_attr(target_arch = "wasm32", no_std)]

use florn_guest::{Actor, ActorId, Port, PortId, TimerId, host};

florn_guest::declare! {
    world: "florn:core/guest@0.1.0",
    kind: "hostile-grow",
    description: "grows its memory until the limit refuses, then traps at that point (or reports the page count)",
    ports: [("p", "core/port", 0, 8)],
    params: [(trap, "trap", bool, false, default = true)],
    telemetry: [(1, "pages", Pages, [(pages, "pages", u64)])],
    resources: [("memory", "1MiB"), ("pending-timers", 1)],
}

struct Hostile {
    counter: u64,
    trap: bool,
}

impl Actor for Hostile {
    fn init(_: ActorId, _: &[Port], config: &[u8]) -> Self {
        host::set_timer(0, 5);
        Hostile {
            counter: 0,
            trap: Config::decode(config).trap,
        }
    }
    fn on_message(&mut self, _: PortId, _: &[u8]) {}
    fn on_timer(&mut self, _: TimerId) {
        loop {
            if core::arch::wasm32::memory_grow(0, 1) == usize::MAX {
                if self.trap {
                    core::arch::wasm32::unreachable();
                }
                // RFC 003 rule 16: the refusal is -1 and no fault; the page count is the limit.
                Pages {
                    pages: core::arch::wasm32::memory_size(0) as u64,
                }
                .report();
                return;
            }
            self.counter += 1;
        }
    }
}

florn_guest::actor!(Hostile);
