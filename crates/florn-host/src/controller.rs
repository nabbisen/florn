//! The Host's side of the control interface (RFC 005 §6; external design §4): every front end
//! drives the Host through this and nothing else. A command is validated before it acts (rule
//! 27); an invalid one changes nothing and produces its code. Pacing is the caller's: `pump`
//! computes a slice when the front end decides to call it.

use std::collections::BTreeMap;

use florn_admission::{Admission, ClassLimits, ModuleManifest};
use florn_control::display::{DisplayPath, Record as DisplayRecord, Transport};
use florn_control::message::{
    ActorInfo, ActorSummary, CapacityInfo, ChangeSummary, ChannelSummary, ConnectionSummary,
    ExportPreview, FaultInfo, ModuleSummary, ParamSummary, PortSummary, RunStateInfo,
    ScenarioSummary, SendSummary, SupportInfo, TelemetryRecord, TraceEntry, control_invalid,
    control_not_now,
};
use florn_control::{
    Answer, At, CONTROL_VERSION, Code, Command, Envelope, ExportKind, FileKind, Message, Pacing,
    Progress, Query, RunState, Scope, SeedChoice, Subscription, SubscriptionKind, Unit, Value,
    decode, encode,
};
use florn_engine::Engine;
use florn_kernel::{Advance, Record};
use florn_model::canonical::hex;
use florn_model::{Manifest, Package, PackageMeta};
use florn_platform::Platform;

use crate::domain::{self, Domains};
use crate::record::{self, Contents};
use crate::run::{HostError, Options, Run, Seed};

/// What an export built: the file name, the bytes, whether payloads are in, the modules by hash.
type Built = Result<(String, Vec<u8>, bool, Vec<(String, String)>), String>;

/// A loaded scenario: the text and the parsed manifest.
struct Loaded {
    text: String,
    manifest: Manifest,
}

#[derive(Default)]
struct Subscriptions {
    telemetry: Option<(Vec<u32>, u32)>,
    telemetry_seen: u32,
    events: bool,
    progress: bool,
    display: bool,
}

pub struct Controller<E: Engine> {
    engine: Option<E>,
    admission: Option<Admission>,
    platform: Box<dyn Platform>,
    domains: Domains,
    options: Options,
    scenario: Option<Loaded>,
    /// Module bytes by their SHA-256 (hex).
    modules: BTreeMap<String, Vec<u8>>,
    run: Option<Run<E>>,
    state: RunState,
    pacing: Pacing,
    /// A paced `advance` to a tick in progress.
    target: Option<u64>,
    subs: Subscriptions,
    log_seen: usize,
    dropped_seen: u64,
    /// The display transport of this Host, when its front end draws (RFC 005 rule 14).
    transport: Option<Transport>,
}

impl<E: Engine> Controller<E> {
    pub fn new(
        engine: E,
        mut admission: Admission,
        platform: Box<dyn Platform>,
        domains: Domains,
        options: Options,
    ) -> Self {
        // The domains' worlds are admitted beside the core's (RFC 008 rule 6).
        for w in domains.worlds() {
            admission.add_world(w);
        }
        Self {
            engine: Some(engine),
            admission: Some(admission),
            platform,
            domains,
            options,
            scenario: None,
            modules: BTreeMap::new(),
            run: None,
            state: RunState::NoScenario,
            pacing: Pacing::Fastest,
            target: None,
            subs: Subscriptions::default(),
            log_seen: 0,
            dropped_seen: 0,
            transport: None,
        }
    }

    pub fn state(&self) -> RunState {
        self.state
    }
    pub fn run(&self) -> Option<&Run<E>> {
        self.run.as_ref()
    }
    pub fn pacing(&self) -> Pacing {
        self.pacing
    }

    /// One encoded message in, the encoded replies out: the transport-facing form.
    pub fn handle_bytes(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        let replies = match decode(bytes) {
            Ok(env) => self.handle(env),
            Err(e) => vec![Message::Notification(e.notification())],
        };
        replies
            .into_iter()
            .map(|m| encode(&Envelope::new(m)))
            .collect()
    }

    /// One message in, the replies out: answers, notifications, and what subscriptions deliver.
    pub fn handle(&mut self, env: Envelope) -> Vec<Message> {
        if env.version != CONTROL_VERSION {
            return vec![Message::Notification(
                florn_control::message::control_version(env.version),
            )];
        }
        let mut out = Vec::new();
        match env.message {
            Message::Command(c) => self.command(c, &mut out),
            Message::Query(q) => match q.validate() {
                Ok(()) => out.push(Message::Answer(self.query(&q))),
                Err(n) => out.push(Message::Notification(*n)),
            },
            Message::Subscribe(s) => match s.validate() {
                Ok(()) => {
                    match s {
                        Subscription::Telemetry {
                            channels,
                            keep_one_in,
                        } => self.subs.telemetry = Some((channels, keep_one_in)),
                        Subscription::Events => self.subs.events = true,
                        Subscription::Progress => self.subs.progress = true,
                        Subscription::Display => self.set_display(true),
                    }
                    out.push(Message::Answer(Answer::Done));
                }
                Err(n) => out.push(Message::Notification(*n)),
            },
            Message::Unsubscribe(k) => {
                match k {
                    SubscriptionKind::Telemetry => self.subs.telemetry = None,
                    SubscriptionKind::Events => self.subs.events = false,
                    SubscriptionKind::Progress => self.subs.progress = false,
                    SubscriptionKind::Display => self.set_display(false),
                }
                out.push(Message::Answer(Answer::Done));
            }
            // A Host receives none of these; they are refused as a front end would refuse a command.
            Message::Notification(_)
            | Message::Answer(_)
            | Message::Telemetry(_)
            | Message::Progress(_) => out.push(Message::Notification(control_invalid(
                "?",
                "unknown-command",
                "message",
            ))),
        }
        out
    }

