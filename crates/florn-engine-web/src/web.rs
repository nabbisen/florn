//! The adapter on the guest target: the imports the worker provides, the exports it calls, and
//! the [`Engine`] implementation over them.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use florn_abi::{AbiError, Flat, Memory, Shape};
use florn_engine::{
    Engine, EngineError, EngineInfo, GlobalValue, ImportHandler, ImportSpec, InstanceId, Trap,
    TrapKind,
};

use crate::sig::{self, MAX_SLOTS};

/// The import bindings of the instance whose guest runs: (shape, slot) to the import's name,
/// for the trampolines beyond the core's named ones (RFC 008 rule 6).
type ImportMap = Rc<HashMap<(u32, u32), String>>;

/// The call statuses the worker returns from `engine_call`.
const OK: i32 = 0;
const TRAP: i32 = 1;
const RANGE: i32 = 2;

/// The failure kinds the worker reports after `engine_instantiate`.
const FAILED_RESOURCE: i32 = 1;

#[link(wasm_import_module = "florn")]
unsafe extern "C" {
    /// Compile the bytes at `ptr`; a module number, or -1 with the text in `engine_last_error`.
    fn engine_compile(ptr: *const u8, len: u32) -> i32;
    /// Instantiate with the imports of the named host interface and the shared budget global;
    /// an instance number, or -1 with the kind in `engine_last_error_kind` and the text.
    /// Instantiate with the import bindings of `sig::binding_text` (one line per import).
    fn engine_instantiate(module: i32, bindings: *const u8, bindings_len: u32) -> i32;
    fn engine_drop(inst: i32);
    /// Place the export in the Host's function table; its index, or -1.
    fn engine_func(inst: i32, name: *const u8, name_len: u32) -> i32;
    /// Call through [`florn_call`] inside a `try`; the status above, text in `engine_last_error`.
    fn engine_call(inst: i32, sig: i32, func: i32, args: *const u64, results: *mut u64) -> i32;
    fn engine_memory_size(inst: i32) -> u32;
    fn engine_memory_read(inst: i32, addr: u32, len: u32, dst: *mut u8) -> i32;
    fn engine_memory_write(inst: i32, addr: u32, src: *const u8, len: u32) -> i32;
    fn engine_memory_grow_to(inst: i32, bytes: u32) -> i32;
    /// An exported global: its type tag (1 i32, 2 i64, 3 f32, 4 f64) with the bits at `out`, or -1.
    /// An exported global of the type tag given (1 i32, 2 i64, 3 f32, 4 f64): its bits at
    /// `out`; 0, or -1 if absent.
    fn engine_global_get(inst: i32, name: *const u8, name_len: u32, tag: i32, out: *mut u64)
    -> i32;
    fn engine_global_set(
        inst: i32,
        name: *const u8,
        name_len: u32,
        tag: i32,
        bits: *const u64,
    ) -> i32;
    fn engine_table_size(inst: i32, name: *const u8, name_len: u32) -> i32;
    fn engine_last_error(dst: *mut u8, cap: u32) -> u32;
    fn engine_last_error_kind() -> i32;
    /// The browser's family and version, for the run identity.
    fn engine_info(dst: *mut u8, cap: u32) -> u32;
    /// The shared budget global, through the budget module (RFC 005 rule 8).
    fn budget_get() -> i64;
    fn budget_set(v: i64);
}

