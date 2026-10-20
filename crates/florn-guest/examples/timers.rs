//! The timers guest (RFC 002 §10, the init and timer rows): sets timers in `init` as its mode
//! says and reports every expiry with the tick it fired at.
// The example guests are WebAssembly modules; on another target the file is empty.
#![cfg(target_arch = "wasm32")]
#![cfg_attr(target_arch = "wasm32", no_std)]

use florn_guest::{Actor, ActorId, Port, PortId, TimerId, host, now};

florn_guest::declare! {
    world: "florn:core/guest@0.1.0",
    kind: "timers",
    description: "Sets timers in init as its mode says; reports every expiry",
    ports: [],
    params: [
        (mode, "mode", text, false, default = "init-delay"),
        (delay, "delay", ticks, false, default = "10ns"),
        (count, "count", u64, false, default = 1),
    ],
    telemetry: [(1, "fired", Fired, [(timer, "timer", u64), (at, "at", u64)])],
    resources: [("memory", "128KiB")],
}

struct Timers;

impl Actor for Timers {
    fn init(_: ActorId, _: &[Port], config: &[u8]) -> Self {
        let c = Config::decode(config);
        match c.mode.as_str() {
            // One timer, fires at `delay`.
            "init-delay" => host::set_timer(1, c.delay),
            // Set then cancelled in one activation: no timer; a second one shows the actor lives.
            "set-cancel" => {
                host::set_timer(1, c.delay);
                host::cancel_timer(1);
                host::set_timer(2, c.delay * 2);
            }
            // Set twice with one identity: the second delay replaces the first; fires once.
            "set-twice" => {
                host::set_timer(1, c.delay);
                host::set_timer(1, c.delay + 5);
            }
            // `count` distinct timers pending after commit: the pending-timers quota applies.
            "many" => {
                for i in 0..c.count {
                    host::set_timer(i as u32, c.delay + i);
                }
            }
            _ => florn_guest::actor::trap(),
        }
        Timers
    }
    fn on_message(&mut self, _: PortId, _: &[u8]) {}
    fn on_timer(&mut self, timer: TimerId) {
        Fired {
            timer: u64::from(timer),
            at: now(),
        }
        .report();
    }
}

florn_guest::actor!(Timers);
