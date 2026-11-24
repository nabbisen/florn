//! The Host's side of the control interface (RFC 005 §6; external design §4): every front end
//! drives the Host through this and nothing else. A command is validated before it acts (rule
//! 27); an invalid one changes nothing and produces its code. Pacing is the caller's: `pump`
//! computes a slice when the front end decides to call it.

use std::collections::BTreeMap;

use florn_admission::{Admission, ClassLimits, ModuleManifest};
use florn_control::display::{DisplayPath, Record as DisplayRecord, Transport};
use florn_control::message::{
    ActorInfo, ActorSummary, CapacityInfo, ChangeSummary, ChannelSummary, CheckpointSummary,
    ConnectionSummary, ExportPreview, FaultInfo, ModuleSummary, ParamSummary, PortSummary,
    ReportEntry, RunStateInfo, ScenarioSummary, SendSummary, SupportInfo, TelemetryRecord,
    TraceEntry, control_invalid, control_not_now,
};
use florn_control::{
    Answer, At, CONTROL_VERSION, Code, Command, Envelope, ExportKind, FileKind, Message, Pacing,
    Progress, Query, RunState, Scope, SeedChoice, Subscription, SubscriptionKind, Unit, Value,
    decode, encode,
};
use florn_engine::Engine;
use florn_kernel::{Advance, Digest, Record};
use florn_model::canonical::hex;
use florn_model::{Manifest, Package, PackageMeta};
use florn_platform::Platform;

use crate::domain::{self, Domains};
use crate::record::{self, Contents};
use crate::run::{HostError, Options, Run, Seed};
use crate::snapshot::{self, Checkpoint};

mod command;
mod export;
mod query;
mod scenario;
mod summaries;

pub use summaries::export_kinds;
use summaries::{
    actor_summary, at, domain_map_text, hex_digest, limit_fields, module_summary, trace_entry,
};

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
    /// The record of the run the current one branched from (RFC 009 rule 11), until the next
    /// run starts.
    parent_record: Option<Contents>,
    /// When `host.limited-by` was last said (the platform's monotonic microseconds), and what.
    limited: Option<(u64, &'static str)>,
}

impl<E: Engine> Controller<E> {
    pub fn new(
        engine: E,
        mut admission: Admission,
        platform: Box<dyn Platform>,
        domains: Domains,
        options: Options,
    ) -> Self {
        // A composition of domains the Host holds that it cannot build is a defect of the Host's
        // own configuration (RFC 012 finding F-9): it stops here, in every build, naming it. One
        // with a domain the Host lacks is left out, and a module of it is refused.
        let defects = domains.composition_defects();
        assert!(defects.is_empty(), "{}", defects.join("; "));
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
            parent_record: None,
            limited: None,
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
            let t0 = self.platform.monotonic_us();
            let (handled0, swaps0, dropped0) =
                (run.kernel().handled(), run.swaps(), run.telemetry_dropped());
            run.advance(Advance::ToTick(to));
            let elapsed = self.platform.monotonic_us().saturating_sub(t0);
            // The unit is complete when every event before the target is handled.
            if to >= target || !run.kernel().has_next() {
                self.state = RunState::Paused;
                self.target = None;
            }
            // NFR-004 (RFC 009 rule 21): a slice that took longer than its share of the
            // second names what limited it, from the Host's own figures, at most once a second.
            if elapsed > Self::SLICE_BUDGET_US {
                let handled = run.kernel().handled().saturating_sub(handled0);
                let swaps = run.swaps().saturating_sub(swaps0);
                let dropped = run.telemetry_dropped().saturating_sub(dropped0);
                let memory = run.memory_counted();
                let capacity = run.capacity().memory_bytes;
                let what = if swaps > 0 && swaps.saturating_mul(2) >= handled {
                    "swapping"
                } else if memory.saturating_mul(10) >= capacity.saturating_mul(9) {
                    "memory"
                } else if dropped > 0 {
                    "display"
                } else {
                    "actors"
                };
                let now_us = self.platform.monotonic_us();
                let say = match self.limited {
                    Some((at, last)) => last != what || now_us.saturating_sub(at) >= 1_000_000,
                    None => true,
                };
                if say {
                    self.limited = Some((now_us, what));
                    out.push(Message::Notification(
                        Code::HostLimitedBy
                            .notification(Scope::run(), vec![Value::Enum(what.into())]),
                    ));
                }
            }
        }
        let events = self.subs.events;
        self.after_advance(&mut out, events);
        out
    }

    /// A paced slice's share of the second: the worker's clock ticks sixty times a second.
    const SLICE_BUDGET_US: u64 = 1_000_000 / 60;

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
}
