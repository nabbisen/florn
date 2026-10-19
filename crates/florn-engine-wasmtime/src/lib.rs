//! The Wasmtime engine adapter (RFC 005 rule 9): deterministic settings in every build, compiled
//! modules cached by their bytes, exports resolved once into cached function references, memory
//! reached through views, the budget global as a Host-owned global, and the epoch as the Host's
//! watchdog against its own defects (ACT-006).
//!
//! `unsafe` appears in one place: the import handler the Host lends for the duration of a call is
//! stashed in the store as a raw pointer, because Wasmtime's import closures must be `'static`.
//! The invariant is stated at that use.

use std::collections::HashMap;

use florn_abi::{AbiError, Flat, FlatType, Memory};
use florn_engine::EngineInfo;
use florn_engine::{
    Engine, EngineError, GlobalValue, ImportHandler, ImportSpec, InstanceId, Trap, TrapKind,
};
use wasmtime::{
    Caller, Config, Extern, Func, FuncType, Global, GlobalType, Instance, Linker, Module,
    Mutability, Store, StoreLimits, StoreLimitsBuilder, Val, ValType,
};

/// The pinned Wasmtime version (RFC 005 rule 44); a test keeps it equal to `Cargo.lock`.
pub const WASMTIME_VERSION: &str = "48.0.6";

/// What the Host allows one store to hold (RFC 003 rule 20; RFC 005 rule 9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub instances: usize,
    pub memories: usize,
    pub tables: usize,
    /// The largest size any one memory may reach, in bytes.
    pub memory_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            instances: 10_000,
            memories: 10_000,
            tables: 10_000,
            memory_bytes: 1024 * 1024 * 1024,
        }
    }
}

/// The store's data: the Host's handler for the call in progress, and the limits.
struct Data {
    /// Set by `call` for its duration, cleared after. Invariant: `Some` only while the
    /// `&mut dyn ImportHandler` it was made from is borrowed by `call`, which is also the only time
    /// the guest can run and therefore the only time an import closure can dereference it. The
    /// `'static` is a lie told to the type system for that window only.
    handler: Option<*mut (dyn ImportHandler + 'static)>,
    limits: StoreLimits,
}

pub struct WasmtimeEngine {
    engine: wasmtime::Engine,
    store: Store<Data>,
    modules: HashMap<[u8; 32], Module>,
    compiled: usize,
    next_id: InstanceId,
}

pub struct Inst {
    id: InstanceId,
    instance: Instance,
    memory: wasmtime::Memory,
    realloc: Option<Func>,
    host_globals: HashMap<(String, String), Global>,
}

impl WasmtimeEngine {
    /// A deterministic engine (RFC 005 rule 9): no threads, no SIMD, no relaxed SIMD, no tail
    /// calls, no multiple memories, no 64-bit memory; no fuel (florn meters); no NaN
    /// canonicalisation (RFC 003 section 9); epoch interruption on, as the watchdog.
    pub fn new(limits: Limits) -> Result<Self, EngineError> {
        let mut cfg = Config::new();
        // Threads are absent at compile time (the `threads` feature is off), as is the GC proposal.
        cfg.wasm_simd(false);
        cfg.wasm_relaxed_simd(false);
        cfg.wasm_tail_call(false);
        cfg.wasm_multi_memory(false);
        cfg.wasm_memory64(false);
        cfg.wasm_bulk_memory(true);
        cfg.consume_fuel(false);
        cfg.cranelift_nan_canonicalization(false);
        cfg.epoch_interruption(true);
        let engine =
            wasmtime::Engine::new(&cfg).map_err(|e| EngineError::Compile(e.to_string()))?;
        let store_limits = StoreLimitsBuilder::new()
            .instances(limits.instances)
            .memories(limits.memories)
            .tables(limits.tables)
            .memory_size(limits.memory_bytes)
            .build();
        let mut store = Store::new(
            &engine,
            Data {
                handler: None,
                limits: store_limits,
            },
        );
        store.limiter(|d| &mut d.limits);
        // One increment of the epoch interrupts the guest (the watchdog's signal).
        store.set_epoch_deadline(1);
        Ok(Self {
            engine,
            store,
            modules: HashMap::new(),
            compiled: 0,
            next_id: 1,
        })
    }

