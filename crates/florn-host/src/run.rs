//! A run (RFC 005 §4): admission and the estimate before anything exists, the instance pool, the
//! run loop of rule 15, activations with the budget in place before lowering (RFC 003 rule 11),
//! classification by globals (rule 13), quotas in the import handlers (rule 17), export and
//! restore of actor state (RFC 003 §6), checkpoints, the diagnostic log, replay.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use florn_abi::core_world;
use florn_abi::{
    AbiError, Flat, Memory, Ty, Val, lift_flat_params, lower_flat_params, lower_result, store,
};
use florn_admission::config::{NoUnits, encode_config};
use florn_admission::estimate::{Capacity as EstimateCapacity, ModuleFacts, estimate};
use florn_admission::{
    ADMISSION_VERSION, Admission, Admitted, BUDGET_IMPORT_MODULE, BUDGET_IMPORT_NAME, ClassLimits,
    Errors, FaultClass, STACK_EXPORT, classify,
};
use florn_control::display::Record as DisplayRecord;
use florn_control::{At, Ends, Notification, Scope, Severity, Value};
use florn_engine::{
    Engine, EngineError, GlobalValue, HostGlobal, ImportFunction, ImportHandler, ImportSpec,
    InstanceId, Refusal, TrapKind,
};
use florn_kernel::trace::Comparison;
use florn_kernel::{
    Activation, Advance, AppliedIntervention, Config, Digest, Event, EventKey, EventKind,
    FaultKind, Identity, Instant, Kernel, Quotas, Sink, SinkRefusal, TiePolicy,
};
use florn_model::manifest::{ActorKind, DomainMap};
use florn_model::{HostMaxima, Manifest};
use florn_platform::{Capacity, Platform};

use crate::domain::{
    self, Action, BuiltinEvent, Composite, DomainEvent, DomainUnits, Domains, WorldSigs,
};
use crate::identity::{EndReason, Environment, ModuleInput, RunInputs, RunToml};
use crate::log::DiagnosticLog;
use crate::record::Contents;
use crate::snapshot::{ActorState, Checkpoint, CheckpointIdentity};
use crate::world::{CLASS_TIMER, Intervention};

/// The SHA-256 of module bytes as hexadecimal: the module identity (RFC 004 rule 16).
/// The cost-model versions this Host publishes (RFC 003 rule 15; RFC 010), for a front end.
pub use florn_admission::{COST_MODEL_VERSION, COST_MODEL_VERSIONS, cost_table};

pub fn sha256_hex(bytes: &[u8]) -> String {
    florn_model::canonical::hex(&florn_admission::sha256(bytes))
}

/// An admission step with the default maxima, for front ends that hold no `florn-admission`
/// dependency of their own (RFC 005 rule 43).
pub fn default_admission() -> Admission {
    Admission::default()
}

/// The config record of an admitted module with every parameter at its default (RFC 002 rule
/// 14), for the benchmarks' `init`; for front ends without an admission dependency.
pub fn default_config(admitted: &Admitted) -> Result<Vec<u8>, Errors> {
    encode_config(
        1,
        &admitted.manifest,
        &BTreeMap::new(),
        florn_model::TickLength::Ns,
        &NoUnits,
    )
}

/// The seed of a new run (RFC 005 rule 26): given, or chosen by the platform and then an input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seed {
    Given(u64),
    Choose,
}

/// What happens to the events of a faulted actor (RFC 001 rule 26; `enum(faulted-policy)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaultPolicy {
    /// The actor's later events are dropped; the run continues.
    Drop,
    StopRun,
}

impl FaultPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Drop => "drop",
            Self::StopRun => "stop-run",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Options {
    pub fault_policy: FaultPolicy,
    /// Keep every trace record in memory (for the trace export and per-actor chains).
    pub keep_records: bool,
    /// Keep payload bytes by digest (for the payload export); hashes each payload once more.
    pub keep_payloads: bool,
    pub chain_interval: u64,
    /// The defaults of RFC 003 rule 19 (measured in Phase 1) for limits a scenario leaves unset.
    pub class_defaults: ClassLimits,
    pub host: HostMaxima,
    /// `native`, `browser`
    pub host_build: String,
    /// Telemetry records kept for the display path before the oldest are dropped.
    pub telemetry_capacity: usize,
    /// Write the display stream of RFC 005 rule 14 from the start (the `display`
    /// subscription); `Run::set_display` changes it later.
    pub display: bool,
    /// A checkpoint by the Host itself every so many handled events, at the next boundary
    /// (RFC 009 rule 2); 0 for none but the requested ones.
    pub checkpoint_interval: u64,
    /// The memory the kept checkpoints may use before the older ones are thinned (RFC 009
    /// rule 3); the newest eight are kept whatever the budget.
    pub checkpoint_memory: u64,
    /// The cost-model version new runs admit under (RFC 003 rule 15; RFC 010 rule 9): a replay
    /// or a restored checkpoint uses its record's version instead.
    pub cost_model: u32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            fault_policy: FaultPolicy::Drop,
            keep_records: false,
            keep_payloads: false,
            chain_interval: florn_kernel::Trace::DEFAULT_INTERVAL,
            class_defaults: ClassLimits::defaults(),
            host: HostMaxima::native(),
            host_build: "native".into(),
            telemetry_capacity: 65_536,
            display: false,
            checkpoint_interval: Self::DEFAULT_CHECKPOINT_INTERVAL,
            checkpoint_memory: 256 * 1024 * 1024,
            cost_model: COST_MODEL_VERSION,
        }
    }
}

impl Options {
    /// Handled events between the Host's own checkpoints (RFC 009 rule 2).
    pub const DEFAULT_CHECKPOINT_INTERVAL: u64 = 10_000;
    /// The checkpoints the budget never drops (RFC 009 rule 3).
    pub const CHECKPOINTS_ALWAYS_KEPT: usize = 8;

    /// The browser Host's options: its maxima (RFC 003 rule 19) and build name.
    pub fn browser() -> Self {
        Self {
            host: HostMaxima::browser(),
            host_build: "browser".into(),
            checkpoint_memory: 64 * 1024 * 1024,
            ..Self::default()
        }
    }
}

/// A kept checkpoint (RFC 009 rule 3): where it stands and what it costs.
pub struct Kept {
    pub instant: Instant,
    pub records: u64,
    pub handled: u64,
    pub bytes: u64,
    checkpoint: Checkpoint,
}

impl Kept {
    pub fn checkpoint(&self) -> &Checkpoint {
        &self.checkpoint
    }
}

/// The memory a checkpoint holds, as the budget counts it: the actors' memories and the rest
/// by a fixed figure per item.
fn size_of_checkpoint(c: &Checkpoint) -> u64 {
    let actors: u64 = c
        .actors
        .values()
        .map(|a| a.memory.len() as u64 + 32 * (a.globals.len() + a.tables.len()) as u64 + 64)
        .sum();
    let kernel = 64 * c.kernel.queue.len() as u64
        + 64 * c.kernel.actors.len() as u64
        + c.kernel
            .applied_interventions
            .iter()
            .map(|i| i.content.len() as u64 + 16)
            .sum::<u64>()
        + 40 * c.kernel.trace.stored.len() as u64;
    let domains: u64 = c.domains.values().map(|d| d.len() as u64 + 16).sum();
    actors + kernel + domains + 256
}

/// A committed `report`, for the display path (RFC 005 rule 14; reduced above the transport).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Telemetry {
    pub at: Instant,
    pub actor: Identity,
    pub channel: u32,
    pub payload: Vec<u8>,
}

/// Why the Host could not do what was asked, outside the coded refusals of admission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostError {
    /// Coded refusals: admission, the estimate, the config.
    Refused(Errors),
    /// Coded refusals of the domains (RFC 008 rule 1, `validate`), and a declared domain this
    /// Host does not provide (`domain.unknown`).
    Domain(Vec<Notification>),
    Engine(EngineError),
    /// The actor is not module-backed, or has no instance.
    NoInstance(Identity),
    /// The engine could not hold the actor's instance: `host.instance-failed` (RFC 005 rule 10).
    InstanceFailed {
        actor: Identity,
        text: String,
    },
    /// The run has ended.
    Ended,
    /// The run stands within an instant, where no checkpoint and no branch is possible (RFC 001
    /// rule 51).
    MidInstant,
    /// A checkpoint does not match this run's inputs: `checkpoint.mismatch` with the field.
    CheckpointMismatch {
        what: &'static str,
        expected: String,
        found: String,
    },
    Malformed(&'static str),
}

impl From<Errors> for HostError {
    fn from(e: Errors) -> Self {
        Self::Refused(e)
    }
}
impl From<EngineError> for HostError {
    fn from(e: EngineError) -> Self {
        Self::Engine(e)
    }
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostError::Refused(e) => write!(f, "{e}"),
            HostError::Domain(ns) => {
                let codes: Vec<&str> = ns.iter().map(|n| n.code.as_str()).collect();
                write!(f, "{}", codes.join(", "))
            }
            HostError::Engine(e) => write!(f, "{e}"),
            HostError::NoInstance(id) => write!(f, "actor {id} has no instance"),
            HostError::InstanceFailed { actor, text } => {
                write!(f, "host.instance-failed: actor {actor}: {text}")
            }
            HostError::Ended => write!(f, "the run has ended"),
            HostError::MidInstant => write!(f, "the run is within an instant"),
            HostError::CheckpointMismatch {
                what,
                expected,
                found,
            } => write!(
                f,
                "checkpoint.mismatch {what}: expected {expected}, found {found}"
            ),
            HostError::Malformed(what) => write!(f, "malformed {what}"),
        }
    }
}

impl std::error::Error for HostError {}

/// The exports of the core world, by index: every world's first three (RFC 002 rule 20).
const INIT: usize = 0;
const ON_MESSAGE: usize = 1;
const ON_TIMER: usize = 2;

/// A module-backed actor in the pool.
struct Slot<E: Engine> {
    instance: Option<E::Instance>,
    /// One reference per export of the world, in the world's order (RFC 005 rule 7).
    funcs: Vec<E::Func>,
    sink: Sink,
    admitted: Arc<Admitted>,
    module_name: String,
    limits: ClassLimits,
    /// The lowered port list for `init`.
    ports: Val,
    config: Vec<u8>,
    spec: ImportSpec,
    /// The signatures of the module's world: the core's and the domain's beyond it.
    world: Arc<WorldSigs>,
}

/// A built-in actor (RFC 008 rule 19): run by its domain, with a sink of its own.
struct Builtin {
    sink: Sink,
    ports: Vec<florn_model::manifest::Port>,
    config: DomainMap,
}

/// What an activation ended with.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Outcome {
    /// Committed, with the channel 0 texts of the activation for the diagnostic log (RFC 002 rule 33).
    Committed { texts: Vec<String> },
    Faulted {
        kind: FaultKind,
        parameter: u32,
        metered: u64,
    },
    /// A Host fault (RFC 001 rule 65): the run ends, not reproducible.
    HostFault { reason: &'static str, text: String },
}

/// The Host's side of the imports for one activation (RFC 003 rule 17; RFC 002 §5): the core's
/// five, and a domain's answered from the activation's starting state (RFC 002 rule 47).
struct Handler<'a> {
    kernel: &'a mut Kernel,
    sink: &'a mut Sink,
    world: &'a mut Composite,
    actor: Identity,
    sigs: &'a WorldSigs,
    /// A lifting failure: the contract fault of RFC 002 rule 8, kept for classification.
    contract: Option<AbiError>,
}

