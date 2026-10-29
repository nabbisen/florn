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
}

impl ExportKind {
    pub fn file_kind(self) -> FileKind {
        match self {
            Self::RunRecord { .. } => FileKind::RunRecord,
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

// ---- encoding -------------------------------------------------------------------------------

fn at_w(e: &mut Encoder, at: At) {
    e.u64(at.tick);
    e.u32(at.microstep);
}
fn at_r(d: &mut Decoder) -> Option<At> {
    Some(At {
        tick: d.u64()?,
        microstep: d.u32()?,
    })
}
fn file_kind_w(e: &mut Encoder, k: FileKind) {
    e.u8(match k {
        FileKind::Manifest => 0,
        FileKind::Package => 1,
        FileKind::Module => 2,
        FileKind::RunRecord => 3,
        FileKind::Checkpoint => 4,
        FileKind::DiagnosticLog => 5,
    });
}
fn file_kind_r(d: &mut Decoder) -> Option<FileKind> {
    Some(match d.u8()? {
        0 => FileKind::Manifest,
        1 => FileKind::Package,
        2 => FileKind::Module,
        3 => FileKind::RunRecord,
        4 => FileKind::Checkpoint,
        5 => FileKind::DiagnosticLog,
        _ => return None,
    })
}
fn run_state_w(e: &mut Encoder, s: RunState) {
    e.u8(match s {
        RunState::NoScenario => 0,
        RunState::Invalid => 1,
        RunState::Ready => 2,
        RunState::Running => 3,
        RunState::Paused => 4,
        RunState::Ended => 5,
    });
}
fn run_state_r(d: &mut Decoder) -> Option<RunState> {
    Some(match d.u8()? {
        0 => RunState::NoScenario,
        1 => RunState::Invalid,
        2 => RunState::Ready,
        3 => RunState::Running,
        4 => RunState::Paused,
        5 => RunState::Ended,
        _ => return None,
    })
}
fn pacing_w(e: &mut Encoder, p: Pacing) {
    match p {
        Pacing::Fastest => e.u8(0),
        Pacing::TicksPerSecond(t) => {
            e.u8(1);
            e.u64(t);
        }
    }
}
fn pacing_r(d: &mut Decoder) -> Option<Pacing> {
    Some(match d.u8()? {
        0 => Pacing::Fastest,
        1 => Pacing::TicksPerSecond(d.u64()?),
        _ => return None,
    })
}
fn bool_w(e: &mut Encoder, b: bool) {
    e.u8(b as u8);
}
fn bool_r(d: &mut Decoder) -> Option<bool> {
    match d.u8()? {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

fn command_w(e: &mut Encoder, c: &Command) {
    match c {
        Command::LoadScenario { bytes } => {
            e.u8(0);
            e.bytes(bytes);
        }
        Command::LoadModule { bytes } => {
            e.u8(1);
            e.bytes(bytes);
        }
        Command::Admit { bytes } => {
            e.u8(2);
            e.bytes(bytes);
        }
        Command::NewRun { seed } => {
            e.u8(3);
            match seed {
                SeedChoice::Given(s) => {
                    e.u8(0);
                    e.u64(*s);
                }
                SeedChoice::Choose => e.u8(1),
            }
        }
        Command::EndRun => e.u8(4),
        Command::Advance { unit } => {
            e.u8(5);
            match unit {
                Unit::OneEvent => e.u8(0),
                Unit::OneInstant => e.u8(1),
                Unit::OneTick => e.u8(2),
                Unit::ToTick(t) => {
                    e.u8(3);
                    e.u64(*t);
                }
            }
        }
        Command::Pause => e.u8(6),
        Command::SetPacing { pacing } => {
            e.u8(7);
            pacing_w(e, *pacing);
        }
        Command::Intervene {
            target,
            content,
            at,
        } => {
            e.u8(8);
            e.u32(*target);
            e.bytes(content);
            e.option(*at, Encoder::u64);
        }
        Command::Export { kind, confirmed } => {
            e.u8(9);
            match kind {
                ExportKind::RunRecord { trace, payloads } => {
                    e.u8(0);
                    bool_w(e, *trace);
                    bool_w(e, *payloads);
                }
                ExportKind::DiagnosticLog => e.u8(1),
                ExportKind::Package => e.u8(2),
                ExportKind::Checkpoint { index } => {
                    e.u8(3);
                    e.u32(*index);
                }
            }
            bool_w(e, *confirmed);
        }
        Command::Replay { record } => {
            e.u8(10);
            e.bytes(record);
        }
        Command::Checkpoint => e.u8(11),
        Command::Restore { checkpoint } => {
            e.u8(12);
            e.u32(*checkpoint);
        }
        Command::LoadCheckpoint { bytes } => {
            e.u8(13);
            e.bytes(bytes);
        }
    }
}

fn command_r(d: &mut Decoder) -> Option<Command> {
    Some(match d.u8()? {
        0 => Command::LoadScenario { bytes: d.bytes()? },
        1 => Command::LoadModule { bytes: d.bytes()? },
        2 => Command::Admit { bytes: d.bytes()? },
        3 => Command::NewRun {
            seed: match d.u8()? {
                0 => SeedChoice::Given(d.u64()?),
                1 => SeedChoice::Choose,
                _ => return None,
            },
        },
        4 => Command::EndRun,
        5 => Command::Advance {
            unit: match d.u8()? {
                0 => Unit::OneEvent,
                1 => Unit::OneInstant,
                2 => Unit::OneTick,
                3 => Unit::ToTick(d.u64()?),
                _ => return None,
            },
        },
        6 => Command::Pause,
        7 => Command::SetPacing {
            pacing: pacing_r(d)?,
        },
        8 => Command::Intervene {
            target: d.u32()?,
            content: d.bytes()?,
            at: d.option(Decoder::u64)?,
        },
        9 => Command::Export {
            kind: match d.u8()? {
                0 => ExportKind::RunRecord {
                    trace: bool_r(d)?,
                    payloads: bool_r(d)?,
                },
                1 => ExportKind::DiagnosticLog,
                2 => ExportKind::Package,
                3 => ExportKind::Checkpoint { index: d.u32()? },
                _ => return None,
            },
            confirmed: bool_r(d)?,
        },
        10 => Command::Replay { record: d.bytes()? },
        11 => Command::Checkpoint,
        12 => Command::Restore {
            checkpoint: d.u32()?,
        },
        13 => Command::LoadCheckpoint { bytes: d.bytes()? },
        _ => return None,
    })
}

fn query_w(e: &mut Encoder, q: &Query) {
    match q {
        Query::Scenario => e.u8(0),
        Query::RunState => e.u8(1),
        Query::Actor { id } => {
            e.u8(2);
            e.u32(*id);
        }
        Query::TraceWindow { from, count } => {
            e.u8(3);
            e.u64(*from);
            e.u32(*count);
        }
        Query::Capacity => e.u8(4),
        Query::Support => e.u8(5),
        Query::Decode { protocol, payload } => {
            e.u8(6);
            e.text(protocol);
            e.bytes(payload);
        }
        Query::Payload { digest } => {
            e.u8(7);
            e.text(digest);
        }
        Query::Checkpoints => e.u8(8),
    }
}
fn query_r(d: &mut Decoder) -> Option<Query> {
    Some(match d.u8()? {
        0 => Query::Scenario,
        1 => Query::RunState,
        2 => Query::Actor { id: d.u32()? },
        3 => Query::TraceWindow {
            from: d.u64()?,
            count: d.u32()?,
        },
        4 => Query::Capacity,
        5 => Query::Support,
        6 => Query::Decode {
            protocol: d.text()?,
            payload: d.bytes()?,
        },
        7 => Query::Payload { digest: d.text()? },
        8 => Query::Checkpoints,
        _ => return None,
    })
}
fn subscription_w(e: &mut Encoder, s: &Subscription) {
    match s {
        Subscription::Telemetry {
            channels,
            keep_one_in,
        } => {
            e.u8(0);
            e.list(channels, |e, c| e.u32(*c));
            e.u32(*keep_one_in);
        }
        Subscription::Events => e.u8(1),
        Subscription::Progress => e.u8(2),
        Subscription::Display => e.u8(3),
    }
}
fn subscription_r(d: &mut Decoder) -> Option<Subscription> {
    Some(match d.u8()? {
        0 => Subscription::Telemetry {
            channels: d.list(Decoder::u32)?,
            keep_one_in: d.u32()?,
        },
        1 => Subscription::Events,
        2 => Subscription::Progress,
        3 => Subscription::Display,
        _ => return None,
    })
}
fn kind_w(e: &mut Encoder, k: SubscriptionKind) {
    e.u8(match k {
        SubscriptionKind::Telemetry => 0,
        SubscriptionKind::Events => 1,
        SubscriptionKind::Progress => 2,
        SubscriptionKind::Display => 3,
    });
}
fn kind_r(d: &mut Decoder) -> Option<SubscriptionKind> {
    Some(match d.u8()? {
        0 => SubscriptionKind::Telemetry,
        1 => SubscriptionKind::Events,
        2 => SubscriptionKind::Progress,
        3 => SubscriptionKind::Display,
        _ => return None,
    })
}
fn telemetry_w(e: &mut Encoder, t: &TelemetryRecord) {
    at_w(e, t.at);
    e.u32(t.actor);
    e.u32(t.channel);
    e.bytes(&t.payload);
}
fn telemetry_r(d: &mut Decoder) -> Option<TelemetryRecord> {
    Some(TelemetryRecord {
        at: at_r(d)?,
        actor: d.u32()?,
        channel: d.u32()?,
        payload: d.bytes()?,
    })
}
fn opt_text_w(e: &mut Encoder, t: &Option<String>) {
    e.option(t.as_deref(), Encoder::text);
}
fn opt_text_r(d: &mut Decoder) -> Option<Option<String>> {
    d.option(Decoder::text)
}
fn pairs_w(e: &mut Encoder, pairs: &[(String, String)]) {
    e.list(pairs, |e, (k, v)| {
        e.text(k);
        e.text(v);
    });
}
fn pairs_r(d: &mut Decoder) -> Option<Vec<(String, String)>> {
    d.list(|d| Some((d.text()?, d.text()?)))
}
fn actor_summary_w(e: &mut Encoder, a: &ActorSummary) {
    e.u32(a.id);
    e.text(&a.name);
    opt_text_w(e, &a.module);
    e.text(&a.class);
    e.list(&a.ports, |e, p| {
        e.u32(p.id);
        e.text(&p.name);
        e.text(&p.ty);
    });
    pairs_w(e, &a.config);
}
fn actor_summary_r(d: &mut Decoder) -> Option<ActorSummary> {
    Some(ActorSummary {
        id: d.u32()?,
        name: d.text()?,
        module: opt_text_r(d)?,
        class: d.text()?,
        ports: d.list(|d| {
            Some(PortSummary {
                id: d.u32()?,
                name: d.text()?,
                ty: d.text()?,
            })
        })?,
        config: pairs_r(d)?,
    })
}
fn module_summary_w(e: &mut Encoder, m: &ModuleSummary) {
    e.text(&m.name);
    e.text(&m.source_sha256);
    e.text(&m.contract);
    e.text(&m.kind);
    opt_text_w(e, &m.description);
    e.list(&m.port_kinds, |e, (k, t, min, max)| {
        e.text(k);
        e.text(t);
        e.u64(*min);
        e.u64(*max);
    });
    e.list(&m.params, |e, p| {
        e.text(&p.name);
        e.text(&p.ty);
        bool_w(e, p.required);
        opt_text_w(e, &p.default);
        opt_text_w(e, &p.unit);
    });
    e.list(&m.channels, |e, c| {
        e.u32(c.channel);
        e.text(&c.name);
        pairs_w(e, &c.fields);
    });
    e.list(&m.needs, |e, (n, v)| {
        e.text(n);
        e.u64(*v);
    });
    e.list(&m.tables, |e, (n, fields)| {
        e.text(n);
        pairs_w(e, fields);
    });
}
fn module_summary_r(d: &mut Decoder) -> Option<ModuleSummary> {
    Some(ModuleSummary {
        name: d.text()?,
        source_sha256: d.text()?,
        contract: d.text()?,
        kind: d.text()?,
        description: opt_text_r(d)?,
        port_kinds: d.list(|d| Some((d.text()?, d.text()?, d.u64()?, d.u64()?)))?,
        params: d.list(|d| {
            Some(ParamSummary {
                name: d.text()?,
                ty: d.text()?,
                required: bool_r(d)?,
                default: opt_text_r(d)?,
                unit: opt_text_r(d)?,
            })
        })?,
        channels: d.list(|d| {
            Some(ChannelSummary {
                channel: d.u32()?,
                name: d.text()?,
                fields: pairs_r(d)?,
            })
        })?,
        needs: d.list(|d| Some((d.text()?, d.u64()?)))?,
        tables: d.list(|d| Some((d.text()?, pairs_r(d)?)))?,
    })
}
fn scenario_w(e: &mut Encoder, s: &ScenarioSummary) {
    e.text(&s.name);
    e.text(&s.sha256);
    e.text(&s.tick);
    e.u64(s.end);
    e.text(&s.tie);
    e.u64(s.seed);
    e.list(&s.actors, actor_summary_w);
    e.list(&s.connections, |e, c| {
        e.u32(c.id);
        e.text(&c.name);
        e.list(&c.ports, |e, (a, p)| {
            e.u32(*a);
            e.u32(*p);
        });
        opt_text_w(e, &c.ty);
        pairs_w(e, &c.attrs);
        e.list(&c.per_port, |e, ((a, p), fields)| {
            e.u32(*a);
            e.u32(*p);
            pairs_w(e, fields);
        });
    });
    e.list(&s.modules, module_summary_w);
    e.list(&s.missing_modules, |e, m| e.text(m));
    e.list(&s.domains, |e, m| e.text(m));
    e.list(&s.changes, |e, c| {
        e.u64(c.at);
        e.u32(c.target);
        pairs_w(e, &c.payload);
    });
    e.list(&s.classes, |e, (c, n)| {
        e.u8(*c);
        e.text(n);
    });
    e.list(&s.limits, |e, (class, fields)| {
        e.text(class);
        pairs_w(e, fields);
    });
}
fn scenario_r(d: &mut Decoder) -> Option<ScenarioSummary> {
    Some(ScenarioSummary {
        name: d.text()?,
        sha256: d.text()?,
        tick: d.text()?,
        end: d.u64()?,
        tie: d.text()?,
        seed: d.u64()?,
        actors: d.list(actor_summary_r)?,
        connections: d.list(|d| {
            Some(ConnectionSummary {
                id: d.u32()?,
                name: d.text()?,
                ports: d.list(|d| Some((d.u32()?, d.u32()?)))?,
                ty: opt_text_r(d)?,
                attrs: pairs_r(d)?,
                per_port: d.list(|d| Some(((d.u32()?, d.u32()?), pairs_r(d)?)))?,
            })
        })?,
        modules: d.list(module_summary_r)?,
        missing_modules: d.list(Decoder::text)?,
        domains: d.list(Decoder::text)?,
        changes: d.list(|d| {
            Some(ChangeSummary {
                at: d.u64()?,
                target: d.u32()?,
                payload: pairs_r(d)?,
            })
        })?,
        classes: d.list(|d| Some((d.u8()?, d.text()?)))?,
        limits: d.list(|d| Some((d.text()?, pairs_r(d)?)))?,
    })
}
fn trace_entry_w(e: &mut Encoder, t: &TraceEntry) {
    match t {
        TraceEntry::Handled {
            at,
            class,
            origin,
            seq,
            target,
            metered,
            draws,
            effects,
            sends,
        } => {
            e.u8(1);
            at_w(e, *at);
            e.u8(*class);
            e.u32(*origin);
            e.u64(*seq);
            e.u32(*target);
            e.u64(*metered);
            e.u32(*draws);
            e.u32(*effects);
            e.list(sends, |e, s| {
                e.u64(s.seq);
                e.u32(s.port);
                e.u32(s.len);
                e.text(&s.digest);
            });
        }
        TraceEntry::Fault {
            at,
            class,
            origin,
            seq,
            actor,
            code,
            parameter,
            metered,
        } => {
            e.u8(2);
            at_w(e, *at);
            e.u8(*class);
            e.u32(*origin);
            e.u64(*seq);
            e.u32(*actor);
            e.text(code);
            e.u32(*parameter);
            e.u64(*metered);
        }
        TraceEntry::Intervention { at, seq, len } => {
            e.u8(3);
            at_w(e, *at);
            e.u64(*seq);
            e.u32(*len);
        }
    }
}
fn trace_entry_r(d: &mut Decoder) -> Option<TraceEntry> {
    Some(match d.u8()? {
        1 => TraceEntry::Handled {
            at: at_r(d)?,
            class: d.u8()?,
            origin: d.u32()?,
            seq: d.u64()?,
            target: d.u32()?,
            metered: d.u64()?,
            draws: d.u32()?,
            effects: d.u32()?,
            sends: d.list(|d| {
                Some(SendSummary {
                    seq: d.u64()?,
                    port: d.u32()?,
                    len: d.u32()?,
                    digest: d.text()?,
                })
            })?,
        },
        2 => TraceEntry::Fault {
            at: at_r(d)?,
            class: d.u8()?,
            origin: d.u32()?,
            seq: d.u64()?,
            actor: d.u32()?,
            code: d.text()?,
            parameter: d.u32()?,
            metered: d.u64()?,
        },
        3 => TraceEntry::Intervention {
            at: at_r(d)?,
            seq: d.u64()?,
            len: d.u32()?,
        },
        _ => return None,
    })
}
fn answer_w(e: &mut Encoder, a: &Answer) {
    match a {
        Answer::Scenario(s) => {
            e.u8(0);
            e.option(s.as_ref(), scenario_w);
        }
        Answer::RunState(r) => {
            e.u8(1);
            run_state_w(e, r.state);
            e.option(r.now, at_w);
            e.u64(r.handled);
            e.u64(r.records);
            opt_text_w(e, &r.chain);
            e.option(r.seed, Encoder::u64);
            pacing_w(e, r.pacing);
            opt_text_w(e, &r.ended);
            bool_w(e, r.reproducible);
            opt_text_w(e, &r.run_id);
        }
        Answer::Actor(a) => {
            e.u8(2);
            e.option(a.as_ref(), |e, a| {
                actor_summary_w(e, &a.summary);
                bool_w(e, a.alive);
                e.list(&a.faults, |e, f| {
                    at_w(e, f.at);
                    e.text(&f.code);
                    e.u64(f.metered);
                    e.u32(f.parameter);
                });
                e.list(&a.recent_telemetry, telemetry_w);
            });
        }
        Answer::TraceWindow(t) => {
            e.u8(3);
            e.list(t, trace_entry_w);
        }
        Answer::Capacity(c) => {
            e.u8(4);
            e.u64(c.instances);
            e.u64(c.memory_bytes);
            e.option(c.swap_cost_us_per_kb, Encoder::u64);
        }
        Answer::Support(s) => {
            e.u8(5);
            e.u32(s.control_version);
            e.text(&s.florn_version);
            e.text(&s.host_build);
            e.text(&s.engine);
            e.text(&s.engine_version);
            e.text(&s.arch);
            e.list(&s.features, |e, f| e.text(f));
        }
        Answer::Admitted {
            module,
            admitted_sha256,
        } => {
            e.u8(6);
            module_summary_w(e, module);
            e.text(admitted_sha256);
        }
        Answer::Preview(p) => {
            e.u8(7);
            file_kind_w(e, p.kind);
            e.u64(p.bytes);
            bool_w(e, p.payloads);
            e.list(&p.modules, |e, (n, h)| {
                e.text(n);
                e.text(h);
            });
        }
        Answer::Exported { kind, name, bytes } => {
            e.u8(8);
            file_kind_w(e, *kind);
            e.text(name);
            e.bytes(bytes);
        }
        Answer::Done => e.u8(9),
        Answer::Decoded(fields) => {
            e.u8(10);
            e.option(fields.as_deref(), pairs_w);
        }
        Answer::Payload(bytes) => {
            e.u8(11);
            e.option(bytes.as_deref(), Encoder::bytes);
        }
        Answer::Checkpoints(list) => {
            e.u8(12);
            e.list(list, |e, c| {
                e.u32(c.index);
                at_w(e, c.instant);
                e.u64(c.records);
                e.u64(c.handled);
                e.u64(c.bytes);
            });
        }
    }
}
fn answer_r(d: &mut Decoder) -> Option<Answer> {
    Some(match d.u8()? {
        0 => Answer::Scenario(d.option(scenario_r)?),
        1 => Answer::RunState(RunStateInfo {
            state: run_state_r(d)?,
            now: d.option(at_r)?,
            handled: d.u64()?,
            records: d.u64()?,
            chain: opt_text_r(d)?,
            seed: d.option(Decoder::u64)?,
            pacing: pacing_r(d)?,
            ended: opt_text_r(d)?,
            reproducible: bool_r(d)?,
            run_id: opt_text_r(d)?,
        }),
        2 => Answer::Actor(d.option(|d| {
            Some(ActorInfo {
                summary: actor_summary_r(d)?,
                alive: bool_r(d)?,
                faults: d.list(|d| {
                    Some(FaultInfo {
                        at: at_r(d)?,
                        code: d.text()?,
                        metered: d.u64()?,
                        parameter: d.u32()?,
                    })
                })?,
                recent_telemetry: d.list(telemetry_r)?,
            })
        })?),
        3 => Answer::TraceWindow(d.list(trace_entry_r)?),
        4 => Answer::Capacity(CapacityInfo {
            instances: d.u64()?,
            memory_bytes: d.u64()?,
            swap_cost_us_per_kb: d.option(Decoder::u64)?,
        }),
        5 => Answer::Support(SupportInfo {
            control_version: d.u32()?,
            florn_version: d.text()?,
            host_build: d.text()?,
            engine: d.text()?,
            engine_version: d.text()?,
            arch: d.text()?,
            features: d.list(Decoder::text)?,
        }),
        6 => Answer::Admitted {
            module: module_summary_r(d)?,
            admitted_sha256: d.text()?,
        },
        7 => Answer::Preview(ExportPreview {
            kind: file_kind_r(d)?,
            bytes: d.u64()?,
            payloads: bool_r(d)?,
            modules: d.list(|d| Some((d.text()?, d.text()?)))?,
        }),
        8 => Answer::Exported {
            kind: file_kind_r(d)?,
            name: d.text()?,
            bytes: d.bytes()?,
        },
        9 => Answer::Done,
        10 => Answer::Decoded(d.option(pairs_r)?),
        11 => Answer::Payload(d.option(Decoder::bytes)?),
        12 => Answer::Checkpoints(d.list(|d| {
            Some(CheckpointSummary {
                index: d.u32()?,
                instant: at_r(d)?,
                records: d.u64()?,
                handled: d.u64()?,
                bytes: d.u64()?,
            })
        })?),
        _ => return None,
    })
}

/// The canonical encoding of an envelope: the version, a tag, the message.
pub fn encode(env: &Envelope) -> Vec<u8> {
    let mut e = Encoder::default();
    e.u32(env.version);
    match &env.message {
        Message::Command(c) => {
            e.u8(0);
            command_w(&mut e, c);
        }
        Message::Query(q) => {
            e.u8(1);
            query_w(&mut e, q);
        }
        Message::Subscribe(s) => {
            e.u8(2);
            subscription_w(&mut e, s);
        }
        Message::Unsubscribe(k) => {
            e.u8(3);
            kind_w(&mut e, *k);
        }
        Message::Notification(n) => {
            e.u8(4);
            n.write(&mut e);
        }
        Message::Answer(a) => {
            e.u8(5);
            answer_w(&mut e, a);
        }
        Message::Telemetry(t) => {
            e.u8(6);
            telemetry_w(&mut e, t);
        }
        Message::Progress(p) => {
            e.u8(7);
            run_state_w(&mut e, p.state);
            at_w(&mut e, p.now);
            e.u64(p.handled);
            e.u64(p.records);
        }
    }
    e.finish()
}

/// Decode an envelope; a version other than [`CONTROL_VERSION`] is refused before anything else.
pub fn decode(bytes: &[u8]) -> Result<Envelope, DecodeError> {
    let mut d = Decoder::new(bytes);
    let version = d.u32().ok_or(DecodeError::Malformed("version"))?;
    if version != CONTROL_VERSION {
        return Err(DecodeError::Version(version));
    }
    let tag = d.u8().ok_or(DecodeError::Malformed("message"))?;
    let message = match tag {
        0 => Message::Command(command_r(&mut d).ok_or(DecodeError::Malformed("command"))?),
        1 => Message::Query(query_r(&mut d).ok_or(DecodeError::Malformed("query"))?),
        2 => Message::Subscribe(
            subscription_r(&mut d).ok_or(DecodeError::Malformed("subscription"))?,
        ),
        3 => Message::Unsubscribe(kind_r(&mut d).ok_or(DecodeError::Malformed("subscription"))?),
        4 => Message::Notification(
            Notification::read(&mut d).ok_or(DecodeError::Malformed("notification"))?,
        ),
        5 => Message::Answer(answer_r(&mut d).ok_or(DecodeError::Malformed("answer"))?),
        6 => Message::Telemetry(telemetry_r(&mut d).ok_or(DecodeError::Malformed("telemetry"))?),
        7 => Message::Progress(Progress {
            state: run_state_r(&mut d).ok_or(DecodeError::Malformed("progress"))?,
            now: at_r(&mut d).ok_or(DecodeError::Malformed("progress"))?,
            handled: d.u64().ok_or(DecodeError::Malformed("progress"))?,
            records: d.u64().ok_or(DecodeError::Malformed("progress"))?,
        }),
        _ => return Err(DecodeError::Malformed("message")),
    };
    if !d.done() {
        return Err(DecodeError::Malformed("trailing bytes"));
    }
    Ok(Envelope { version, message })
}