    fn notify(&self, out: &mut Vec<Message>, n: florn_control::Notification) {
        out.push(Message::Notification(n));
    }

    /// New diagnostic-log entries since the last delivery, as notifications.
    fn drain_log(&mut self, out: &mut Vec<Message>) {
        if let Some(run) = &self.run {
            let entries = run.log().entries();
            for e in &entries[self.log_seen.min(entries.len())..] {
                out.push(Message::Notification(e.notification.clone()));
            }
            self.log_seen = entries.len();
        }
    }

    fn drain_telemetry(&mut self, out: &mut Vec<Message>) {
        let Some(run) = &mut self.run else { return };
        let records = run.take_telemetry();
        let Some((channels, keep)) = &self.subs.telemetry else {
            return;
        };
        for t in records {
            if !channels.is_empty() && !channels.contains(&t.channel) {
                continue;
            }
            self.subs.telemetry_seen += 1;
            if !self.subs.telemetry_seen.is_multiple_of(*keep) {
                continue;
            }
            out.push(Message::Telemetry(TelemetryRecord {
                at: at(t.at),
                actor: t.actor,
                channel: t.channel,
                payload: t.payload,
            }));
        }
    }

    fn progress(&self, out: &mut Vec<Message>) {
        if let (true, Some(run)) = (self.subs.progress, &self.run) {
            out.push(Message::Progress(Progress {
                state: self.state,
                now: at(run.kernel().now()),
                handled: run.kernel().handled(),
                records: run.kernel().trace.records(),
            }));
        }
    }

    /// After a run advanced: the state, the notifications the advance produced (a command's
    /// reply always carries them; a `pump` carries them under the `events` subscription), then
    /// what the other subscriptions deliver.
    fn after_advance(&mut self, out: &mut Vec<Message>, events: bool) {
        if let Some(run) = &self.run
            && run.ended().is_some()
        {
            self.state = RunState::Ended;
            self.target = None;
        }
        if events {
            self.drain_log(out);
        } else if let Some(run) = &self.run {
            self.log_seen = run.log().entries().len();
        }
        self.drain_telemetry(out);
        // Records the display path dropped since the last advance (RFC 005 rule 14).
        if let Some(run) = &self.run {
            let dropped = run.telemetry_dropped();
            if dropped > self.dropped_seen {
                let n = Code::TelemetryDropped.notification(
                    Scope::run(),
                    vec![
                        Value::Count(dropped - self.dropped_seen),
                        Value::Enum("messages".into()),
                    ],
                );
                self.dropped_seen = dropped;
                out.push(Message::Notification(n));
            }
        }
        self.progress(out);
    }

    /// A slice of a paced advance (state `running`): at most `max_ticks` ticks, then the
    /// subscriptions' messages. The front end calls this when its pacing says so.
    pub fn pump(&mut self, max_ticks: u64) -> Vec<Message> {
        let mut out = Vec::new();
        if self.state != RunState::Running {
            return out;
        }
        let Some(target) = self.target else {
            self.state = RunState::Paused;
            return out;
        };
        if let Some(run) = &mut self.run {
            // From the tick reached, not the last event's: a slice advances by its ticks even
            // where no event lies (the kernel's clock stays at the last event handled).
            let now = run.reached().max(run.kernel().now().tick);
            let to = now.saturating_add(max_ticks.max(1)).min(target);
            run.advance(Advance::ToTick(to));
            // The unit is complete when every event before the target is handled.
            if to >= target || !run.kernel().has_next() {
                self.state = RunState::Paused;
                self.target = None;
            }
        }
        let events = self.subs.events;
        self.after_advance(&mut out, events);
        out
    }

    /// The display transport this Host's front end reads from (RFC 005 rule 14); each run's
    /// log then opens with `telemetry.transport`.
    pub fn set_transport(&mut self, transport: Transport) {
        self.transport = Some(transport);
    }

    fn set_display(&mut self, on: bool) {
        self.subs.display = on;
        if let Some(run) = &mut self.run {
            run.set_display(on);
        }
    }

    /// The display batch since the last take (rule 14): the front end's transport carries it.
    /// Empty without the `display` subscription or a run.
    pub fn take_display(&mut self) -> Vec<DisplayRecord> {
        self.run.as_mut().map(Run::take_display).unwrap_or_default()
    }

