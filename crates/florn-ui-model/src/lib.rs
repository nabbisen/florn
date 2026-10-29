//! The view model (RFC 005 rule 37; external design §3): what is shown, over the control
//! interface's types only. A shell feeds it every message the Host sends and every display
//! batch, asks it for the messages it wants sent (the outbox), and draws the scene from it.
//! It holds selection, the visible area, the time window and filters, the fields of the
//! inspectors and the draft, and nothing a language decides: every text a person reads comes
//! from the shell's catalogue, keyed by the codes and names kept here. It compiles and tests
//! natively, with no framework, `web-sys`, `js-sys` or `wasm-bindgen` (checked in CI).
//!
//! The 60 Hz path (rule 39) is `display`: a batch of records goes to the topology and the
//! scene reads it; nothing reactive runs per record.
#![forbid(unsafe_code)]

pub mod draft;
pub mod fields;
/// The control interface, for a scene or a shell to name its types through the view model.
pub use florn_control;
pub mod inspector;
pub mod timeline;
pub mod topology;

use std::collections::BTreeMap;

use florn_control::display::Record;
use florn_control::message::{
    ActorInfo, CapacityInfo, CheckpointSummary, ExportPreview, ModuleSummary, RunStateInfo,
    ScenarioSummary, SupportInfo, TelemetryRecord,
};
use florn_control::{
    Answer, At, Command, ExportKind, FieldValue, FileKind, Message, Notification, Pacing, Progress,
    Query, RunState, SeedChoice, Subscription, Unit, Value, intervention_content,
};

pub use draft::Draft;
pub use fields::Field;
pub use inspector::Inspector;
pub use timeline::Timeline;
pub use topology::{Point, Topology, Viewport};

/// What is selected (external design §3.6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Selection {
    #[default]
    None,
    Actor(u32),
    Connection(u32),
    /// A committed send of a timeline entry: the entry's index and the send's.
    Message {
        entry: usize,
        send: usize,
    },
    World,
}

/// The run as the status line shows it (§3.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunView {
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

impl Default for RunView {
    fn default() -> Self {
        Self {
            state: RunState::NoScenario,
            now: None,
            handled: 0,
            records: 0,
            chain: None,
            seed: None,
            pacing: Pacing::Fastest,
            ended: None,
            reproducible: true,
            run_id: None,
        }
    }
}

/// What the status area says beyond the instant (§3.13; §3.8 "nothing silent").
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    /// `shared` or `messages`, from `telemetry.transport`.
    pub transport: Option<String>,
    /// Records the display path dropped in this run, from `telemetry.dropped`.
    pub dropped: u64,
    /// `enum(limiting-factor)` from `host.limited-by`.
    pub limited_by: Option<String>,
    /// The replay's outcome code, when a replay ran.
    pub replay: Option<String>,
}

/// An intervention of this run (§3.5): asked, then applied at an instant the Host reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterventionView {
    pub target: u32,
    pub kind: String,
    pub content: Vec<(String, String)>,
    pub requested_at: Option<u64>,
    pub applied_at: Option<At>,
}