    /// The watchdog's signal: interrupts a guest that is running. Safe to call from another thread
    /// through the handle of [`Self::watchdog`].
    pub fn watchdog(&self) -> Watchdog {
        Watchdog {
            engine: self.engine.clone(),
        }
    }
}

/// A handle that can interrupt the running guest from another thread (ACT-006).
#[derive(Clone)]
pub struct Watchdog {
    engine: wasmtime::Engine,
}

impl Watchdog {
    pub fn interrupt(&self) {
        self.engine.increment_epoch();
    }
}

fn val_type(t: FlatType) -> ValType {
    match t {
        FlatType::I32 => ValType::I32,
        FlatType::I64 => ValType::I64,
        FlatType::F32 => ValType::F32,
        FlatType::F64 => ValType::F64,
    }
}

fn to_flat(v: &Val) -> Flat {
    match v {
        Val::I32(x) => Flat::I32(*x),
        Val::I64(x) => Flat::I64(*x),
        Val::F32(b) => Flat::F32(f32::from_bits(*b)),
        Val::F64(b) => Flat::F64(f64::from_bits(*b)),
        _ => Flat::I32(0),
    }
}

fn to_val(f: &Flat) -> Val {
    match f {
        Flat::I32(x) => Val::I32(*x),
        Flat::I64(x) => Val::I64(*x),
        Flat::F32(x) => Val::F32(x.to_bits()),
        Flat::F64(x) => Val::F64(x.to_bits()),
    }
}

/// The error an import closure returns when the handler refuses; found again by downcast, through
/// whatever context the engine wraps around it.
#[derive(Debug)]
struct Refused;

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("refused by host")
    }
}

impl std::error::Error for Refused {}

fn classify(e: &wasmtime::Error) -> Trap {
    let message = format!("{e:#}");
    let kind = if e.downcast_ref::<Refused>().is_some() {
        TrapKind::HostRefused
    } else {
        match e.downcast_ref::<wasmtime::Trap>() {
            Some(wasmtime::Trap::StackOverflow) => TrapKind::EngineStack,
            Some(wasmtime::Trap::Interrupt) => TrapKind::Watchdog,
            Some(_) => TrapKind::Guest,
            None => TrapKind::Other,
        }
    };
    Trap { kind, message }
}

/// The instance's memory inside an import call: reads and writes through the caller's view; no
/// allocation, since the guest's allocator cannot be re-entered from an import.
struct CallerMemory<'a, 'b> {
    caller: &'a mut Caller<'b, Data>,
    memory: wasmtime::Memory,
}

impl Memory for CallerMemory<'_, '_> {
    fn size(&self) -> u32 {
        self.memory.data_size(&*self.caller) as u32
    }
    fn read(&self, addr: u32, len: u32) -> Result<Vec<u8>, AbiError> {
        let size = self.size();
        let end = addr
            .checked_add(len)
            .filter(|e| *e <= size)
            .ok_or(AbiError::OutOfRange { addr, len, size })?;
        Ok(self.memory.data(&*self.caller)[addr as usize..end as usize].to_vec())
    }
    fn write(&mut self, addr: u32, data: &[u8]) -> Result<(), AbiError> {
        let size = self.size();
        let len = data.len() as u32;
        let end = addr
            .checked_add(len)
            .filter(|e| *e <= size)
            .ok_or(AbiError::OutOfRange { addr, len, size })?;
        self.memory.data_mut(&mut *self.caller)[addr as usize..end as usize].copy_from_slice(data);
        Ok(())
    }
    fn realloc(&mut self, _: u32, _: u32, _: u32, new_size: u32) -> Result<u32, AbiError> {
        Err(AbiError::Realloc {
            requested: new_size,
        })
    }
}

/// The instance's memory outside a call: a view over the store, with the guest's allocator.
struct StoreMemory<'a> {
    store: &'a mut Store<Data>,
    memory: wasmtime::Memory,
    realloc: Option<Func>,
}

