//! A run (RFC 005 §4): admission and the estimate before anything exists, the instance pool, the
//! run loop of rule 15, activations with the budget in place before lowering (RFC 003 rule 11),
//! classification by globals (rule 13), quotas in the import handlers (rule 17), export and
//! restore of actor state (RFC 003 §6), checkpoints, the diagnostic log, replay.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use florn_abi::core_world;
use florn_abi::{AbiError, Flat, Memory, Ty, Val, lift_flat_params, lower_flat_params};
use florn_admission::config::{NoUnits, encode_config};
use florn_admission::estimate::{Capacity as EstimateCapacity, ModuleFacts, estimate};
use florn_admission::{
    ADMISSION_VERSION, Admission, Admitted, BUDGET_IMPORT_MODULE, BUDGET_IMPORT_NAME,
    COST_MODEL_V1_TABLE, COST_MODEL_VERSION, ClassLimits, Errors, FaultClass, STACK_EXPORT,
    classify,
};
use florn_control::{At, Ends, Notification, Scope, Severity, Value};
use florn_engine::{
    Engine, EngineError, GlobalValue, HostGlobal, ImportFunction, ImportHandler, ImportSpec,
    InstanceId, Refusal, TrapKind,
};
use florn_kernel::trace::Comparison;
use florn_kernel::{
    Activation, Advance, Config, Digest, Event, EventKey, EventKind, FaultKind, Identity, Instant,
    Kernel, Quotas, Sink, SinkRefusal, TiePolicy,
};
use florn_model::manifest::ActorKind;
use florn_model::{HostMaxima, Manifest};
use florn_platform::{Capacity, Platform};

use crate::identity::{EndReason, Environment, ModuleInput, RunInputs, RunToml};
use crate::log::DiagnosticLog;
use crate::snapshot::{ActorState, Checkpoint, CheckpointIdentity};
use crate::world::{CLASS_TIMER, CoreWorld, Intervention};

/// The SHA-256 of module bytes as hexadecimal: the module identity (RFC 004 rule 16).
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
    /// The provisional defaults of RFC 003 rule 19 for limits a scenario leaves unset.
    pub class_defaults: ClassLimits,
    pub host: HostMaxima,
    /// `native`, `browser`
    pub host_build: String,
    /// Telemetry records kept for the display path before the oldest are dropped.
    pub telemetry_capacity: usize,
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
        }
    }
}

impl Options {
    /// The browser Host's options: its maxima (RFC 003 rule 19) and build name.
    pub fn browser() -> Self {
        Self {
            host: HostMaxima::browser(),
            host_build: "browser".into(),
            ..Self::default()
        }
    }
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
    Engine(EngineError),
    /// The actor is not module-backed, or has no instance.
    NoInstance(Identity),
    /// The run has ended.
    Ended,
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
            HostError::Engine(e) => write!(f, "{e}"),
            HostError::NoInstance(id) => write!(f, "actor {id} has no instance"),
            HostError::Ended => write!(f, "the run has ended"),
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

/// The exports of the core world, by index.
const INIT: usize = 0;
const ON_MESSAGE: usize = 1;
const ON_TIMER: usize = 2;

/// A module-backed actor in the pool.
struct Slot<E: Engine> {
    instance: Option<E::Instance>,
    funcs: Vec<E::Func>,
    sink: Sink,
    admitted: Arc<Admitted>,
    module_name: String,
    limits: ClassLimits,
    /// The lowered port list for `init`.
    ports: Val,
    config: Vec<u8>,
    spec: ImportSpec,
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

/// The Host's side of the imports for one activation (RFC 003 rule 17; RFC 002 §5).
struct Handler<'a> {
    kernel: &'a mut Kernel,
    sink: &'a mut Sink,
    actor: Identity,
    sigs: &'a [(&'static str, Vec<Ty>)],
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
        let Some((_, tys)) = self.sigs.iter().find(|(n, _)| *n == function) else {
            return Err(Refusal);
        };
        let vals = match lift_flat_params(memory, tys, args) {
            Ok(v) => v,
            Err(e) => {
                self.contract = Some(e);
                return Err(Refusal);
            }
        };
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