impl ImportHandler for Handler<'_> {
    fn call(
        &mut self,
        _: InstanceId,
        function: &str,
        args: &[Flat],
        memory: &mut dyn Memory,
    ) -> Result<Vec<Flat>, Refusal> {
        let Some((_, module, tys, result)) = self.sigs.imports.iter().find(|(n, ..)| n == function)
        else {
            return Err(Refusal);
        };
        // A result beyond one flat value comes back through a return pointer the guest passes
        // as the last argument (RFC 002 §2).
        let retptr = result
            .as_ref()
            .is_some_and(|t| t.flat_count() > florn_abi::MAX_FLAT_RESULTS);
        let (flats, ret) = match (retptr, args) {
            (true, [rest @ .., Flat::I32(p)]) => (rest, Some(*p as u32)),
            (true, _) => return Err(Refusal),
            (false, _) => (args, None),
        };
        let vals = match lift_flat_params(memory, tys, flats) {
            Ok(v) => v,
            Err(e) => {
                self.contract = Some(e);
                return Err(Refusal);
            }
        };
        if module != core_world::HOST_MODULE {
            let Some(d) = self.world.of_module(module) else {
                return Err(Refusal);
            };
            let answer = d.answer(self.actor, function, &vals);
            return match (result, answer, ret) {
                (None, None, None) => Ok(Vec::new()),
                (Some(t), Some(v), Some(ptr)) => {
                    store(memory, t, &v, ptr).map(|()| Vec::new()).map_err(|e| {
                        self.contract = Some(e);
                        Refusal
                    })
                }
                (Some(t), Some(v), None) => lower_result(memory, Some(t), Some(&v))
                    .map(|f| f.into_iter().collect())
                    .map_err(|e| {
                        self.contract = Some(e);
                        Refusal
                    }),
                _ => Err(Refusal),
            };
        }
        let r = match (function, vals.as_slice()) {
            ("send", [Val::U32(port), payload]) => self
                .sink
                .send(*port, payload.as_bytes().unwrap_or_default())
                .map(|()| Vec::new()),
            ("set-timer", [Val::U32(timer), Val::U64(delay)]) => {
                self.sink.set_timer(*timer, *delay).map(|()| Vec::new())
            }
            ("cancel-timer", [Val::U32(timer)]) => {
                self.sink.cancel_timer(*timer).map(|()| Vec::new())
            }
            ("random", []) => Ok(vec![Flat::I64(self.kernel.draw(self.actor) as i64)]),
            ("report", [Val::U32(channel), payload]) => self
                .sink
                .report(*channel, payload.as_bytes().unwrap_or_default())
                .map(|()| Vec::new()),
            _ => return Err(Refusal),
        };
        r.map_err(|_| Refusal)
    }
}

/// The parameter of a fault record (RFC 005 rule 20): for `quota`, the quota's index in RFC 003
/// rule 16's table; for `contract`, the number of the RFC 002 rule violated; otherwise zero.
fn fault_parameter(r: &SinkRefusal) -> u32 {
    match r {
        SinkRefusal::Quota { quota, .. } => match *quota {
            "effects-per-activation" => 4,
            "payload" => 5,
            "pending-timers" => 6,
            "telemetry-per-activation" => 7,
            _ => 0,
        },
        SinkRefusal::Contract { rule, .. } => rule
            .rsplit(' ')
            .next()
            .and_then(|n| n.parse().ok())
            .unwrap_or(0),
    }
}

pub(crate) fn fault_code(kind: FaultKind) -> &'static str {
    match kind {
        FaultKind::Trap => "fault.trap",
        FaultKind::Budget => "fault.budget",
        FaultKind::Stack => "fault.stack",
        FaultKind::Memory => "fault.memory",
        FaultKind::Quota => "fault.quota",
        FaultKind::Contract => "fault.contract",
    }
}

fn at(i: Instant) -> At {
    At {
        tick: i.tick,
        microstep: i.microstep,
    }
}

/// The kind and the fields of an intervention's canonical map (RFC 005 rule 26), for the kinds
/// a domain decodes (RFC 008 rule 4). A recorded intervention of a domain kind carries its
/// target under `target`, which the decoding leaves to the Host.
pub fn decode_intervention(content: &[u8]) -> Option<(String, Option<Identity>, DomainMap)> {
    let mut map = florn_model::canonical::Decoder::new(content).domain_map()?;
    let kind = match map.remove("kind")? {
        florn_model::manifest::DomainValue::Text(k) => k,
        _ => return None,
    };
    let target = match map.remove("target") {
        Some(florn_model::manifest::DomainValue::Integer(t)) => Some(u32::try_from(t).ok()?),
        Some(_) => return None,
        None => None,
    };
    Some((kind, target, map))
}

/// The recorded content of a domain intervention: the request's map with its `target`, so
/// that a replay addresses it the same (RFC 001 rules 35 and 36).
fn encode_targeted(target: Identity, content: &[u8]) -> Vec<u8> {
    let Some(mut map) = florn_model::canonical::Decoder::new(content).domain_map() else {
        return content.to_vec();
    };
    map.insert(
        "target".into(),
        florn_model::manifest::DomainValue::Integer(i64::from(target)),
    );
    let mut e = florn_model::canonical::Encoder::default();
    florn_model::canonical::domain_map(&mut e, &map);
    e.finish()
}

pub struct Run<E: Engine> {
    engine: E,
    admission: Admission,
    scenario: Manifest,
    kernel: Kernel,
    world: Composite,
    actors: BTreeMap<Identity, Slot<E>>,
    builtins: BTreeMap<Identity, Builtin>,
    inputs: RunInputs,
    environment: Environment,
    run_id: String,
    created_ms: u64,
    wall_ms: u64,
    log: DiagnosticLog,
    telemetry: VecDeque<Telemetry>,
    telemetry_dropped: u64,
    /// Each actor's activations in the display batch in progress (count, faulted, last), when
    /// a front end subscribed to the display stream (RFC 005 rule 14).
    activity: Option<BTreeMap<Identity, (u16, bool, u64)>>,
    /// The tick the last advance reached: every event before it is handled (the kernel's clock
    /// stays at the last event handled). The display batch's instant.
    reached: u64,
    payloads: BTreeMap<Digest, Vec<u8>>,
    options: Options,
    ended: Option<EndReason>,
    reproducible: bool,
    replay_against: Option<Vec<(u64, Digest)>>,
    /// The signatures of every world a module of the run uses, by world name.
    worlds: BTreeMap<String, Arc<WorldSigs>>,
    capacity: Capacity,
    /// The kept checkpoints, oldest first (RFC 009 rule 3).
    kept: Vec<Kept>,
    /// The handled count at the last checkpoint taken by the Host itself (rule 2).
    last_checkpoint_handled: u64,
    /// A `checkpoint` command waiting for the next boundary (rule 2).
    checkpoint_requested: bool,
    /// Detail by replay in progress (rule 9): nothing is logged, kept or displayed meanwhile.
    detail: bool,
    /// The parent, when this run is a branch (RFC 009 rule 12).
    parent: Option<crate::identity::Parent>,
}

