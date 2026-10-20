//! The benchmarks: the crossing benchmark of RFC 005 rule 11 (the call floor, a 64-byte and a
//! 64 KiB crossing, metered and unmetered), the cost benchmark of RFC 003 §7 step 2 (the
//! protocol-style and compute-style guests, 10,000 activations, run-only time, metered against
//! unmetered), and the kernel-path cost of RFC 005 §11 (one activation under the Host). The
//! figures are published in the notes of every minor release.
//!
//! The engine-level benchmarks drive the adapter as the Host does: the budget global set before
//! each call, the parameters lowered through `florn-abi`, the imports lifted by a handler.

use std::collections::BTreeMap;
use std::time::Instant;

use florn_abi::core_world;
use florn_abi::{Flat, Memory, Ty, Val, lift_flat_params, lower_flat_params};
use florn_admission::config::{NoUnits, encode_config};
use florn_admission::{Admission, ClassLimits};
use florn_engine::conformance::core_import_spec;
use florn_engine::{Engine, GlobalValue, ImportHandler, ImportSpec, InstanceId, Refusal};
use florn_engine_wasmtime::{Limits, WasmtimeEngine};
use florn_model::TickLength;

use crate::guests;

const BUDGET: i64 = 1_000_000_000;

/// A handler that lifts every call and keeps nothing: the Host's lifting cost without the
/// kernel's recording.
struct Lifting {
    sigs: Vec<(String, Vec<Ty>)>,
    calls: u64,
}

impl Lifting {
    fn new() -> Self {
        Self {
            sigs: core_world::host_functions()
                .into_iter()
                .map(|s| (s.name.to_string(), s.param_types()))
                .collect(),
            calls: 0,
        }
    }
}

impl ImportHandler for Lifting {
    fn call(
        &mut self,
        _: InstanceId,
        function: &str,
        args: &[Flat],
        memory: &mut dyn Memory,
    ) -> Result<Vec<Flat>, Refusal> {
        let (_, tys) = self
            .sigs
            .iter()
            .find(|(n, _)| n == function)
            .ok_or(Refusal)?;
        let _vals = lift_flat_params(memory, tys, args).map_err(|_| Refusal)?;
        self.calls += 1;
        Ok(if function == "random" {
            vec![Flat::I64(0)]
        } else {
            Vec::new()
        })
    }
}

/// One instance of a guest under the adapter, metered or not.
struct Driver {
    engine: WasmtimeEngine,
    inst: <WasmtimeEngine as Engine>::Instance,
    funcs: Vec<<WasmtimeEngine as Engine>::Func>,
    tys: Vec<Vec<Ty>>,
    metered: bool,
    handler: Lifting,
}

impl Driver {
    fn new(bytes: &[u8], metered: bool, spec: &ImportSpec) -> Self {
        let mut engine = WasmtimeEngine::new(Limits::default()).expect("engine");
        let module = engine.compile(bytes).expect("compile");
        let inst = engine.instantiate(&module, spec).expect("instantiate");
        let funcs = [
            core_world::EXPORT_INIT,
            core_world::EXPORT_ON_MESSAGE,
            core_world::EXPORT_ON_TIMER,
        ]
        .iter()
        .map(|name| {
            engine
                .func(&inst, name)
                .unwrap_or_else(|| panic!("export {name}"))
        })
        .collect();
        let tys = core_world::actor_functions()
            .iter()
            .map(|f| f.param_types())
            .collect();
        Self {
            engine,
            inst,
            funcs,
            tys,
            metered,
            handler: Lifting::new(),
        }
    }

    fn call(&mut self, which: usize, vals: &[Val]) {
        if self.metered {
            self.engine
                .global_set(
                    &mut self.inst,
                    Some(florn_admission::BUDGET_IMPORT_MODULE),
                    florn_admission::BUDGET_IMPORT_NAME,
                    GlobalValue::I64(BUDGET),
                )
                .expect("budget");
        }
        let tys = &self.tys[which];
        let mut flats = Ok(Vec::new());
        self.engine
            .with_memory(&mut self.inst, &mut |m| {
                flats = lower_flat_params(m, tys, vals)
            })
            .expect("memory");
        let flats = flats.expect("lowering");
        let func = self.funcs[which];
        self.engine
            .call(&mut self.inst, &func, &flats, &mut self.handler)
            .unwrap_or_else(|t| panic!("the benchmark guest trapped: {t}"));
    }
}

fn admitted(name: &str, bytes: &[u8]) -> (Vec<u8>, florn_admission::ModuleManifest) {
    let a = Admission::default()
        .admit(name, bytes, &ClassLimits::defaults())
        .unwrap_or_else(|e| panic!("{name}: {:?}", e.codes()));
    (a.bytes.clone(), a.manifest.clone())
}

fn ports(n: u32) -> Val {
    Val::List(
        (0..n)
            .map(|p| Val::Record(vec![Val::U32(p), Val::U32(0), Val::str(&format!("p{p}"))]))
            .collect(),
    )
}

fn init_vals(manifest: &florn_admission::ModuleManifest, ports_n: u32) -> Vec<Val> {
    let config = encode_config(1, manifest, &BTreeMap::new(), TickLength::Ns, &NoUnits)
        .expect("the defaults fill the config");
    vec![
        Val::U64(0),
        Val::U32(1),
        ports(ports_n),
        Val::bytes(&config),
    ]
}