pub struct Run<E: Engine> {
    engine: E,
    admission: Admission,
    scenario: Manifest,
    kernel: Kernel,
    world: CoreWorld,
    actors: BTreeMap<Identity, Slot<E>>,
    inputs: RunInputs,
    environment: Environment,
    run_id: String,
    created_ms: u64,
    wall_ms: u64,
    log: DiagnosticLog,
    telemetry: VecDeque<Telemetry>,
    telemetry_dropped: u64,
    payloads: BTreeMap<Digest, Vec<u8>>,
    options: Options,
    ended: Option<EndReason>,
    reproducible: bool,
    replay_against: Option<Vec<(u64, Digest)>>,
    sigs: Vec<(&'static str, Vec<Ty>)>,
    export_tys: Vec<Vec<Ty>>,
    capacity: Capacity,
}

impl<E: Engine> Run<E> {
    /// A new run (RFC 005 rule 26, `new-run`): admit every module, estimate, instantiate the pool,
    /// initialise every actor in ascending identity order (RFC 002 rule 36). Nothing is
    /// instantiated before the estimate passes (RFC 003 rule 4).
    pub fn new(
        engine: E,
        admission: Admission,
        platform: &mut dyn Platform,
        scenario: Manifest,
        modules: &BTreeMap<String, Vec<u8>>,
        seed: Seed,
        options: Options,
    ) -> Result<Self, HostError> {
        let seed = match seed {
            Seed::Given(s) => s,
            Seed::Choose => platform.new_seed(),
        };
        let mut run = Self::prepare(
            engine, admission, platform, scenario, modules, seed, options,
        )?;
        run.instantiate_all()?;
        run.init_all();
        Ok(run)
    }