thread_local! {
    /// The handler of the call in progress. Invariant: `Some` only while the
    /// `&mut dyn ImportHandler` it was made from is borrowed by `call`, which is the only time a
    /// guest runs and therefore the only time an import export can dereference it.
    static HANDLER: Cell<Option<*mut (dyn ImportHandler + 'static)>> = const { Cell::new(None) };
    static CURRENT: Cell<(i32, InstanceId)> = const { Cell::new((-1, 0)) };
    static CURRENT_IMPORTS: RefCell<Option<ImportMap>> = const { RefCell::new(None) };
    static REFUSED: Cell<bool> = const { Cell::new(false) };
}

fn last_error() -> String {
    let mut buf = vec![0u8; 512];
    // SAFETY: the worker writes at most `cap` bytes into the buffer and returns the count.
    let n = unsafe { engine_last_error(buf.as_mut_ptr(), buf.len() as u32) } as usize;
    buf.truncate(n.min(512));
    String::from_utf8_lossy(&buf).into_owned()
}

/// The browser's family and version, as the worker reports them.
pub fn user_agent() -> String {
    let mut buf = vec![0u8; 128];
    // SAFETY: as `last_error`.
    let n = unsafe { engine_info(buf.as_mut_ptr(), buf.len() as u32) } as usize;
    buf.truncate(n.min(128));
    String::from_utf8_lossy(&buf).into_owned()
}

/// The trampoline the worker calls inside a `try`: a `call_indirect` into the guest through the
/// Host's function table, with the arguments read from the slots at `args`. One arm per shape of
/// `florn_abi::Shape`'s table (RFC 008 rule 6); admission keeps every world's function in it.
///
/// # Safety
/// `args` and `results` point into this module's memory at [`MAX_SLOTS`] slots each, written by
/// [`WebEngine::call`] just before; `func` is an index the worker placed an export of shape
/// `shape` at. A mismatch traps at the call, which the worker's `try` catches.
#[unsafe(no_mangle)]
// The argument counter of the macro is unused by the shape without parameters.
#[allow(unused_mut, unused_assignments, unused_variables)]
pub unsafe extern "C" fn florn_call(shape: i32, func: u32, args: *const u64, results: *mut u64) {
    let a = |i: usize| unsafe { *args.add(i) };
    let i32_at = |i: usize| a(i) as i32;
    let i64_at = |i: usize| a(i) as i64;
    macro_rules! call {
        (($($t:ident),*)) => {{
            let f: unsafe extern "C" fn($(call!(@ty $t)),*) = core::mem::transmute(func as usize);
            let mut i = 0usize;
            f($({ let v = call!(@at $t, i); i += 1; v }),*);
        }};
        (($($t:ident),*) -> i32) => {{
            let f: unsafe extern "C" fn($(call!(@ty $t)),*) -> i32 =
                core::mem::transmute(func as usize);
            let mut i = 0usize;
            let r = f($({ let v = call!(@at $t, i); i += 1; v }),*);
            *results = r as u32 as u64;
        }};
        (($($t:ident),*) -> i64) => {{
            let f: unsafe extern "C" fn($(call!(@ty $t)),*) -> i64 =
                core::mem::transmute(func as usize);
            let mut i = 0usize;
            let r = f($({ let v = call!(@at $t, i); i += 1; v }),*);
            *results = r as u64;
        }};
        (@ty i32) => { i32 };
        (@ty i64) => { i64 };
        (@at i32, $i:ident) => { i32_at($i) };
        (@at i64, $i:ident) => { i64_at($i) };
    }
    unsafe {
        match shape {
            1 => call!((i64, i32, i32, i32, i32, i32)),
            2 => call!((i64, i32, i32, i32)),
            3 => call!((i64, i32)),
            4 => call!((i32, i32, i32, i32) -> i32),
            5 => call!((i32, i32, i32)),
            6 => call!((i32, i64)),
            7 => call!((i32)),
            8 => call!(() -> i64),
            9 => call!((i64, i32, i32)),
            10 => call!((i32, i32)),
            11 => call!((i32) -> i32),
            _ => core::arch::wasm32::unreachable(),
        }
    }
}

/// A refusal by the handler: the activation faults (RFC 003 rule 17). The flag tells `call` that
/// the trap was the Host's; the trap unwinds the guest to the worker's `try`.
fn refuse() -> ! {
    REFUSED.with(|r| r.set(true));
    core::arch::wasm32::unreachable()
}

/// Dispatch a guest's import call to the handler of the call in progress.
fn dispatch(name: &str, args: &[Flat]) -> Vec<Flat> {
    let Some(ptr) = HANDLER.with(|h| h.get()) else {
        refuse()
    };
    // SAFETY: see `HANDLER`: the pointer was set by `call` from a live handler and is cleared
    // before `call` returns; this runs only while the guest runs, inside `call`.
    let handler: &mut dyn ImportHandler = unsafe { &mut *ptr };
    let (js, id) = CURRENT.with(|c| c.get());
    let mut mem = JsMemory {
        inst: js,
        realloc: None,
    };
    match handler.call(id, name, args, &mut mem) {
        Ok(v) => v,
        Err(_) => refuse(),
    }
}

/// The guest's imports: bound by the worker to these exports, so that a guest calls the Host
/// WebAssembly-to-WebAssembly (RFC 005 rule 7), 64-bit values included.
#[unsafe(no_mangle)]
pub extern "C" fn florn_import_send(port: i32, ptr: i32, len: i32) {
    dispatch("send", &[Flat::I32(port), Flat::I32(ptr), Flat::I32(len)]);
}

#[unsafe(no_mangle)]
pub extern "C" fn florn_import_set_timer(timer: i32, delay: i64) {
    dispatch("set-timer", &[Flat::I32(timer), Flat::I64(delay)]);
}

#[unsafe(no_mangle)]
pub extern "C" fn florn_import_cancel_timer(timer: i32) {
    dispatch("cancel-timer", &[Flat::I32(timer)]);
}

#[unsafe(no_mangle)]
pub extern "C" fn florn_import_random() -> i64 {
    match dispatch("random", &[]).first() {
        Some(Flat::I64(v)) => *v,
        _ => refuse(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn florn_import_report(channel: i32, ptr: i32, len: i32) {
    dispatch(
        "report",
        &[Flat::I32(channel), Flat::I32(ptr), Flat::I32(len)],
    );
}

/// Dispatch an import bound by shape and slot (RFC 008 rule 6): the name comes from the
/// bindings of the instance whose guest runs.
fn dispatch_slot(shape: u32, slot: u32, args: &[Flat]) -> Vec<Flat> {
    let name = CURRENT_IMPORTS.with(|m| {
        m.borrow()
            .as_ref()
            .and_then(|m| m.get(&(shape, slot)).cloned())
    });
    match name {
        Some(name) => dispatch(&name, args),
        None => refuse(),
    }
}

fn first_i32(out: Vec<Flat>) -> i32 {
    match out.first() {
        Some(Flat::I32(v)) => *v,
        _ => refuse(),
    }
}

fn first_i64(out: Vec<Flat>) -> i64 {
    match out.first() {
        Some(Flat::I64(v)) => *v,
        _ => refuse(),
    }
}

/// The trampolines beyond the core's named imports: `Shape::IMPORT_SLOTS` per shape of the
/// table, named `florn_import_s<shape>_<slot>`, bound by the worker from `sig::binding_text`.
macro_rules! import_slot {
    ($f:ident, $shape:literal, $slot:literal, ($($a:ident: $t:ident),*)) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $f($($a: $t),*) {
            dispatch_slot($shape, $slot, &[$(import_slot!(@flat $t, $a)),*]);
        }
    };
    ($f:ident, $shape:literal, $slot:literal, ($($a:ident: $t:ident),*) -> i32) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $f($($a: $t),*) -> i32 {
            first_i32(dispatch_slot($shape, $slot, &[$(import_slot!(@flat $t, $a)),*]))
        }
    };
    ($f:ident, $shape:literal, $slot:literal, ($($a:ident: $t:ident),*) -> i64) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $f($($a: $t),*) -> i64 {
            first_i64(dispatch_slot($shape, $slot, &[$(import_slot!(@flat $t, $a)),*]))
        }
    };
    (@flat i32, $a:ident) => { Flat::I32($a) };
    (@flat i64, $a:ident) => { Flat::I64($a) };
}

macro_rules! import_slots {
    ($shape:literal, [$f0:ident, $f1:ident, $f2:ident, $f3:ident], $($sig:tt)*) => {
        import_slot!($f0, $shape, 0, $($sig)*);
        import_slot!($f1, $shape, 1, $($sig)*);
        import_slot!($f2, $shape, 2, $($sig)*);
        import_slot!($f3, $shape, 3, $($sig)*);
    };
}

import_slots!(1, [florn_import_s1_0, florn_import_s1_1, florn_import_s1_2, florn_import_s1_3], (a: i64, b: i32, c: i32, d: i32, e: i32, f: i32));
import_slots!(2, [florn_import_s2_0, florn_import_s2_1, florn_import_s2_2, florn_import_s2_3], (a: i64, b: i32, c: i32, d: i32));
import_slots!(3, [florn_import_s3_0, florn_import_s3_1, florn_import_s3_2, florn_import_s3_3], (a: i64, b: i32));
import_slots!(4, [florn_import_s4_0, florn_import_s4_1, florn_import_s4_2, florn_import_s4_3], (a: i32, b: i32, c: i32, d: i32) -> i32);
import_slots!(5, [florn_import_s5_0, florn_import_s5_1, florn_import_s5_2, florn_import_s5_3], (a: i32, b: i32, c: i32));
import_slots!(6, [florn_import_s6_0, florn_import_s6_1, florn_import_s6_2, florn_import_s6_3], (a: i32, b: i64));
import_slots!(7, [florn_import_s7_0, florn_import_s7_1, florn_import_s7_2, florn_import_s7_3], (a: i32));
import_slots!(8, [florn_import_s8_0, florn_import_s8_1, florn_import_s8_2, florn_import_s8_3], () -> i64);
import_slots!(9, [florn_import_s9_0, florn_import_s9_1, florn_import_s9_2, florn_import_s9_3], (a: i64, b: i32, c: i32));
import_slots!(10, [florn_import_s10_0, florn_import_s10_1, florn_import_s10_2, florn_import_s10_3], (a: i32, b: i32));
import_slots!(11, [florn_import_s11_0, florn_import_s11_1, florn_import_s11_2, florn_import_s11_3], (a: i32) -> i32);

/// A guest's memory as the worker exposes it: one copy per read or write, no object per crossing.
struct JsMemory {
    inst: i32,
    realloc: Option<Func>,
}

impl Memory for JsMemory {
    fn size(&self) -> u32 {
        // SAFETY: a plain query of the worker.
        unsafe { engine_memory_size(self.inst) }
    }
    fn read(&self, addr: u32, len: u32) -> Result<Vec<u8>, AbiError> {
        let size = self.size();
        if addr.checked_add(len).is_none_or(|end| end > size) {
            return Err(AbiError::OutOfRange { addr, len, size });
        }
        let mut out = vec![0u8; len as usize];
        // SAFETY: `out` holds `len` bytes; the worker copies exactly that many after its own check.
        if unsafe { engine_memory_read(self.inst, addr, len, out.as_mut_ptr()) } != 0 {
            return Err(AbiError::OutOfRange { addr, len, size });
        }
        Ok(out)
    }
    fn write(&mut self, addr: u32, data: &[u8]) -> Result<(), AbiError> {
        let size = self.size();
        let len = data.len() as u32;
        if addr.checked_add(len).is_none_or(|end| end > size) {
            return Err(AbiError::OutOfRange { addr, len, size });
        }
        // SAFETY: `data` is a live slice of `len` bytes in this module's memory.
        if unsafe { engine_memory_write(self.inst, addr, data.as_ptr(), len) } != 0 {
            return Err(AbiError::OutOfRange { addr, len, size });
        }
        Ok(())
    }
    fn realloc(
        &mut self,
        old_ptr: u32,
        old_size: u32,
        align: u32,
        new_size: u32,
    ) -> Result<u32, AbiError> {
        let f = self.realloc.ok_or(AbiError::Realloc {
            requested: new_size,
        })?;
        let mut args = [0u64; MAX_SLOTS];
        let mut results = [0u64; MAX_SLOTS];
        sig::encode(
            Shape::REALLOC,
            &[
                Flat::I32(old_ptr as i32),
                Flat::I32(old_size as i32),
                Flat::I32(align as i32),
                Flat::I32(new_size as i32),
            ],
            &mut args,
        );
        // SAFETY: the slots live on this stack frame for the duration of the call; the worker
        // calls `florn_call` with these pointers.
        let status = unsafe {
            engine_call(
                self.inst,
                Shape::REALLOC.number() as i32,
                f.index as i32,
                args.as_ptr(),
                results.as_mut_ptr(),
            )
        };
        if status != OK {
            return Err(AbiError::Realloc {
                requested: new_size,
            });
        }
        let p = results[0] as u32;
        if p.checked_add(new_size).is_none_or(|end| end > self.size()) {
            return Err(AbiError::Realloc {
                requested: new_size,
            });
        }
        Ok(p)
    }
}

/// The types of a module's exported globals, by export name, as tags (1 i32, 2 i64, 3 f32,
/// 4 f64): the worker cannot tell a global's type from its value, so the adapter reads them from
/// the bytes once at compilation.
type GlobalTags = Rc<HashMap<String, i32>>;

fn global_tags(bytes: &[u8]) -> Result<GlobalTags, EngineError> {
    use wasmparser::{Parser, Payload, TypeRef, ValType};
    let mut types: Vec<ValType> = Vec::new();
    let mut exports: Vec<(String, u32)> = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|e| EngineError::Compile(e.to_string()))? {
            Payload::ImportSection(r) => {
                for i in r {
                    if let wasmparser::Imports::Single(_, imp) =
                        i.map_err(|e| EngineError::Compile(e.to_string()))?
                        && let TypeRef::Global(g) = imp.ty
                    {
                        types.push(g.content_type);
                    }
                }
            }
            Payload::GlobalSection(r) => {
                for g in r {
                    let g = g.map_err(|e| EngineError::Compile(e.to_string()))?;
                    types.push(g.ty.content_type);
                }
            }
            Payload::ExportSection(r) => {
                for e in r {
                    let e = e.map_err(|e| EngineError::Compile(e.to_string()))?;
                    if e.kind == wasmparser::ExternalKind::Global {
                        exports.push((e.name.to_string(), e.index));
                    }
                }
            }
            _ => {}
        }
    }
    let mut tags = HashMap::new();
    for (name, index) in exports {
        let tag = match types.get(index as usize) {
            Some(ValType::I32) => 1,
            Some(ValType::I64) => 2,
            Some(ValType::F32) => 3,
            Some(ValType::F64) => 4,
            _ => continue,
        };
        tags.insert(name, tag);
    }
    Ok(Rc::new(tags))
}