/// A notification shown as a state (§3.8), until dismissed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub notification: Notification,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewModel {
    pub scenario: Option<ScenarioSummary>,
    pub run: RunView,
    pub capacity: Option<CapacityInfo>,
    pub support: Option<SupportInfo>,
    pub status: Status,
    pub topology: Topology,
    pub timeline: Timeline,
    pub interventions: Vec<InterventionView>,
    /// The ended run's interventions, kept so that a person can turn them into scheduled
    /// changes or discard them (§3.3, Reset).
    pub kept: Vec<InterventionView>,
    pub notices: Vec<Notice>,
    pub selection: Selection,
    pub inspector: Inspector,
    pub draft: Draft,
    /// The modules admitted from files or the palette (§3.9): shown before they can be
    /// placed, and placeable in the draft.
    pub admitted: Vec<ModuleSummary>,
    /// The last export's preview (§3.9: explicit and previewed), until confirmed or dismissed.
    pub preview: Option<ExportPreview>,
    /// The run's kept checkpoints (RFC 009 rule 3), as the Host last listed them.
    pub checkpoints: Vec<CheckpointSummary>,
    /// The last confirmed export: the file to hand to the person, taken by the shell.
    pub exported: Option<(FileKind, String, Vec<u8>)>,
    /// The latest report on each actor's channels: (at, payload).
    reports: BTreeMap<u32, BTreeMap<u32, (At, Vec<u8>)>>,
    /// The world's events so far: (at, target, event, detail).
    events: Vec<(At, u32, String, String)>,
    outbox: Vec<Message>,
    /// Trace records fetched so far, and whether a window is on its way.
    trace_seen: u64,
    window_pending: bool,
    /// A `payload` or `decode` answer on its way for the message inspector.
    message_pending: Option<(usize, usize)>,
    /// A `load-scenario` on its way: its refusals go to the draft's fields.
    loading_draft: bool,
    /// Trace records of the run the Host holds, from the last progress or state.
    records_available: u64,
}

impl ViewModel {
    pub fn new() -> Self {
        Self {
            draft: Draft::new("scenario"),
            ..Self::default()
        }
    }

    /// The messages the shell should send now, in order (they are gone from here).
    pub fn take_outbox(&mut self) -> Vec<Message> {
        std::mem::take(&mut self.outbox)
    }
    fn send(&mut self, m: Message) {
        self.outbox.push(m);
    }
    fn command(&mut self, c: Command) {
        self.outbox.push(Message::Command(c));
    }
    fn query(&mut self, q: Query) {
        self.outbox.push(Message::Query(q));
    }

    /// The shell is connected to a Host: subscribe to everything a view reads and ask what
    /// the Host is.
    pub fn connect(&mut self) {
        self.send(Message::Subscribe(Subscription::Events));
        self.send(Message::Subscribe(Subscription::Progress));
        self.send(Message::Subscribe(Subscription::Telemetry {
            channels: Vec::new(),
            keep_one_in: 1,
        }));
        self.send(Message::Subscribe(Subscription::Display));
        self.query(Query::Support);
        self.query(Query::Capacity);
        self.query(Query::Scenario);
        self.query(Query::RunState);
    }

    // ---- what a person does (§3.3 to §3.5, §3.9, §3.10) ----