impl<E: Engine> Run<E> {
    /// A new run (RFC 005 rule 26, `new-run`): compose the domains the scenario declares and
    /// validate through them (RFC 008 rule 1), admit every module, estimate, instantiate the
    /// pool, initialise every actor in ascending identity order (RFC 002 rule 36). Nothing is
    /// instantiated before the estimate passes (RFC 003 rule 4).
    // The Host's parts and the run's inputs, each named: a struct would only move the list.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        engine: E,
        admission: Admission,
        platform: &mut dyn Platform,
        domains: &Domains,
        scenario: Manifest,
        modules: &BTreeMap<String, Vec<u8>>,
        seed: Seed,
        options: Options,
    ) -> Result<Self, HostError> {
        Self::try_new(
            engine, admission, platform, domains, scenario, modules, seed, options,
        )
        .map_err(|parts| parts.0)
    }

    /// As [`Self::new`], but a failure gives the engine and the admission cache back (boxed, as
    /// the engine is large), so that a front end can start another run (RFC 005 rule 10: the
    /// run ends, the Host does not).
    // The Host's parts and the run's inputs, each named: a struct would only move the list.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        engine: E,
        admission: Admission,
        platform: &mut dyn Platform,
        domains: &Domains,
        scenario: Manifest,
        modules: &BTreeMap<String, Vec<u8>>,
        seed: Seed,
        options: Options,
    ) -> Result<Self, Box<(HostError, E, Admission)>> {
        let seed = match seed {
            Seed::Given(s) => s,
            Seed::Choose => platform.new_seed(),
        };
        let mut run = Self::prepare(
            engine, admission, platform, domains, scenario, modules, seed, options,
        )?;
        if let Err(e) = run.instantiate_all() {
            let (engine, admission) = run.into_parts();
            return Err(Box::new((e, engine, admission)));
        }
        run.keep_start();
        run.init_all();
        Ok(run)
    }

    /// Everything before instances exist: admission, the estimate, the kernel, the log's first
    /// entries.
    // The Host's parts and the run's inputs, each named: a struct would only move the list.
    #[allow(clippy::too_many_arguments)]
    fn prepare(
        engine: E,
        mut admission: Admission,
        platform: &mut dyn Platform,
        domains: &Domains,
        scenario: Manifest,
        modules: &BTreeMap<String, Vec<u8>>,
        seed: u64,
        options: Options,
    ) -> Result<Self, Box<(HostError, E, Admission)>> {
        let capacity = platform.capacity();
        let created_ms = platform.wall_clock_ms();
        let run_id = format!("{created_ms:x}-{seed:x}");
        let mut log = DiagnosticLog::default();
        let mut push = |n: Notification| log.push(created_ms, &run_id, n);
        push(
            Notification::new("host.capacity", Severity::Info, Scope::run(), Ends::Nothing)
                .with("instances", Value::Count(capacity.instances))
                .with("memory", Value::Bytes(capacity.memory_bytes)),
        );

        // The domains (RFC 008 rule 1): a declared one this Host lacks, then their validation,
        // every refusal reported together.
        let domain_values = match domains.instantiate(&scenario) {
            Ok(d) => d,
            Err(n) => {
                push((*n).clone());
                return Err(Box::new((HostError::Domain(vec![*n]), engine, admission)));
            }
        };
        let refusals = domain::validate(&domain_values, &scenario);
        if !refusals.is_empty() {
            for n in &refusals {
                push(n.clone());
            }
            return Err(Box::new((HostError::Domain(refusals), engine, admission)));
        }
        for d in &domain_values {
            if let Some(w) = d.world() {
                admission.add_world(w.spec());
            }
        }
        // Stages 1, 2, 4 per module and class; every refusal reported together.
        let mut errors = Errors::default();
        let mut admitted: BTreeMap<(String, ClassLimits), Arc<Admitted>> = BTreeMap::new();
        let mut facts: BTreeMap<String, ModuleFacts> = BTreeMap::new();
        let mut inputs_modules: Vec<ModuleInput> = Vec::new();
        let mut contracts: Vec<String> = Vec::new();
        for actor in &scenario.actors {
            let ActorKind::Module(name) = &actor.kind else {
                continue;
            };
            let limits = ClassLimits::resolve(
                &scenario.limits_for(&actor.class),
                &options.class_defaults,
                &options.host,
            );
            let key = (name.clone(), limits);
            if admitted.contains_key(&key) {
                continue;
            }
            let Some(bytes) = modules.get(name) else {
                errors.push(florn_admission::Error::at(
                    florn_admission::Kind::Model(Box::new(
                        florn_model::Kind::ReferenceUnresolved {
                            reference: name.clone(),
                            kind: "module",
                        },
                    )),
                    format!("modules.{name}"),
                ));
                continue;
            };
            match admission.admit_under(name, bytes, &limits, options.cost_model) {
                Ok(a) => {
                    if !facts.contains_key(name) {
                        facts.insert(
                            name.clone(),
                            ModuleFacts {
                                source_sha256: a.source_sha256,
                                manifest: a.manifest.clone(),
                                memory_initial_pages: a.memory_initial_pages,
                            },
                        );
                        inputs_modules.push(ModuleInput {
                            name: name.clone(),
                            source_sha256: a.source_sha256,
                            admitted_sha256: a.admitted_sha256,
                        });
                        if !contracts.contains(&a.world.name) {
                            contracts.push(a.world.name.clone());
                        }
                        push(
                            Notification::new(
                                "admission.result",
                                Severity::Info,
                                Scope::file(name),
                                Ends::Nothing,
                            )
                            .with("module", Value::Name(name.clone()))
                            .with("hash", Value::Text(a.admitted_hex()))
                            .with("admitted", Value::Enum("validated".into())),
                        );
                    }
                    admitted.insert(key, a);
                }
                Err(e) => {
                    for err in &e.0 {
                        push(Notification::from_error(
                            err.code(),
                            err.params(),
                            err.field.clone(),
                            Some(name),
                            Ends::Command,
                        ));
                    }
                    errors.0.extend(e.0);
                }
            }
        }
        if !errors.is_empty() {
            return Err(Box::new((errors.into(), engine, admission)));
        }
        // The domains' view of each module's manifest (RFC 008 rule 1, `telemetry`; rule 16).
        let mut refusals = Vec::new();
        for (name, f) in &facts {
            for d in &domain_values {
                for n in d.validate_module(name, &f.manifest) {
                    push(n.clone());
                    if n.severity == Severity::Error {
                        refusals.push(n);
                    }
                }
            }
        }
        if !refusals.is_empty() {
            return Err(Box::new((HostError::Domain(refusals), engine, admission)));
        }
        // Stage 3: the estimate against limits and capacity (RFC 003 rule 20).
        let est = estimate(
            &scenario,
            &facts,
            &options.class_defaults,
            &options.host,
            EstimateCapacity {
                instances: capacity.instances,
                memory_bytes: capacity.memory_bytes,
            },
        );
        let est = match est {
            Ok(e) => e,
            Err(e) => {
                for err in &e.0 {
                    push(Notification::from_error(
                        err.code(),
                        err.params(),
                        err.field.clone(),
                        None,
                        Ends::Command,
                    ));
                }
                return Err(Box::new((e.into(), engine, admission)));
            }
        };
        for (what, used, cap) in [
            ("instances", est.instances, capacity.instances),
            ("memory", est.memory_bytes, capacity.memory_bytes),
        ] {
            if used.saturating_mul(10) >= cap.saturating_mul(9) {
                push(
                    Notification::new(
                        "host.capacity-warning",
                        Severity::Warning,
                        Scope::run(),
                        Ends::Nothing,
                    )
                    .with("used", Value::Count(used))
                    .with("capacity", Value::Count(cap))
                    .with("what", Value::Enum(what.into())),
                );
            }
        }
        let config = Config {
            root_seed: seed,
            tie: match scenario.order.tie {
                florn_model::TiePolicy::ByIdentity => TiePolicy::ByIdentity,
                florn_model::TiePolicy::BySeed => TiePolicy::BySeed,
            },
            end: scenario.time.end,
            microsteps_per_tick: scenario
                .order
                .microsteps_per_tick
                .and_then(|m| u32::try_from(m).ok())
                .unwrap_or(Config::default().microsteps_per_tick),
            events_per_instant: scenario
                .order
                .events_per_instant
                .unwrap_or(Config::default().events_per_instant),
            timer_class: CLASS_TIMER,
            chain_interval: options.chain_interval,
            keep_records: options.keep_records,
        };
        let kernel = Kernel::new(config);
        let mut world = match Composite::new(&scenario, domain_values, seed) {
            Ok(w) => w,
            Err(what) => return Err(Box::new((HostError::Malformed(what), engine, admission))),
        };
        world.set_display(options.display);
        let mut worlds: BTreeMap<String, Arc<WorldSigs>> = BTreeMap::new();
        let core_sigs = WorldSigs::core();
        worlds.insert(core_sigs.name.clone(), Arc::new(core_sigs));
        for d in world.domains() {
            if let Some(w) = d.world() {
                let sigs = WorldSigs::of(&w);
                worlds.insert(sigs.name.clone(), Arc::new(sigs));
            }
        }
        let info = engine.info();
        let environment = Environment {
            florn_version: env!("CARGO_PKG_VERSION").to_string(),
            host_build: options.host_build.clone(),
            engine: info.name,
            engine_version: info.version,
            arch: std::env::consts::ARCH.to_string(),
        };
        let inputs = RunInputs {
            scenario_sha256: scenario.scenario_hash(),
            modules: inputs_modules,
            admission_version: ADMISSION_VERSION,
            cost_model_version: options.cost_model,
            cost_table: cost_table(options.cost_model).unwrap_or("").to_string(),
            contracts,
            root_seed: seed,
            interventions: Vec::new(),
            generator: florn_kernel::GENERATOR_NAME.to_string(),
            derivation: florn_kernel::DERIVATION_NAME.to_string(),
            digest: florn_kernel::DIGEST_NAME.to_string(),
        };
        let mut run = Self {
            engine,
            admission,
            scenario,
            kernel,
            world,
            actors: BTreeMap::new(),
            builtins: BTreeMap::new(),
            inputs,
            environment,
            run_id,
            created_ms,
            wall_ms: created_ms,
            log,
            telemetry: VecDeque::new(),
            telemetry_dropped: 0,
            activity: options.display.then(BTreeMap::new),
            reached: 0,
            payloads: BTreeMap::new(),
            options,
            ended: None,
            reproducible: true,
            replay_against: None,
            worlds,
            capacity,
            kept: Vec::new(),
            last_checkpoint_handled: 0,
            checkpoint_requested: false,
            detail: false,
            parent: None,
        };
        // The slots, without instances yet.
        let tick = run.scenario.time.tick;
        let mut errors = Errors::default();
        for (id, name, limits) in &est.actors {
            let actor = run.scenario.actor(*id).expect("estimated actor exists");
            let a = admitted
                .get(&(name.clone(), *limits))
                .expect("admitted above")
                .clone();
            let config = match encode_config(
                *id,
                &a.manifest,
                &actor.config,
                tick,
                &DomainUnits(run.world.domains()),
            ) {
                Ok(c) => c,
                Err(e) => {
                    errors.0.extend(e.0);
                    continue;
                }
            };
            let Some(world) = run.worlds.get(&a.world.name).cloned() else {
                errors.push(florn_admission::Error::at(
                    florn_admission::Kind::ContractUnsupported {
                        declared: a.world.name.clone(),
                        supported: run.worlds.keys().cloned().collect::<Vec<_>>().join(", "),
                    },
                    format!("modules.{name}"),
                ));
                continue;
            };
            let ports = Val::List(
                actor
                    .ports
                    .iter()
                    .map(|p| {
                        let kind = a
                            .manifest
                            .ports
                            .iter()
                            .position(|k| k.ty == p.r#type)
                            .unwrap_or(0) as u32;
                        Val::Record(vec![Val::U32(p.id), Val::U32(kind), Val::str(&p.name)])
                    })
                    .collect(),
            );
            let quotas = Quotas {
                effects_per_activation: limits.effects_per_activation as usize,
                payload_bytes: limits.payload_bytes as usize,
                telemetry_per_activation_bytes: limits.telemetry_per_activation_bytes as usize,
                pending_timers: limits.pending_timers as usize,
            };
            let sink = Sink::new(
                quotas,
                actor.ports.iter().map(|p| p.id).collect(),
                a.manifest.telemetry.iter().map(|c| c.channel).collect(),
            );
            let spec = ImportSpec {
                functions: a
                    .world
                    .imports
                    .iter()
                    .flat_map(|i| i.functions.iter().map(move |f| (i.module.clone(), f)))
                    .map(|(module, f)| ImportFunction {
                        module,
                        name: f.name.clone(),
                        params: f.params.clone(),
                        results: f.results.clone(),
                    })
                    .collect(),
                globals: vec![HostGlobal {
                    module: BUDGET_IMPORT_MODULE.into(),
                    name: BUDGET_IMPORT_NAME.into(),
                    initial: 0,
                }],
            };
            run.kernel.add_actor(*id);
            run.actors.insert(
                *id,
                Slot {
                    instance: None,
                    funcs: Vec::new(),
                    sink,
                    admitted: a,
                    module_name: name.clone(),
                    limits: *limits,
                    ports,
                    config,
                    spec,
                    world,
                },
            );
        }
        // The built-in actors (RFC 008 rule 19): no instance, no limits; a sink of their own.
        for actor in &run.scenario.actors {
            let ActorKind::Builtin(kind) = &actor.kind else {
                continue;
            };
            let channels = run
                .world
                .builtin(actor.id)
                .map(|(d, kind)| d.builtin_channels(&kind))
                .unwrap_or_default();
            run.kernel.add_actor(actor.id);
            let _ = kind;
            run.builtins.insert(
                actor.id,
                Builtin {
                    sink: Sink::new(
                        Quotas::default(),
                        actor.ports.iter().map(|p| p.id).collect(),
                        channels,
                    ),
                    ports: actor.ports.clone(),
                    config: actor.config.clone(),
                },
            );
        }
        if !errors.is_empty() {
            for err in &errors.0 {
                run.notify(Notification::from_error(
                    err.code(),
                    err.params(),
                    err.field.clone(),
                    None,
                    Ends::Command,
                ));
            }
            let (engine, admission) = run.into_parts();
            return Err(Box::new((errors.into(), engine, admission)));
        }
        for c in &run.scenario.changes {
            let mut e = florn_model::canonical::Encoder::default();
            florn_model::canonical::domain_map(&mut e, &c.payload);
            run.kernel
                .schedule_change(c.at, c.target.identity(), e.finish());
        }
        Ok(run)
    }

    fn instantiate_all(&mut self) -> Result<(), HostError> {
        let ids: Vec<Identity> = self.actors.keys().copied().collect();
        for id in ids {
            self.instantiate(id)?;
        }
        Ok(())
    }

    /// A fresh instance for an actor from its cached module (RFC 005 rule 9: never compiles
    /// twice); exports resolved once (rule 7).
    fn instantiate(&mut self, id: Identity) -> Result<(), HostError> {
        let slot = self.actors.get_mut(&id).ok_or(HostError::NoInstance(id))?;
        let module = self.engine.compile(&slot.admitted.bytes)?;
        let inst = match self.engine.instantiate(&module, &slot.spec) {
            Ok(i) => i,
            Err(e) => {
                // RFC 005 rule 10: a failure to hold the instance names the actor and the
                // engine's text; a refusal of the module names the module (RFC 003 rule 26).
                let n = match &e {
                    EngineError::Resource(text) => {
                        let n = Notification::new(
                            "host.instance-failed",
                            Severity::Error,
                            Scope::actor(id),
                            Ends::Run,
                        )
                        .with("actor", Value::Identity(id))
                        .with("text", Value::Text(text.clone()));
                        self.log.push(self.wall_ms, &self.run_id, n);
                        return Err(HostError::InstanceFailed {
                            actor: id,
                            text: text.clone(),
                        });
                    }
                    other => Notification::new(
                        "host.engine-refused",
                        Severity::Error,
                        Scope::actor(id),
                        Ends::Run,
                    )
                    .with("module", Value::Name(slot.module_name.clone()))
                    .with("text", Value::Text(other.to_string())),
                };
                self.log.push(self.wall_ms, &self.run_id, n);
                return Err(e.into());
            }
        };
        let mut funcs = Vec::with_capacity(slot.world.exports.len());
        for (n, _, shape) in &slot.world.exports {
            funcs.push(
                self.engine
                    .func(&inst, n, *shape)
                    .ok_or_else(|| EngineError::Export(n.to_string()))?,
            );
        }
        slot.instance = Some(inst);
        slot.funcs = funcs;
        Ok(())
    }

    /// `init` for every actor, module-backed or built-in, in ascending identity order (RFC 002
    /// rule 36).
    fn init_all(&mut self) {
        let mut ids: Vec<Identity> = self.actors.keys().copied().collect();
        ids.extend(self.builtins.keys().copied());
        ids.sort_unstable();
        for id in ids {
            if self.ended.is_some() {
                break;
            }
            let event = Event {
                key: EventKey {
                    tick: 0,
                    microstep: 0,
                    class: florn_kernel::CLASS_CONTROL,
                    tie: 0,
                    origin: id,
                    seq: 0,
                },
                target: id,
                kind: EventKind::Domain {
                    payload: Vec::new(),
                },
            };
            if let Some(b) = self.builtins.get(&id) {
                let (ports, config) = (b.ports.clone(), b.config.clone());
                self.activate_builtin(
                    id,
                    event,
                    &BuiltinEvent::Init {
                        ports: &ports,
                        config: &config,
                    },
                );
                continue;
            }
            let slot = &self.actors[&id];
            let vals = vec![
                Val::U64(0),
                Val::U32(id),
                slot.ports.clone(),
                Val::bytes(&slot.config),
            ];
            self.activate(id, event, INIT, &vals);
        }
    }

    fn notify(&mut self, n: Notification) {
        if self.detail {
            return;
        }
        self.log.push(self.wall_ms, &self.run_id, n);
    }

    /// The platform's wall clock, for the log's metadata; call between advances.
    pub fn set_wall_clock(&mut self, ms: u64) {
        self.wall_ms = ms;
    }

    pub fn scenario(&self) -> &Manifest {
        &self.scenario
    }
    /// The composed domains (RFC 008 rule 1), in the scenario's order.
    pub fn domains(&self) -> &[Box<dyn crate::domain::Domain>] {
        self.world.domains()
    }
    /// The event classes of the run with their names (RFC 001 rule 15).
    pub fn class_names(&self) -> &BTreeMap<u8, &'static str> {
        self.world.class_names()
    }
    pub fn kernel(&self) -> &Kernel {
        &self.kernel
    }
    pub fn inputs(&self) -> &RunInputs {
        &self.inputs
    }
    pub fn environment(&self) -> &Environment {
        &self.environment
    }
    pub fn log(&self) -> &DiagnosticLog {
        &self.log
    }
    pub fn run_id(&self) -> &str {
        &self.run_id
    }
    pub fn ended(&self) -> Option<EndReason> {
        self.ended
    }
    pub fn reproducible(&self) -> bool {
        self.reproducible
    }
    pub fn capacity(&self) -> Capacity {
        self.capacity
    }
    pub fn engine(&self) -> &E {
        &self.engine
    }
    /// The chain value now.
    pub fn chain(&self) -> Digest {
        self.kernel.trace.chain()
    }
    pub fn chain_hex(&self) -> String {
        self.kernel.trace.chain_hex()
    }
    /// Whether an actor is alive (not faulted).
    pub fn alive(&self, id: Identity) -> bool {
        self.kernel.actor(id).is_some_and(|a| a.alive)
    }
    /// Payload bytes by digest, when kept.
    pub fn payload(&self, digest: &Digest) -> Option<&[u8]> {
        self.payloads.get(digest).map(Vec::as_slice)
    }
    pub fn payloads(&self) -> &BTreeMap<Digest, Vec<u8>> {
        &self.payloads
    }
    /// Committed telemetry since the last take, oldest first.
    pub fn take_telemetry(&mut self) -> Vec<Telemetry> {
        self.telemetry.drain(..).collect()
    }
    pub fn telemetry_dropped(&self) -> u64 {
        self.telemetry_dropped
    }
    /// The tick the last advance reached: every event before it is handled.
    pub fn reached(&self) -> u64 {
        self.reached
    }

    /// Write the display stream (RFC 005 rule 14) from now on, or stop; off until asked, so
    /// that a run nobody draws pays nothing for it.
    pub fn set_display(&mut self, on: bool) {
        self.world.set_display(on);
        self.activity = on.then(BTreeMap::new);
    }
    /// The display batch since the last take: the flights in order, then each actor's
    /// activity, then the tick reached (every event before it is handled); empty when the
    /// display is off.
    pub fn take_display(&mut self) -> Vec<DisplayRecord> {
        let Some(activity) = &mut self.activity else {
            return Vec::new();
        };
        let mut records = self.world.take_display();
        records.extend(std::mem::take(activity).into_iter().map(
            |(actor, (count, faulted, last))| DisplayRecord::Activity {
                actor,
                count,
                faulted,
                last,
            },
        ));
        records.push(DisplayRecord::Instant { tick: self.reached });
        records
    }
    /// Count an activation for the display batch.
    fn note_activity(&mut self, id: Identity, faulted: bool, instant: Instant) {
        if let Some(activity) = &mut self.activity {
            let a = activity.entry(id).or_insert((0, false, 0));
            a.0 = a.0.saturating_add(1);
            a.1 |= faulted;
            a.2 = instant.tick;
        }
    }
    /// A notification into the run's log (a front end's own, such as the display path's).
    pub fn notify_now(&mut self, n: Notification) {
        self.notify(n);
    }

    /// Take the engine and the admission cache back when the run is over, for the next run.
    pub fn into_parts(mut self) -> (E, Admission) {
        let ids: Vec<Identity> = self.actors.keys().copied().collect();
        for id in ids {
            if let Some(inst) = self.actors.get_mut(&id).and_then(|s| s.instance.take()) {
                self.engine.drop_instance(inst);
            }
        }
        (self.engine, self.admission)
    }

    /// Request a core intervention (RFC 001 rules 33 to 38): applied at the next boundary,
    /// recorded as an input.
    pub fn intervene(&mut self, at_tick: Option<u64>, intervention: &Intervention) {
        self.kernel.intervene(at_tick, intervention.encode());
    }

    /// Request an intervention by its canonical map (RFC 005 rule 26): the core's `message` or
    /// a kind of a composed domain (RFC 008 rule 4). A refusal names the field; nothing is
    /// queued then.
    pub fn intervene_map(
        &mut self,
        at_tick: Option<u64>,
        target: Identity,
        content: &[u8],
    ) -> Result<(), Box<Notification>> {
        let invalid = |field: &str| {
            Box::new(
                Notification::new(
                    "control.invalid",
                    Severity::Error,
                    Scope::run(),
                    Ends::Command,
                )
                .with("command", Value::Name("intervene".into()))
                .with("reason", Value::Enum("wrong-type".into()))
                .with("field", Value::Text(field.into())),
            )
        };
        if let Some(i) = Intervention::decode(content) {
            let Intervention::Message { actor, .. } = &i;
            if *actor != target {
                return Err(invalid("target"));
            }
            if !self.alive(*actor) {
                return Err(invalid("target"));
            }
            self.kernel.intervene(at_tick, content.to_vec());
            return Ok(());
        }
        let (kind, _, map) = decode_intervention(content).ok_or_else(|| invalid("content"))?;
        let Some(d) = self.world.of_kind(&kind) else {
            return Err(invalid("kind"));
        };
        d.check_intervention(target, &kind, &map)?;
        self.kernel
            .intervene(at_tick, encode_targeted(target, content));
        Ok(())
    }

    /// Advance by one unit (RFC 001 §2.1; RFC 005 rule 15). Returns the events handled. Pacing is
    /// the caller's: this computes, it never waits.
    pub fn advance(&mut self, unit: Advance) -> u64 {
        if self.ended.is_some() {
            return 0;
        }
        let start = self.kernel.now();
        let mut handled = 0u64;
        loop {
            if self.ended.is_some() {
                break;
            }
            self.checkpoint_if_due();
            let ev = self
                .kernel
                .next_event_if(|i| unit.includes(start, i, handled));
            let Some(ev) = ev else {
                if let Some(f) = self.kernel.run_fault().cloned() {
                    let (what, limit, origins, instant) = match f {
                        florn_kernel::RunFault::MicrostepLimit {
                            tick,
                            limit,
                            origins,
                        } => (
                            "microsteps-per-tick",
                            limit as u64,
                            origins,
                            Instant::new(tick, limit),
                        ),
                        florn_kernel::RunFault::EventsPerInstantLimit {
                            instant,
                            limit,
                            origins,
                        } => ("events-per-instant", limit, origins, instant),
                    };
                    let n = Notification::new(
                        "run.limit-exceeded",
                        Severity::Error,
                        Scope::run(),
                        Ends::Run,
                    )
                    .with("what", Value::Enum(what.into()))
                    .with("limit", Value::Count(limit))
                    .with(
                        "origins",
                        Value::Text(
                            origins
                                .iter()
                                .map(|o| o.to_string())
                                .collect::<Vec<_>>()
                                .join(","),
                        ),
                    )
                    .at(at(instant));
                    self.notify(n);
                    self.end(EndReason::HostFault);
                } else if !self.kernel.has_next() {
                    self.end(EndReason::EndReached);
                }
                break;
            };
            handled += 1;
            self.handle(ev);
        }
        let now = self.kernel.now().tick;
        let end = self.scenario.time.end;
        self.reached = match (self.ended, unit) {
            (Some(EndReason::EndReached), _) => end,
            (Some(_), _) => now,
            (None, Advance::ToTick(t)) => t.min(end).max(now),
            (None, Advance::OneTick) => now.saturating_add(1).min(end),
            (None, _) => now,
        }
        .max(self.reached);
        handled
    }

    /// Run to the end (`end` reached, the queue empty, or a fault that stops the run).
    pub fn run_to_end(&mut self) -> EndReason {
        while self.ended.is_none() {
            self.advance(Advance::ToTick(u64::MAX));
        }
        self.ended.expect("ended")
    }

    fn end(&mut self, reason: EndReason) {
        if self.ended.is_some() {
            return;
        }
        self.ended = Some(reason);
        self.inputs.interventions = self.kernel.applied_interventions().to_vec();
        let n = Notification::new("run.ended", Severity::Info, Scope::run(), Ends::Run)
            .with("reason", Value::Enum(reason.as_str().into()))
            .at(at(self.kernel.now()));
        self.notify(n);
        if let Some(stored) = self.replay_against.take() {
            let c = florn_kernel::trace::compare_reference(&self.kernel.trace.stored(), &stored);
            let n = match c {
                Comparison::Matched { points } => Notification::new(
                    "replay.matched",
                    Severity::Info,
                    Scope::run(),
                    Ends::Nothing,
                )
                .with("points", Value::Count(points)),
                Comparison::Diverged { from, to } => Notification::new(
                    "replay.diverged",
                    Severity::Error,
                    Scope::run(),
                    Ends::Nothing,
                )
                .with("from", Value::Count(from))
                .with("to", Value::Count(to)),
                Comparison::Incomparable => Notification::new(
                    "replay.diverged",
                    Severity::Error,
                    Scope::run(),
                    Ends::Nothing,
                )
                .with("from", Value::Count(0))
                .with("to", Value::Count(0)),
            };
            self.notify(n);
            self.replay_against = Some(stored);
        }
    }

    /// End the run on the user's word.
    pub fn end_run(&mut self) {
        self.end(EndReason::User);
    }

    fn handle(&mut self, ev: Event) {
        let now = ev.key.tick;
        let instant = ev.key.instant();
        match &ev.kind {
            EventKind::Timer { timer } => {
                if self.builtins.contains_key(&ev.target) {
                    self.activate_builtin(
                        ev.target,
                        ev.clone(),
                        &BuiltinEvent::Timer { timer: *timer },
                    );
                } else {
                    let vals = vec![Val::U64(now), Val::U32(*timer)];
                    self.activate(ev.target, ev.clone(), ON_TIMER, &vals);
                }
            }
            EventKind::Message { port, payload } => {
                // A message of a domain's class to an actor that faulted: dropped at delivery
                // (RFC 008 rule 5), the domain told.
                if !self.alive(ev.target)
                    && let Some(d) = self.world.of_class(ev.key.class)
                {
                    let actions = d.undelivered(&DomainEvent {
                        key: ev.key,
                        target: ev.target,
                        payload: payload.clone(),
                    });
                    self.apply(instant, &ev, actions);
                } else if self.builtins.contains_key(&ev.target) {
                    self.activate_builtin(
                        ev.target,
                        ev.clone(),
                        &BuiltinEvent::Message {
                            port: *port,
                            payload,
                        },
                    );
                } else {
                    let vals = vec![Val::U64(now), Val::U32(*port), Val::bytes(payload)];
                    self.activate(ev.target, ev.clone(), ON_MESSAGE, &vals);
                }
            }
            EventKind::Intervention { payload } => self.handle_intervention(&ev, payload),
            EventKind::ScheduledChange { payload } => {
                // The core gives a change no meaning for an actor; a domain's connection or
                // world takes it (RFC 004 rule 20; RFC 008 rule 1, `change`).
                let map = florn_model::canonical::Decoder::new(payload).domain_map();
                if let Some(map) = map
                    && let Some(d) = self.world.of_target(ev.target)
                {
                    let actions = d.change(instant, ev.target, &map);
                    self.apply(instant, &ev, actions);
                }
            }
            EventKind::Domain { payload } => {
                let event = DomainEvent {
                    key: ev.key,
                    target: ev.target,
                    payload: payload.clone(),
                };
                if self.builtins.contains_key(&ev.target) && self.alive(ev.target) {
                    self.activate_builtin(ev.target, ev.clone(), &BuiltinEvent::Domain(&event));
                    return;
                }
                let alive = self.alive(ev.target);
                if let Some(d) = self.world.of_class(ev.key.class) {
                    let actions = d.event(&event, alive);
                    self.apply(instant, &ev, actions);
                }
            }
        }
    }

    fn handle_intervention(&mut self, ev: &Event, payload: &[u8]) {
        let now = ev.key.tick;
        let instant = ev.key.instant();
        let applied = |kind: &str, scope: Scope| {
            Notification::new("intervention.applied", Severity::Info, scope, Ends::Nothing)
                .with("at", Value::Instant(at(instant)))
                .with("kind", Value::Name(kind.into()))
        };
        if let Some(Intervention::Message {
            actor,
            port,
            payload,
        }) = Intervention::decode(payload)
        {
            self.notify(applied("message", Scope::actor(actor)));
            if self.builtins.contains_key(&actor) {
                self.activate_builtin(
                    actor,
                    ev.clone(),
                    &BuiltinEvent::Message {
                        port,
                        payload: &payload,
                    },
                );
            } else {
                let vals = vec![Val::U64(now), Val::U32(port), Val::bytes(&payload)];
                self.activate(actor, ev.clone(), ON_MESSAGE, &vals);
            }
            return;
        }
        // The kernel addresses every intervention event to the interventions' origin; a
        // domain kind's target travels in the content (`encode_targeted`).
        let decoded = decode_intervention(payload);
        let Some((kind, Some(target), map)) = decoded else {
            self.notify(applied("unknown", Scope::run()));
            return;
        };
        let Some(d) = self.world.of_kind_mut(&kind) else {
            self.notify(applied("unknown", Scope::run()));
            return;
        };
        let actions = d.intervene(instant, target, &kind, &map);
        self.notify(applied(&kind, Scope::run()));
        self.apply(instant, ev, actions);
    }

    /// What a domain asked for (RFC 008 rule 1): calls into actors, events of its classes,
    /// notes for the log.
    fn apply(&mut self, instant: Instant, ev: &Event, actions: Vec<Action>) {
        for a in actions {
            match a {
                Action::Call {
                    actor,
                    export,
                    values,
                } => {
                    let which = self
                        .actors
                        .get(&actor)
                        .and_then(|s| s.world.export_index(&export));
                    if let Some(which) = which {
                        self.activate(actor, ev.clone(), which, &values);
                    }
                }
                Action::Schedule {
                    origin,
                    delay,
                    class,
                    target,
                    payload,
                } => {
                    if let Some(at) = instant.after(delay) {
                        let seq = self.world.next_seq(origin);
                        self.kernel
                            .schedule_domain(at, class, origin, seq, target, payload);
                    }
                }
                Action::Notify(n) => self.notify(n.at(at(instant))),
            }
        }
    }

    /// A built-in actor's activation (RFC 008 rule 19): the domain's code over a sink, committed
    /// like a guest's with `metered = 0`.
    fn activate_builtin(&mut self, id: Identity, event: Event, be: &BuiltinEvent<'_>) {
        if !self.alive(id) {
            return;
        }
        let instant = self.kernel.now();
        let act = self.kernel.begin(id, event);
        let Some(b) = self.builtins.get_mut(&id) else {
            self.kernel.fault(act, FaultKind::Trap, 0, 0);
            self.note_activity(id, true, instant);
            return;
        };
        b.sink.reset();
        let Some((d, kind)) = self.world.builtin(id) else {
            self.kernel.fault(act, FaultKind::Trap, 0, 0);
            self.note_activity(id, true, instant);
            return;
        };
        d.builtin(id, &kind, be, &mut b.sink);
        let reports: Vec<(u32, Vec<u8>)> = b
            .sink
            .effects()
            .iter()
            .filter_map(|e| match e {
                florn_kernel::Effect::Report { channel, payload } => {
                    Some((*channel, payload.clone()))
                }
                _ => None,
            })
            .collect();
        let kept: Vec<Vec<u8>> = if self.options.keep_payloads {
            b.sink
                .effects()
                .iter()
                .filter_map(|e| match e {
                    florn_kernel::Effect::Send { payload, .. }
                    | florn_kernel::Effect::Report { payload, .. } => Some(payload.clone()),
                    _ => None,
                })
                .collect()
        } else {
            Vec::new()
        };
        match self.kernel.commit(act, &mut b.sink, 0, &mut self.world) {
            Ok(()) => {
                self.note_activity(id, false, instant);
                for (channel, payload) in reports {
                    if channel == 0 {
                        let n = Notification::new(
                            "actor.text",
                            Severity::Info,
                            Scope::actor(id),
                            Ends::Nothing,
                        )
                        .with("actor", Value::Identity(id))
                        .with(
                            "text",
                            Value::Text(String::from_utf8_lossy(&payload).into_owned()),
                        )
                        .at(at(instant));
                        self.notify(n);
                    }
                    if self.telemetry.len() >= self.options.telemetry_capacity {
                        self.telemetry.pop_front();
                        self.telemetry_dropped += 1;
                    }
                    self.telemetry.push_back(Telemetry {
                        at: instant,
                        actor: id,
                        channel,
                        payload,
                    });
                }
                for p in kept {
                    self.payloads
                        .entry(*blake3::hash(&p).as_bytes())
                        .or_insert(p);
                }
            }
            Err(refusal) => {
                let (kind, parameter) = match &refusal {
                    florn_kernel::CommitRefusal::DelayBeyondLargestTick { timer, .. } => {
                        (FaultKind::Contract, *timer)
                    }
                    florn_kernel::CommitRefusal::PendingTimers { .. } => (FaultKind::Quota, 6),
                    florn_kernel::CommitRefusal::Refused(r) => {
                        (FaultKind::from_refusal(r), fault_parameter(r))
                    }
                };
                self.note_activity(id, true, instant);
                self.faulted(id, kind, parameter, 0, instant);
            }
        }
    }

    /// One activation (RFC 005 rule 15; RFC 003 rules 11 to 13; RFC 001 rules 22 to 24).
    fn activate(&mut self, id: Identity, event: Event, which: usize, vals: &[Val]) {
        if !self.alive(id) || !self.actors.get(&id).is_some_and(|s| s.instance.is_some()) {
            return;
        }
        let instant = self.kernel.now();
        let act = self.kernel.begin(id, event);
        let outcome = {
            let Self {
                engine,
                kernel,
                world,
                actors,
                options,
                payloads,
                telemetry,
                telemetry_dropped,
                ..
            } = self;
            let slot = actors.get_mut(&id).expect("checked above");
            activate_slot(
                engine,
                kernel,
                world,
                slot,
                act,
                which,
                vals,
                options.keep_payloads,
                payloads,
                telemetry,
                telemetry_dropped,
                options.telemetry_capacity,
                instant,
            )
        };
        self.note_activity(id, !matches!(outcome, Outcome::Committed { .. }), instant);
        match outcome {
            Outcome::Committed { texts } => {
                // Channel 0 is text for the author; it reaches the diagnostic log (RFC 002 §10).
                for text in texts {
                    let n = Notification::new(
                        "actor.text",
                        Severity::Info,
                        Scope::actor(id),
                        Ends::Nothing,
                    )
                    .with("actor", Value::Identity(id))
                    .with("text", Value::Text(text))
                    .at(at(instant));
                    self.notify(n);
                }
            }
            Outcome::Faulted {
                kind,
                parameter,
                metered,
            } => self.faulted(id, kind, parameter, metered, instant),
            Outcome::HostFault { reason, text } => {
                self.reproducible = false;
                let n = Notification::new(
                    "host.not-reproducible",
                    Severity::Error,
                    Scope::actor(id),
                    Ends::Run,
                )
                .with("reason", Value::Enum(reason.into()))
                .with("text", Value::Text(text))
                .at(at(instant));
                self.notify(n);
                self.drop_instance(id);
                self.end(EndReason::HostFault);
            }
        }
    }

    fn drop_instance(&mut self, id: Identity) {
        if let Some(inst) = self.actors.get_mut(&id).and_then(|s| s.instance.take()) {
            self.engine.drop_instance(inst);
        }
        if let Some(s) = self.actors.get_mut(&id) {
            s.funcs.clear();
        }
    }

    fn faulted(
        &mut self,
        id: Identity,
        kind: FaultKind,
        parameter: u32,
        metered: u64,
        instant: Instant,
    ) {
        // RFC 003 rule 13: the instance is discarded; the actor handles nothing further.
        self.drop_instance(id);
        self.world.dead.insert(id);
        let policy = self.options.fault_policy;
        let n = Notification::new(
            "actor.faulted",
            Severity::Error,
            Scope::actor(id),
            Ends::Activation,
        )
        .with("actor", Value::Identity(id))
        .with("fault", Value::Code(fault_code(kind).into()))
        .with("metered", Value::Count(metered))
        .with("policy", Value::Enum(policy.as_str().into()))
        .with("parameter", Value::Count(parameter as u64))
        .at(at(instant));
        self.notify(n);
        if policy == FaultPolicy::StopRun {
            self.end(EndReason::User);
        }
    }

    /// Export an actor's state between activations (RFC 003 rule 21).
    pub fn export_actor(&mut self, id: Identity) -> Result<ActorState, HostError> {
        let slot = self.actors.get_mut(&id).ok_or(HostError::NoInstance(id))?;
        let inst = slot.instance.as_mut().ok_or(HostError::NoInstance(id))?;
        let size = self.engine.memory_size(inst);
        let mut memory = Vec::new();
        let mut err = None;
        self.engine
            .with_memory(inst, &mut |m| match m.read(0, size) {
                Ok(b) => memory = b,
                Err(e) => err = Some(e),
            })?;
        if let Some(e) = err {
            return Err(HostError::Engine(EngineError::Memory(e.to_string())));
        }
        let mut globals = Vec::new();
        for (name, kind) in &slot.admitted.layout.globals {
            let v = self
                .engine
                .global_get(inst, None, name)
                .ok_or_else(|| EngineError::Global(name.clone()))?;
            let bits = match v {
                GlobalValue::I32(x) => x as u32 as u64,
                GlobalValue::I64(x) => x as u64,
                GlobalValue::F32(b) => b as u64,
                GlobalValue::F64(b) => b,
            };
            globals.push((name.clone(), *kind, bits));
        }
        let mut tables = Vec::new();
        for (name, size) in &slot.admitted.layout.tables {
            let found = self
                .engine
                .table_size(inst, name)
                .ok_or_else(|| EngineError::Export(name.clone()))?;
            if u64::from(found) != *size {
                return Err(HostError::Malformed("table size"));
            }
            tables.push((name.clone(), *size));
        }
        let kernel = self
            .kernel
            .actor(id)
            .cloned()
            .ok_or(HostError::NoInstance(id))?;
        Ok(ActorState {
            memory,
            globals,
            tables,
            kernel,
        })
    }

    /// Restore an actor into a fresh instance (RFC 003 rule 22): grow the memory, write the bytes,
    /// set the globals, verify the tables, restore the kernel's state. Host-owned globals reset.
    pub fn restore_actor(&mut self, id: Identity, state: &ActorState) -> Result<(), HostError> {
        let limit = self
            .actors
            .get(&id)
            .ok_or(HostError::NoInstance(id))?
            .limits
            .memory_bytes;
        // RFC 006 section 7: a checkpoint's memory is at most the actor's memory limit.
        if state.memory.len() as u64 > limit {
            return Err(HostError::Malformed("memory above the actor's limit"));
        }
        self.drop_instance(id);
        self.instantiate(id)?;
        let slot = self.actors.get_mut(&id).expect("checked");
        let inst = slot.instance.as_mut().expect("instantiated");
        let size = u32::try_from(state.memory.len()).map_err(|_| HostError::Malformed("memory"))?;
        self.engine.memory_grow_to(inst, size)?;
        let mut err = None;
        self.engine
            .with_memory(inst, &mut |m| err = m.write(0, &state.memory).err())?;
        if let Some(e) = err {
            return Err(HostError::Engine(EngineError::Memory(e.to_string())));
        }
        for (name, kind, bits) in &state.globals {
            let v = match kind {
                florn_admission::GlobalKind::I32 => GlobalValue::I32(*bits as u32 as i32),
                florn_admission::GlobalKind::I64 => GlobalValue::I64(*bits as i64),
                florn_admission::GlobalKind::F32 => GlobalValue::F32(*bits as u32),
                florn_admission::GlobalKind::F64 => GlobalValue::F64(*bits),
            };
            self.engine.global_set(inst, None, name, v)?;
        }
        for (name, size) in &state.tables {
            let found = self
                .engine
                .table_size(inst, name)
                .ok_or_else(|| EngineError::Export(name.clone()))?;
            if u64::from(found) != *size {
                return Err(HostError::Malformed("table size"));
            }
        }
        self.engine
            .global_set(inst, None, STACK_EXPORT, GlobalValue::I32(0))?;
        self.kernel.restore_actor(id, state.kernel.clone());
        Ok(())
    }

    /// Ask for a checkpoint (RFC 009 rule 2): taken now when the kernel stands at a boundary,
    /// else at the next one the run reaches. Returns whether it was taken now.
    pub fn request_checkpoint(&mut self) -> bool {
        self.checkpoint_requested = true;
        self.checkpoint_if_due()
    }

    /// A checkpoint when one is due at this boundary: requested, or the interval of rule 2
    /// since the Host's last. Returns whether one was taken.
    fn checkpoint_if_due(&mut self) -> bool {
        if self.detail {
            return false;
        }
        let interval = self.options.checkpoint_interval;
        let due_by_interval = interval > 0
            && self
                .kernel
                .handled()
                .saturating_sub(self.last_checkpoint_handled)
                >= interval;
        if !(self.checkpoint_requested || due_by_interval) || !self.kernel.at_boundary() {
            return false;
        }
        match self.checkpoint() {
            Ok(c) => {
                self.checkpoint_requested = false;
                self.last_checkpoint_handled = self.kernel.handled();
                self.keep(c);
                true
            }
            Err(e) => {
                // An actor whose state cannot be read is a Host fault in the log; the run
                // goes on, and the request stands for the next boundary.
                let n = Notification::new(
                    "host.instance-failed",
                    Severity::Error,
                    Scope::run(),
                    Ends::Nothing,
                )
                .with("actor", Value::Identity(0))
                .with("text", Value::Text(format!("checkpoint: {e}")))
                .at(at(self.kernel.now()));
                self.notify(n);
                false
            }
        }
    }

    /// Keep a checkpoint within the budget (RFC 009 rule 3): when the new one does not fit, every
    /// second checkpoint among those older than the newest eight is dropped, oldest first, until
    /// it does; the newest eight are never dropped by the budget.
    fn keep(&mut self, checkpoint: Checkpoint) {
        let bytes = size_of_checkpoint(&checkpoint);
        let k = Kept {
            instant: checkpoint.kernel.now,
            records: checkpoint.kernel.trace.records,
            handled: checkpoint.kernel.handled,
            bytes,
            checkpoint,
        };
        self.kept.push(k);
        let budget = self.options.checkpoint_memory;
        let total = |kept: &[Kept]| kept.iter().map(|k| k.bytes).sum::<u64>();
        let mut dropped = 0u64;
        while total(&self.kept) > budget {
            let old = self
                .kept
                .len()
                .saturating_sub(Options::CHECKPOINTS_ALWAYS_KEPT);
            if old <= 1 {
                break;
            }
            // The even positions of the old part but the start, oldest first: half of it in one
            // pass. The start (rule 2) is kept whatever the budget: moving back always has it.
            let mut i = 0;
            let mut keep = Vec::with_capacity(self.kept.len());
            for (n, k) in self.kept.drain(..).enumerate() {
                if n > 0 && n < old && n % 2 == 0 {
                    dropped += 1;
                    i += 1;
                } else {
                    keep.push(k);
                }
            }
            self.kept = keep;
            if i == 0 {
                break;
            }
        }
        let n = Notification::new(
            "checkpoint.taken",
            Severity::Info,
            Scope::run(),
            Ends::Nothing,
        )
        .with("instant", Value::Tick(self.kernel.now().tick))
        .with("bytes", Value::Bytes(bytes))
        .with("kept", Value::Count(self.kept.len() as u64))
        .at(at(self.kernel.now()));
        self.notify(n);
        if dropped > 0 {
            let n = Notification::new(
                "checkpoint.budget",
                Severity::Info,
                Scope::run(),
                Ends::Nothing,
            )
            .with("kept", Value::Count(self.kept.len() as u64))
            .with("dropped", Value::Count(dropped))
            .with("bytes", Value::Bytes(total(&self.kept)))
            .at(at(self.kernel.now()));
            self.notify(n);
        }
    }

    /// The kept checkpoints, oldest first; the first is the start (rule 2).
    pub fn kept(&self) -> &[Kept] {
        &self.kept
    }

    /// The start as a kept checkpoint (rule 2): the instances before `init`, so that a restore of
    /// it runs `init` again and the run begins as a fresh one, records and all; what moving back
    /// to the beginning restores. Never dropped by the budget; not said in the log.
    fn keep_start(&mut self) {
        if let Ok(c) = self.checkpoint() {
            let bytes = size_of_checkpoint(&c);
            self.kept.insert(
                0,
                Kept {
                    instant: c.kernel.now,
                    records: c.kernel.trace.records,
                    handled: c.kernel.handled,
                    bytes,
                    checkpoint: c,
                },
            );
            self.last_checkpoint_handled = 0;
        }
    }

    /// Move to an earlier tick (RFC 001 rule 53; RFC 009 rule 7): restore the nearest kept
    /// checkpoint at or before it, then execute forward to it. The interventions after it leave
    /// the inputs and are said (`run.rewound`); a replay re-applies them at their instants.
    pub fn go_back(&mut self, to: u64) -> Result<(), HostError> {
        let index = self
            .kept
            .iter()
            .rposition(|k| k.instant.tick <= to)
            .ok_or(HostError::Malformed("no checkpoint at or before the tick"))?;
        self.restore_kept(index)?;
        if self.kernel.now().tick < to {
            self.advance(Advance::ToTick(to));
        }
        Ok(())
    }

    /// The records of a window (RFC 001 rule 55; RFC 009 rule 9): from the kept records when
    /// they hold it, else by re-execution through this run's own instances: the present is
    /// checkpointed, the nearest kept checkpoint at or before the window is restored with
    /// recording on, the run executes to the window's end, and the present is restored. The
    /// run's instant, inputs, trace and kept set are as before; nothing of the way is logged,
    /// kept or displayed. Needs a boundary (RFC 001 rule 51); mid-instant, the kept records
    /// alone answer.
    pub fn trace_window(&mut self, from: u64, count: u32) -> Vec<florn_kernel::Record> {
        let end = from.saturating_add(u64::from(count));
        let from_kept =
            |records: &[florn_kernel::Record], first: u64| -> Vec<florn_kernel::Record> {
                let lo = from.saturating_sub(first) as usize;
                let hi = end.saturating_sub(first) as usize;
                records
                    .get(lo.min(records.len())..hi.min(records.len()))
                    .map(|r| r.to_vec())
                    .unwrap_or_default()
            };
        // The kept records cover the window: the Host keeps them from the start, or from the
        // last checkpoint (the browser), so the first kept record's index is known.
        let total = self.kernel.trace.records();
        if let Some(records) = self.kernel.trace.kept_records() {
            let first = total.saturating_sub(records.len() as u64);
            if from >= first {
                return from_kept(records, first);
            }
        }
        if !self.kernel.at_boundary() {
            return self
                .kernel
                .trace
                .kept_records()
                .map(|r| from_kept(r, total.saturating_sub(r.len() as u64)))
                .unwrap_or_default();
        }
        let Some(index) = self.kept.iter().rposition(|k| k.records <= from) else {
            return Vec::new();
        };
        let Ok(present) = self.checkpoint() else {
            return Vec::new();
        };
        let own_records = self.kernel.trace.kept_records().map(|r| r.to_vec());
        // The present's own pending replay list, as it was: the detail run's leftover is not it.
        let own_replay = self.kernel.replay_pending().to_vec();
        let ended = self.ended;
        let reached = self.reached;
        self.detail = true;
        let start = self.kept[index].checkpoint.clone();
        let mut out = Vec::new();
        if self.restore_quietly(&start).is_ok() {
            while self.ended.is_none() && self.kernel.trace.records() < end {
                if self.advance(Advance::OneInstant) == 0 && !self.kernel.has_next() {
                    break;
                }
            }
            let first = self.kept[index].records;
            if let Some(records) = self.kernel.trace.kept_records() {
                out = from_kept(records, first);
            }
        }
        let _ = self.restore_quietly(&present);
        self.kernel.set_replay(own_replay);
        // The Host's own kept records, as they were; none when it kept none.
        match own_records {
            Some(records) => self.kernel.trace.set_kept(records),
            None => self.kernel.trace.stop_recording(),
        }
        self.ended = ended;
        self.reached = reached;
        self.detail = false;
        out
    }

    /// A restore for detail by replay (rule 9): the undone interventions are re-applied at their
    /// instants whatever the run, nothing is said, and the kept set is untouched.
    fn restore_quietly(&mut self, checkpoint: &Checkpoint) -> Result<(), HostError> {
        self.restore_with(checkpoint, true)
    }

    /// Restore a kept checkpoint in this run (RFC 009 rules 5 and 7).
    pub fn restore_kept(&mut self, index: usize) -> Result<(), HostError> {
        if index >= self.kept.len() {
            return Err(HostError::Malformed("no such checkpoint"));
        }
        let k = self.kept.remove(index);
        let result = self.restore_in_place(&k.checkpoint);
        // The restored one is kept again at its place; those after it belong to a future that is
        // executed again (rule 7).
        self.kept.truncate(index);
        self.kept.push(k);
        result
    }

    /// Continue this run from a checkpoint (RFC 009 rule 7): the identity is checked first
    /// (RFC 003 rule 22), then the kernel, the domains and every actor are restored, the trace
    /// and everything after the checkpoint's instant discarded, and the interventions applied
    /// after it leave the inputs (rule 8; a replay re-applies them at their instants).
    pub fn restore_in_place(&mut self, checkpoint: &Checkpoint) -> Result<(), HostError> {
        self.restore_with(checkpoint, false)
    }

    fn restore_with(&mut self, checkpoint: &Checkpoint, detail: bool) -> Result<(), HostError> {
        self.check_identity(&checkpoint.identity)?;
        let scenario_actors = self.actors.len() + self.builtins.len();
        if checkpoint.actors.len() > scenario_actors
            || checkpoint
                .actors
                .keys()
                .any(|id| !self.actors.contains_key(id))
        {
            return Err(self.mismatch(
                "actors",
                checkpoint.actors.len().to_string(),
                scenario_actors.to_string(),
            ));
        }
        let applied_before = self.kernel.applied_interventions().to_vec();
        let undone: Vec<AppliedIntervention> = applied_before
            .get(checkpoint.kernel.applied_interventions.len()..)
            .map(|s| s.to_vec())
            .unwrap_or_default();
        let replay_pending = self.kernel.replay_pending().to_vec();
        let watched = self.kernel.trace.watched().to_vec();
        let kept_records = self
            .kernel
            .trace
            .kept_records()
            .map(|r| r.to_vec())
            .unwrap_or_default();
        let mut config = self.kernel.config().clone();
        // Detail by replay records from the restore on, the start's `init` included.
        config.keep_records |= detail;
        self.kernel = Kernel::restore(config, checkpoint.kernel.clone());
        self.kernel.trace.set_kept(kept_records);
        self.kernel.trace.watch(watched);
        if !self.world.restore(&checkpoint.domains) {
            return Err(HostError::Malformed("domain state"));
        }
        self.world.restore_origin_seqs(&checkpoint.origins);
        self.world.dead.clear();
        for (id, a) in &checkpoint.kernel.actors {
            if !a.alive {
                self.world.dead.insert(*id);
            }
        }
        for (id, state) in &checkpoint.actors {
            self.restore_actor(*id, state)?;
        }
        if self.replay_against.is_some() || detail {
            // A replay, or detail by replay: what was undone is still to come at its instant
            // (RFC 001 rule 36).
            let mut again = undone;
            again.extend(replay_pending);
            self.kernel.set_replay(again);
        } else if !undone.is_empty() {
            let n = Notification::new("run.rewound", Severity::Info, Scope::run(), Ends::Nothing)
                .with("to", Value::Tick(checkpoint.kernel.now.tick))
                .with("undone", Value::Count(undone.len() as u64))
                .at(at(self.kernel.now()));
            self.notify(n);
        }
        self.ended = None;
        self.reached = checkpoint.kernel.now.tick;
        // The start (rule 2) is the run before `init`: initialised again, as a fresh run is.
        if checkpoint.kernel.handled == 0 && checkpoint.kernel.trace.records == 0 {
            self.init_all();
        }
        self.telemetry.clear();
        if let Some(a) = &mut self.activity {
            a.clear();
        }
        self.world.take_display();
        self.last_checkpoint_handled = checkpoint.kernel.handled;
        self.checkpoint_requested = false;
        Ok(())
    }

    /// The identity check of RFC 003 rule 22 (DATA-005), field by field.
    fn check_identity(&mut self, id: &CheckpointIdentity) -> Result<(), HostError> {
        let hex = florn_model::canonical::hex;
        if id.scenario_sha256 != self.inputs.scenario_sha256 {
            return Err(self.mismatch(
                "scenario-hash",
                hex(&id.scenario_sha256),
                hex(&self.inputs.scenario_sha256),
            ));
        }
        for (name, h) in &id.modules {
            let found = self.inputs.modules.iter().find(|m| &m.name == name);
            match found {
                Some(m) if m.source_sha256 == *h => {}
                Some(m) => {
                    return Err(self.mismatch("module-hash", hex(h), hex(&m.source_sha256)));
                }
                None => return Err(self.mismatch("module-hash", hex(h), String::new())),
            }
        }
        if id.admission_version != ADMISSION_VERSION {
            return Err(self.mismatch(
                "admission-version",
                id.admission_version.to_string(),
                ADMISSION_VERSION.to_string(),
            ));
        }
        if id.cost_model_version != self.inputs.cost_model_version {
            return Err(self.mismatch(
                "cost-model",
                id.cost_model_version.to_string(),
                self.inputs.cost_model_version.to_string(),
            ));
        }
        Ok(())
    }

    /// A checkpoint at the boundary (RFC 001 rule 51): the kernel and every actor.
    pub fn checkpoint(&mut self) -> Result<Checkpoint, HostError> {
        let ids: Vec<Identity> = self.actors.keys().copied().collect();
        let mut actors = BTreeMap::new();
        for id in ids {
            if self.actors[&id].instance.is_some() {
                actors.insert(id, self.export_actor(id)?);
            }
        }
        Ok(Checkpoint {
            identity: CheckpointIdentity {
                scenario_sha256: self.inputs.scenario_sha256,
                modules: self
                    .inputs
                    .modules
                    .iter()
                    .map(|m| (m.name.clone(), m.source_sha256))
                    .collect(),
                admission_version: ADMISSION_VERSION,
                cost_model_version: self.inputs.cost_model_version,
                root_seed: self.inputs.root_seed,
            },
            kernel: self.kernel.snapshot(),
            actors,
            origins: self.world.origin_seqs(),
            domains: self.world.export(),
        })
    }

    /// A run continuing from a checkpoint (RFC 001 rule 52; RFC 003 rule 22): the inputs are
    /// checked first (DATA-005), then every actor is restored and the kernel continues.
    // The Host's parts and the run's inputs, each named: a struct would only move the list.
    #[allow(clippy::too_many_arguments)]
    pub fn from_checkpoint(
        engine: E,
        admission: Admission,
        platform: &mut dyn Platform,
        domains: &Domains,
        scenario: Manifest,
        modules: &BTreeMap<String, Vec<u8>>,
        checkpoint: &Checkpoint,
        options: Options,
    ) -> Result<Self, HostError> {
        // The root seed and the cost-model version are inputs the checkpoint carries (RFC 001
        // rule 40, items 3 and 5): the run admits under them, or names the version it lacks.
        let seed = checkpoint.identity.root_seed;
        let wanted = checkpoint.identity.cost_model_version;
        let mut options = options;
        let known = cost_table(wanted).is_some();
        if known {
            options.cost_model = wanted;
        }
        let mut run = Self::prepare(
            engine, admission, platform, domains, scenario, modules, seed, options,
        )
        .map_err(|parts| parts.0)?;
        if !known {
            return Err(run.mismatch("cost-model", wanted.to_string(), "none".to_string()));
        }
        run.check_identity(&checkpoint.identity)?;
        if checkpoint
            .actors
            .keys()
            .any(|id| !run.actors.contains_key(id))
        {
            return Err(run.mismatch(
                "actors",
                checkpoint.actors.len().to_string(),
                run.actors.len().to_string(),
            ));
        }
        let config = run.kernel.config().clone();
        run.kernel = Kernel::restore(config, checkpoint.kernel.clone());
        if !run.world.restore(&checkpoint.domains) {
            return Err(HostError::Malformed("domain state"));
        }
        run.world.restore_origin_seqs(&checkpoint.origins);
        for (id, state) in &checkpoint.actors {
            run.restore_actor(*id, state)?;
        }
        // The start (RFC 009 rule 2) is the run before `init`: initialised, as a fresh run is.
        if checkpoint.kernel.handled == 0 && checkpoint.kernel.trace.records == 0 {
            run.init_all();
        }
        // The built-in actors' kernel state is in the kernel's snapshot; the dead ones too.
        for (id, a) in &checkpoint.kernel.actors {
            if !a.alive {
                run.world.dead.insert(*id);
            }
        }
        Ok(run)
    }

    fn mismatch(&mut self, what: &'static str, expected: String, found: String) -> HostError {
        let n = Notification::new(
            "checkpoint.mismatch",
            Severity::Error,
            Scope::run(),
            Ends::Command,
        )
        .with("what", Value::Enum(what.into()))
        .with("expected", Value::Text(expected.clone()))
        .with("found", Value::Text(found.clone()));
        self.notify(n);
        HostError::CheckpointMismatch {
            what,
            expected,
            found,
        }
    }

    /// A replay (RFC 001 rules 49 and 50; RFC 005 rule 26): a new run from the record's inputs,
    /// the recorded interventions applied at their instants, compared at the end with the stored
    /// chain. The scenario and modules must be the record's (`checkpoint.mismatch` otherwise).
    // The Host's parts and the run's inputs, each named: a struct would only move the list.
    #[allow(clippy::too_many_arguments)]
    pub fn replay(
        engine: E,
        admission: Admission,
        platform: &mut dyn Platform,
        domains: &Domains,
        scenario: Manifest,
        modules: &BTreeMap<String, Vec<u8>>,
        record: &RunToml,
        mut options: Options,
    ) -> Result<Self, HostError> {
        options.chain_interval = record.chain_interval;
        let seed = record.inputs.root_seed;
        let wanted = record.inputs.cost_model_version;
        let known = cost_table(wanted).is_some();
        if known {
            options.cost_model = wanted;
        }
        let mut run = Self::prepare(
            engine, admission, platform, domains, scenario, modules, seed, options,
        )
        .map_err(|parts| parts.0)?;
        if !known {
            return Err(run.mismatch("cost-model", wanted.to_string(), "none".to_string()));
        }
        let hex = florn_model::canonical::hex;
        let i = &record.inputs;
        if i.scenario_sha256 != run.inputs.scenario_sha256 {
            return Err(run.mismatch(
                "scenario-hash",
                hex(&i.scenario_sha256),
                hex(&run.inputs.scenario_sha256),
            ));
        }
        for m in &i.modules {
            let found = run.inputs.modules.iter().find(|x| x.name == m.name);
            match found {
                Some(x) if x.source_sha256 == m.source_sha256 => {}
                Some(x) => {
                    return Err(run.mismatch(
                        "module-hash",
                        hex(&m.source_sha256),
                        hex(&x.source_sha256),
                    ));
                }
                None => {
                    return Err(run.mismatch("module-hash", hex(&m.source_sha256), String::new()));
                }
            }
        }
        if i.admission_version != ADMISSION_VERSION {
            return Err(run.mismatch(
                "admission-version",
                i.admission_version.to_string(),
                ADMISSION_VERSION.to_string(),
            ));
        }
        if i.cost_model_version != run.inputs.cost_model_version {
            return Err(run.mismatch(
                "cost-model",
                i.cost_model_version.to_string(),
                run.inputs.cost_model_version.to_string(),
            ));
        }
        run.kernel.set_replay(i.interventions.clone());
        // The record's points are stored by this run too, so that a record exported before
        // its run ended compares at every point it holds (RFC 001 rule 50).
        run.kernel
            .trace
            .watch(record.chain.iter().map(|(n, _)| *n).collect());
        run.replay_against = Some(record.chain.clone());
        // A branch's provenance travels with its replay (RFC 009 rule 12: compared by nothing).
        run.parent = record.parent.clone();
        run.instantiate_all()?;
        run.keep_start();
        run.init_all();
        Ok(run)
    }

    /// The comparison of a replay with its record, once the run has ended: at every point the
    /// record holds (RFC 001 rule 50; a record exported before its run ended, or a branch's
    /// parent, holds fewer than the scenario's end), as `replay.matched` says it.
    pub fn comparison(&self) -> Option<Comparison> {
        let stored = self.replay_against.as_ref()?;
        self.ended?;
        Some(florn_kernel::trace::compare_reference(
            &self.kernel.trace.stored(),
            stored,
        ))
    }

    /// `run.toml` of the run record (RFC 005 rule 29).
    pub fn run_toml(&self) -> RunToml {
        let mut inputs = self.inputs.clone();
        inputs.interventions = self.kernel.applied_interventions().to_vec();
        RunToml {
            environment: self.environment.clone(),
            inputs,
            parent: self.parent.clone(),
            chain: self.kernel.trace.stored(),
            chain_interval: self.kernel.trace.interval(),
            records: self.kernel.trace.records(),
            ended: self.ended,
            reproducible: self.reproducible,
            run_id: self.run_id.clone(),
            created_ms: self.created_ms,
        }
    }

    /// The parent of a branch (RFC 009 rule 12), when this run is one.
    pub fn parent(&self) -> Option<&crate::identity::Parent> {
        self.parent.as_ref()
    }

    /// A branch (RFC 001 rule 54; RFC 009 rules 10 to 12): this run ends at the current instant
    /// `T`, at a boundary, and becomes a new run from the checkpoint there, with the inputs so
    /// far as its own and an identity naming the parent. The parent's record is returned for
    /// keeping and export (rule 11). The branch begins at `T`: the checkpoint there is its start,
    /// kept whatever the budget, and the parent's earlier ones leave with the parent; the point
    /// `T` is stored in the branch's own chain, so that its replay compares there too.
    pub fn branch(&mut self, platform: &mut dyn Platform) -> Result<Contents, HostError> {
        if self.detail || !self.kernel.at_boundary() {
            return Err(HostError::MidInstant);
        }
        self.kernel.trace.mark();
        let c = self.checkpoint()?;
        let start = Kept {
            instant: c.kernel.now,
            records: c.kernel.trace.records,
            handled: c.kernel.handled,
            bytes: size_of_checkpoint(&c),
            checkpoint: c,
        };
        // The parent ends here (rule 11); its record is the Host's to offer.
        self.end(EndReason::User);
        let parent_record = Contents {
            run: Some(self.run_toml()),
            trace: self.records().map(|r| r.to_vec()),
            payloads: None,
            log: self.log.clone(),
            readme: None,
        };
        let parent = crate::identity::Parent {
            chain: self.chain(),
            records: self.kernel.trace.records(),
            instant: self.kernel.now(),
        };
        let parent_id = std::mem::take(&mut self.run_id);
        // The branch: its own identity and log; the parent's inputs after `T` are nobody's
        // (rule 10), so a replay's remaining interventions and watched points leave.
        self.kernel.set_replay(Vec::new());
        self.kernel.trace.watch(Vec::new());
        self.replay_against = None;
        self.ended = None;
        self.created_ms = platform.wall_clock_ms();
        self.wall_ms = self.created_ms;
        self.run_id = format!(
            "{:x}-{:x}-{:x}",
            self.created_ms, self.inputs.root_seed, parent.records
        );
        self.kept = vec![start];
        self.last_checkpoint_handled = self.kernel.handled();
        self.checkpoint_requested = false;
        self.log = DiagnosticLog::default();
        self.notify(
            Notification::new("host.capacity", Severity::Info, Scope::run(), Ends::Nothing)
                .with("instances", Value::Count(self.capacity.instances))
                .with("memory", Value::Bytes(self.capacity.memory_bytes)),
        );
        let n = Notification::new("run.branched", Severity::Info, Scope::run(), Ends::Nothing)
            .with("instant", Value::Tick(parent.instant.tick))
            .with("parent", Value::Text(parent_id))
            .at(at(parent.instant));
        self.notify(n);
        self.parent = Some(parent);
        Ok(parent_record)
    }

    /// The kept trace records, when `keep_records` is on.
    pub fn records(&self) -> Option<&[florn_kernel::Record]> {
        self.kernel.trace.kept_records()
    }

    /// The per-actor chain from kept records (RFC 005 rule 24).
    pub fn actor_chain(&self, id: Identity) -> Option<Digest> {
        self.kernel.trace.actor_chain(id)
    }
}

