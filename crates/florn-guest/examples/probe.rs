//! The NaN probe (RFC 003 §9): computes NaN bit patterns inside the guest and reports them, so
//! that engines and architectures can be compared. Fires once, at its first timer.
// The example guests are WebAssembly modules; on another target the file is empty.
#![cfg(target_arch = "wasm32")]
#![cfg_attr(target_arch = "wasm32", no_std)]

use florn_guest::{Actor, ActorId, Port, PortId, TimerId, host};

florn_guest::declare! {
    world: "florn:core/guest@0.1.0",
    kind: "probe",
    description: "Reports NaN bit patterns produced inside the guest",
    ports: [],
    params: [],
    telemetry: [(1, "nan", Nan, [(name, "name", text), (bits, "bits", u64)])],
    resources: [("memory", "128KiB"), ("pending-timers", 1)],
}

struct Probe;

#[inline(never)]
fn black(x: f64) -> f64 {
    core::hint::black_box(x)
}

impl Actor for Probe {
    fn init(_: ActorId, _: &[Port], _: &[u8]) -> Self {
        host::set_timer(0, 1);
        Probe
    }
    fn on_message(&mut self, _: PortId, _: &[u8]) {}
    fn on_timer(&mut self, _: TimerId) {
        let probes: [(&str, f64); 5] = [
            ("0/0", black(0.0) / black(0.0)),
            ("inf-inf", black(f64::INFINITY) - black(f64::INFINITY)),
            ("sqrt(-1)", libm::sqrt(black(-1.0))),
            ("nan+1", f64::NAN + black(1.0)),
            ("-nan*1", -f64::NAN * black(1.0)),
        ];
        for (name, v) in probes {
            Nan {
                name: name.into(),
                bits: v.to_bits(),
            }
            .report();
        }
    }
}

florn_guest::actor!(Probe);
