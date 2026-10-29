//! The admission step (RFC 003; RFC 002 §3): validation, both custom sections, the estimate, the
//! transformation with the versioned cost model, the state layout, the admission cache, and the
//! config encoder. One implementation for both Hosts (rule 1); nothing here touches an engine.
//!
//! Implemented by PR-06 of `rfcs/handoffs/005-host-architecture/task-breakdown-pr-plan.md`.
#![forbid(unsafe_code)]

pub mod config;
pub mod error;
pub mod estimate;
pub mod manifest;
pub mod meter;
mod transform;
pub mod validate;
pub mod world;

use std::collections::HashMap;
use std::sync::Arc;

use sha2::{Digest, Sha256};

pub use error::{Error, Errors, Kind, Param};
pub use estimate::{Capacity, ClassLimits, Estimate, ModuleFacts};
pub use manifest::{ModuleManifest, ParamType};
pub use transform::{GlobalKind, StateLayout};
pub use validate::ModuleMaxima;
pub use world::WorldSpec;

/// The admission version (RFC 003 rule 10): the passes of rule 7 in their order with
/// `fvm-wasm-instrument` 0.4.
pub const ADMISSION_VERSION: u32 = 1;
/// The cost-model version new runs default to (RFC 003 rules 14 and 15; RFC 010 rule 9): the
/// Host keeps every published version, and a run names its own.
pub const COST_MODEL_VERSION: u32 = 1;
/// The published cost-model versions.
pub const COST_MODEL_VERSIONS: [u32; 2] = [1, 2];
pub use meter::COST_MODEL_V2_TABLE;

/// The table of a cost-model version (rule 14), or `None` for a version this Host lacks.
pub fn cost_table(version: u32) -> Option<&'static str> {
    match version {
        1 => Some(COST_MODEL_V1_TABLE),
        2 => Some(COST_MODEL_V2_TABLE),
        _ => None,
    }
}
/// The cost model's table, recorded with the version in the run record (rule 14).
pub const COST_MODEL_V1_TABLE: &str = "\
cost-model = 1
library = \"fvm-wasm-instrument 0.4.0\"
instruction = 1          # units per instruction, every instruction alike
memory-grow = 1          # units per memory.grow call; nothing per page
charge = 0               # units for the charge itself
stack-unit = \"max operand stack and locals of a function, summed along the call chain\"
budget-global = \"env.gas_counter\"
height-global = \"florn.stack\"
";

/// The budget global the metering pass imports (RFC 003 rule 7, pass 3; rule 8).
pub const BUDGET_IMPORT_MODULE: &str = "env";
pub const BUDGET_IMPORT_NAME: &str = fvm_wasm_instrument::gas_metering::GAS_COUNTER_NAME;
/// The height global the limiter exports (pass 2).
pub const STACK_EXPORT: &str = "florn.stack";
/// The state exports of pass 4.
pub const GLOBAL_EXPORT_PREFIX: &str = "florn.global.";
pub const TABLE_EXPORT_PREFIX: &str = "florn.table.";
/// Names a source module may not import or export (rule 8).
pub const RESERVED_PREFIX: &str = "florn.";

/// An admitted module: what stages 1, 2, and 4 produced for one source under one class's limits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Admitted {
    pub source_sha256: [u8; 32],
    pub admitted_sha256: [u8; 32],
    pub bytes: Vec<u8>,
    pub manifest: ModuleManifest,
    pub world: WorldSpec,
    pub layout: StateLayout,
    pub memory_initial_pages: u64,
    pub limits: ClassLimits,
    pub admission_version: u32,
    pub cost_model_version: u32,
    /// The longest check-free path of any function, as instructions (RFC 010 rule 3): what the
    /// class's budget must exceed under version 2; reported under version 1 too.
    pub longest_path: u64,
}

impl Admitted {
    pub fn source_hex(&self) -> String {
        florn_model::canonical::hex(&self.source_sha256)
    }
    pub fn admitted_hex(&self) -> String {
        florn_model::canonical::hex(&self.admitted_sha256)
    }
}

/// The cache key of RFC 003 rule 3.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct CacheKey {
    source: [u8; 32],
    admission: u32,
    cost_model: u32,
    limits: ClassLimits,
}

/// The admission step with its cache. Stage 3 is [`estimate::estimate`], run once per scenario.
/// The worlds it admits are the core's and those the Host's domains add (RFC 008 rule 1,
/// `exports` and `imports`), composed once by [`Admission::add_world`].
pub struct Admission {
    pub maxima: ModuleMaxima,
    worlds: Vec<WorldSpec>,
    cache: HashMap<CacheKey, Arc<Admitted>>,
    transformed: usize,
}

