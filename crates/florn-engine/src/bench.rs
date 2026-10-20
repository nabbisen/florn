//! The benchmark drivers of the conformance kit, generic over the adapter so that the native and
//! the browser Host run the same measurement (RFC 005 rule 11; RFC 003 §7 step 2): a guest
//! driven as the Host drives it — the budget global set before each call, the parameters lowered
//! through `florn-abi`, the imports lifted by a handler that keeps nothing. The clock is the
//! caller's (seconds as `f64`): the kit's is `std::time`, the worker's is `performance.now()`.

use florn_abi::core_world;
use florn_abi::{Flat, Memory, Ty, Val, lift_flat_params, lower_flat_params};

use crate::{Engine, GlobalValue, ImportHandler, ImportSpec, InstanceId, Refusal};

/// The budget set before each call of a metered guest.
pub const BUDGET: i64 = 1_000_000_000;

/// The Host-owned budget global as admission names it (RFC 003 §3; `fvm-wasm-instrument`'s import).
pub const BUDGET_MODULE: &str = "env";
pub const BUDGET_NAME: &str = "gas_counter";

/// A handler that lifts every call and keeps nothing: the Host's lifting cost without the
/// kernel's recording.
pub struct Lifting {
    sigs: Vec<(String, Vec<Ty>)>,
    pub calls: u64,
}