/// The adapter: compiled modules cached by their bytes (rule 10), instance numbers of the worker.
pub struct WebEngine {
    modules: HashMap<[u8; 32], (i32, GlobalTags)>,
    compiled: usize,
    next_id: InstanceId,
    info: EngineInfo,
}

impl Default for WebEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl WebEngine {
    pub fn new() -> Self {
        let ua = user_agent();
        let (name, version) = ua.split_once(' ').unwrap_or((ua.as_str(), ""));
        Self {
            modules: HashMap::new(),
            compiled: 0,
            next_id: 1,
            info: EngineInfo {
                name: if name.is_empty() {
                    "browser".into()
                } else {
                    name.to_string()
                },
                version: version.to_string(),
            },
        }
    }
}

pub struct Inst {
    js: i32,
    id: InstanceId,
    realloc: Option<Func>,
    tags: GlobalTags,
    imports: ImportMap,
}

/// A typed reference: an index of the Host's function table and the shape to call it with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Func {
    index: u32,
    shape: Shape,
}

fn with_name<R>(name: &str, f: impl FnOnce(*const u8, u32) -> R) -> R {
    f(name.as_ptr(), name.len() as u32)
}

impl Engine for WebEngine {
    type Module = (i32, GlobalTags);
    type Instance = Inst;
    type Func = Func;

