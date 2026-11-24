//! The messages of the control interface (RFC 005 rules 25 to 28; external design §4): commands,
//! queries, subscriptions, answers, notifications, telemetry, progress; one canonical binary
//! encoding over every transport; the interface version; the structural validation of a command.

use florn_model::canonical::{Decoder, Encoder};

use crate::catalogue::Code;
use crate::notification::{At, Notification, Scope, Value};

/// The interface version (RFC 005 rule 25). A front end and a Host of different versions refuse
/// each other with `control.version`.
pub const CONTROL_VERSION: u32 = 1;

/// A unit of advance (RFC 001 §2.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    OneEvent,
    OneInstant,
    OneTick,
    ToTick(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeedChoice {
    Given(u64),
    /// The platform chooses; the seed is then shown and is an input.
    Choose,
}

/// When the Host works (RFC 001 rule 9): never what it computes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pacing {
    Fastest,
    /// Logical ticks per wall-clock second.
    TicksPerSecond(u64),
}

/// `enum(residency)` of RFC 009 rule 14: every actor resident, or swapping asked for.
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

/// `enum(file-kind)` of RFC 007.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    Manifest,
    Package,
    Module,
    RunRecord,
    Checkpoint,
    DiagnosticLog,
}

impl FileKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manifest => "manifest",
            Self::Package => "package",
            Self::Module => "module",
            Self::RunRecord => "run-record",
            Self::Checkpoint => "checkpoint",
            Self::DiagnosticLog => "diagnostic-log",
        }
    }
    /// The file extension (RFC 004 rule 33; RFC 005 rule 29).
    pub fn extension(self) -> &'static str {
        match self {
            Self::Manifest => "toml",
            Self::Package => "florn",
            Self::Module => "wasm",
            Self::RunRecord => "florn-run",
            Self::Checkpoint => "florn-checkpoint",
            Self::DiagnosticLog => "jsonl",
        }
    }
}

/// What `export` writes (RFC 005 rules 26, 29, 30).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportKind {
    RunRecord {
        trace: bool,
        payloads: bool,
    },
    DiagnosticLog,
    Package,
    /// A kept checkpoint by its index in the `checkpoints` answer (RFC 009 rule 6; 0.3).
    Checkpoint {
        index: u32,
    },
    /// The record of the run the current one branched from, as the Host kept it at the
    /// branching (RFC 009 rule 11; 0.3): with the trace it kept, without payloads.
    ParentRecord,
}

impl ExportKind {
    pub fn file_kind(self) -> FileKind {
        match self {
            Self::RunRecord { .. } | Self::ParentRecord => FileKind::RunRecord,
            Self::DiagnosticLog => FileKind::DiagnosticLog,
            Self::Package => FileKind::Package,
            Self::Checkpoint { .. } => FileKind::Checkpoint,
        }
    }
}

/// The commands of RFC 005 rule 26 for release 0.1. `load-module` supplies a module a manifest
/// references by hash; a package carries its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    LoadScenario {
        bytes: Vec<u8>,
    },
    LoadModule {
        bytes: Vec<u8>,
    },
    Admit {
        bytes: Vec<u8>,
    },
    NewRun {
        seed: SeedChoice,
        /// Where the actors live (RFC 009 rule 14; 0.3): `resident` unless `swap` is asked
        /// for. Absent in a 0.2 front end's message, which means `resident`.
        residency: Residency,
    },
    EndRun,
    Advance {
        unit: Unit,
    },
    Pause,
    SetPacing {
        pacing: Pacing,
    },
    /// An intervention: `content` is the domain's canonical map (RFC 005 rule 20).
    Intervene {
        target: u32,
        content: Vec<u8>,
        at: Option<u64>,
    },
    /// Previewed first (`confirmed: false`), then written (`confirmed: true`).
    Export {
        kind: ExportKind,
        confirmed: bool,
    },
    Replay {
        record: Vec<u8>,
    },
    /// A checkpoint at the next boundary (RFC 009 rule 2; 0.3).
    Checkpoint,
    /// Restore a kept checkpoint by its index (RFC 009 rules 5 and 7; 0.3): the run continues
    /// from its instant, the trace after it discarded.
    Restore {
        checkpoint: u32,
    },
    /// A new run from a checkpoint file (RFC 009 rule 6; 0.3), with the loaded scenario and
    /// its modules.
    LoadCheckpoint {
        bytes: Vec<u8>,
    },
    /// The cost-model version new runs admit under (RFC 010 rule 9; 0.3): one the Host
    /// publishes, else `control.invalid`.
    SetCostModel {
        version: u32,
    },
    /// A new run from the current instant of the current run, which ends (RFC 009 rules 10 to
    /// 12; 0.3).
    Branch,
}