    /// What the `events` subscription has not delivered yet: the notifications logged since the
    /// last reply, such as the display path's drops reported after the run's last advance.
    /// Empty without the subscription (the log keeps them either way).
    pub fn drain(&mut self) -> Vec<Message> {
        let mut out = Vec::new();
        if self.subs.events {
            self.drain_log(&mut out);
        } else if let Some(run) = &self.run {
            self.log_seen = run.log().entries().len();
        }
        out
    }

    /// Records the display path dropped (rule 14): the transport counted them, the run's log
    /// records them as `telemetry.dropped`, and the trace is untouched.
    pub fn display_dropped(&mut self, records: u64, path: DisplayPath) {
        if let (Some(run), true) = (&mut self.run, records > 0) {
            run.notify_now(Code::TelemetryDropped.notification(
                Scope::run(),
                vec![Value::Count(records), Value::Enum(path.as_str().into())],
            ));
        }
    }

    fn not_now(&self, command: &str) -> florn_control::Notification {
        control_not_now(command, self.state)
    }

    fn command(&mut self, c: Command, out: &mut Vec<Message>) {
        if let Err(n) = c.validate() {
            self.notify(out, *n);
            return;
        }
        let name = c.name();
        match c {
            Command::LoadScenario { bytes } => {
                if matches!(self.state, RunState::Running | RunState::Paused) {
                    return self.notify(out, self.not_now(name));
                }
                self.load_scenario(&bytes, out);
            }
            Command::LoadModule { bytes } => {
                if matches!(self.state, RunState::Running | RunState::Paused) {
                    return self.notify(out, self.not_now(name));
                }
                // The module maximum applies before anything is held (RFC 006 section 7).
                let max = florn_model::package::MAX_MODULE_BYTES;
                if bytes.len() as u64 > max {
                    return self.notify(
                        out,
                        Code::ModuleTooLarge.notification(
                            Scope::file("module"),
                            vec![
                                Value::Enum("bytes".into()),
                                Value::Count(bytes.len() as u64),
                                Value::Count(max),
                            ],
                        ),
                    );
                }
                let h = hex(&florn_admission::sha256(&bytes));
                self.modules.insert(h, bytes);
                self.refresh_ready();
                out.push(Message::Answer(Answer::Done));
            }
            Command::Admit { bytes } => {
                let Some(admission) = self.admission.as_mut() else {
                    return self.notify(out, self.not_now(name));
                };
                // Under the loaded scenario's default class when there is one, so that the hash
                // printed is the hash a run of that scenario records; else the Host's defaults.
                let limits = match &self.scenario {
                    Some(l) => ClassLimits::resolve(
                        &l.manifest.limits_for("default"),
                        &self.options.class_defaults,
                        &self.options.host,
                    ),
                    None => self.options.class_defaults,
                };
                match admission.admit("module", &bytes, &limits) {
                    Ok(a) => {
                        // A module from a file has no `[modules]` name yet: its kind names it.
                        let summary =
                            module_summary(&a.manifest.kind, &a.manifest, &a.source_sha256);
                        out.push(Message::Answer(Answer::Admitted {
                            module: summary,
                            admitted_sha256: a.admitted_hex(),
                        }));
                        out.push(Message::Notification(Code::AdmissionResult.notification(
                            Scope::file("module"),
                            vec![
                                Value::Name("module".into()),
                                Value::Text(a.admitted_hex()),
                                Value::Enum("validated".into()),
                            ],
                        )));
                    }
                    Err(e) => {
                        for err in &e.0 {
                            out.push(Message::Notification(
                                florn_control::Notification::from_error(
                                    err.code(),
                                    err.params(),
                                    err.field.clone(),
                                    Some("module"),
                                    florn_control::Ends::Command,
                                ),
                            ));
                        }
                    }
                }
            }
            Command::NewRun { seed } => {
                if !matches!(self.state, RunState::Ready | RunState::Ended) {
                    return self.notify(out, self.not_now(name));
                }
                self.new_run(seed, None, out);
            }
            Command::EndRun => {
                if !matches!(self.state, RunState::Running | RunState::Paused) {
                    return self.notify(out, self.not_now(name));
                }
                if let Some(run) = &mut self.run {
                    run.end_run();
                }
                self.target = None;
                self.after_advance(out, true);
                out.push(Message::Answer(Answer::Done));
            }
            Command::Advance { unit } => {
                if self.state != RunState::Paused {
                    return self.notify(out, self.not_now(name));
                }
                let Some(run) = &mut self.run else {
                    return self.notify(out, self.not_now(name));
                };
                run.set_wall_clock(self.platform.wall_clock_ms());
                match (unit, self.pacing) {
                    (Unit::OneEvent, _) => {
                        run.advance(Advance::OneEvent);
                    }
                    (Unit::OneInstant, _) => {
                        run.advance(Advance::OneInstant);
                    }
                    (Unit::OneTick, _) => {
                        run.advance(Advance::OneTick);
                    }
                    (Unit::ToTick(t), Pacing::Fastest) => {
                        run.advance(Advance::ToTick(t));
                    }
                    (Unit::ToTick(t), Pacing::TicksPerSecond(_)) => {
                        if t <= run.kernel().now().tick {
                            return self.notify(out, control_invalid(name, "out-of-range", "tick"));
                        }
                        self.target = Some(t);
                        self.state = RunState::Running;
                    }
                }
                self.after_advance(out, true);
                out.push(Message::Answer(Answer::Done));
            }
            Command::Pause => {
                if self.state != RunState::Running {
                    return self.notify(out, self.not_now(name));
                }
                self.state = RunState::Paused;
                self.target = None;
                out.push(Message::Answer(Answer::Done));
            }
            Command::SetPacing { pacing } => {
                self.pacing = pacing;
                out.push(Message::Answer(Answer::Done));
            }
            Command::Intervene {
                target,
                content,
                at,
            } => {
                if !matches!(self.state, RunState::Running | RunState::Paused) {
                    return self.notify(out, self.not_now(name));
                }
                let Some(run) = &mut self.run else {
                    return self.notify(out, self.not_now(name));
                };
                if let Some(t) = at
                    && (t < run.kernel().now().tick || t >= run.scenario().time.end)
                {
                    return self.notify(out, control_invalid(name, "out-of-range", "at"));
                }
                // The core's `message` and the domains' kinds alike (RFC 008 rule 4).
                if let Err(n) = run.intervene_map(at, target, &content) {
                    return self.notify(out, *n);
                }
                out.push(Message::Answer(Answer::Done));
            }
            Command::Export { kind, confirmed } => self.export(kind, confirmed, out),
            Command::Replay { record } => {
                if !matches!(self.state, RunState::Ready | RunState::Ended) {
                    return self.notify(out, self.not_now(name));
                }
                match record::read(&record) {
                    Ok(read) => self.new_run(
                        SeedChoice::Given(read.run.inputs.root_seed),
                        Some(read.run),
                        out,
                    ),
                    Err(reason) if reason == "file.too-large" => {
                        out.push(Message::Notification(Code::FileTooLarge.notification(
                            Scope::file("record"),
                            vec![
                                Value::Enum("run-record".into()),
                                Value::Bytes(record.len() as u64),
                                Value::Bytes(record::MAX_BYTES),
                            ],
                        )))
                    }
                    Err(reason) => {
                        out.push(Message::Notification(Code::FileUnreadable.notification(
                            Scope::file("record"),
                            vec![Value::Enum("run-record".into()), Value::Text(reason)],
                        )))
                    }
                }
            }
        }
    }