impl Memory for StoreMemory<'_> {
    fn size(&self) -> u32 {
        self.memory.data_size(&*self.store) as u32
    }
    fn read(&self, addr: u32, len: u32) -> Result<Vec<u8>, AbiError> {
        let size = self.size();
        let end = addr
            .checked_add(len)
            .filter(|e| *e <= size)
            .ok_or(AbiError::OutOfRange { addr, len, size })?;
        Ok(self.memory.data(&*self.store)[addr as usize..end as usize].to_vec())
    }
    fn write(&mut self, addr: u32, data: &[u8]) -> Result<(), AbiError> {
        let size = self.size();
        let len = data.len() as u32;
        let end = addr
            .checked_add(len)
            .filter(|e| *e <= size)
            .ok_or(AbiError::OutOfRange { addr, len, size })?;
        self.memory.data_mut(&mut *self.store)[addr as usize..end as usize].copy_from_slice(data);
        Ok(())
    }
    fn realloc(
        &mut self,
        old_ptr: u32,
        old_size: u32,
        align: u32,
        new_size: u32,
    ) -> Result<u32, AbiError> {
        let f = self.realloc.as_ref().ok_or(AbiError::Realloc {
            requested: new_size,
        })?;
        let mut out = [Val::I32(0)];
        f.call(
            &mut *self.store,
            &[
                Val::I32(old_ptr as i32),
                Val::I32(old_size as i32),
                Val::I32(align as i32),
                Val::I32(new_size as i32),
            ],
            &mut out,
        )
        .map_err(|_| AbiError::Realloc {
            requested: new_size,
        })?;
        match out[0] {
            Val::I32(p) => {
                let p = p as u32;
                if p.checked_add(new_size).is_none_or(|end| end > self.size()) {
                    return Err(AbiError::Realloc {
                        requested: new_size,
                    });
                }
                Ok(p)
            }
            _ => Err(AbiError::Realloc {
                requested: new_size,
            }),
        }
    }
}

impl Engine for WasmtimeEngine {
    type Module = Module;
    type Instance = Inst;
    type Func = Func;

    fn compile(&mut self, bytes: &[u8]) -> Result<Module, EngineError> {
        let key = *blake3::hash(bytes).as_bytes();
        if let Some(m) = self.modules.get(&key) {
            return Ok(m.clone());
        }
        let m =
            Module::new(&self.engine, bytes).map_err(|e| EngineError::Compile(e.to_string()))?;
        self.modules.insert(key, m.clone());
        self.compiled += 1;
        Ok(m)
    }

    fn info(&self) -> EngineInfo {
        EngineInfo {
            name: "wasmtime".into(),
            version: WASMTIME_VERSION.into(),
        }
    }

    fn compiled_count(&self) -> usize {
        self.compiled
    }

    fn instantiate(&mut self, module: &Module, spec: &ImportSpec) -> Result<Inst, EngineError> {
        let id = self.next_id;
        self.next_id += 1;
        let mut linker: Linker<Data> = Linker::new(&self.engine);
        for f in &spec.functions {
            let ty = FuncType::new(
                &self.engine,
                f.params.iter().copied().map(val_type),
                f.results.iter().copied().map(val_type),
            );
            let name = f.name.clone();
            linker
                .func_new(
                    &spec.host_module,
                    &f.name,
                    ty,
                    move |mut caller: Caller<'_, Data>,
                          params: &[Val],
                          results: &mut [Val]|
                          -> wasmtime::Result<()> {
                        let flats: Vec<Flat> = params.iter().map(to_flat).collect();
                        let memory = match caller.get_export("memory") {
                            Some(Extern::Memory(m)) => m,
                            _ => return Err(wasmtime::Error::msg("no memory export")),
                        };
                        let ptr = caller.data().handler;
                        let Some(ptr) = ptr else {
                            return Err(Refused.into());
                        };
                        // SAFETY: `ptr` was set by `call` from a live `&mut dyn ImportHandler` and is
                        // cleared before `call` returns; this closure only runs while the guest runs,
                        // which is only inside `call`. No other reference to the handler exists then.
                        let handler: &mut dyn ImportHandler = unsafe { &mut *ptr };
                        let mut mem = CallerMemory {
                            caller: &mut caller,
                            memory,
                        };
                        match handler.call(id, &name, &flats, &mut mem) {
                            Ok(out) => {
                                for (slot, v) in results.iter_mut().zip(out.iter()) {
                                    *slot = to_val(v);
                                }
                                Ok(())
                            }
                            Err(_) => Err(Refused.into()),
                        }
                    },
                )
                .map_err(|e| EngineError::Instantiate(e.to_string()))?;
        }
        let mut host_globals = HashMap::new();
        for g in &spec.globals {
            let global = Global::new(
                &mut self.store,
                GlobalType::new(ValType::I64, Mutability::Var),
                Val::I64(g.initial),
            )
            .map_err(|e| EngineError::Instantiate(e.to_string()))?;
            linker
                .define(&self.store, &g.module, &g.name, global)
                .map_err(|e| EngineError::Instantiate(e.to_string()))?;
            host_globals.insert((g.module.clone(), g.name.clone()), global);
        }
        let instance = linker
            .instantiate(&mut self.store, module)
            .map_err(|e| EngineError::Instantiate(e.to_string()))?;
        let memory = instance
            .get_memory(&mut self.store, "memory")
            .ok_or_else(|| EngineError::Export("memory".into()))?;
        let realloc = instance.get_func(&mut self.store, "cabi_realloc");
        Ok(Inst {
            id,
            instance,
            memory,
            realloc,
            host_globals,
        })
    }

