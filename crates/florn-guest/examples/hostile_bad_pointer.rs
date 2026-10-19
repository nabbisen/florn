//! A hostile guest of RFC 003 §10: calls `send` with a pointer outside its memory (the contract
//! fault of RFC 002 rule 8). It bypasses the SDK's safe wrapper to do so.
// The example guests are WebAssembly modules; on another target the file is empty.
#![cfg(target_arch = "wasm32")]
#![cfg_attr(target_arch = "wasm32", no_std)]

use florn_guest::{Actor, ActorId, Port, PortId, TimerId, host};

florn_guest::declare! {
    world: "florn:core/guest@0.1.0",
    kind: "hostile-bad-pointer",
    description: "Sends from an address outside its memory",
    ports: [("p", "core/port", 0, 8)],
    params: [],
    telemetry: [],
    resources: [("memory", "1MiB"), ("pending-timers", 1)],
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "florn:core/host@0.1.0")]
unsafe extern "C" {
    #[link_name = "send"]
    fn raw_send(port: i32, ptr: i32, len: i32);
}

struct Hostile;

impl Actor for Hostile {
    fn init(_: ActorId, _: &[Port], _: &[u8]) -> Self {
        host::set_timer(0, 5);
        Hostile
    }
    fn on_message(&mut self, _: PortId, _: &[u8]) {}
    fn on_timer(&mut self, _: TimerId) {
        #[cfg(target_arch = "wasm32")]
        // SAFETY: none; the point of this guest is the Host's refusal of the range (RFC 002 rule 8).
        unsafe {
            raw_send(0, 0x7fff_0000, 16);
        }
    }
}

florn_guest::actor!(Hostile);