    /// Everything before instances exist: admission, the estimate, the kernel, the log's first
    /// entries.
    fn prepare(
        engine: E,
        mut admission: Admission,
        platform: &mut dyn Platform,
        scenario: Manifest,
        modules: &BTreeMap<String, Vec<u8>>,
        seed: u64,
        options: Options,
    ) -> Result<Self, HostError> {
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
            match admission.admit(name, bytes, &limits) {
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
            return Err(errors.into());
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
                return Err(e.into());
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
        let world = CoreWorld::from_scenario(&scenario);
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
            cost_model_version: COST_MODEL_VERSION,
            cost_table: COST_MODEL_V1_TABLE.to_string(),
            contracts,
            root_seed: seed,
            interventions: Vec::new(),
            generator: florn_kernel::GENERATOR_NAME.to_string(),
            derivation: florn_kernel::DERIVATION_NAME.to_string(),
            digest: florn_kernel::DIGEST_NAME.to_string(),
        };
        let sigs: Vec<(&'static str, Vec<Ty>)> = core_world::host_functions()
            .iter()
            .map(|s| (s.name, s.param_types()))
            .collect();
        let export_tys: Vec<Vec<Ty>> = core_world::actor_functions()
            .iter()
            .map(|s| s.param_types())
            .collect();
        let mut run = Self {
            engine,
            admission,
            scenario,
            kernel,
            world,
            actors: BTreeMap::new(),
            inputs,
            environment,
            run_id,
            created_ms,
            wall_ms: created_ms,
            log,
            telemetry: VecDeque::new(),
            telemetry_dropped: 0,
            payloads: BTreeMap::new(),
            options,
            ended: None,
            reproducible: true,
            replay_against: None,
            sigs,
            export_tys,
            capacity,
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
            let config = match encode_config(*id, &a.manifest, &actor.config, tick, &NoUnits) {
                Ok(c) => c,
                Err(e) => {
                    errors.0.extend(e.0);
                    continue;
                }
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
                host_module: a.world.imports[0].module.clone(),
                functions: a
                    .world
                    .imports
                    .iter()
                    .flat_map(|i| i.functions.iter())
                    .map(|f| ImportFunction {
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
            return Err(errors.into());
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
                    EngineError::Resource(text) => Notification::new(
                        "host.instance-failed",
                        Severity::Error,
                        Scope::actor(id),
                        Ends::Run,
                    )
                    .with("actor", Value::Identity(id))
                    .with("text", Value::Text(text.clone())),
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
        let names = [
            core_world::EXPORT_INIT,
            core_world::EXPORT_ON_MESSAGE,
            core_world::EXPORT_ON_TIMER,
        ];
        let mut funcs = Vec::with_capacity(3);
        for n in names {
            funcs.push(
                self.engine
                    .func(&inst, n)
                    .ok_or_else(|| EngineError::Export(n.to_string()))?,
            );
        }
        slot.instance = Some(inst);
        slot.funcs = funcs;
        Ok(())
    }

    /// `init` for every actor, in ascending identity order (RFC 002 rule 36).
    fn init_all(&mut self) {
        let ids: Vec<Identity> = self.actors.keys().copied().collect();
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
        self.log.push(self.wall_ms, &self.run_id, n);
    }

    /// The platform's wall clock, for the log's metadata; call between advances.
    pub fn set_wall_clock(&mut self, ms: u64) {
        self.wall_ms = ms;
    }

    pub fn scenario(&self) -> &Manifest {
        &self.scenario
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

    /// Request an intervention (RFC 001 rules 33 to 38): applied at the next boundary, recorded
    /// as an input.
    pub fn intervene(&mut self, at_tick: Option<u64>, intervention: &Intervention) {
        self.kernel.intervene(at_tick, intervention.encode());
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
            let c = self.kernel.trace.compare(&stored);
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
        match &ev.kind {
            EventKind::Timer { timer } => {
                let vals = vec![Val::U64(now), Val::U32(*timer)];
                self.activate(ev.target, ev.clone(), ON_TIMER, &vals);
            }
            EventKind::Message { port, payload } => {
                let vals = vec![Val::U64(now), Val::U32(*port), Val::bytes(payload)];
                self.activate(ev.target, ev.clone(), ON_MESSAGE, &vals);
            }
            EventKind::Intervention { payload } => match Intervention::decode(payload) {
                Some(Intervention::Message {
                    actor,
                    port,
                    payload,
                }) => {
                    let n = Notification::new(
                        "intervention.applied",
                        Severity::Info,
                        Scope::actor(actor),
                        Ends::Nothing,
                    )
                    .with("at", Value::Instant(at(ev.key.instant())))
                    .with("kind", Value::Name("message".into()));
                    self.notify(n);
                    let vals = vec![Val::U64(now), Val::U32(port), Val::bytes(&payload)];
                    self.activate(actor, ev.clone(), ON_MESSAGE, &vals);
                }
                None => {
                    let n = Notification::new(
                        "intervention.applied",
                        Severity::Info,
                        Scope::run(),
                        Ends::Nothing,
                    )
                    .with("at", Value::Instant(at(ev.key.instant())))
                    .with("kind", Value::Name("unknown".into()));
                    self.notify(n);
                }
            },
            // The core gives scheduled changes and domain events no meaning for a module actor.
            EventKind::ScheduledChange { .. } | EventKind::Domain { .. } => {}
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
                sigs,
                export_tys,
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
                &export_tys[which],
                vals,
                sigs,
                options.keep_payloads,
                payloads,
                telemetry,
                telemetry_dropped,
                options.telemetry_capacity,
                instant,
            )
        };
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
        if !self.actors.contains_key(&id) {
            return Err(HostError::NoInstance(id));
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
                cost_model_version: COST_MODEL_VERSION,
                root_seed: self.inputs.root_seed,
            },
            kernel: self.kernel.snapshot(),
            actors,
        })
    }

    /// A run continuing from a checkpoint (RFC 001 rule 52; RFC 003 rule 22): the inputs are
    /// checked first (DATA-005), then every actor is restored and the kernel continues.
    pub fn from_checkpoint(
        engine: E,
        admission: Admission,
        platform: &mut dyn Platform,
        scenario: Manifest,
        modules: &BTreeMap<String, Vec<u8>>,
        checkpoint: &Checkpoint,
        options: Options,
    ) -> Result<Self, HostError> {
        // The root seed is an input the checkpoint carries (RFC 001 rule 40, item 5).
        let seed = checkpoint.identity.root_seed;
        let mut run = Self::prepare(
            engine, admission, platform, scenario, modules, seed, options,
        )?;
        let id = &checkpoint.identity;
        let hex = florn_model::canonical::hex;
        if id.scenario_sha256 != run.inputs.scenario_sha256 {
            return Err(run.mismatch(
                "scenario-hash",
                hex(&id.scenario_sha256),
                hex(&run.inputs.scenario_sha256),
            ));
        }
        for (name, h) in &id.modules {
            let found = run.inputs.modules.iter().find(|m| &m.name == name);
            match found {
                Some(m) if m.source_sha256 == *h => {}
                Some(m) => {
                    return Err(run.mismatch("module-hash", hex(h), hex(&m.source_sha256)));
                }
                None => return Err(run.mismatch("module-hash", hex(h), String::new())),
            }
        }
        if id.admission_version != ADMISSION_VERSION {
            return Err(run.mismatch(
                "admission-version",
                id.admission_version.to_string(),
                ADMISSION_VERSION.to_string(),
            ));
        }
        if id.cost_model_version != COST_MODEL_VERSION {
            return Err(run.mismatch(
                "cost-model",
                id.cost_model_version.to_string(),
                COST_MODEL_VERSION.to_string(),
            ));
        }
        let config = run.kernel.config().clone();
        run.kernel = Kernel::restore(config, checkpoint.kernel.clone());
        for (id, state) in &checkpoint.actors {
            run.restore_actor(*id, state)?;
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
    pub fn replay(
        engine: E,
        admission: Admission,
        platform: &mut dyn Platform,
        scenario: Manifest,
        modules: &BTreeMap<String, Vec<u8>>,
        record: &RunToml,
        mut options: Options,
    ) -> Result<Self, HostError> {
        options.chain_interval = record.chain_interval;
        let seed = record.inputs.root_seed;
        let mut run = Self::prepare(
            engine, admission, platform, scenario, modules, seed, options,
        )?;
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
        if i.cost_model_version != COST_MODEL_VERSION {
            return Err(run.mismatch(
                "cost-model",
                i.cost_model_version.to_string(),
                COST_MODEL_VERSION.to_string(),
            ));
        }
        run.kernel.set_replay(i.interventions.clone());
        run.replay_against = Some(record.chain.clone());
        run.instantiate_all()?;
        run.init_all();
        Ok(run)
    }

    /// The comparison of a replay with its record, once the run has ended.
    pub fn comparison(&self) -> Option<Comparison> {
        let stored = self.replay_against.as_ref()?;
        self.ended?;
        Some(self.kernel.trace.compare(stored))
    }

    /// `run.toml` of the run record (RFC 005 rule 29).
    pub fn run_toml(&self) -> RunToml {
        let mut inputs = self.inputs.clone();
        inputs.interventions = self.kernel.applied_interventions().to_vec();
        RunToml {
            environment: self.environment.clone(),
            inputs,
            chain: self.kernel.trace.stored(),
            chain_interval: self.kernel.trace.interval(),
            records: self.kernel.trace.records(),
            ended: self.ended,
            reproducible: self.reproducible,
            run_id: self.run_id.clone(),
            created_ms: self.created_ms,
        }
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
    world: &mut CoreWorld,
    slot: &mut Slot<E>,
    act: Activation,
    which: usize,
    tys: &[Ty],
    vals: &[Val],
    sigs: &[(&'static str, Vec<Ty>)],
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
            actor,
            sigs,
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
