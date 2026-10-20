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
        bench::Subject {
            name: "crossing",
            raw: &raw,
            metered: &metered,
            config: &config,
        },
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
                bench::Subject {
                    name: guest,
                    raw: &raw,
                    metered: &metered,
                    config: &config,
                },
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

/// One hash figure (RFC 005 rule 22): microseconds to digest a payload of the size, with each
/// function, as the trace does for every effect.
#[derive(Clone, Debug, PartialEq)]
pub struct Hash {
    pub bytes: usize,
    pub blake3_micros: f64,
    pub sha256_micros: f64,
}

/// The hash benchmark: BLAKE3 against SHA-256 at 64 bytes, 1,500 bytes and 64 KiB, best of
/// five rounds; `scale` divides the rounds' lengths.
pub fn hash(scale: u64) -> Vec<Hash> {
    use sha2::Digest;
    let mut out = Vec::new();
    for size in [64usize, 1_500, 65_536] {
        let payload: Vec<u8> = (0..size).map(|i| (i * 7) as u8).collect();
        let n = (match size {
            64 => 200_000,
            1_500 => 50_000,
            _ => 2_000,
        } / scale.max(1))
        .max(10);
        let mut sink = 0u8;
        let blake3_micros = bench::best_of(5, n, &clock, || {
            sink ^= blake3::hash(&payload).as_bytes()[0];
            Ok(())
        })
        .expect("hashes");
        let sha256_micros = bench::best_of(5, n, &clock, || {
            sink ^= sha2::Sha256::digest(&payload)[0];
            Ok(())
        })
        .expect("hashes");
        std::hint::black_box(sink);
        out.push(Hash {
            bytes: size,
            blake3_micros,
            sha256_micros,
        });
    }
    out
}

/// What a guest needs (RFC 003 rule 19): the smallest stack limit under which a reference
/// scenario runs without a fault, the largest memory an instance reached, and the most
/// cost-model units one activation consumed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Need {
    pub scenario: String,
    pub stack: u64,
    pub memory_bytes: u64,
    pub units: u64,
}

fn faults_under(s: &crate::scenario::Scenario) -> bool {
    match crate::host::try_start(s, florn_host::Options::default()) {
        Ok(mut run) => {
            run.run_to_end();
            run.log().codes().contains(&"actor.faulted")
        }
        Err(_) => true,
    }
}

/// The needs of the kit's guests, measured over the shipped scenarios that use them (without
/// the hostile ones, which fault by design).
pub fn needs() -> Vec<Need> {
    let mut out = Vec::new();
    for s in crate::shipped::all() {
        let name = s.scenario.name.clone();
        if name.starts_with("hostile-") || name == "zero-delay" || name.ends_with("-reversed") {
            continue;
        }
        let base = s
            .scenario
            .clone()
            .limit("budget", "1000000000")
            .limit("memory", "\"64MiB\"");
        // Stack: the smallest power-of-two-free bound by bisection over 1..=65,536.
        let (mut lo, mut hi) = (1u64, 65_536u64);
        if faults_under(&base.clone().limit("stack", hi.to_string())) {
            lo = hi;
        } else {
            while lo < hi {
                let mid = (lo + hi) / 2;
                if faults_under(&base.clone().limit("stack", mid.to_string())) {
                    lo = mid + 1;
                } else {
                    hi = mid;
                }
            }
        }
        // Memory reached and units consumed, from one run with records kept.
        let mut run = crate::host::try_start(&base, crate::host::options()).expect("starts");
        run.run_to_end();
        let mut memory_bytes = 0u64;
        for a in run.scenario().actors.clone() {
            if let Ok(state) = run.export_actor(a.id) {
                memory_bytes = memory_bytes.max(state.memory.len() as u64);
            }
        }
        let units = run
            .records()
            .unwrap_or(&[])
            .iter()
            .map(|r| match r {
                florn_kernel::Record::Handled { metered, .. } => *metered,
                _ => 0,
            })
            .max()
            .unwrap_or(0);
        out.push(Need {
            scenario: name,
            stack: lo,
            memory_bytes,
            units,
        });
    }
    out
}

/// The native capacity probe (RFC 003 rule 20): instances of the echo guest under one Wasmtime
/// store, up to `max`, and the time each took.
#[derive(Clone, Debug, PartialEq)]
pub struct NativeCapacity {
    pub instances: u64,
    pub micros_per_instance: f64,
    pub stopped_by: String,
}

pub fn capacity_native(max: u64) -> NativeCapacity {
    use florn_engine::Engine;
    let raw = guests::bytes("echo");
    let (metered, _) = admitted("echo", &raw);
    let spec = core_import_spec();
    let mut engine = WasmtimeEngine::new(Limits {
        instances: max as usize + 1,
        memories: max as usize + 1,
        tables: max as usize + 1,
        memory_bytes: 64 * 1024 * 1024,
    })
    .expect("engine");
    let module = engine.compile(&metered).expect("compile");
    let mut kept = Vec::new();
    let start = Instant::now();
    let mut stopped_by = "the probe's own limit".to_string();
    while (kept.len() as u64) < max {
        match engine.instantiate(&module, &spec) {
            Ok(i) => kept.push(i),
            Err(e) => {
                stopped_by = e.to_string();
                break;
            }
        }
    }
    let n = kept.len() as u64;
    NativeCapacity {
        instances: n,
        micros_per_instance: start.elapsed().as_secs_f64() * 1e6 / n.max(1) as f64,
        stopped_by,
    }
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
    pub hash: Vec<Hash>,
    pub needs: Vec<Need>,
    pub capacity: Option<NativeCapacity>,
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
        hash: hash(if quick { 20 } else { 1 }),
        needs: if quick { Vec::new() } else { needs() },
        capacity: if quick {
            None
        } else {
            Some(capacity_native(20_000))
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
        t.push_str("\n### Cost (RFC 003 section 7, step 2), microseconds per activation\n\n| Guest | Activations | Unmetered | Metered | Ratio | Units per activation |\n|---|---|---|---|---|---|\n");
        for c in &self.cost {
            t.push_str(&format!(
                "| {} | {} | {:.1} | {:.1} | {:.2}× | {:.0} |\n",
                c.guest,
                c.activations,
                c.unmetered_micros,
                c.metered_micros,
                c.ratio(),
                c.units_per_activation
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
        t.push_str("\n### Hash (RFC 005 rule 22), microseconds per digest\n\n| Payload | BLAKE3 | SHA-256 |\n|---|---|---|\n");
        for h in &self.hash {
            t.push_str(&format!(
                "| {} bytes | {:.2} | {:.2} |\n",
                h.bytes, h.blake3_micros, h.sha256_micros
            ));
        }
        if !self.needs.is_empty() {
            t.push_str("\n### What the guests need (RFC 003 rule 19)\n\n| Scenario | Stack | Memory reached | Most units in one activation |\n|---|---|---|---|\n");
            for n in &self.needs {
                t.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    n.scenario, n.stack, n.memory_bytes, n.units
                ));
            }
        }
        if let Some(c) = &self.capacity {
            t.push_str(&format!(
                "\n### Native capacity (RFC 003 rule 20)\n\n{} instances of the echo guest in one store at {:.1} µs each; stopped by {}.\n",
                c.instances, c.micros_per_instance, c.stopped_by
            ));
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