    /// Open a manifest or a package.
    pub fn open_scenario(&mut self, bytes: Vec<u8>) {
        self.loading_draft = false;
        self.command(Command::LoadScenario { bytes });
        self.query(Query::Scenario);
        self.query(Query::RunState);
    }
    /// Load the draft as the scenario: the Host's refusals come back to the fields.
    pub fn load_draft(&mut self) {
        self.draft.commit_all();
        self.draft.apply_errors(&[]);
        let text = self.draft.to_toml();
        self.loading_draft = true;
        self.command(Command::LoadScenario {
            bytes: text.into_bytes(),
        });
        self.query(Query::Scenario);
        self.query(Query::RunState);
    }
    /// Supply a module the scenario references by hash.
    pub fn load_module(&mut self, bytes: Vec<u8>) {
        self.command(Command::LoadModule { bytes });
        self.query(Query::Scenario);
        self.query(Query::RunState);
    }
    /// Admit a module from a file, to show its manifest before it can be placed (§3.9).
    pub fn admit(&mut self, bytes: Vec<u8>) {
        self.command(Command::Admit { bytes });
    }
    pub fn new_run(&mut self, seed: Option<u64>) {
        self.command(Command::NewRun {
            seed: seed.map_or(SeedChoice::Choose, SeedChoice::Given),
            residency: florn_control::Residency::Resident,
        });
        self.query(Query::RunState);
    }
    /// Run at the chosen speed until paused or until `end`.
    pub fn run(&mut self) {
        let end = self.scenario.as_ref().map_or(u64::MAX, |s| s.end);
        self.command(Command::Advance {
            unit: Unit::ToTick(end),
        });
        self.query(Query::RunState);
    }
    pub fn pause(&mut self) {
        self.command(Command::Pause);
        self.query(Query::RunState);
    }
    /// Step by the unit shown on the control.
    pub fn step(&mut self, unit: Unit) {
        self.command(Command::Advance { unit });
        self.query(Query::RunState);
    }
    pub fn set_speed(&mut self, pacing: Pacing) {
        self.command(Command::SetPacing { pacing });
        self.query(Query::RunState);
    }
    /// End the run and return to tick 0 with the same inputs.
    pub fn reset(&mut self) {
        if matches!(self.run.state, RunState::Running | RunState::Paused) {
            self.command(Command::EndRun);
        }
        let seed = self.run.seed;
        self.new_run(seed);
    }
    /// Advance so that the events of the selected timeline entry's instant are handled.
    pub fn step_to(&mut self, entry: usize) {
        if let Some(e) = self.timeline.entries.get(entry) {
            let tick = e.at.tick.saturating_add(1);
            self.timeline.selected = Some(entry);
            self.command(Command::Advance {
                unit: Unit::ToTick(tick),
            });
            self.query(Query::RunState);
        }
    }
    /// An intervention of a domain's kind on a target (§3.5): the control that causes it says
    /// so; the Host applies it at the next instant boundary and reports where.
    pub fn intervene(
        &mut self,
        target: u32,
        kind: &str,
        fields: Vec<(String, FieldValue)>,
        at: Option<u64>,
    ) {
        let content = intervention_content(kind, &fields);
        self.interventions.push(InterventionView {
            target,
            kind: kind.into(),
            content: fields
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        match v {
                            FieldValue::Boolean(b) => b.to_string(),
                            FieldValue::Integer(i) => i.to_string(),
                            FieldValue::Text(t) => t.clone(),
                        },
                    )
                })
                .collect(),
            requested_at: at,
            applied_at: None,
        });
        self.command(Command::Intervene {
            target,
            content,
            at,
        });
    }
    /// Take a link down or bring it up (RFC 008 rule 13, the `link` kind).
    pub fn set_link(&mut self, connection: u32, up: bool) {
        self.intervene(
            connection,
            "link",
            vec![("enabled".into(), FieldValue::Boolean(up))],
            None,
        );
    }
    /// Take an actor down or bring it up (the `node` kind).
    pub fn set_node(&mut self, actor: u32, up: bool) {
        self.intervene(
            actor,
            "node",
            vec![("up".into(), FieldValue::Boolean(up))],
            None,
        );
    }
    /// Preview an export (§3.9: explicit and previewed), then write it with `confirm_export`.
    pub fn preview_export(&mut self, kind: ExportKind) {
        self.command(Command::Export {
            kind,
            confirmed: false,
        });
    }
    pub fn confirm_export(&mut self, kind: ExportKind) {
        self.command(Command::Export {
            kind,
            confirmed: true,
        });
    }
    /// Replay a run record (§3.10): the interface says the recorded interventions apply. A run
    /// in progress ends first; its interventions are kept (§3.3).
    pub fn replay(&mut self, record: Vec<u8>) {
        if matches!(self.run.state, RunState::Running | RunState::Paused) {
            self.command(Command::EndRun);
        }
        self.command(Command::Replay { record });
        self.query(Query::RunState);
    }
    /// The exported file, once; the shell writes it out.
    pub fn take_exported(&mut self) -> Option<(FileKind, String, Vec<u8>)> {
        self.exported.take()
    }
    pub fn dismiss_preview(&mut self) {
        self.preview = None;
    }
    pub fn dismiss(&mut self, notice: usize) {
        if notice < self.notices.len() {
            self.notices.remove(notice);
        }
    }

    // ---- selection and inspectors (§3.6) ----

    /// Select; the inspector's fields are set from the model now and not while a person
    /// types into them (rule 39).
    pub fn select(&mut self, s: Selection) {
        self.selection = s;
        self.message_pending = None;
        self.inspector = match s {
            Selection::None => Inspector::None,
            Selection::Actor(id) => self.actor_inspector(id),
            Selection::Connection(id) => self.connection_inspector(id),
            Selection::Message { entry, send } => self.message_inspector(entry, send),
            Selection::World => Inspector::World(inspector::WorldInspector {
                domains: self
                    .scenario
                    .as_ref()
                    .map(|s| s.domains.clone())
                    .unwrap_or_default(),
                classes: self.timeline.class_names.clone(),
                events: self.events.clone(),
            }),
        };
        if let Selection::Actor(id) = s {
            self.query(Query::Actor { id });
        }
    }
    /// Keyboard: the next or previous actor in list order (the list is one Tab stop).
    pub fn select_step(&mut self, step: i32) {
        let n = self.topology.nodes.len() as i32;
        if n == 0 {
            return;
        }
        let cur = match self.selection {
            Selection::Actor(id) => self.topology.node_index(id).map_or(-1, |i| i as i32),
            _ => -1,
        };
        let next = (cur + step).rem_euclid(n) as usize;
        self.select(Selection::Actor(self.topology.nodes[next].id));
    }

    fn module_of(&self, actor: u32) -> Option<&ModuleSummary> {
        let s = self.scenario.as_ref()?;
        let a = s.actors.iter().find(|a| a.id == actor)?;
        let m = a.module.as_ref()?;
        s.modules.iter().find(|x| x.name == *m)
    }

    fn actor_inspector(&self, id: u32) -> Inspector {
        let Some(s) = &self.scenario else {
            return Inspector::None;
        };
        let Some(a) = s.actors.iter().find(|a| a.id == id) else {
            return Inspector::None;
        };
        let module = self.module_of(id).cloned();
        let node = self.topology.node(id);
        Inspector::Actor(Box::new(inspector::ActorInspector {
            summary: a.clone(),
            config: inspector::config_fields(a, module.as_ref()),
            module,
            alive: node.is_some_and(|n| !n.faulted),
            down: node.is_some_and(|n| n.down),
            faults: Vec::new(),
            reports: self.reports_of(id),
        }))
    }

    fn reports_of(&self, actor: u32) -> Vec<inspector::ReportView> {
        let Some(by_channel) = self.reports.get(&actor) else {
            return Vec::new();
        };
        let module = self.module_of(actor);
        by_channel
            .iter()
            .map(|(channel, (at, payload))| {
                let layout = module.and_then(|m| m.channels.iter().find(|c| c.channel == *channel));
                inspector::ReportView {
                    channel: *channel,
                    name: layout.map(|c| c.name.clone()).unwrap_or_default(),
                    at: *at,
                    fields: layout
                        .map(|c| inspector::decode_report(&c.fields, payload))
                        .unwrap_or_default(),
                }
            })
            .collect()
    }

    fn connection_inspector(&self, id: u32) -> Inspector {
        let Some(s) = &self.scenario else {
            return Inspector::None;
        };
        let Some(c) = s.connections.iter().find(|c| c.id == id) else {
            return Inspector::None;
        };
        let index = self.topology.link_index(id);
        Inspector::Connection(inspector::ConnectionInspector {
            summary: c.clone(),
            up: self.topology.link(id).is_none_or(|l| l.up),
            attrs: inspector::attr_fields(c),
            in_flight: index.map_or(0, |i| self.topology.in_flight(i)),
        })
    }

    fn message_inspector(&mut self, entry: usize, send: usize) -> Inspector {
        let Some(e) = self.timeline.entries.get(entry) else {
            return Inspector::None;
        };
        let Some(s) = e.sends.get(send) else {
            return Inspector::None;
        };
        let protocol = self.module_of(e.target).and_then(|m| {
            m.tables.iter().find_map(|(_, fields)| {
                fields
                    .iter()
                    .find(|(k, _)| k == "protocol")
                    .map(|(_, v)| v.clone())
            })
        });
        let m = inspector::MessageInspector {
            entry,
            send: s.clone(),
            origin: e.target,
            at: e.at,
            protocol,
            payload: None,
            decoded: None,
            pending: true,
        };
        self.message_pending = Some((entry, send));
        self.query(Query::Payload {
            digest: s.digest.clone(),
        });
        Inspector::Message(m)
    }

    /// A field of the inspector committed: an actor's configuration before a run changes the
    /// draft; during a run nothing here changes the world (interventions do, by name).
    pub fn commit_field(&mut self, field: usize) -> bool {
        match &mut self.inspector {
            Inspector::Actor(a) => a.config.get_mut(field).is_some_and(Field::commit),
            Inspector::Connection(c) => c.attrs.get_mut(field).is_some_and(Field::commit),
            _ => false,
        }
    }

    // ---- what the Host says ----

    /// Every message the Host sends: a reply, a notification, a report, a progress.
    pub fn apply(&mut self, m: &Message) {
        match m {
            Message::Answer(a) => self.answer(a),
            Message::Notification(n) => self.notification(n),
            Message::Telemetry(t) => self.telemetry(t),
            Message::Progress(p) => self.progress(p),
            Message::Command(_)
            | Message::Query(_)
            | Message::Subscribe(_)
            | Message::Unsubscribe(_) => {}
        }
    }

    /// A display batch (RFC 005 rule 14): the 60 Hz path, straight to the topology.
    pub fn display(&mut self, records: &[Record]) {
        self.topology.apply_display(records);
    }

    fn answer(&mut self, a: &Answer) {
        match a {
            Answer::Scenario(s) => {
                self.scenario = s.clone();
                match s {
                    Some(s) => {
                        self.topology = Topology::from_scenario(s);
                        self.timeline = Timeline {
                            class_names: s.classes.clone(),
                            ..Timeline::default()
                        };
                        self.timeline.push_changes(&s.changes);
                        if !self.loading_draft {
                            self.draft = Draft::from_scenario(s);
                        }
                    }
                    None => {
                        self.topology = Topology::default();
                        self.timeline = Timeline::default();
                    }
                }
                self.loading_draft = false;
                self.forget_run();
                self.select(self.selection);
            }
            Answer::RunState(r) => self.run_state(r),
            Answer::Actor(info) => {
                if let (Some(info), Inspector::Actor(a)) = (info, &mut self.inspector)
                    && a.summary.id == info.summary.id
                {
                    a.alive = info.alive;
                    a.faults = info.faults.clone();
                    // The declared values changed only if the scenario did; the fields keep
                    // what a person typed (rule 39).
                    for (k, v) in &info.summary.config {
                        if let Some(f) = a.config.iter_mut().find(|f| f.name == *k) {
                            f.refresh(v);
                        }
                    }
                    let _: &ActorInfo = info;
                }
            }
            Answer::TraceWindow(entries) => {
                self.timeline.push_trace(self.trace_seen, entries);
                self.trace_seen += entries.len() as u64;
                self.window_pending = false;
                self.fetch_trace();
            }
            Answer::Capacity(c) => self.capacity = Some(c.clone()),
            Answer::Support(s) => self.support = Some(s.clone()),
            Answer::Payload(bytes) => {
                if let (Some((entry, send)), Inspector::Message(m)) =
                    (self.message_pending, &mut self.inspector)
                    && (m.entry, m.send.seq) == (entry, m.send.seq)
                {
                    let _ = send;
                    m.payload = bytes.clone();
                    match (&m.protocol, bytes) {
                        (Some(p), Some(b)) => {
                            let q = Query::Decode {
                                protocol: p.clone(),
                                payload: b.clone(),
                            };
                            self.outbox.push(Message::Query(q));
                        }
                        _ => {
                            m.pending = false;
                            self.message_pending = None;
                        }
                    }
                }
            }
            Answer::Decoded(fields) => {
                if let Inspector::Message(m) = &mut self.inspector
                    && self.message_pending.is_some()
                {
                    m.decoded = fields.clone();
                    m.pending = false;
                    self.message_pending = None;
                }
            }
            Answer::Admitted {
                module,
                admitted_sha256: _,
            } => {
                self.admitted
                    .retain(|m| m.source_sha256 != module.source_sha256);
                self.admitted.push(module.clone());
                self.draft.add_module(draft::DraftModule {
                    name: module.name.clone(),
                    sha256: module.source_sha256.clone(),
                    contract: module.contract.clone(),
                    summary: Some(module.clone()),
                });
            }
            Answer::Preview(p) => self.preview = Some(p.clone()),
            Answer::Exported { kind, name, bytes } => {
                self.preview = None;
                self.exported = Some((*kind, name.clone(), bytes.clone()));
            }
            Answer::Done => {}
            Answer::Checkpoints(list) => self.checkpoints = list.clone(),
        }
    }

    fn run_state(&mut self, r: &RunStateInfo) {
        // A new run: another identity, or the counts started again (a replay keeps the
        // recorded run's identity).
        let new_run = r.run_id.is_some()
            && (r.run_id != self.run.run_id
                || r.records < self.run.records
                || r.handled < self.run.handled);
        if new_run {
            self.forget_run();
        }
        self.run = RunView {
            state: r.state,
            now: r.now,
            handled: r.handled,
            records: r.records,
            chain: r.chain.clone(),
            seed: r.seed,
            pacing: r.pacing,
            ended: r.ended.clone(),
            reproducible: r.reproducible,
            run_id: r.run_id.clone(),
        };
        self.records_available = r.records;
        self.fetch_trace();
    }

    /// A new run: what the previous one told is forgotten; the scenario and the draft stay.
    fn forget_run(&mut self) {
        self.topology.reset_run();
        self.timeline.clear_run();
        if !self.interventions.is_empty() {
            self.kept = std::mem::take(&mut self.interventions);
        }
        self.reports.clear();
        self.events.clear();
        self.trace_seen = 0;
        self.window_pending = false;
        self.records_available = 0;
        self.status.dropped = 0;
        self.status.replay = None;
        self.status.limited_by = None;
    }

    fn fetch_trace(&mut self) {
        if self.window_pending || self.records_available <= self.trace_seen {
            return;
        }
        let count = (self.records_available - self.trace_seen).min(u64::from(Query::MAX_WINDOW));
        self.window_pending = true;
        self.query(Query::TraceWindow {
            from: self.trace_seen,
            count: count as u32,
        });
    }

    fn progress(&mut self, p: &Progress) {
        if p.records < self.run.records || p.handled < self.run.handled {
            self.forget_run();
        }
        // The run ended under pacing: its digest and end are in the state answer.
        if p.state == RunState::Ended && self.run.state != RunState::Ended {
            self.query(Query::RunState);
        }
        self.run.state = p.state;
        self.run.now = Some(p.now);
        self.run.handled = p.handled;
        self.run.records = p.records;
        self.records_available = p.records;
        self.fetch_trace();
    }

    fn telemetry(&mut self, t: &TelemetryRecord) {
        self.reports
            .entry(t.actor)
            .or_default()
            .insert(t.channel, (t.at, t.payload.clone()));
        // The channels the view reads are the timeline's too (NET-009).
        if let Some(c) = self
            .module_of(t.actor)
            .and_then(|m| m.channels.iter().find(|c| c.channel == t.channel))
            && matches!(c.name.as_str(), "neighbour" | "route")
        {
            let fields = inspector::decode_report(&c.fields, &t.payload);
            let name = c.name.clone();
            self.timeline.push_report(t.at, t.actor, &name, &fields);
        }
        if let Selection::Actor(id) = self.selection
            && id == t.actor
        {
            let reports = self.reports_of(id);
            if let Inspector::Actor(a) = &mut self.inspector {
                a.reports = reports;
            }
        }
    }

    fn notification(&mut self, n: &Notification) {
        let get = |name: &str| n.params.iter().find(|(k, _)| k == name).map(|(_, v)| v);
        match n.code.as_str() {
            "telemetry.transport" => {
                if let Some(Value::Enum(t)) = get("transport") {
                    self.status.transport = Some(t.clone());
                }
                return;
            }
            "telemetry.dropped" => {
                if let Some(Value::Count(c)) = get("records") {
                    self.status.dropped += c;
                }
                return;
            }
            "host.limited-by" => {
                if let Some(Value::Enum(w)) = get("what") {
                    self.status.limited_by = Some(w.clone());
                }
                return;
            }
            "domain.event" => {
                let target = match get("target") {
                    Some(Value::Identity(i)) => *i,
                    _ => 0,
                };
                let event = match get("event") {
                    Some(Value::Name(e)) => e.clone(),
                    _ => String::new(),
                };
                let detail = match get("detail") {
                    Some(Value::Text(t)) => t.clone(),
                    _ => String::new(),
                };
                match event.as_str() {
                    "link-changed" => self.topology.link_changed(target, &detail),
                    "node-changed" => self.topology.node_changed(target, &detail),
                    _ => {}
                }
                self.events
                    .push((n.at.unwrap_or_default(), target, event, detail));
                self.timeline.push_notification(n);
                self.refresh_selected();
                return;
            }
            "intervention.applied" => {
                if let Some(Value::Instant(at)) = get("at") {
                    match self
                        .interventions
                        .iter_mut()
                        .find(|i| i.applied_at.is_none())
                    {
                        Some(i) => i.applied_at = Some(*at),
                        // Applied without a request of this view: a replay's (§3.10), or
                        // another front end's.
                        None => self.interventions.push(InterventionView {
                            target: n.scope.connection.or(n.scope.actor).unwrap_or(0),
                            kind: match get("kind") {
                                Some(Value::Name(k)) => k.clone(),
                                _ => String::new(),
                            },
                            content: Vec::new(),
                            requested_at: None,
                            applied_at: Some(*at),
                        }),
                    }
                }
                return;
            }
            "actor.faulted" => {
                if let Some(a) = n.scope.actor {
                    self.topology.faulted(a);
                    self.refresh_selected();
                }
            }
            "replay.matched" | "replay.diverged" => {
                self.status.replay = Some(n.code.clone());
            }
            "run.ended" => {
                if let Some(Value::Enum(r)) = get("reason") {
                    self.run.ended = Some(r.clone());
                }
                if self.run.state != RunState::Ended {
                    self.query(Query::RunState);
                }
                self.run.state = RunState::Ended;
            }
            _ => {}
        }
        if self.loading_draft && (n.scope.field.is_some() || n.scope.command.is_some()) {
            // The draft's refusals stay with the draft (and the loading ends with the answer).
            let mut errors = self.draft.errors.clone();
            errors.push(n.clone());
            self.draft.apply_errors(&errors);
        }
        self.timeline.push_notification(n);
        if matches!(
            n.severity,
            florn_control::Severity::Error | florn_control::Severity::Warning
        ) {
            self.notices.push(Notice {
                notification: n.clone(),
            });
        }
    }

    /// The selected element's state changed: refresh the inspector's facts, not its fields.
    fn refresh_selected(&mut self) {
        match (self.selection, &mut self.inspector) {
            (Selection::Actor(id), Inspector::Actor(a)) => {
                if let Some(n) = self.topology.node(id) {
                    a.alive = !n.faulted;
                    a.down = n.down;
                }
            }
            (Selection::Connection(id), Inspector::Connection(c)) => {
                c.up = self.topology.link(id).is_none_or(|l| l.up);
                c.in_flight = self
                    .topology
                    .link_index(id)
                    .map_or(0, |i| self.topology.in_flight(i));
            }
            (Selection::World, Inspector::World(w)) => w.events = self.events.clone(),
            _ => {}
        }
    }
}
