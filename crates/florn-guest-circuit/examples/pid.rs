//! The ladder's first controller (RFC 012 rule 26): a PID controller that samples its measurement,
//! compares it with a set point, and sets one output. It reads two inputs, the port named
//! `measurement` and, when that port is on a signal, the port named `setpoint`, whose default is
//! the `setpoint` parameter; it sets the signal of its `output` port and reports `control` (what it
//! read, what it set, the error) and `terms` (the three terms of the sum). Its gains are those of the
//! discrete form: the integral gains `ki × error` at every sample and the derivative is `kd` times the
//! change of the error between two samples, so the controller needs no tick length. The output is
//! limited to `[out-min, out-max]` and the integral is not increased while the output is saturated.
// The example guests are WebAssembly modules; on another target the file is empty.
#![cfg(target_arch = "wasm32")]
#![cfg_attr(target_arch = "wasm32", no_std)]

extern crate alloc;

use florn_guest::{Actor, ActorId, Port, PortId};
use florn_guest_circuit::{Controller, read_input, set_output};

florn_guest::declare! {
    world: "florn:circuit/controller@0.1.0",
    kind: "pid",
    description: "A PID controller: reads a measurement and a set point, sets one output",
    ports: [("in", "circuit/signal-in", 1, 2), ("out", "circuit/signal-out", 1, 1)],
    params: [
        (period, "period", ticks, true),
        (phase, "phase", ticks, false, default = "0ns"),
        (kp, "kp", f64, false, default = 1.0),
        (ki, "ki", f64, false, default = 0.0),
        (kd, "kd", f64, false, default = 0.0),
        (out_min, "out-min", f64, false, default = -1000000.0),
        (out_max, "out-max", f64, false, default = 1000000.0),
        (setpoint, "setpoint", f64, false, default = 0.0),
    ],
    telemetry: [
        (1, "control", Control, [(input, "input", f64), (output, "output", f64), (error, "error", f64)]),
        (2, "terms", Terms, [(p, "p", f64), (i, "i", f64), (d, "d", f64)]),
    ],
    resources: [("memory", "1MiB")],
    tables: [(
        "circuit",
        [
            ("kind", "controller"),
            ("period", "period"),
            ("phase", "phase"),
            ("control-units", { input = "rad/s", error = "rad/s" }),
        ]
    )],
}

struct Pid {
    c: Config,
    measurement: Option<PortId>,
    setpoint: Option<PortId>,
    output: Option<PortId>,
    integral: f64,
    last_error: f64,
    started: bool,
}

impl Actor for Pid {
    fn init(_: ActorId, ports: &[Port], config: &[u8]) -> Self {
        let named = |n: &str| ports.iter().find(|p| p.name == n).map(|p| p.id);
        Self {
            c: Config::decode(config),
            measurement: named("measurement"),
            setpoint: named("setpoint"),
            output: named("output"),
            integral: 0.0,
            last_error: 0.0,
            started: false,
        }
    }

    // A controller of the plain world has no frames and no timers of its own (RFC 012 rule 18).
    fn on_message(&mut self, _port: PortId, _payload: &[u8]) {}

    fn on_timer(&mut self, _timer: florn_guest::TimerId) {}
}

impl Controller for Pid {
    fn on_sample(&mut self, _index: u64) {
        let c = &self.c;
        let measurement = self.measurement.and_then(read_input).unwrap_or(0.0);
        let setpoint = self.setpoint.and_then(read_input).unwrap_or(c.setpoint);
        let error = setpoint - measurement;
        let p = c.kp * error;
        // The derivative of the first sample is zero: there is no sample before it.
        let d = if self.started {
            c.kd * (error - self.last_error)
        } else {
            0.0
        };
        let raw = p + self.integral + d;
        let output = if raw > c.out_max {
            c.out_max
        } else if raw < c.out_min {
            c.out_min
        } else {
            // Not saturated: the integral gains this sample's share.
            self.integral += c.ki * error;
            raw
        };
        self.last_error = error;
        self.started = true;
        if let Some(port) = self.output {
            set_output(port, output);
        }
        Control {
            input: measurement,
            output,
            error,
        }
        .report();
        Terms {
            p,
            i: self.integral,
            d,
        }
        .report();
    }
}

florn_guest_circuit::controller!(Pid);
