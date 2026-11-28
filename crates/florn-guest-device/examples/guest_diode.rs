//! The ladder's second guest (RFC 012 rule 26): the diode of rule 23 as a guest device, with the
//! SDK's limiter. Its two pins are the anode (the lower port identity) and the cathode; it answers
//! the current into each pin and the Jacobian of the junction `is·(exp(v/vte) − 1) + gmin·v`,
//! continued linearly beyond `v/vte = 80`, at the junction voltage the limiter gives (RFC 013 rule
//! 8), and reports the voltage and the current of the junction **when a step is accepted**: a step
//! the solver rejects leaves what the guest carries, and what it reported, as it was.
//!
//! The arithmetic is the built-in diode's, in the same order and with the solver's own `exp`, `ln`
//! and `pnjlim` literals, so that the two agree to the last bit where they take the same
//! iterations, and within the profile's tolerance where they do not.
// The example guests are WebAssembly modules; on another target the file is empty.
#![cfg(target_arch = "wasm32")]
#![cfg_attr(target_arch = "wasm32", no_std)]

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use florn_guest::{Actor, ActorId, Port, PortId};
use florn_guest_device::{Device, Trial, VT, math, pnjlim, vcrit};

florn_guest::declare! {
    world: "florn:circuit/device@0.1.0",
    kind: "guest-diode",
    description: "A diode as a guest device model: the Shockley junction with the SDK's limiter",
    ports: [("pin", "circuit/pin", 2, 2)],
    params: [
        (is, "is", f64, false, default = 1e-14),
        (n, "n", f64, false, default = 1.0),
        (gmin, "gmin", f64, false, default = 1e-12),
    ],
    telemetry: [
        (1, "junction", Junction, [(v, "v", f64), (i, "i", f64)]),
    ],
    resources: [("memory", "1MiB")],
    tables: [("circuit", [("kind", "device")])],
}

/// Where `exp` stops: beyond `v/vte = 80` the characteristic continues with its slope there.
const KNEE: f64 = 80.0;

struct Diode {
    is: f64,
    gmin: f64,
    vte: f64,
    vcrit: f64,
    /// The junction voltage the last evaluation used (RFC 013 rule 8): committed at each accepted
    /// step, tried inside one.
    junction: Trial<f64>,
}

impl Diode {
    /// The current and the conductance at a junction voltage: `is·(exp(v/vte) − 1) + gmin·v` and
    /// its slope, continued linearly beyond the knee so that `exp` never overflows.
    fn characteristic(&self, v: f64) -> (f64, f64) {
        let x = v / self.vte;
        if x <= KNEE {
            let e = math::exp(x);
            (
                self.is * (e - 1.0) + self.gmin * v,
                self.is * e / self.vte + self.gmin,
            )
        } else {
            let e = math::exp(KNEE);
            let v_knee = KNEE * self.vte;
            let i = self.is * (e - 1.0) + self.gmin * v_knee;
            let g = self.is * e / self.vte + self.gmin;
            (i + g * (v - v_knee), g)
        }
    }
}

impl Actor for Diode {
    fn init(_: ActorId, _ports: &[Port], config: &[u8]) -> Self {
        let c = Config::decode(config);
        let vte = c.n * VT;
        Self {
            is: c.is,
            gmin: c.gmin,
            vte,
            vcrit: vcrit(vte, c.is),
            junction: Trial::new(0.0),
        }
    }

    // A device has no frames and no timers: the core world's two activations are stubs the Host
    // never calls (RFC 012 rule 19).
    fn on_message(&mut self, _port: PortId, _payload: &[u8]) {}

    fn on_timer(&mut self, _timer: florn_guest::TimerId) {}
}

impl Device for Diode {
    fn begin_step(&mut self, _start: u64, _h: f64) {
        self.junction.begin();
    }

    fn evaluate(&mut self, trial: &[f64]) -> Vec<f64> {
        let v = trial[0] - trial[1];
        let v_old = *self.junction.trial();
        let (v_lim, limited) = pnjlim(v, v_old, self.vte, self.vcrit);
        let (i, g) = self.characteristic(v_lim);
        // The current extended linearly from the limited voltage to the trial one.
        let i = i + g * (v - v_lim);
        *self.junction.trial() = v_lim;
        vec![i, -i, g, -g, -g, g, if limited { 1.0 } else { 0.0 }]
    }

    fn accept_step(&mut self, solution: &[f64]) {
        let v = solution[0] - solution[1];
        // What the next step starts from is the junction voltage of the accepted solution.
        *self.junction.trial() = v;
        self.junction.accept();
        let (i, _) = self.characteristic(v);
        Junction { v, i }.report();
    }

    fn reject_step(&mut self) {
        self.junction.reject();
    }
}

florn_guest_device::device!(Diode);