impl Command {
    pub fn name(&self) -> &'static str {
        match self {
            Self::LoadScenario { .. } => "load-scenario",
            Self::LoadModule { .. } => "load-module",
            Self::Admit { .. } => "admit",
            Self::NewRun { .. } => "new-run",
            Self::EndRun => "end-run",
            Self::Advance { .. } => "advance",
            Self::Pause => "pause",
            Self::SetPacing { .. } => "set-pacing",
            Self::Intervene { .. } => "intervene",
            Self::Export { .. } => "export",
            Self::Replay { .. } => "replay",
            Self::Checkpoint => "checkpoint",
            Self::Restore { .. } => "restore",
            Self::LoadCheckpoint { .. } => "load-checkpoint",
            Self::SetCostModel { .. } => "set-cost-model",
            Self::Branch => "branch",
        }
    }

    /// Whether the command changes the logical world (RFC 005 rule 27): only `intervene` does.
    pub fn is_intervention(&self) -> bool {
        matches!(self, Self::Intervene { .. })
    }

    /// The structural validation (CTL-002): what can be judged without the Host's state.
    pub fn validate(&self) -> Result<(), Box<Notification>> {
        let invalid =
            |reason: &str, field: &str| Err(Box::new(control_invalid(self.name(), reason, field)));
        match self {
            Self::LoadScenario { bytes } | Self::LoadModule { bytes } | Self::Admit { bytes } => {
                if bytes.is_empty() {
                    return invalid("missing-field", "bytes");
                }
            }
            Self::Replay { record } => {
                if record.is_empty() {
                    return invalid("missing-field", "record");
                }
            }
            Self::SetPacing {
                pacing: Pacing::TicksPerSecond(0),
            } => return invalid("out-of-range", "ticks-per-second"),
            Self::Intervene { content, .. } if content.is_empty() => {
                return invalid("missing-field", "content");
            }
            _ => {}
        }
        Ok(())
    }
}

/// `control.invalid` for a command, naming the field (RFC 007 §4.4).
pub fn control_invalid(command: &str, reason: &str, field: &str) -> Notification {
    Code::ControlInvalid.notification(
        Scope::command(command),
        vec![
            Value::Name(command.to_string()),
            Value::Enum(reason.to_string()),
            Value::Text(field.to_string()),
        ],
    )
}

/// `control.not-now`: the command does not apply in this state.
pub fn control_not_now(command: &str, state: RunState) -> Notification {
    Code::ControlNotNow.notification(
        Scope::command(command),
        vec![
            Value::Name(command.to_string()),
            Value::Enum(state.as_str().to_string()),
        ],
    )
}

