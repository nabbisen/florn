//! A hostile guest of RFC 002 §10: imports a WASI function. Admission refuses it with
//! `module.import-refused` (RFC 003 rule 5); it never runs.
// The example guests are WebAssembly modules; on another target the file is empty.
#![cfg(target_arch = "wasm32")]
#![cfg_attr(target_arch = "wasm32", no_std)]

use florn_guest::{Actor, ActorId, Port, PortId, TimerId};

florn_guest::declare! {
    world: "florn:core/guest@0.1.0",
    kind: "hostile-wasi",
    description: "Imports a WASI function",
    ports: [],
    params: [],
    telemetry: [],
    resources: [("memory", "128KiB"), ("pending-timers", 1)],
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "wasi:cli/environment@0.2.0")]
unsafe extern "C" {
    #[link_name = "get-environment"]
    fn get_environment(out: i32);
}

struct Hostile;

impl Actor for Hostile {
    fn init(_: ActorId, _: &[Port], _: &[u8]) -> Self {
        #[cfg(target_arch = "wasm32")]
        // SAFETY: never reached; the import alone makes admission refuse the module.
        unsafe {
            get_environment(0);
        }
        Hostile
    }
    fn on_message(&mut self, _: PortId, _: &[u8]) {}
    fn on_timer(&mut self, _: TimerId) {}
}

florn_guest::actor!(Hostile);