    fn load_scenario(&mut self, bytes: &[u8], out: &mut Vec<Message>) {
        // The ended run gives its engine and admission cache back: the next run needs them.
        if let Some(run) = self.run.take() {
            let (e, a) = run.into_parts();
            self.engine = Some(e);
            self.admission = Some(a);
        }
        self.scenario = None;
        self.state = RunState::NoScenario;
        let host = self.options.host;
        let is_zip = bytes.starts_with(b"PK\x03\x04");
        let loaded = if is_zip {
            Package::read(bytes, &host).map(|p| {
                for (h, b) in p.modules {
                    self.modules.insert(h, b);
                }
                if !p.missing.is_empty() {
                    out.push(Message::Notification(Code::PackagePartial.notification(
                        Scope::file("package"),
                        vec![Value::Count(p.missing.len() as u64)],
                    )));
                }
                Loaded {
                    text: p.manifest_text,
                    manifest: p.manifest,
                }
            })
        } else {
            match std::str::from_utf8(bytes) {
                Ok(text) => Manifest::parse(text, &host).map(|manifest| Loaded {
                    text: text.to_string(),
                    manifest,
                }),
                Err(e) => Err(florn_model::Errors(vec![florn_model::Error::new(
                    florn_model::Kind::FileUnreadable {
                        kind: "manifest",
                        reason: e.to_string(),
                    },
                )])),
            }
        };
        // The domains' validation (RFC 008 rule 1) follows the core's, at load.
        let loaded = loaded.and_then(|l| {
            let refusals = match self.domains.instantiate(&l.manifest) {
                Ok(d) => domain::validate(&d, &l.manifest),
                Err(n) => vec![*n],
            };
            if refusals.is_empty() {
                Ok(l)
            } else {
                self.state = RunState::Invalid;
                out.extend(refusals.into_iter().map(Message::Notification));
                Err(florn_model::Errors::default())
            }
        });
        match loaded {
            Ok(l) => {
                self.scenario = Some(l);
                self.refresh_ready();
                out.push(Message::Answer(Answer::Done));
            }
            Err(errors) => {
                self.state = RunState::Invalid;
                for e in errors.0 {
                    out.push(Message::Notification(
                        florn_control::Notification::from_error(
                            e.code(),
                            e.params(),
                            e.field.clone(),
                            Some(if is_zip { "package" } else { "manifest" }),
                            florn_control::Ends::Command,
                        ),
                    ));
                }
            }
        }
    }

    /// The modules the scenario references and the Host holds, by `[modules]` name; the missing ones.
    fn modules_for(&self, manifest: &Manifest) -> (BTreeMap<String, Vec<u8>>, Vec<String>) {
        let mut found = BTreeMap::new();
        let mut missing = Vec::new();
        for (name, m) in &manifest.modules {
            let h = hex(&m.sha256);
            match self.modules.get(&h) {
                Some(b) => {
                    found.insert(name.clone(), b.clone());
                }
                None => missing.push(h),
            }
        }
        (found, missing)
    }