/// `control.version`: the front end's and the Host's versions.
pub fn control_version(front_end: u32) -> Notification {
    Code::ControlVersion.notification(
        Scope::default(),
        vec![
            Value::Text(front_end.to_string()),
            Value::Text(CONTROL_VERSION.to_string()),
        ],
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Query {
    Scenario,
    RunState,
    Actor {
        id: u32,
    },
    /// Trace records by index, when the Host keeps them.
    TraceWindow {
        from: u64,
        count: u32,
    },
    Capacity,
    Support,
    /// A payload's fields by a domain's decoder for the protocol a module declares (OBS-006;
    /// 0.2). The answer is `None` when no domain of the scenario decodes the protocol.
    Decode {
        protocol: String,
        payload: Vec<u8>,
    },
    /// The bytes of a payload the run kept, by its BLAKE3 digest in hex (0.2): `None` when the
    /// run keeps no payloads or holds none with that digest.
    Payload {
        digest: String,
    },
    /// The kept checkpoints of the run (RFC 009 rule 3; 0.3).
    Checkpoints,
}

impl Query {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Scenario => "scenario",
            Self::RunState => "run-state",
            Self::Actor { .. } => "actor",
            Self::TraceWindow { .. } => "trace-window",
            Self::Capacity => "capacity",
            Self::Support => "support",
            Self::Decode { .. } => "decode",
            Self::Payload { .. } => "payload",
            Self::Checkpoints => "checkpoints",
        }
    }
    /// The largest trace window one answer carries.
    pub const MAX_WINDOW: u32 = 10_000;
    pub fn validate(&self) -> Result<(), Box<Notification>> {
        if let Self::TraceWindow { count, .. } = self
            && (*count == 0 || *count > Self::MAX_WINDOW)
        {
            return Err(Box::new(control_invalid(
                self.name(),
                "out-of-range",
                "count",
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Subscription {
    /// Committed reports on these channels (empty: every channel), one in `keep_one_in`.
    Telemetry {
        channels: Vec<u32>,
        keep_one_in: u32,
    },
    /// Faults, limits, interventions applied: the notifications produced while running.
    Events,
    Progress,
    /// The display stream of RFC 005 rule 14 (0.2): the Host writes its records from the next
    /// advance on; they travel by the display transport, never as control messages.
    Display,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubscriptionKind {
    Telemetry,
    Events,
    Progress,
    Display,
}

impl Subscription {
    pub fn kind(&self) -> SubscriptionKind {
        match self {
            Self::Telemetry { .. } => SubscriptionKind::Telemetry,
            Self::Events => SubscriptionKind::Events,
            Self::Progress => SubscriptionKind::Progress,
            Self::Display => SubscriptionKind::Display,
        }
    }
    pub fn validate(&self) -> Result<(), Box<Notification>> {
        if let Self::Telemetry { keep_one_in: 0, .. } = self {
            return Err(Box::new(control_invalid(
                "subscribe",
                "out-of-range",
                "keep-one-in",
            )));
        }
        Ok(())
    }
}

/// `enum(run-state)` of RFC 007.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunState {
    NoScenario,
    Invalid,
    Ready,
    Running,
    Paused,
    Ended,
}

impl RunState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoScenario => "no-scenario",
            Self::Invalid => "invalid",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Ended => "ended",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortSummary {
    pub id: u32,
    pub name: String,
    pub ty: String,
}

/// Fields as text: (name, value), in the manifest's order.
pub type Fields = Vec<(String, String)>;

/// A telemetry channel a module declares (RFC 002 rule 15): its fields as (name, type name),
/// so that a front end decodes the reports it shows (0.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelSummary {
    pub channel: u32,
    pub name: String,
    pub fields: Vec<(String, String)>,
}

/// A scheduled change of the scenario (RFC 004 rule 20), for the timeline before a run (0.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeSummary {
    pub at: u64,
    pub target: u32,
    /// The payload's fields as text.
    pub payload: Vec<(String, String)>,
}

/// A committed `send` of a handled event (0.2): the payload by digest, for the inspector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SendSummary {
    pub seq: u64,
    pub port: u32,
    pub len: u32,
    /// BLAKE3 of the payload, hex.
    pub digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorSummary {
    pub id: u32,
    pub name: String,
    /// The `[modules]` name; `None` for a built-in.
    pub module: Option<String>,
    pub class: String,
    pub ports: Vec<PortSummary>,
    /// `[actors.config]` as text, in the manifest's order (0.2).
    pub config: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamSummary {
    pub name: String,
    pub ty: String,
    pub required: bool,
    pub default: Option<String>,
    pub unit: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleSummary {
    pub name: String,
    pub source_sha256: String,
    pub contract: String,
    pub kind: String,
    pub description: Option<String>,
    /// (kind, type, min, max)
    pub port_kinds: Vec<(String, String, u64, u64)>,
    pub params: Vec<ParamSummary>,
    pub channels: Vec<ChannelSummary>,
    /// (limit name, need)
    pub needs: Vec<(String, u64)>,
    /// The module manifest's domain tables (RFC 008 rule 16), each as text fields (0.2).
    pub tables: Vec<(String, Vec<(String, String)>)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionSummary {
    pub id: u32,
    pub name: String,
    pub ports: Vec<(u32, u32)>,
    /// `<domain>/<name>` as written, `None` for the core's (0.2).
    pub ty: Option<String>,
    /// `[connections.attrs]` as text (0.2).
    pub attrs: Vec<(String, String)>,
    /// `[connections.attrs.per-port]` as text, by (actor, port) (0.2).
    pub per_port: Vec<((u32, u32), Fields)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScenarioSummary {
    pub name: String,
    pub sha256: String,
    /// The tick length as written (`1ns`).
    pub tick: String,
    pub end: u64,
    pub tie: String,
    pub seed: u64,
    pub actors: Vec<ActorSummary>,
    pub connections: Vec<ConnectionSummary>,
    pub modules: Vec<ModuleSummary>,
    /// Hashes the manifest references and the Host does not hold.
    pub missing_modules: Vec<String>,
    /// The domains the scenario declares, in order (0.2).
    pub domains: Vec<String>,
    /// The scheduled changes (0.2).
    pub changes: Vec<ChangeSummary>,
    /// The event classes of a run of this scenario with their names (RFC 001 rule 15): the
    /// core's three, then each domain's (0.2).
    pub classes: Vec<(u8, String)>,
    /// `[limits.<class>]` as text, by class (0.2): counts bare, bytes with the unit `B`.
    pub limits: Vec<(String, Fields)>,
}

/// A value of an intervention's content or a scheduled change, as a front end states it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldValue {
    Boolean(bool),
    Integer(i64),
    /// A quantity or any other text the domain reads.
    Text(String),
}

/// The content of `intervene` for a domain's kind (RFC 008 rule 1; RFC 005 rule 26): the
/// kind and the fields as the canonical map the Host decodes. A front end builds it from what
/// the person asked, with no encoder of its own.
pub fn intervention_content(kind: &str, fields: &[(String, FieldValue)]) -> Vec<u8> {
    use florn_model::manifest::{DomainMap, DomainValue};
    let mut m = DomainMap::new();
    m.insert("kind".into(), DomainValue::Text(kind.into()));
    for (k, v) in fields {
        let v = match v {
            FieldValue::Boolean(b) => DomainValue::Boolean(*b),
            FieldValue::Integer(i) => DomainValue::Integer(*i),
            FieldValue::Text(t) => DomainValue::Text(t.clone()),
        };
        m.insert(k.clone(), v);
    }
    let mut e = florn_model::canonical::Encoder::default();
    florn_model::canonical::domain_map(&mut e, &m);
    e.finish()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunStateInfo {
    pub state: RunState,
    pub now: Option<At>,
    pub handled: u64,
    pub records: u64,
    pub chain: Option<String>,
    pub seed: Option<u64>,
    pub pacing: Pacing,
    pub ended: Option<String>,
    pub reproducible: bool,
    pub run_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FaultInfo {
    pub at: At,
    pub code: String,
    pub metered: u64,
    pub parameter: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TelemetryRecord {
    pub at: At,
    pub actor: u32,
    pub channel: u32,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorInfo {
    pub summary: ActorSummary,
    pub alive: bool,
    pub faults: Vec<FaultInfo>,
    pub recent_telemetry: Vec<TelemetryRecord>,
}

/// A trace record as a front end sees it (RFC 005 rule 20, without the effects' payload digests).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TraceEntry {
    Handled {
        at: At,
        class: u8,
        origin: u32,
        seq: u64,
        target: u32,
        metered: u64,
        draws: u32,
        effects: u32,
        /// The committed sends among the effects (0.2).
        sends: Vec<SendSummary>,
    },
    Fault {
        at: At,
        class: u8,
        origin: u32,
        seq: u64,
        actor: u32,
        code: String,
        parameter: u32,
        metered: u64,
    },
    Intervention {
        at: At,
        seq: u64,
        len: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapacityInfo {
    pub instances: u64,
    pub memory_bytes: u64,
    pub swap_cost_us_per_kb: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupportInfo {
    pub control_version: u32,
    pub florn_version: String,
    pub host_build: String,
    pub engine: String,
    pub engine_version: String,
    pub arch: String,
    pub features: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportPreview {
    pub kind: FileKind,
    pub bytes: u64,
    pub payloads: bool,
    /// Every module by name and hash (RFC 005 rule 30).
    pub modules: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    Scenario(Option<ScenarioSummary>),
    RunState(RunStateInfo),
    Actor(Option<ActorInfo>),
    TraceWindow(Vec<TraceEntry>),
    Capacity(CapacityInfo),
    Support(SupportInfo),
    /// `admit`: the manifest and the admitted hash.
    Admitted {
        module: ModuleSummary,
        admitted_sha256: String,
    },
    Preview(ExportPreview),
    Exported {
        kind: FileKind,
        name: String,
        bytes: Vec<u8>,
    },
    /// A command applied with nothing else to say.
    Done,
    /// `decode` (0.2): the fields, or `None` when no domain decodes the protocol.
    Decoded(Option<Vec<(String, String)>>),
    /// `payload` (0.2): the bytes, or `None`.
    Payload(Option<Vec<u8>>),
    /// `checkpoints` (0.3): the kept checkpoints, oldest first.
    Checkpoints(Vec<CheckpointSummary>),
}

/// One kept checkpoint (RFC 009 rule 3): its index for `restore` and `export`, the instant it
/// holds, the records and the handled events up to it, and its size in memory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckpointSummary {
    pub index: u32,
    pub instant: At,
    pub records: u64,
    pub handled: u64,
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Progress {
    pub state: RunState,
    pub now: At,
    pub handled: u64,
    pub records: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    Command(Command),
    Query(Query),
    Subscribe(Subscription),
    Unsubscribe(SubscriptionKind),
    Notification(Notification),
    Answer(Answer),
    Telemetry(TelemetryRecord),
    Progress(Progress),
}

/// A message with the interface version in front.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Envelope {
    pub version: u32,
    pub message: Message,
}

impl Envelope {
    pub fn new(message: Message) -> Self {
        Self {
            version: CONTROL_VERSION,
            message,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// Another interface version.
    Version(u32),
    /// Not a message: what was expected where it failed.
    Malformed(&'static str),
}

impl DecodeError {
    /// The notification a Host answers with.
    pub fn notification(&self) -> Notification {
        match self {
            Self::Version(v) => control_version(*v),
            Self::Malformed(what) => control_invalid("?", "wrong-type", what),
        }
    }
}

mod answer;
mod codec;

pub use codec::{decode, encode};
