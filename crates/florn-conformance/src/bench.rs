//! The benchmarks: the crossing benchmark of RFC 005 rule 11 (the call floor, a 64-byte and a
//! 64 KiB crossing, metered and unmetered), the cost benchmark of RFC 003 §7 step 2 (the
//! protocol-style and compute-style guests, 10,000 activations, run-only time, metered against
//! unmetered), and the kernel-path cost of RFC 005 §11 (one activation under the Host). The
//! figures are published in the notes of every minor release.
//!
//! The engine-level benchmarks drive the adapter as the Host does: the budget global set before
//! each call, the parameters lowered through `florn-abi`, the imports lifted by a handler.

use std::time::Instant;

use florn_admission::Admission;
use florn_engine::bench::{self, core_import_spec};
pub use florn_engine::bench::{Cost, Crossing, protocol_buffer};
use florn_engine_wasmtime::{Limits, WasmtimeEngine};

use crate::guests;

fn engine() -> WasmtimeEngine {
    WasmtimeEngine::new(Limits::default()).expect("engine")
}

/// Admit under the kit's limits: the admitted bytes and the config of the defaults.
fn admitted(name: &str, bytes: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let a = Admission::default()
        .admit(name, bytes, &crate::host::kit_limits())
        .unwrap_or_else(|e| panic!("{name}: {:?}", e.codes()));
    let config = florn_host::run::default_config(&a).expect("the defaults fill the config");
    (a.bytes.clone(), config)
}