    fn refresh_ready(&mut self) {
        if self.scenario.is_some() && matches!(self.state, RunState::NoScenario | RunState::Ready) {
            self.state = RunState::Ready;
        }
    }

    fn new_run(
        &mut self,
        seed: SeedChoice,
        replay: Option<crate::identity::RunToml>,
        out: &mut Vec<Message>,
    ) {
        let Some(loaded) = &self.scenario else {
            return self.notify(out, self.not_now("new-run"));
        };
        let (modules, missing) = self.modules_for(&loaded.manifest);
        if let Some(h) = missing.first() {
            return self.notify(
                out,
                control_invalid("new-run", "missing-field", &format!("modules.{h}")),
            );
        }
        // The previous run gives its engine and admission cache back.
        if let Some(run) = self.run.take() {
            let (e, a) = run.into_parts();
            self.engine = Some(e);
            self.admission = Some(a);
        }
        let (Some(engine), Some(admission)) = (self.engine.take(), self.admission.take()) else {
            return self.notify(out, self.not_now("new-run"));
        };
        let manifest = loaded.manifest.clone();
        let mut options = self.options.clone();
        options.display = self.subs.display;
        let result = match replay {
            Some(record) => Run::replay(
                engine,
                admission,
                self.platform.as_mut(),
                &self.domains,
                manifest,
                &modules,
                &record,
                options,
            ),
            None => {
                let seed = match seed {
                    SeedChoice::Given(s) => Seed::Given(s),
                    SeedChoice::Choose => Seed::Choose,
                };
                Run::try_new(
                    engine,
                    admission,
                    self.platform.as_mut(),
                    &self.domains,
                    manifest,
                    &modules,
                    seed,
                    options,
                )
                .map_err(|parts| {
                    // A failed start gives the engine back: the Host goes on (RFC 005 rule 10).
                    let (e, engine, admission) = *parts;
                    self.engine = Some(engine);
                    self.admission = Some(admission);
                    e
                })
            }
        };
        match result {
            Ok(mut run) => {
                self.log_seen = 0;
                // The run's log says what carries its display stream (rule 14): one line.
                if let Some(t) = self.transport {
                    run.notify_now(
                        Code::TelemetryTransport
                            .notification(Scope::run(), vec![Value::Enum(t.as_str().into())]),
                    );
                }
                self.run = Some(run);
                self.state = RunState::Paused;
                self.target = None;
                self.drain_log(out);
                out.push(Message::Answer(Answer::Done));
            }
            Err(e) => {
                self.state = RunState::Ready;
                match e {
                    HostError::InstanceFailed { actor, text } => {
                        out.push(Message::Notification(
                            Code::HostInstanceFailed.notification(
                                Scope::actor(actor),
                                vec![Value::Identity(actor), Value::Text(text)],
                            ),
                        ));
                    }
                    HostError::Refused(errors) => {
                        for err in errors.0 {
                            out.push(Message::Notification(
                                florn_control::Notification::from_error(
                                    err.code(),
                                    err.params(),
                                    err.field.clone(),
                                    None,
                                    florn_control::Ends::Command,
                                ),
                            ));
                        }
                    }
                    HostError::CheckpointMismatch {
                        what,
                        expected,
                        found,
                    } => {
                        out.push(Message::Notification(
                            Code::CheckpointMismatch.notification(
                                Scope::file("record"),
                                vec![
                                    Value::Enum(what.into()),
                                    Value::Text(expected),
                                    Value::Text(found),
                                ],
                            ),
                        ));
                    }
                    other => out.push(Message::Notification(
                        Code::HostInstanceFailed.notification(
                            Scope::run(),
                            vec![Value::Identity(0), Value::Text(other.to_string())],
                        ),
                    )),
                }
            }
        }
    }

