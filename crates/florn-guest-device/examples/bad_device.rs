//! A device for the kit's rows that a good one cannot show (RFC 012 rules 17, 19 and 20; RFC 013
//! rules 8, 9 and 16): a linear conductance of `g` siemens between its two pins that misbehaves as
//! its `mode` says, at the `at`-th evaluation (counting from 1 since the module started):
//!
//! | `mode` | what it does |
//! |---|---|
//! | `good` | nothing wrong: the conductance |
//! | `nan` | a NaN current: the stamp is not finite |
//! | `short` | one value fewer than `n + n² + 1` |
//! | `long` | one value more than `n + n² + 1` (over the bound: never lifted) |
//! | `flag` | the last entry is 2.0, neither 0 nor 1 |
//! | `timer` | sets a timer: a contract violation (RFC 012 rule 19, parameter 12019) |
//! | `send` | sends on its pin: a contract violation (rule 19, parameter 12019) |
//! | `trap` | traps |
//! | `diverge` | from the `at`-th evaluation on, a current that flips sign at every call: Newton never converges |
//! | `limit` | reports `limited = 1.0` at the first evaluation of every step, and behaves as `good` |
//!
//! It is not one of the ladder's guests.
// The example guests are WebAssembly modules; on another target the file is empty.
#![cfg(target_arch = "wasm32")]
#![cfg_attr(target_arch = "wasm32", no_std)]

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use florn_guest::{Actor, ActorId, Port, PortId};
use florn_guest_device::Device;

florn_guest::declare! {
    world: "florn:circuit/device@0.1.0",
    kind: "bad-device",
    description: "A conductance that misbehaves as its mode says: a NaN, a wrong length, a timer, a trap, no convergence",
    ports: [("pin", "circuit/pin", 2, 2)],
    params: [
        (mode, "mode", text, false, default = "good"),
        (at, "at", u64, false, default = 1),
        (g, "g", f64, false, default = 0.001),
    ],
    telemetry: [
        (1, "state", State, [(v, "v", f64)]),
    ],
    resources: [("memory", "1MiB")],
    tables: [("circuit", [("kind", "device")])],
}

struct Bad {
    c: Config,
    /// The first pin (the lowest port identity), for a `send`.
    pin: PortId,
    /// The evaluations since the module started.
    calls: u64,
    /// Whether the step in hand has had its first evaluation.
    first: bool,
}

impl Actor for Bad {
    fn init(_: ActorId, ports: &[Port], config: &[u8]) -> Self {
        Self {
            c: Config::decode(config),
            pin: ports.first().map_or(0, |p| p.id),
            calls: 0,
            first: false,
        }
    }

    fn on_message(&mut self, _port: PortId, _payload: &[u8]) {}

    fn on_timer(&mut self, _timer: florn_guest::TimerId) {}
}

impl Device for Bad {
    fn begin_step(&mut self, _start: u64, _h: f64) {
        self.first = true;
        if self.c.mode == "timer" && self.calls + 1 == self.c.at {
            florn_guest::host::set_timer(1, 10);
        }
        if self.c.mode == "send" && self.calls + 1 == self.c.at {
            florn_guest::host::send(self.pin, &[1, 2, 3]);
        }
    }

    fn evaluate(&mut self, trial: &[f64]) -> Vec<f64> {
        self.calls += 1;
        let at = self.calls == self.c.at;
        let first = core::mem::take(&mut self.first);
        let g = self.c.g;
        let i = g * (trial[0] - trial[1]);
        let mut out = vec![i, -i, g, -g, -g, g, 0.0];
        match self.c.mode.as_str() {
            "nan" if at => out[1] = f64::NAN,
            "short" if at => {
                out.pop();
            }
            "long" if at => out.push(0.0),
            "flag" if at => out[6] = 2.0,
            "trap" if at => {
                #[cfg(target_arch = "wasm32")]
                core::arch::wasm32::unreachable();
            }
            "diverge" if self.calls >= self.c.at => {
                // A current that does not follow the voltage: +1 A at one call, −1 A at the next,
                // with the slope a conductance of 1 mS would have.
                let flip = if self.calls % 2 == 0 { 1.0 } else { -1.0 };
                out[0] = flip;
                out[1] = -flip;
            }
            "limit" if first => out[6] = 1.0,
            _ => {}
        }
        out
    }

    fn accept_step(&mut self, solution: &[f64]) {
        State {
            v: solution[0] - solution[1],
        }
        .report();
    }

    fn reject_step(&mut self) {}
}

florn_guest_device::device!(Bad);