    fn instance_id(&self, inst: &Inst) -> InstanceId {
        inst.id
    }

    fn drop_instance(&mut self, inst: Inst) {
        drop(inst);
    }

    fn func(&mut self, inst: &Inst, name: &str) -> Option<Func> {
        inst.instance.get_func(&mut self.store, name)
    }

    fn call(
        &mut self,
        _inst: &mut Inst,
        func: &Func,
        args: &[Flat],
        handler: &mut dyn ImportHandler,
    ) -> Result<Vec<Flat>, Trap> {
        let params: Vec<Val> = args.iter().map(to_val).collect();
        let n = func.ty(&self.store).results().len();
        let mut results = vec![Val::I32(0); n];
        let ptr: *mut (dyn ImportHandler + '_) = handler;
        // SAFETY: the lifetime is erased only for the duration of `func.call` below; the pointer is
        // cleared before `handler`'s borrow ends, and nothing else dereferences it (see `Data`).
        let ptr: *mut (dyn ImportHandler + 'static) = unsafe { std::mem::transmute(ptr) };
        self.store.data_mut().handler = Some(ptr);
        let r = func.call(&mut self.store, &params, &mut results);
        self.store.data_mut().handler = None;
        r.map_err(|e| classify(&e))?;
        Ok(results.iter().map(to_flat).collect())
    }

    fn with_memory(
        &mut self,
        inst: &mut Inst,
        f: &mut dyn FnMut(&mut dyn Memory),
    ) -> Result<(), EngineError> {
        let mut mem = StoreMemory {
            store: &mut self.store,
            memory: inst.memory,
            realloc: inst.realloc,
        };
        f(&mut mem);
        Ok(())
    }

    fn memory_size(&mut self, inst: &mut Inst) -> u32 {
        inst.memory.data_size(&self.store) as u32
    }

    fn memory_grow_to(&mut self, inst: &mut Inst, bytes: u32) -> Result<(), EngineError> {
        let have = inst.memory.data_size(&self.store) as u64;
        let want = bytes as u64;
        if want > have {
            inst.memory
                .grow(&mut self.store, (want - have).div_ceil(65_536))
                .map_err(|e| EngineError::Memory(e.to_string()))?;
        }
        Ok(())
    }

    fn global_get(
        &mut self,
        inst: &mut Inst,
        module: Option<&str>,
        name: &str,
    ) -> Option<GlobalValue> {
        let g = match module {
            Some(m) => *inst.host_globals.get(&(m.to_string(), name.to_string()))?,
            None => inst.instance.get_global(&mut self.store, name)?,
        };
        Some(match g.get(&mut self.store) {
            Val::I32(x) => GlobalValue::I32(x),
            Val::I64(x) => GlobalValue::I64(x),
            Val::F32(b) => GlobalValue::F32(b),
            Val::F64(b) => GlobalValue::F64(b),
            _ => return None,
        })
    }

    fn global_set(
        &mut self,
        inst: &mut Inst,
        module: Option<&str>,
        name: &str,
        value: GlobalValue,
    ) -> Result<(), EngineError> {
        let g = match module {
            Some(m) => *inst
                .host_globals
                .get(&(m.to_string(), name.to_string()))
                .ok_or_else(|| EngineError::Global(format!("{m}.{name}")))?,
            None => inst
                .instance
                .get_global(&mut self.store, name)
                .ok_or_else(|| EngineError::Global(name.to_string()))?,
        };
        let v = match value {
            GlobalValue::I32(x) => Val::I32(x),
            GlobalValue::I64(x) => Val::I64(x),
            GlobalValue::F32(b) => Val::F32(b),
            GlobalValue::F64(b) => Val::F64(b),
        };
        g.set(&mut self.store, v)
            .map_err(|e| EngineError::Global(e.to_string()))
    }

    fn table_size(&mut self, inst: &mut Inst, name: &str) -> Option<u32> {
        let t = inst.instance.get_table(&mut self.store, name)?;
        Some(t.size(&self.store) as u32)
    }
}