    fn export(&mut self, kind: ExportKind, confirmed: bool, out: &mut Vec<Message>) {
        // A record is a run at rest: not while a paced advance is in progress.
        if self.state == RunState::Running {
            return self.notify(out, self.not_now("export"));
        }
        let file_kind = kind.file_kind();
        let built: Built = match kind {
            ExportKind::RunRecord { trace, payloads } => match &self.run {
                None => return self.notify(out, self.not_now("export")),
                Some(run) => {
                    let contents = Contents {
                        run: Some(run.run_toml()),
                        trace: if trace {
                            run.records().map(|r| r.to_vec())
                        } else {
                            None
                        },
                        payloads: if payloads {
                            Some(run.payloads().clone())
                        } else {
                            None
                        },
                        log: run.log().clone(),
                        readme: None,
                    };
                    let modules = contents
                        .run
                        .as_ref()
                        .map(|r| {
                            r.inputs
                                .modules
                                .iter()
                                .map(|m| (m.name.clone(), hex(&m.source_sha256)))
                                .collect()
                        })
                        .unwrap_or_default();
                    record::write(&contents).map(|b| {
                        (
                            format!("{}.{}", run.run_id(), file_kind.extension()),
                            b,
                            payloads,
                            modules,
                        )
                    })
                }
            },
            ExportKind::DiagnosticLog => match &self.run {
                None => return self.notify(out, self.not_now("export")),
                Some(run) => Ok((
                    format!("{}.{}", run.run_id(), file_kind.extension()),
                    run.log().to_jsonl().into_bytes(),
                    false,
                    Vec::new(),
                )),
            },
            ExportKind::Package => match &self.scenario {
                None => return self.notify(out, self.not_now("export")),
                Some(l) => {
                    let (found, _) = self.modules_for(&l.manifest);
                    let by_hash: BTreeMap<String, Vec<u8>> = found
                        .values()
                        .map(|b| (hex(&florn_admission::sha256(b)), b.clone()))
                        .collect();
                    let meta = PackageMeta {
                        partial: by_hash.len() < l.manifest.modules.len(),
                        created_by: Some(format!("florn {}", env!("CARGO_PKG_VERSION"))),
                        notes: None,
                    };
                    let modules = l
                        .manifest
                        .modules
                        .iter()
                        .map(|(n, m)| (n.clone(), hex(&m.sha256)))
                        .collect();
                    Package::write(&l.text, &l.manifest, &by_hash, &meta, None)
                        .map(|b| {
                            (
                                format!(
                                    "{}.{}",
                                    l.manifest.name.replace(' ', "-"),
                                    file_kind.extension()
                                ),
                                b,
                                false,
                                modules,
                            )
                        })
                        .map_err(|e| e.codes().join(","))
                }
            },
        };
        match built {
            Ok((name, bytes, payloads, modules)) => {
                let preview = ExportPreview {
                    kind: file_kind,
                    bytes: bytes.len() as u64,
                    payloads,
                    modules,
                };
                out.push(Message::Notification(Code::ExportPreview.notification(
                    Scope::file(&name),
                    vec![
                        Value::Enum(file_kind.as_str().into()),
                        Value::Bytes(bytes.len() as u64),
                        Value::Enum(if payloads { "yes" } else { "no" }.into()),
                    ],
                )));
                if confirmed {
                    out.push(Message::Answer(Answer::Exported {
                        kind: file_kind,
                        name,
                        bytes,
                    }));
                } else {
                    out.push(Message::Answer(Answer::Preview(preview)));
                }
            }
            Err(reason) => out.push(Message::Notification(Code::FileUnreadable.notification(
                Scope::file(file_kind.as_str()),
                vec![Value::Enum(file_kind.as_str().into()), Value::Text(reason)],
            ))),
        }
    }