impl Default for Lifting {
    fn default() -> Self {
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

/// One instance of a guest of the core world under an adapter, metered or not.
pub struct Driver<E: Engine> {
    engine: E,
    inst: E::Instance,
    funcs: Vec<E::Func>,
    tys: Vec<Vec<Ty>>,
    metered: bool,
    pub handler: Lifting,
}

impl<E: Engine> Driver<E> {
    /// Compile and instantiate `bytes` (admitted bytes when `metered`) with the core world's
    /// imports; resolve the three exports once.
    pub fn new(
        mut engine: E,
        bytes: &[u8],
        metered: bool,
        spec: &ImportSpec,
    ) -> Result<Self, String> {
        let module = engine.compile(bytes).map_err(|e| e.to_string())?;
        let inst = engine
            .instantiate(&module, spec)
            .map_err(|e| e.to_string())?;
        let mut funcs = Vec::with_capacity(3);
        for name in [
            core_world::EXPORT_INIT,
            core_world::EXPORT_ON_MESSAGE,
            core_world::EXPORT_ON_TIMER,
        ] {
            funcs.push(
                engine
                    .func(&inst, name)
                    .ok_or_else(|| format!("export {name}"))?,
            );
        }
        let tys = core_world::actor_functions()
            .iter()
            .map(|f| f.param_types())
            .collect();
        Ok(Self {
            engine,
            inst,
            funcs,
            tys,
            metered,
            handler: Lifting::default(),
        })
    }

    /// Call export `which` (0 init, 1 on-message, 2 on-timer) with `vals`, as the Host does.
    pub fn call(&mut self, which: usize, vals: &[Val]) -> Result<(), String> {
        if self.metered {
            self.engine
                .global_set(
                    &mut self.inst,
                    Some(BUDGET_MODULE),
                    BUDGET_NAME,
                    GlobalValue::I64(BUDGET),
                )
                .map_err(|e| e.to_string())?;
        }
        let tys = &self.tys[which];
        let mut flats = Ok(Vec::new());
        self.engine
            .with_memory(&mut self.inst, &mut |m| {
                flats = lower_flat_params(m, tys, vals)
            })
            .map_err(|e| e.to_string())?;
        let flats = flats.map_err(|e| e.to_string())?;
        let func = self.funcs[which].clone();
        self.engine
            .call(&mut self.inst, &func, &flats, &mut self.handler)
            .map(|_| ())
            .map_err(|t| t.to_string())
    }

    /// `init` with one port per `ports` and the config bytes.
    pub fn init(&mut self, ports: u32, config: &[u8]) -> Result<(), String> {
        let ports = Val::List(
            (0..ports)
                .map(|p| Val::Record(vec![Val::U32(p), Val::U32(0), Val::str(&format!("p{p}"))]))
                .collect(),
        );
        self.call(0, &[Val::U64(0), Val::U32(1), ports, Val::bytes(config)])
    }

    pub fn message(&mut self, now: u64, port: u32, payload: &[u8]) -> Result<(), String> {
        self.call(1, &[Val::U64(now), Val::U32(port), Val::bytes(payload)])
    }

    pub fn timer(&mut self, now: u64, timer: u32) -> Result<(), String> {
        self.call(2, &[Val::U64(now), Val::U32(timer)])
    }

    pub fn into_engine(self) -> E {
        self.engine
    }
}

/// Microseconds per call: the best of `repeats` rounds of `n` calls, after a warm-up, with
/// `clock` in seconds.
pub fn best_of(
    repeats: usize,
    n: u64,
    clock: &dyn Fn() -> f64,
    mut call: impl FnMut() -> Result<(), String>,
) -> Result<f64, String> {
    for _ in 0..n.min(100) {
        call()?;
    }
    let mut best = f64::INFINITY;
    for _ in 0..repeats {
        let start = clock();
        for _ in 0..n {
            call()?;
        }
        let us = (clock() - start) * 1e6 / n as f64;
        best = best.min(us);
    }
    Ok(best)
}

/// One crossing figure (RFC 005 rule 11).
#[derive(Clone, Debug, PartialEq)]
pub struct Crossing {
    /// `0` is the call floor: a call that carries nothing and returns at once.
    pub bytes: usize,
    pub metered: bool,
    pub micros: f64,
}

/// The crossing sizes: the call floor, 64 bytes, the 1,500-byte point RFC 005 §11 compares the
/// kernel path with, and 64 KiB.
pub const CROSSING_SIZES: [usize; 4] = [0, 64, 1_500, 65_536];

/// The crossing benchmark over the kit's crossing guest: `raw` and its admitted form `metered`,
/// each driven through `make`'s adapter; `scale` divides the iteration counts (1 publishes).
pub fn crossing<E: Engine>(
    make: &mut dyn FnMut() -> E,
    raw: &[u8],
    metered: &[u8],
    config: &[u8],
    spec: &ImportSpec,
    scale: u64,
    clock: &dyn Fn() -> f64,
) -> Result<Vec<Crossing>, String> {
    let mut out = Vec::new();
    for is_metered in [false, true] {
        let bytes = if is_metered { metered } else { raw };
        let mut d = Driver::new(make(), bytes, is_metered, spec)?;
        d.init(1, config)?;
        for size in CROSSING_SIZES {
            let n = match size {
                0 | 64 => 20_000,
                1_500 => 10_000,
                _ => 1_000,
            } / scale.max(1);
            let micros = if size == 0 {
                best_of(5, n, clock, || d.timer(1, 0))?
            } else {
                let payload: Vec<u8> = (0..size).map(|i| i as u8).collect();
                best_of(5, n, clock, || d.message(1, 0, &payload))?
            };
            out.push(Crossing {
                bytes: size,
                metered: is_metered,
                micros,
            });
        }
        if d.handler.calls == 0 {
            return Err("the guest never crossed back".into());
        }
    }
    Ok(out)
}

/// One cost figure (RFC 003 §7 step 2): a guest's activation, metered against unmetered.
#[derive(Clone, Debug, PartialEq)]
pub struct Cost {
    pub guest: String,
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
    p
}

/// The cost benchmark for one guest: `activations` messages each, run-only time. `payload`
/// gives the message of each ordinal.
pub fn cost<E: Engine>(
    make: &mut dyn FnMut() -> E,
    guest: &str,
    raw: &[u8],
    metered: &[u8],
    config: &[u8],
    spec: &ImportSpec,
    activations: u64,
    payload: &dyn Fn(u64) -> Vec<u8>,
    clock: &dyn Fn() -> f64,
) -> Result<Cost, String> {
    let mut micros = [0.0; 2];
    for (i, is_metered) in [false, true].into_iter().enumerate() {
        let bytes = if is_metered { metered } else { raw };
        let mut d = Driver::new(make(), bytes, is_metered, spec)?;
        d.init(1, config)?;
        let mut ordinal = 0u64;
        micros[i] = best_of(1, activations, clock, || {
            let p = payload(ordinal);
            ordinal += 1;
            d.message(ordinal, 0, &p)
        })?;
    }
    Ok(Cost {
        guest: guest.to_string(),
        activations,
        unmetered_micros: micros[0],
        metered_micros: micros[1],
    })
}

/// The core world's imports as flat signatures, plus the budget global.
pub fn core_import_spec() -> ImportSpec {
    use crate::{HostGlobal, ImportFunction};
    let functions = core_world::host_functions()
        .into_iter()
        .map(|s| ImportFunction {
            name: s.name.to_string(),
            params: s.param_types().iter().flat_map(Ty::flat_types).collect(),
            results: s.result.as_ref().map_or(Vec::new(), Ty::flat_types),
        })
        .collect();
    ImportSpec {
        host_module: core_world::HOST_MODULE.to_string(),
        functions,
        globals: vec![HostGlobal {
            module: BUDGET_MODULE.into(),
            name: BUDGET_NAME.into(),
            initial: 0,
        }],
    }
}