fn clock() -> f64 {
    use std::sync::OnceLock;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

/// The crossing benchmark (RFC 005 rule 11) over the kit's crossing guest, under Wasmtime.
/// `scale` divides the iteration counts (1 publishes; 20 for a quick check).
pub fn crossing(scale: u64) -> Vec<Crossing> {
    let raw = guests::crossing();
    let (metered, config) = admitted("crossing", &raw);
    bench::crossing(
        &mut engine,
        &raw,
        &metered,
        &config,
        &core_import_spec(),
        scale,
        &clock,
    )
    .expect("the crossing benchmark runs")
}

/// The cost benchmark (RFC 003 §7 step 2): the two guests, `activations` each, run-only time.
pub fn cost(activations: u64) -> Vec<Cost> {
    let mut out = Vec::new();
    for guest in ["bench_protocol", "bench_compute"] {
        let raw = guests::bytes(guest);
        let (metered, config) = admitted(guest, &raw);
        let protocol = guest == "bench_protocol";
        let payload = move |ordinal: u64| {
            if protocol {
                protocol_buffer(ordinal)
            } else {
                ordinal.to_le_bytes().to_vec()
            }
        };
        out.push(
            bench::cost(
                &mut engine,
                guest,
                &raw,
                &metered,
                &config,
                &core_import_spec(),
                activations,
                &payload,
                &clock,
            )
            .expect("the cost benchmark runs"),
        );
    }
    out
}

/// The kernel path: a shipped benchmark scenario under the Host, microseconds per activation.
#[derive(Clone, Debug, PartialEq)]
pub struct HostPath {
    pub scenario: String,
    pub activations: u64,
    pub micros: f64,
}

pub fn host_path() -> Vec<HostPath> {
    crate::shipped::all()
        .into_iter()
        .filter(|s| s.scenario.name.starts_with("bench-"))
        .map(|s| {
            let mut run = crate::host::try_start(&s.scenario, florn_host::Options::default())
                .unwrap_or_else(|e| panic!("{}: {e}", s.scenario.name));
            let start = Instant::now();
            run.run_to_end();
            let elapsed = start.elapsed().as_secs_f64() * 1e6;
            HostPath {
                scenario: s.scenario.name,
                activations: run.kernel().handled(),
                micros: elapsed / run.kernel().handled() as f64,
            }
        })
        .collect()
}

/// The NaN probe's reports: (name, bits), from the probe guest under the Host (RFC 003 §9).
pub fn nan_probe() -> Vec<(String, u64)> {
    let s =
        crate::scenario::Scenario::new("probe", 10).actor(crate::scenario::actor(1, "probe", 0));
    let run = crate::host::run(&s);
    crate::host::reports(&run, 1, 1)
        .into_iter()
        .map(|(_, payload)| {
            let len = u32::from_le_bytes(payload[..4].try_into().unwrap()) as usize;
            let name = String::from_utf8_lossy(&payload[4..4 + len]).into_owned();
            let bits = u64::from_le_bytes(payload[4 + len..12 + len].try_into().unwrap());
            (name, bits)
        })
        .collect()
}

/// Every published figure.
#[derive(Clone, Debug)]
pub struct Figures {
    pub arch: String,
    pub engine: String,
    pub crossing: Vec<Crossing>,
    pub cost: Vec<Cost>,
    pub host_path: Vec<HostPath>,
    pub nan: Vec<(String, u64)>,
    /// This host's fresh build of the guests against the shipped modules.
    pub fresh: Option<crate::shipped::FreshReport>,
}

/// The Stage 0 baseline of RFC 005 rule 11 for the native engine, microseconds.
pub const BASELINE_NATIVE: [(usize, f64); 3] = [(0, 0.3), (64, 0.6), (65_536, 2.3)];

pub fn figures(quick: bool) -> Figures {
    let engine = {
        let info = florn_engine::Engine::info(&engine());
        format!("{} {}", info.name, info.version)
    };
    Figures {
        arch: std::env::consts::ARCH.to_string(),
        engine,
        crossing: crossing(if quick { 20 } else { 1 }),
        cost: cost(if quick { 200 } else { 10_000 }),
        host_path: if quick { Vec::new() } else { host_path() },
        nan: nan_probe(),
        fresh: if quick {
            None
        } else {
            crate::shipped::Digests::read(&crate::shipped::directory())
                .ok()
                .map(|d| crate::shipped::fresh_report(&d))
        },
    }
}

impl Figures {
    pub fn markdown(&self) -> String {
        let mut t = format!(
            "## Conformance kit figures: {} on {}\n\n### Crossing (RFC 005 rule 11), microseconds per call\n\n| Crossing | Unmetered | Metered | Stage 0 baseline |\n|---|---|---|---|\n",
            self.engine, self.arch
        );
        for size in bench::CROSSING_SIZES {
            let get = |m: bool| {
                self.crossing
                    .iter()
                    .find(|c| c.bytes == size && c.metered == m)
                    .map_or(f64::NAN, |c| c.micros)
            };
            let label = match size {
                0 => "call floor".to_string(),
                65_536 => "64 KiB".to_string(),
                n => format!("{n} bytes"),
            };
            let base = BASELINE_NATIVE
                .iter()
                .find(|(b, _)| *b == size)
                .map_or("—".to_string(), |(_, us)| format!("{us:.1}"));
            t.push_str(&format!(
                "| {label} | {:.2} | {:.2} | {base} |\n",
                get(false),
                get(true)
            ));
        }
        t.push_str("\n### Cost (RFC 003 section 7, step 2), microseconds per activation\n\n| Guest | Activations | Unmetered | Metered | Ratio |\n|---|---|---|---|---|\n");
        for c in &self.cost {
            t.push_str(&format!(
                "| {} | {} | {:.1} | {:.1} | {:.2}× |\n",
                c.guest,
                c.activations,
                c.unmetered_micros,
                c.metered_micros,
                c.ratio()
            ));
        }
        if !self.host_path.is_empty() {
            t.push_str("\n### Kernel path (RFC 005 section 11), microseconds per activation under the Host\n\n| Scenario | Activations | Per activation |\n|---|---|---|\n");
            for h in &self.host_path {
                t.push_str(&format!(
                    "| {} | {} | {:.1} |\n",
                    h.scenario, h.activations, h.micros
                ));
            }
        }
        t.push_str("\n### NaN probe (RFC 003 section 9)\n\n| Expression | Bits |\n|---|---|\n");
        for (name, bits) in &self.nan {
            t.push_str(&format!("| `{name}` | `{bits:#018x}` |\n"));
        }
        if let Some(f) = &self.fresh {
            t.push_str("\n### This host's build of the guests against the shipped modules\n\n| Guest | Same bytes |\n|---|---|\n");
            for (name, equal) in &f.guests {
                t.push_str(&format!(
                    "| {name} | {} |\n",
                    if *equal { "yes" } else { "no" }
                ));
            }
            t.push_str("\n| Scenario over the fresh build | Recorded chain |\n|---|---|\n");
            for (name, equal) in &f.scenarios {
                t.push_str(&format!(
                    "| {name} | {} |\n",
                    if *equal { "equal" } else { "differs" }
                ));
            }
        }
        t
    }
}