    fn query(&mut self, q: &Query) -> Answer {
        match q {
            Query::Scenario => {
                let Some(l) = &self.scenario else {
                    return Answer::Scenario(None);
                };
                let manifest = l.manifest.clone();
                let (found, missing) = self.modules_for(&manifest);
                let mut classes: Vec<(u8, String)> = vec![
                    (0, "control".into()),
                    (crate::world::CLASS_TIMER, "timer".into()),
                    (crate::world::CLASS_MESSAGE, "message".into()),
                ];
                if let Ok(domains) = self.domains.instantiate(&manifest) {
                    for d in &domains {
                        classes.extend(d.classes().into_iter().map(|(c, n)| (c, n.to_string())));
                    }
                }
                classes.sort();
                let had_admission = self.admission.is_some();
                let mut admission = self.admission.take().unwrap_or_default();
                let modules = manifest
                    .modules
                    .iter()
                    .map(|(name, m)| {
                        match found
                            .get(name)
                            .and_then(|b| admission.admit(name, b, &ClassLimits::defaults()).ok())
                        {
                            Some(a) => module_summary(name, &a.manifest, &m.sha256),
                            None => ModuleSummary {
                                name: name.clone(),
                                source_sha256: hex(&m.sha256),
                                contract: m.contract.clone(),
                                kind: String::new(),
                                description: None,
                                port_kinds: Vec::new(),
                                params: Vec::new(),
                                channels: Vec::new(),
                                needs: Vec::new(),
                                tables: Vec::new(),
                            },
                        }
                    })
                    .collect();
                if had_admission {
                    self.admission = Some(admission);
                }
                Answer::Scenario(Some(ScenarioSummary {
                    name: manifest.name.clone(),
                    sha256: manifest.scenario_hash_hex(),
                    tick: manifest.time.tick.as_str().to_string(),
                    end: manifest.time.end,
                    tie: manifest.order.tie.as_str().to_string(),
                    seed: manifest.seed,
                    actors: manifest.actors.iter().map(actor_summary).collect(),
                    connections: manifest
                        .connections
                        .iter()
                        .map(|c| ConnectionSummary {
                            id: c.id,
                            name: c.name.clone(),
                            ports: c.ports.iter().map(|p| (p.actor, p.port)).collect(),
                            ty: c.r#type.clone(),
                            attrs: domain_map_text(&c.attrs),
                            per_port: c
                                .per_port
                                .iter()
                                .map(|(p, m)| ((p.actor, p.port), domain_map_text(m)))
                                .collect(),
                        })
                        .collect(),
                    modules,
                    missing_modules: missing,
                    domains: manifest.domains.clone(),
                    changes: manifest
                        .changes
                        .iter()
                        .map(|c| ChangeSummary {
                            at: c.at,
                            target: c.target.identity(),
                            payload: domain_map_text(&c.payload),
                        })
                        .collect(),
                    classes,
                    limits: manifest
                        .limits
                        .iter()
                        .map(|(class, l)| (class.clone(), limit_fields(l)))
                        .collect(),
                }))
            }
            Query::Decode { protocol, payload } => {
                // The scenario's domains, as composed for a run would be (RFC 008 rule 1).
                let domains = self
                    .scenario
                    .as_ref()
                    .and_then(|l| self.domains.instantiate(&l.manifest).ok())
                    .unwrap_or_default();
                Answer::Decoded(domains.iter().find_map(|d| d.decode(protocol, payload)))
            }
            Query::Payload { digest } => {
                let bytes = hex_digest(digest).and_then(|d| {
                    self.run
                        .as_ref()
                        .and_then(|r| r.payload(&d).map(<[u8]>::to_vec))
                });
                Answer::Payload(bytes)
            }
            Query::RunState => Answer::RunState(RunStateInfo {
                state: self.state,
                now: self.run.as_ref().map(|r| at(r.kernel().now())),
                handled: self.run.as_ref().map_or(0, |r| r.kernel().handled()),
                records: self.run.as_ref().map_or(0, |r| r.kernel().trace.records()),
                chain: self.run.as_ref().map(|r| r.chain_hex()),
                seed: self.run.as_ref().map(|r| r.inputs().root_seed),
                pacing: self.pacing,
                ended: self
                    .run
                    .as_ref()
                    .and_then(|r| r.ended())
                    .map(|e| e.as_str().to_string()),
                reproducible: self.run.as_ref().is_none_or(|r| r.reproducible()),
                run_id: self.run.as_ref().map(|r| r.run_id().to_string()),
            }),
            Query::Actor { id } => Answer::Actor(
                self.scenario
                    .as_ref()
                    .and_then(|l| l.manifest.actor(*id))
                    .map(|a| {
                        let summary = actor_summary(a);
                        let (alive, faults, recent) = match &self.run {
                            Some(run) => {
                                let faults = run
                                    .log()
                                    .entries()
                                    .iter()
                                    .filter(|e| {
                                        e.notification.code == "actor.faulted"
                                            && e.notification.scope.actor == Some(*id)
                                    })
                                    .map(|e| {
                                        let n = &e.notification;
                                        let get = |name: &str| {
                                            n.params
                                                .iter()
                                                .find(|(k, _)| k == name)
                                                .map(|(_, v)| v.clone())
                                        };
                                        FaultInfo {
                                            at: n.at.unwrap_or_default(),
                                            code: match get("fault") {
                                                Some(Value::Code(c)) => c,
                                                _ => String::new(),
                                            },
                                            metered: match get("metered") {
                                                Some(Value::Count(c)) => c,
                                                _ => 0,
                                            },
                                            parameter: match get("parameter") {
                                                Some(Value::Count(c)) => c as u32,
                                                _ => 0,
                                            },
                                        }
                                    })
                                    .collect();
                                (run.alive(*id), faults, Vec::new())
                            }
                            None => (false, Vec::new(), Vec::new()),
                        };
                        ActorInfo {
                            summary,
                            alive,
                            faults,
                            recent_telemetry: recent,
                        }
                    }),
            ),
            Query::TraceWindow { from, count } => Answer::TraceWindow(
                self.run
                    .as_ref()
                    .and_then(|r| r.records())
                    .map(|records| {
                        records
                            .iter()
                            .skip(*from as usize)
                            .take(*count as usize)
                            .map(trace_entry)
                            .collect()
                    })
                    .unwrap_or_default(),
            ),
            Query::Capacity => {
                let c = self.platform.capacity();
                Answer::Capacity(CapacityInfo {
                    instances: c.instances,
                    memory_bytes: c.memory_bytes,
                    swap_cost_us_per_kb: c.swap_cost_us_per_kb,
                })
            }
            Query::Support => {
                let info = match (&self.engine, &self.run) {
                    (Some(e), _) => e.info(),
                    (None, Some(r)) => r.engine().info(),
                    (None, None) => florn_engine::EngineInfo {
                        name: String::new(),
                        version: String::new(),
                    },
                };
                Answer::Support(SupportInfo {
                    control_version: CONTROL_VERSION,
                    florn_version: env!("CARGO_PKG_VERSION").to_string(),
                    host_build: self.options.host_build.clone(),
                    engine: info.name,
                    engine_version: info.version,
                    arch: std::env::consts::ARCH.to_string(),
                    features: vec!["core".into(), "replay".into(), "export-state".into()],
                })
            }
        }
    }
}

fn at(i: florn_kernel::Instant) -> At {
    At {
        tick: i.tick,
        microstep: i.microstep,
    }
}

fn actor_summary(a: &florn_model::Actor) -> ActorSummary {
    ActorSummary {
        id: a.id,
        name: a.name.clone(),
        module: match &a.kind {
            florn_model::ActorKind::Module(m) => Some(m.clone()),
            _ => None,
        },
        class: a.class.clone(),
        ports: a
            .ports
            .iter()
            .map(|p| PortSummary {
                id: p.id,
                name: p.name.clone(),
                ty: p.r#type.clone(),
            })
            .collect(),
        config: domain_map_text(&a.config),
    }
}