    fn info(&self) -> EngineInfo {
        self.info.clone()
    }

    fn compile(&mut self, bytes: &[u8]) -> Result<(i32, GlobalTags), EngineError> {
        let key = *blake3::hash(bytes).as_bytes();
        if let Some(m) = self.modules.get(&key) {
            return Ok(m.clone());
        }
        let tags = global_tags(bytes)?;
        // SAFETY: `bytes` is a live slice; the worker copies it into a module synchronously.
        let m = unsafe { engine_compile(bytes.as_ptr(), bytes.len() as u32) };
        if m < 0 {
            return Err(EngineError::Compile(last_error()));
        }
        self.modules.insert(key, (m, tags.clone()));
        self.compiled += 1;
        Ok((m, tags))
    }

    fn compiled_count(&self) -> usize {
        self.compiled
    }

    fn instantiate(
        &mut self,
        module: &(i32, GlobalTags),
        spec: &ImportSpec,
    ) -> Result<Inst, EngineError> {
        // The bindings: admission keeps every import in the table (RFC 008 rule 6).
        let bindings = sig::bindings(spec).ok_or_else(|| {
            EngineError::Instantiate("an import outside the Host's shape table".into())
        })?;
        let text = sig::binding_text(&bindings);
        let imports: ImportMap = Rc::new(
            bindings
                .iter()
                .filter_map(|b| {
                    b.slot
                        .map(|slot| ((b.shape.number(), slot), b.name.clone()))
                })
                .collect(),
        );
        // SAFETY: the binding text is a live string.
        let js = unsafe { engine_instantiate(module.0, text.as_ptr(), text.len() as u32) };
        if js < 0 {
            let text = last_error();
            // SAFETY: a plain query.
            return Err(if unsafe { engine_last_error_kind() } == FAILED_RESOURCE {
                EngineError::Resource(text)
            } else {
                EngineError::Instantiate(text)
            });
        }
        let id = self.next_id;
        self.next_id += 1;
        let mut inst = Inst {
            js,
            id,
            realloc: None,
            tags: module.1.clone(),
            imports,
        };
        inst.realloc = self.func(&inst, florn_abi::core_world::EXPORT_REALLOC, Shape::REALLOC);
        Ok(inst)
    }