/// Microseconds per call: the best of `repeats` rounds of `n` calls, after a warm-up.
fn best_of(repeats: usize, n: u64, mut call: impl FnMut()) -> f64 {
    for _ in 0..n.min(100) {
        call();
    }
    let mut best = f64::INFINITY;
    for _ in 0..repeats {
        let start = Instant::now();
        for _ in 0..n {
            call();
        }
        let us = start.elapsed().as_secs_f64() * 1e6 / n as f64;
        best = best.min(us);
    }
    best
}

/// One crossing figure.
#[derive(Clone, Debug, PartialEq)]
pub struct Crossing {
    /// `0` is the call floor: a call that carries nothing and returns at once.
    pub bytes: usize,
    pub metered: bool,
    pub micros: f64,
}

/// The crossing benchmark (RFC 005 rule 11), plus the 1,500-byte point RFC 005 §11 compares the
/// kernel path with. `scale` divides the iteration counts (1 publishes; 20 for a quick check).
pub fn crossing(scale: u64) -> Vec<Crossing> {
    let spec = core_import_spec();
    let raw = guests::crossing();
    let (metered_bytes, manifest) = admitted("crossing", &raw);
    let mut out = Vec::new();
    for metered in [false, true] {
        let bytes = if metered { &metered_bytes } else { &raw };
        let mut d = Driver::new(bytes, metered, &spec);
        d.call(0, &init_vals(&manifest, 1));
        for size in [0usize, 64, 1_500, 65_536] {
            let n = match size {
                0 | 64 => 20_000,
                1_500 => 10_000,
                _ => 1_000,
            } / scale;
            let micros = if size == 0 {
                let vals = [Val::U64(1), Val::U32(0)];
                best_of(5, n, || d.call(2, &vals))
            } else {
                let payload: Vec<u8> = (0..size).map(|i| i as u8).collect();
                let vals = [Val::U64(1), Val::U32(0), Val::bytes(&payload)];
                best_of(5, n, || d.call(1, &vals))
            };
            out.push(Crossing {
                bytes: size,
                metered,
                micros,
            });
        }
        assert!(d.handler.calls > 0, "the guest crossed back");
    }
    out
}

/// One cost figure: a guest's activation, metered against unmetered.
#[derive(Clone, Debug, PartialEq)]
pub struct Cost {
    pub guest: &'static str,
    pub activations: u64,
    pub unmetered_micros: f64,
    pub metered_micros: f64,
}

impl Cost {
    pub fn ratio(&self) -> f64 {
        self.metered_micros / self.unmetered_micros
    }
}

/// The 1,500-byte record buffer the protocol guest parses: 124 records of a key and a value.
pub fn protocol_buffer(ordinal: u64) -> Vec<u8> {
    let mut p = Vec::with_capacity(1_500);
    p.extend_from_slice(&124u32.to_le_bytes());
    for i in 0..124u64 {
        let key = ((ordinal.wrapping_mul(131) + i * 7) % 1_000) as u32;
        p.extend_from_slice(&key.to_le_bytes());
        p.extend_from_slice(&(ordinal + i).to_le_bytes());
    }
    p.extend_from_slice(&0u64.to_le_bytes());
    assert_eq!(p.len(), 1_500);
    p
}

/// The cost benchmark (RFC 003 §7 step 2): the two guests, `activations` each, run-only time.
pub fn cost(activations: u64) -> Vec<Cost> {
    let spec = core_import_spec();
    let mut out = Vec::new();
    for (guest, name) in [("bench_protocol", "B1"), ("bench_compute", "B2")] {
        let raw = guests::bytes(guest);
        let (metered_bytes, manifest) = admitted(guest, &raw);
        let mut micros = [0.0; 2];
        for (i, metered) in [false, true].into_iter().enumerate() {
            let bytes = if metered { &metered_bytes } else { &raw };
            let mut d = Driver::new(bytes, metered, &spec);
            d.call(0, &init_vals(&manifest, 1));
            let mut ordinal = 0u64;
            micros[i] = best_of(1, activations, || {
                let payload = if guest == "bench_protocol" {
                    protocol_buffer(ordinal)
                } else {
                    ordinal.to_le_bytes().to_vec()
                };
                ordinal += 1;
                d.call(1, &[Val::U64(ordinal), Val::U32(0), Val::bytes(&payload)]);
            });
        }
        let _ = name;
        out.push(Cost {
            guest,
            activations,
            unmetered_micros: micros[0],
            metered_micros: micros[1],
        });
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
}

/// The Stage 0 baseline of RFC 005 rule 11 for the native engine, microseconds.
pub const BASELINE_NATIVE: [(usize, f64); 3] = [(0, 0.3), (64, 0.6), (65_536, 2.3)];

pub fn figures(quick: bool) -> Figures {
    let engine = {
        let e = WasmtimeEngine::new(Limits::default()).expect("engine");
        let info = e.info();
        format!("{} {}", info.name, info.version)
    };
    Figures {
        arch: std::env::consts::ARCH.to_string(),
        engine,
        crossing: crossing(if quick { 20 } else { 1 }),
        cost: cost(if quick { 200 } else { 10_000 }),
        host_path: if quick { Vec::new() } else { host_path() },
        nan: nan_probe(),
    }
}

impl Figures {
    pub fn markdown(&self) -> String {
        let mut t = format!(
            "## Conformance kit figures: {} on {}\n\n### Crossing (RFC 005 rule 11), microseconds per call\n\n| Crossing | Unmetered | Metered | Stage 0 baseline |\n|---|---|---|---|\n",
            self.engine, self.arch
        );
        for size in [0usize, 64, 1_500, 65_536] {
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
        t
    }
}