fn module_summary(name: &str, m: &ModuleManifest, source: &[u8; 32]) -> ModuleSummary {
    let needs = [
        ("budget", m.resources.budget),
        ("stack", m.resources.stack),
        ("memory", m.resources.memory_bytes),
        ("effects-per-activation", m.resources.effects_per_activation),
        ("payload", m.resources.payload_bytes),
        ("pending-timers", m.resources.pending_timers),
        (
            "telemetry-per-activation",
            m.resources.telemetry_per_activation_bytes,
        ),
    ]
    .into_iter()
    .filter_map(|(n, v)| v.map(|v| (n.to_string(), v)))
    .collect();
    ModuleSummary {
        name: name.to_string(),
        source_sha256: hex(source),
        contract: m.world.clone(),
        kind: m.kind.clone(),
        description: m.description.clone(),
        port_kinds: m
            .ports
            .iter()
            .map(|p| (p.kind.clone(), p.ty.clone(), p.min, p.max))
            .collect(),
        params: m
            .params
            .iter()
            .map(|p| ParamSummary {
                name: p.name.clone(),
                ty: p.ty.name().to_string(),
                required: p.required,
                default: p.default.as_ref().map(domain_value_text),
                unit: p.unit.clone(),
            })
            .collect(),
        channels: m
            .telemetry
            .iter()
            .map(|c| ChannelSummary {
                channel: c.channel,
                name: c.name.clone(),
                fields: c
                    .fields
                    .iter()
                    .map(|(n, t)| (n.clone(), t.name().to_string()))
                    .collect(),
            })
            .collect(),
        needs,
        tables: m
            .domain_tables
            .iter()
            .map(|(n, t)| (n.clone(), domain_map_text(t)))
            .collect(),
    }
}

/// A 64-hex digest's bytes.
fn hex_digest(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&text[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}

/// `[limits.<class>]` as text: counts bare, bytes with the unit `B`.
fn limit_fields(l: &florn_model::manifest::Limits) -> Vec<(String, String)> {
    let mut f = Vec::new();
    let count = |f: &mut Vec<(String, String)>, k: &str, v: Option<u64>| {
        if let Some(v) = v {
            f.push((k.to_string(), v.to_string()));
        }
    };
    let bytes = |f: &mut Vec<(String, String)>, k: &str, v: Option<u64>| {
        if let Some(v) = v {
            f.push((k.to_string(), format!("{v}B")));
        }
    };
    count(&mut f, "budget", l.budget);
    count(&mut f, "stack", l.stack);
    bytes(&mut f, "memory", l.memory_bytes);
    count(&mut f, "effects-per-activation", l.effects_per_activation);
    bytes(&mut f, "payload", l.payload_bytes);
    count(&mut f, "pending-timers", l.pending_timers);
    bytes(
        &mut f,
        "telemetry-per-activation",
        l.telemetry_per_activation_bytes,
    );
    f
}

fn domain_map_text(m: &florn_model::manifest::DomainMap) -> Vec<(String, String)> {
    m.iter()
        .map(|(k, v)| (k.clone(), domain_value_text(v)))
        .collect()
}

fn domain_value_text(v: &florn_model::DomainValue) -> String {
    match v {
        florn_model::DomainValue::Integer(i) => i.to_string(),
        florn_model::DomainValue::Decimal(d) | florn_model::DomainValue::Text(d) => d.clone(),
        florn_model::DomainValue::Boolean(b) => b.to_string(),
        other => format!("{other:?}"),
    }
}

fn trace_entry(r: &Record) -> TraceEntry {
    match r {
        Record::Handled {
            key,
            target,
            metered,
            draws,
            effects,
        } => TraceEntry::Handled {
            at: at(key.instant()),
            class: key.class,
            origin: key.origin,
            seq: key.seq,
            target: *target,
            metered: *metered,
            draws: *draws,
            effects: effects.len() as u32,
            sends: effects
                .iter()
                .filter_map(|e| match e {
                    florn_kernel::trace::CommittedEffect::Send {
                        seq,
                        port,
                        len,
                        digest,
                    } => Some(SendSummary {
                        seq: *seq,
                        port: *port,
                        len: *len,
                        digest: hex(digest),
                    }),
                    _ => None,
                })
                .collect(),
        },
        Record::Fault {
            key,
            actor,
            kind,
            parameter,
            metered,
        } => TraceEntry::Fault {
            at: at(key.instant()),
            class: key.class,
            origin: key.origin,
            seq: key.seq,
            actor: *actor,
            code: crate::run::fault_code(*kind).to_string(),
            parameter: *parameter,
            metered: *metered,
        },
        Record::Intervention {
            key, content_len, ..
        } => TraceEntry::Intervention {
            at: at(key.instant()),
            seq: key.seq,
            len: *content_len,
        },
    }
}

/// The file kinds a Host writes, for the front end's file dialogs.
pub fn export_kinds() -> [FileKind; 3] {
    [
        FileKind::RunRecord,
        FileKind::DiagnosticLog,
        FileKind::Package,
    ]
}