    fn instance_id(&self, inst: &Inst) -> InstanceId {
        inst.id
    }

    fn drop_instance(&mut self, inst: Inst) {
        // SAFETY: a plain call; the worker forgets the instance.
        unsafe { engine_drop(inst.js) }
    }

    fn func(&mut self, inst: &Inst, name: &str, shape: Shape) -> Option<Func> {
        // SAFETY: `name` is a live string.
        let index = with_name(name, |p, n| unsafe { engine_func(inst.js, p, n) });
        (index >= 0).then_some(Func {
            index: index as u32,
            shape,
        })
    }

    fn call(
        &mut self,
        inst: &mut Inst,
        func: &Func,
        args: &[Flat],
        handler: &mut dyn ImportHandler,
    ) -> Result<Vec<Flat>, Trap> {
        let mut slots = [0u64; MAX_SLOTS];
        let mut results = [0u64; MAX_SLOTS];
        if sig::encode(func.shape, args, &mut slots).is_none() {
            return Err(Trap {
                kind: TrapKind::Other,
                message: "arguments do not match the export's signature".into(),
            });
        }
        let ptr: *mut (dyn ImportHandler + '_) = handler;
        // SAFETY: the lifetime is erased only for the duration of `engine_call` below; the pointer
        // is cleared before `handler`'s borrow ends, and nothing else dereferences it (see `HANDLER`).
        let ptr: *mut (dyn ImportHandler + 'static) = unsafe { std::mem::transmute(ptr) };
        HANDLER.with(|h| h.set(Some(ptr)));
        CURRENT.with(|c| c.set((inst.js, inst.id)));
        CURRENT_IMPORTS.with(|m| *m.borrow_mut() = Some(inst.imports.clone()));
        REFUSED.with(|r| r.set(false));
        // SAFETY: the slots live on this frame; the worker calls `florn_call` with them.
        let status = unsafe {
            engine_call(
                inst.js,
                func.shape.number() as i32,
                func.index as i32,
                slots.as_ptr(),
                results.as_mut_ptr(),
            )
        };
        HANDLER.with(|h| h.set(None));
        if status == OK {
            return Ok(sig::decode(func.shape, &results));
        }
        let kind = if REFUSED.with(|r| r.get()) {
            TrapKind::HostRefused
        } else {
            match status {
                TRAP => TrapKind::Guest,
                RANGE => TrapKind::EngineStack,
                _ => TrapKind::Other,
            }
        };
        Err(Trap {
            kind,
            message: last_error(),
        })
    }

    fn with_memory(
        &mut self,
        inst: &mut Inst,
        f: &mut dyn FnMut(&mut dyn Memory),
    ) -> Result<(), EngineError> {
        let mut mem = JsMemory {
            inst: inst.js,
            realloc: inst.realloc,
        };
        f(&mut mem);
        Ok(())
    }

    fn memory_size(&mut self, inst: &mut Inst) -> u32 {
        // SAFETY: a plain query.
        unsafe { engine_memory_size(inst.js) }
    }

    fn memory_grow_to(&mut self, inst: &mut Inst, bytes: u32) -> Result<(), EngineError> {
        // SAFETY: a plain call.
        if unsafe { engine_memory_grow_to(inst.js, bytes) } != 0 {
            return Err(EngineError::Memory(last_error()));
        }
        Ok(())
    }

    fn global_get(
        &mut self,
        inst: &mut Inst,
        module: Option<&str>,
        name: &str,
    ) -> Option<GlobalValue> {
        if module.is_some() {
            // The budget, through the budget module: no JavaScript value in between.
            // SAFETY: a WebAssembly-to-WebAssembly call.
            return Some(GlobalValue::I64(unsafe { budget_get() }));
        }
        let tag = *inst.tags.get(name)?;
        let mut bits = 0u64;
        // SAFETY: `name` is live; `bits` receives the value.
        let r = with_name(name, |p, n| unsafe {
            engine_global_get(inst.js, p, n, tag, &mut bits)
        });
        if r != 0 {
            return None;
        }
        Some(match tag {
            1 => GlobalValue::I32(bits as i32),
            2 => GlobalValue::I64(bits as i64),
            3 => GlobalValue::F32(bits as u32),
            4 => GlobalValue::F64(bits),
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
        if module.is_some() {
            if let GlobalValue::I64(v) = value {
                // SAFETY: a WebAssembly-to-WebAssembly call.
                unsafe { budget_set(v) };
                return Ok(());
            }
            return Err(EngineError::Global(name.to_string()));
        }
        let (tag, bits) = match value {
            GlobalValue::I32(x) => (1, x as u32 as u64),
            GlobalValue::I64(x) => (2, x as u64),
            GlobalValue::F32(b) => (3, b as u64),
            GlobalValue::F64(b) => (4, b),
        };
        // SAFETY: `name` and `bits` are live.
        let r = with_name(name, |p, n| unsafe {
            engine_global_set(inst.js, p, n, tag, &bits)
        });
        if r != 0 {
            return Err(EngineError::Global(name.to_string()));
        }
        Ok(())
    }

    fn table_size(&mut self, inst: &mut Inst, name: &str) -> Option<u32> {
        // SAFETY: `name` is live.
        let n = with_name(name, |p, n| unsafe { engine_table_size(inst.js, p, n) });
        (n >= 0).then_some(n as u32)
    }
}
