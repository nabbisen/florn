//! A run (RFC 005 §4): admission and the estimate before anything exists, the instance pool, the
//! run loop of rule 15, activations with the budget in place before lowering (RFC 003 rule 11),
//! classification by globals (rule 13), quotas in the import handlers (rule 17), export and
//! restore of actor state (RFC 003 §6), checkpoints, the diagnostic log, replay.
//!
//! The run's fields and accessors are here; the rest is in `run/` by area: `start` (the start and the
//! instance pool), `activation` (one activation and its outcome), `actions` (what a boundary does with
//! the events it takes), `end` (the run loop, the end, the comparison, replay) and `checkpoint`
//! (export and restore of state, checkpoints, a branch).

mod actions;
mod activation;
mod checkpoint;
mod compound;
mod end;
mod start;

pub use actions::decode_intervention;
pub(crate) use activation::fault_code;

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use florn_abi::Val;
use florn_admission::config::{NoUnits, encode_config};
use florn_admission::{Admission, Admitted, ClassLimits, Errors};
use florn_control::display::Record as DisplayRecord;
use florn_control::{At, Notification};
use florn_engine::{Engine, EngineError, ImportSpec};
use florn_kernel::{Digest, Identity, Instant, Kernel, Sink};
use florn_model::manifest::DomainMap;
use florn_model::{HostMaxima, Manifest};
use florn_platform::Capacity;

use crate::domain::{Composite, WorldSigs};
use crate::identity::{EndReason, Environment, RunInputs};
use crate::log::DiagnosticLog;
use crate::snapshot::{ActorState, Checkpoint};

/// The cost-model versions this Host publishes (RFC 003 rule 15; RFC 010), for a front end.
pub use florn_admission::{COST_MODEL_VERSION, COST_MODEL_VERSIONS, cost_table};

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

/// Where a run's guest-backed actors live (RFC 009 rule 14): every one in its own resident
/// instance, or a resident set smaller than the actor count with the rest held as exported state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Residency {
    #[default]
    Resident,
    Swap,
}

impl Residency {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Resident => "resident",
            Self::Swap => "swap",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "resident" => Self::Resident,
            "swap" => Self::Swap,
            _ => return None,
        })
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
    /// Keep the trace records since the last checkpoint only (DATA-008; the browser): an earlier
    /// window is produced by replay (RFC 009 rule 9), and an export's trace is assembled.
    pub records_since_checkpoint: bool,
    /// Swapping asked for, or every actor resident (RFC 009 rule 14; `resident` by default).
    pub residency: Residency,
    /// A cap on the resident set when swapping (the conformance kit's pools of one and two);
    /// the probed capacity less the reserve otherwise.
    pub resident_limit: Option<u64>,
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
            records_since_checkpoint: false,
            residency: Residency::Resident,
            resident_limit: None,
        }
    }
}

impl Options {
    /// Handled events between the Host's own checkpoints (RFC 009 rule 2).
    pub const DEFAULT_CHECKPOINT_INTERVAL: u64 = 10_000;
    /// The checkpoints the budget never drops (RFC 009 rule 3).
    pub const CHECKPOINTS_ALWAYS_KEPT: usize = 8;
    /// The instances the Host keeps for itself out of the probed capacity when swapping (RFC
    /// 009 rule 14): the budget module's (RFC 005 rule 8).
    pub const RESIDENT_RESERVE: u64 = 1;

    /// The browser Host's options: its maxima (RFC 003 rule 19) and build name.
    pub fn browser() -> Self {
        Self {
            host: HostMaxima::browser(),
            host_build: "browser".into(),
            checkpoint_memory: 64 * 1024 * 1024,
            records_since_checkpoint: true,
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
        .map(|a| a.memory.stored() as u64 + 32 * (a.globals.len() + a.tables.len()) as u64 + 64)
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
    /// The actor's state while it is not resident (RFC 009 rule 13): what a fresh instance
    /// receives before its next activation. The kernel's part is the kernel's, not this.
    swapped: Option<ActorState>,
    /// The activation counter at the actor's last activation: the pool's victim is the least.
    last_activated: u64,
    /// One reference per export of the world, in the world's order (RFC 005 rule 7).
    funcs: Vec<E::Func>,
    /// The post-return export of each export of the world whose result is or holds a list or a
    /// string, in the same order (RFC 002 rule 2); `None` for an export that needs none.
    posts: Vec<Option<E::Func>>,
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
    /// The resident set's size when swapping (RFC 009 rule 13); `None` for a resident run.
    pool: Option<u64>,
    /// Activations so far: the clock of the pool's least-recently-activated policy.
    activation_seq: u64,
    /// Swaps in so far (an actor restored into a fresh instance before its activation).
    swaps: u64,
}

impl<E: Engine> Run<E> {
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

    /// The resident set's size when the run swaps (RFC 009 rule 13); `None` for a resident run.
    pub fn pool(&self) -> Option<u64> {
        self.pool
    }

    /// The actors resident now.
    pub fn resident(&self) -> u64 {
        self.resident_count()
    }

    /// The swaps in so far: actors restored into a fresh instance before an activation.
    pub fn swaps(&self) -> u64 {
        self.swaps
    }

    /// The parent of a branch (RFC 009 rule 12), when this run is one.
    pub fn parent(&self) -> Option<&crate::identity::Parent> {
        self.parent.as_ref()
    }

    /// The memory the Host counts for this run (RFC 005 rule 18): the resident instances'
    /// limits and the kept checkpoints.
    pub fn memory_counted(&self) -> u64 {
        let instances: u64 = self
            .actors
            .values()
            .filter(|s| s.instance.is_some())
            .map(|s| s.limits.memory_bytes)
            .sum();
        instances.saturating_add(self.kept.iter().map(|k| k.bytes).sum())
    }

    /// The per-actor chain from kept records (RFC 005 rule 24).
    pub fn actor_chain(&self, id: Identity) -> Option<Digest> {
        self.kernel.trace.actor_chain(id)
    }
}