impl Default for Admission {
    fn default() -> Self {
        Self {
            maxima: ModuleMaxima::default(),
            worlds: world::supported_worlds(),
            cache: HashMap::new(),
            transformed: 0,
        }
    }
}

pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

impl Admission {
    pub fn new(maxima: ModuleMaxima) -> Self {
        Self {
            maxima,
            ..Default::default()
        }
    }

    /// A world a domain adds (RFC 008 rule 6). A world already present is replaced by name, so
    /// that composing the same domains twice gives one list.
    pub fn add_world(&mut self, world: WorldSpec) {
        self.worlds.retain(|w| w.name != world.name);
        self.worlds.push(world);
    }

    /// The worlds this step admits (RFC 003 rule 2, stage 2).
    pub fn worlds(&self) -> &[WorldSpec] {
        &self.worlds
    }

    /// Stages 1, 2, and 4 for one source module under one class's limits, cached (rule 3).
    pub fn admit(
        &mut self,
        name: &str,
        source: &[u8],
        limits: &ClassLimits,
    ) -> Result<Arc<Admitted>, Errors> {
        self.admit_under(name, source, limits, COST_MODEL_VERSION)
    }

    /// `admit` under a named cost-model version (RFC 003 rule 15: the Host keeps every published
    /// version; a replay or a restored checkpoint admits under the run's).
    pub fn admit_under(
        &mut self,
        name: &str,
        source: &[u8],
        limits: &ClassLimits,
        cost_model: u32,
    ) -> Result<Arc<Admitted>, Errors> {
        let source_sha256 = sha256(source);
        let key = CacheKey {
            source: source_sha256,
            admission: ADMISSION_VERSION,
            cost_model,
            limits: *limits,
        };
        if let Some(a) = self.cache.get(&key) {
            return Ok(a.clone());
        }
        // Stage 1 needs the world for its import and export rows, and the world is named by the
        // sections stage 2 reads; so the sections are read first, but stage 1's refusals come first.
        let custom = validate::custom_sections(source)?;
        let sections = manifest::read_sections(source, &custom);
        let parsed = sections
            .as_ref()
            .ok()
            .map(|s| manifest::read_manifest(name, s, &self.worlds));
        let world = parsed
            .as_ref()
            .and_then(|p| p.as_ref().ok())
            .map(|(_, w)| w.clone())
            .unwrap_or_else(WorldSpec::core);
        let info = match validate::validate(source, &world, &self.maxima, limits.memory_pages()) {
            Ok(info) => info,
            Err(mut e) => {
                // With the manifest unreadable, the world was the core's: the manifest's own
                // refusals are reported with stage 1's, so that the cause is not hidden.
                if let Some(Err(m)) = &parsed {
                    e.0.extend(m.0.iter().cloned());
                }
                return Err(e);
            }
        };
        let (module_manifest, world) = match (sections, parsed) {
            (Err(e), _) | (Ok(_), Some(Err(e))) => return Err(e),
            (Ok(_), Some(Ok(p))) => p,
            (Ok(_), None) => unreachable!("read_manifest runs whenever the sections were read"),
        };
        let transformed = transform::transform(source, limits, cost_model)?;
        let bytes = transformed.bytes;
        let layout = transform::state_layout(&bytes)?;
        self.transformed += 1;
        let admitted = Arc::new(Admitted {
            source_sha256,
            admitted_sha256: sha256(&bytes),
            bytes,
            manifest: module_manifest,
            world,
            layout,
            memory_initial_pages: info.memory_initial_pages,
            limits: *limits,
            admission_version: ADMISSION_VERSION,
            cost_model_version: cost_model,
            longest_path: transformed.longest_path,
        });
        self.cache.insert(key, admitted.clone());
        Ok(admitted)
    }

    /// How many transformations ran (to verify the cache).
    pub fn transformed(&self) -> usize {
        self.transformed
    }
}

/// The fault kinds of RFC 002 rule 41 that admission's instrumentation produces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaultClass {
    Budget,
    Stack,
    Trap,
}

/// Classify a trap after an activation by the globals alone (RFC 003 rule 13): remaining budget
/// below zero is `budget`; height at or above the limit is `stack`; otherwise `trap`.
pub fn classify(remaining_budget: i64, height: i32, stack_limit: u64) -> FaultClass {
    if remaining_budget < 0 {
        FaultClass::Budget
    } else if u64::try_from(height).is_ok_and(|h| h >= stack_limit) {
        FaultClass::Stack
    } else {
        FaultClass::Trap
    }
}
