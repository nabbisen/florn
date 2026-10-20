//! The engine adapter trait (RFC 005 rule 5): the operations the Host needs from a WebAssembly
//! engine, and nothing else. Two implementations exist, for the browser and for Wasmtime; both
//! pass the cases of [`conformance`]. The Host lifts and lowers through `florn-abi` over the
//! memory an adapter exposes; an adapter never interprets a value.
//!
//! Implemented by PR-05 of `rfcs/handoffs/005-host-architecture/task-breakdown-pr-plan.md`.
#![forbid(unsafe_code)]

#[cfg(feature = "bench")]
pub mod bench;
#[cfg(feature = "conformance")]
pub mod conformance;

use std::fmt;

pub use florn_abi::{Flat, FlatType, Memory};

/// Identifies an instance to the import handler.
pub type InstanceId = u64;

/// What an instance imports: the host interface's functions with their flat signatures, and the
/// mutable `i64` globals the Host owns (RFC 003 rule 8: the budget counter).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportSpec {
    /// The import module name of the host interface, `florn:core/host@0.1.0` for the core world.
    pub host_module: String,
    pub functions: Vec<ImportFunction>,
    pub globals: Vec<HostGlobal>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportFunction {
    pub name: String,
    pub params: Vec<FlatType>,
    pub results: Vec<FlatType>,
}

/// A mutable `i64` global the Host supplies at instantiation and reads and writes afterwards.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostGlobal {
    pub module: String,
    pub name: String,
    pub initial: i64,
}

/// The Host's side of an import call. The handler lifts the arguments from `memory`, records the
/// effect or answers the query, and returns the flat results — or refuses, which traps the guest
/// (RFC 003 rule 17). The reason stays with the handler; the adapter only needs the trap.
pub trait ImportHandler {
    fn call(
        &mut self,
        instance: InstanceId,
        function: &str,
        args: &[Flat],
        memory: &mut dyn Memory,
    ) -> Result<Vec<Flat>, Refusal>;
}

/// The handler refused the call; the activation faults (the handler keeps why).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Refusal;

/// Why a call into the guest ended abnormally.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrapKind {
    /// The guest trapped: `unreachable`, an access out of bounds, a division by zero, an indirect
    /// call mismatch — or the instrumentation's trap for the budget or the stack, which the Host
    /// tells apart by reading the globals (RFC 003 rule 13).
    Guest,
    /// An import handler refused the call.
    HostRefused,
    /// The engine's own stack was exhausted: a Host fault, not reproducible (RFC 001 rule 65).
    EngineStack,
    /// The watchdog interrupted the guest: a Host fault (ACT-006).
    Watchdog,
    /// Anything else the engine reported.
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trap {
    pub kind: TrapKind,
    /// The engine's text, opaque, for the diagnostic log.
    pub message: String,
}

impl fmt::Display for Trap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineError {
    Compile(String),
    /// The engine refused an admitted module: `host.engine-refused` (RFC 003 rule 26).
    Instantiate(String),
    /// The engine could not hold another instance: a `RangeError`, an out-of-memory condition,
    /// a bare string from the browser; `host.instance-failed` (RFC 005 rule 10).
    Resource(String),
    /// A required export is missing or has the wrong type.
    Export(String),
    Memory(String),
    /// A Host-owned global is not known to this instance.
    Global(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::Compile(m) => write!(f, "compile: {m}"),
            EngineError::Instantiate(m) => write!(f, "instantiate: {m}"),
            EngineError::Resource(m) => write!(f, "instance failed: {m}"),
            EngineError::Export(m) => write!(f, "export: {m}"),
            EngineError::Memory(m) => write!(f, "memory: {m}"),
            EngineError::Global(m) => write!(f, "global: {m}"),
        }
    }
}

impl std::error::Error for EngineError {}

/// A global's value, by its core type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlobalValue {
    I32(i32),
    I64(i64),
    /// The bits of an `f32`.
    F32(u32),
    /// The bits of an `f64`.
    F64(u64),
}

/// What an engine is, for the run identity (RFC 001 rule 41).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineInfo {
    /// `wasmtime`, `browser`
    pub name: String,
    /// The engine's version as pinned, or the browser's user agent family and version.
    pub version: String,
}

/// The engine adapter (RFC 005 rule 5). Every operation that takes an address or a length checks it
/// against the memory's size and returns an error, never panics.
pub trait Engine {
    /// A compiled module, shared by every instance of it (ACT-007).
    type Module;
    type Instance;
    /// A typed, cached reference to an export, resolved once at instantiation (RFC 005 rule 7).
    type Func: Clone;

    /// The engine's name and version, for the run identity.
    fn info(&self) -> EngineInfo;

    /// Compile admitted bytes, or return the cached compilation of the same bytes.
    fn compile(&mut self, bytes: &[u8]) -> Result<Self::Module, EngineError>;
    /// How many distinct modules were compiled (to verify the cache: a restore never compiles).
    fn compiled_count(&self) -> usize;

    /// Instantiate with exactly the imports of `spec`; supplies the Host-owned globals.
    fn instantiate(
        &mut self,
        module: &Self::Module,
        spec: &ImportSpec,
    ) -> Result<Self::Instance, EngineError>;
    fn instance_id(&self, inst: &Self::Instance) -> InstanceId;
    fn drop_instance(&mut self, inst: Self::Instance);

    /// The reference to an exported function, or `None` if absent.
    fn func(&mut self, inst: &Self::Instance, name: &str) -> Option<Self::Func>;
    /// Call an export. Imports the guest calls go to `handler` for the duration of the call.
    fn call(
        &mut self,
        inst: &mut Self::Instance,
        func: &Self::Func,
        args: &[Flat],
        handler: &mut dyn ImportHandler,
    ) -> Result<Vec<Flat>, Trap>;

    /// The instance's memory, for lifting and lowering outside a call. The view is taken when
    /// `f` runs, after any call that may have grown the memory (RFC 005 rule 6).
    fn with_memory(
        &mut self,
        inst: &mut Self::Instance,
        f: &mut dyn FnMut(&mut dyn Memory),
    ) -> Result<(), EngineError>;
    fn memory_size(&mut self, inst: &mut Self::Instance) -> u32;
    /// Grow the memory to at least `bytes` (whole pages), for restore (RFC 003 rule 22).
    fn memory_grow_to(&mut self, inst: &mut Self::Instance, bytes: u32) -> Result<(), EngineError>;

    /// A global: a Host-owned one by `(Some(module), name)`, an exported one by `(None, name)`.
    fn global_get(
        &mut self,
        inst: &mut Self::Instance,
        module: Option<&str>,
        name: &str,
    ) -> Option<GlobalValue>;
    fn global_set(
        &mut self,
        inst: &mut Self::Instance,
        module: Option<&str>,
        name: &str,
        value: GlobalValue,
    ) -> Result<(), EngineError>;
    /// The size of an exported table, for restore's check (RFC 003 rule 22).
    fn table_size(&mut self, inst: &mut Self::Instance, name: &str) -> Option<u32>;
}
