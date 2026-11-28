//! The ladder's third guest (RFC 012 rule 26): `pid` whose set point arrives as a frame on its
//! interface, the composed world of rule 22 (two domains in one actor). It samples as `pid` does:
//! reads the signal its `measurement` port is on, compares it with the set point, and sets the
//! signal of its `output` port. Its `eth` port is a `network/interface`: a frame of eight bytes, the
//! little-endian `f64` of the new set point in the unit of the measurement, **replaces the set
//! point at once**, and the controller answers it in the same activation, with the proportional and
//! integral terms it has and the measurement the signal holds now, without waiting for the next
//! sample: a `set-output` committed from `on-message`, an input change at a tick that is not a
//! sample (RFC 012 rules 9 and 22). A frame of another length, or a value that is not finite, is
//! ignored and reported on channel `frame` with the set point it left.
// The example guests are WebAssembly modules; on another target the file is empty.
#![cfg(target_arch = "wasm32")]
#![cfg_attr(target_arch = "wasm32", no_std)]

extern crate alloc;

use florn_guest::{Actor, ActorId, Port, PortId};
use florn_guest_cps::{NetworkedController, read_input, set_output};

florn_guest::declare! {
    world: "florn:cps/networked-controller@0.1.0",
    kind: "networked-pid",
    description: "A PID controller whose set point arrives as a frame on its interface",
    ports: [
        ("eth", "network/interface", 1, 1),
        ("in", "circuit/signal-in", 1, 1),
        ("out", "circuit/signal-out", 1, 1),
    ],
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
        (3, "frame", Frame, [(setpoint, "setpoint", f64), (accepted, "accepted", bool)]),
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

struct NetworkedPid {
    c: Config,
    measurement: Option<PortId>,
    output: Option<PortId>,
    /// The set point in force: the parameter, then what the last frame said.
    setpoint: f64,
    integral: f64,
    last_error: f64,
    started: bool,
}

impl NetworkedPid {
    /// The output the proportional and integral terms give at `error`, limited.
    fn limited(&self, raw: f64) -> f64 {
        if raw > self.c.out_max {
            self.c.out_max
        } else if raw < self.c.out_min {
            self.c.out_min
        } else {
            raw
        }
    }
}

impl Actor for NetworkedPid {
    fn init(_: ActorId, ports: &[Port], config: &[u8]) -> Self {
        let named = |n: &str| ports.iter().find(|p| p.name == n).map(|p| p.id);
        let c = Config::decode(config);
        Self {
            setpoint: c.setpoint,
            c,
            measurement: named("measurement"),
            output: named("output"),
            integral: 0.0,
            last_error: 0.0,
            started: false,
        }
    }

    // A frame of the interface: the new set point (RFC 012 rule 22).
    fn on_message(&mut self, _port: PortId, payload: &[u8]) {
        let accepted = match <[u8; 8]>::try_from(payload) {
            Ok(bytes) => {
                let sp = f64::from_le_bytes(bytes);
                if sp.is_finite() {
                    self.setpoint = sp;
                    true
                } else {
                    false
                }
            }
            Err(_) => false,
        };
        if accepted {
            // The plant is told at once, from this activation's tick: the terms the controller has,
            // on the measurement the signal holds now. The integral and the derivative do not
            // advance: they belong to the samples.
            let measurement = self.measurement.and_then(read_input).unwrap_or(0.0);
            let error = self.setpoint - measurement;
            let output = self.limited(self.c.kp * error + self.integral);
            if let Some(port) = self.output {
                set_output(port, output);
            }
        }
        Frame {
            setpoint: self.setpoint,
            accepted,
        }
        .report();
    }

    fn on_timer(&mut self, _timer: florn_guest::TimerId) {}
}

impl NetworkedController for NetworkedPid {
    fn on_link(&mut self, _port: PortId, _up: bool) {}

    fn on_sample(&mut self, _index: u64) {
        let c = &self.c;
        let measurement = self.measurement.and_then(read_input).unwrap_or(0.0);
        let error = self.setpoint - measurement;
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

florn_guest_cps::networked_controller!(NetworkedPid);
