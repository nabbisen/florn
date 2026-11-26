//! A controller for the kit's rows that a good one cannot show (RFC 012 rules 11, 17 and 18; RFC 014
//! rule 3): it sets a constant `value` on its output, traps at the sample `trap-at` when that is
//! not zero, sets a value that is not finite at the sample `nan-at` when that is not zero (a
//! contract violation, rule 18), and reports on two channels both named `control`, so that the
//! indices of its series run on across both (RFC 014 rule 3). It is not one of the ladder's guests.
// The example guests are WebAssembly modules; on another target the file is empty.
#![cfg(target_arch = "wasm32")]
#![cfg_attr(target_arch = "wasm32", no_std)]

extern crate alloc;

use florn_guest::{Actor, ActorId, Port, PortId};
use florn_guest_circuit::{Controller, read_input, set_output};

florn_guest::declare! {
    world: "florn:circuit/controller@0.1.0",
    kind: "bad-controller",
    description: "Sets a constant; traps or sets a NaN at a chosen sample; two control channels",
    ports: [("in", "circuit/signal-in", 0, 2), ("out", "circuit/signal-out", 1, 1)],
    params: [
        (period, "period", ticks, true),
        (phase, "phase", ticks, false, default = "0ns"),
        (value, "value", f64, false, default = 0.0),
        (trap_at, "trap-at", u64, false, default = 0),
        (nan_at, "nan-at", u64, false, default = 0),
    ],
    telemetry: [
        (1, "control", First, [(input, "input", f64), (output, "output", f64), (error, "error", f64)]),
        (2, "control", Second, [(input, "input", f64), (output, "output", f64), (error, "error", f64)]),
    ],
    resources: [("memory", "1MiB")],
    tables: [(
        "circuit",
        [
            ("kind", "controller"),
            ("period", "period"),
            ("phase", "phase"),
            ("control-units", { input = "V", error = "V" }),
        ]
    )],
}

struct Bad {
    c: Config,
    input: Option<PortId>,
    output: Option<PortId>,
}

impl Actor for Bad {
    fn init(_: ActorId, ports: &[Port], config: &[u8]) -> Self {
        Self {
            c: Config::decode(config),
            input: ports.iter().find(|p| p.name == "input").map(|p| p.id),
            output: ports.iter().find(|p| p.name == "output").map(|p| p.id),
        }
    }

    fn on_message(&mut self, _port: PortId, _payload: &[u8]) {}

    fn on_timer(&mut self, _timer: florn_guest::TimerId) {}
}

impl Controller for Bad {
    fn on_sample(&mut self, index: u64) {
        let c = &self.c;
        let read = self.input.and_then(read_input).unwrap_or(0.0);
        if c.trap_at != 0 && index + 1 == c.trap_at {
            #[cfg(target_arch = "wasm32")]
            core::arch::wasm32::unreachable();
        }
        let out = if c.nan_at != 0 && index + 1 == c.nan_at {
            f64::NAN
        } else {
            c.value
        };
        if let Some(port) = self.output {
            set_output(port, out);
        }
        First {
            input: read,
            output: out,
            error: 0.0,
        }
        .report();
        Second {
            input: read,
            output: out,
            error: 0.0,
        }
        .report();
    }
}

florn_guest_circuit::controller!(Bad);