/// The activation itself, over disjoint borrows of the run.
#[allow(clippy::too_many_arguments)]
fn activate_slot<E: Engine>(
    engine: &mut E,
    kernel: &mut Kernel,
    world: &mut Composite,
    slot: &mut Slot<E>,
    act: Activation,
    which: usize,
    vals: &[Val],
    keep_payloads: bool,
    payloads: &mut BTreeMap<Digest, Vec<u8>>,
    telemetry: &mut VecDeque<Telemetry>,
    telemetry_dropped: &mut u64,
    telemetry_capacity: usize,
    instant: Instant,
) -> Outcome {
    let actor = act.actor;
    let budget = slot.limits.budget;
    let stack = slot.limits.stack;
    let sigs = slot.world.clone();
    let tys: &[Ty] = &sigs.exports[which].1;
    let inst = slot.instance.as_mut().expect("checked by the caller");
    slot.sink.reset();
    // RFC 003 rule 11: the budget is in place before any guest code, the lowering's realloc included.
    if let Err(e) = engine.global_set(
        inst,
        Some(BUDGET_IMPORT_MODULE),
        BUDGET_IMPORT_NAME,
        GlobalValue::I64(budget as i64),
    ) {
        kernel.fault(act, FaultKind::Trap, 0, 0);
        return Outcome::HostFault {
            reason: "engine-stack",
            text: e.to_string(),
        };
    }
    let mut lowered: Result<Vec<Flat>, AbiError> = Ok(Vec::new());
    if let Err(e) = engine.with_memory(inst, &mut |m| lowered = lower_flat_params(m, tys, vals)) {
        kernel.fault(act, FaultKind::Trap, 0, 0);
        return Outcome::HostFault {
            reason: "engine-stack",
            text: e.to_string(),
        };
    }
    let read_globals = |engine: &mut E, inst: &mut E::Instance| -> (i64, i32) {
        let remaining =
            match engine.global_get(inst, Some(BUDGET_IMPORT_MODULE), BUDGET_IMPORT_NAME) {
                Some(GlobalValue::I64(r)) => r,
                _ => 0,
            };
        let height = match engine.global_get(inst, None, STACK_EXPORT) {
            Some(GlobalValue::I32(h)) => h,
            _ => 0,
        };
        (remaining, height)
    };
    let metered_of = |remaining: i64| -> u64 {
        if remaining < 0 {
            budget
        } else {
            budget.saturating_sub(remaining as u64)
        }
    };
    let flats = match lowered {
        Ok(f) => f,
        Err(e) => {
            // The guest's allocator ran under the budget: classify by the globals (rule 13).
            let (remaining, height) = read_globals(engine, inst);
            let metered = metered_of(remaining);
            let kind = match (classify(remaining, height, stack), &e) {
                (FaultClass::Budget, _) => FaultKind::Budget,
                (FaultClass::Stack, _) => FaultKind::Stack,
                (FaultClass::Trap, AbiError::Realloc { .. }) => FaultKind::Memory,
                (FaultClass::Trap, _) => FaultKind::Contract,
            };
            kernel.fault(act, kind, 0, metered);
            return Outcome::Faulted {
                kind,
                parameter: 0,
                metered,
            };
        }
    };
    let func = slot.funcs[which].clone();
    let result = {
        let mut handler = Handler {
            kernel,
            sink: &mut slot.sink,
            world,
            actor,
            sigs: &sigs,
            contract: None,
        };
        let r = engine.call(inst, &func, &flats, &mut handler);
        (r, handler.contract)
    };
    let (remaining, height) = read_globals(engine, inst);
    let metered = metered_of(remaining);
    match result {
        (Ok(_), None) => {
            // Collect what the display path and the payload store need before the kernel takes
            // the effects; they are kept only if the commit succeeds (RFC 002 rule 34).
            let mut reports = Vec::new();
            let mut kept = Vec::new();
            for e in slot.sink.effects() {
                match e {
                    florn_kernel::Effect::Report { channel, payload } => {
                        reports.push((*channel, payload.clone()));
                        if keep_payloads {
                            kept.push(payload.clone());
                        }
                    }
                    florn_kernel::Effect::Send { payload, .. } if keep_payloads => {
                        kept.push(payload.clone());
                    }
                    _ => {}
                }
            }
            match kernel.commit(act, &mut slot.sink, metered, world) {
                Ok(()) => {
                    let texts = reports
                        .iter()
                        .filter(|(channel, _)| *channel == 0)
                        .map(|(_, payload)| String::from_utf8_lossy(payload).into_owned())
                        .collect();
                    for (channel, payload) in reports {
                        if telemetry.len() >= telemetry_capacity {
                            telemetry.pop_front();
                            *telemetry_dropped += 1;
                        }
                        telemetry.push_back(Telemetry {
                            at: instant,
                            actor,
                            channel,
                            payload,
                        });
                    }
                    for p in kept {
                        payloads.entry(*blake3::hash(&p).as_bytes()).or_insert(p);
                    }
                    Outcome::Committed { texts }
                }
                Err(refusal) => {
                    let (kind, parameter) = match &refusal {
                        florn_kernel::CommitRefusal::DelayBeyondLargestTick { timer, .. } => {
                            (FaultKind::Contract, *timer)
                        }
                        florn_kernel::CommitRefusal::PendingTimers { .. } => (FaultKind::Quota, 6),
                        florn_kernel::CommitRefusal::Refused(r) => {
                            (FaultKind::from_refusal(r), fault_parameter(r))
                        }
                    };
                    Outcome::Faulted {
                        kind,
                        parameter,
                        metered,
                    }
                }
            }
        }
        (Ok(_), Some(_)) | (Err(_), Some(_)) => {
            // Lifting failed inside an import: the contract fault of RFC 002 rule 8.
            kernel.fault(act, FaultKind::Contract, 8, metered);
            Outcome::Faulted {
                kind: FaultKind::Contract,
                parameter: 8,
                metered,
            }
        }
        (Err(trap), None) => match trap.kind {
            TrapKind::HostRefused => {
                let (kind, parameter) = match slot.sink.refusal() {
                    Some(r) => (FaultKind::from_refusal(r), fault_parameter(r)),
                    None => (FaultKind::Contract, 0),
                };
                kernel.fault(act, kind, parameter, metered);
                Outcome::Faulted {
                    kind,
                    parameter,
                    metered,
                }
            }
            TrapKind::Guest => {
                let kind = match classify(remaining, height, stack) {
                    FaultClass::Budget => FaultKind::Budget,
                    FaultClass::Stack => FaultKind::Stack,
                    FaultClass::Trap => FaultKind::Trap,
                };
                kernel.fault(act, kind, 0, metered);
                Outcome::Faulted {
                    kind,
                    parameter: 0,
                    metered,
                }
            }
            TrapKind::EngineStack | TrapKind::Watchdog | TrapKind::Other => {
                kernel.fault(act, FaultKind::Trap, 0, metered);
                Outcome::HostFault {
                    reason: match trap.kind {
                        TrapKind::EngineStack => "engine-stack",
                        _ => "watchdog",
                    },
                    text: trap.message,
                }
            }
        },
    }
}
