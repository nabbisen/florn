//! The kit's test domain (RFC 008 §12, "Interface"): one connection type over one port type,
//! a built-in, an intervention kind, three event classes, a world with one import and one
//! export, units, a decoder, telemetry layouts and checkpoint state, so that every operation
//! of RFC 008 rule 1 is exercised by the Host with no real domain in the tree. The network
//! domain of PR-16 is the first real one; this one stays as the interface's regression test.
//!
//! The domain: a `test/wire` joins `test/pin` ports; a frame sent on a pin reaches every other
//! pin of the wire after `delay`, unless the wire is disabled, the receiver faulted, or the
//! wire's `loss` stream says so; `cost` per pin is what `probe` answers; `cut` sets the wire's
//! attributes at a boundary; a `test/sink` built-in counts the frames it receives.

use std::collections::{BTreeMap, BTreeSet};

use florn_abi::Ty;
use florn_abi::core_world::Signature;
use florn_admission::Admission;
use florn_control::message::{InspectField, InspectValue};
use florn_control::{Ends, Notification, Scope, Severity, Value};
use florn_engine_wasmtime::WasmtimeEngine;
use florn_host::compound::CallIn;
use florn_host::domain::{
    Action, BuiltinEvent, Domain, DomainEvent, DomainWorld, EndRequest, Guest, quantity_si, refusal,
};
use florn_host::{
    Domains, FaultPolicy, HostError, Numeric, NumericField, Options, Run, Seed, Site, Tolerance,
    UnitDef,
};
use florn_kernel::{Delivery, EventKind, Identity, Instant, Sink, Stream, StreamKey};
use florn_model::canonical::{Decoder, Encoder};
use florn_model::manifest::{ActorKind, DomainMap, DomainValue};
use florn_model::{Decimal, HostMaxima, Manifest, Param, Quantity};
use florn_platform::FixedPlatform;
use wasm_encoder::{CustomSection, Section};

use crate::host::{self, Host};

pub mod second;
pub mod steps;

pub const NAME: &str = "test";
pub const WIRE: &str = "test/wire";
pub const PIN: &str = "test/pin";
pub const SINK: &str = "test/sink";
pub const CUT: &str = "cut";
/// The intervention that asks the domain to end the run (RFC 012 finding F-7): `reason` names
/// why, `non-convergence` or `actor-fault`.
pub const END: &str = "end";
pub const WORLD: &str = "test:kit/guest@0.1.0";
pub const PROBE_MODULE: &str = "test:kit/probe@0.1.0";
pub const EVENTS_ID: &str = "test:kit/events@0.1.0";
pub const EXPORT_ON_WIRE: &str = "test:kit/events@0.1.0#on-wire";
/// The classes (RFC 001 rule 15): a wire's state reached an actor; a frame; a drop, observed.
pub const CLASS_WIRE_STATE: u8 = 3;
pub const CLASS_FRAME: u8 = 4;
pub const CLASS_DROPPED: u8 = 5;

#[derive(Clone, Debug)]
struct Wire {
    ports: Vec<(Identity, u32)>,
    delay: u64,
    /// `loss` as a threshold on a 64-bit draw: dropped when the draw is below it.
    threshold: u64,
    enabled: bool,
    cost: BTreeMap<(Identity, u32), u32>,
    stream: Stream,
}

/// The test domain's state for one run.
#[derive(Clone, Debug, Default)]
pub struct TestDomain {
    wires: BTreeMap<Identity, Wire>,
    by_pin: BTreeMap<(Identity, u32), Identity>,
    /// Frames counted per `test/sink`.
    sinks: BTreeMap<Identity, u64>,
    dropped: u64,
    /// The `[test]` table: what the domain does at the start of a run (RFC 012 finding F-15).
    refuse_start: bool,
    start: Option<(Identity, String)>,
    /// What happens to the events of a guest that faulted (RFC 012 finding F-14).
    policy: Option<FaultPolicy>,
    /// An end request at the start (an action), and from the first delivery at or after a tick (a
    /// flag read after the commit), RFC 012 finding F-7.
    end_at_start: Option<EndRequest>,
    end_from: Option<(u64, EndRequest)>,
    pending_end: Option<EndRequest>,
    /// `[test] atol` and `rtol`: the domain is **continuous** and its runs are compared across
    /// Hosts within this tolerance (RFC 013 rule 20); `[test] continuous-actors` are the actors of
    /// a continuous class, whose consumed work and draws the comparison does not read. A report on
    /// channel 1 of 16 bytes is a `level` (an `f64` in `khop`) and a whole `located` tick.
    tolerance: Option<Tolerance>,
    continuous_actors: BTreeSet<Identity>,
    /// `[test] inspect-guests`: the domain answers `inspect` for any identity, a guest's too, to
    /// show that the Host does not ask it (a guest is the `actor` query's).
    inspect_guests: bool,
    /// `[test] list-guests`: the start hook says what it was given of each guest (RFC 012 finding
    /// F-18). `[test] warn-validate`: the validation returns a warning, which must not refuse the
    /// run.
    list_guests: bool,
}

fn decimal_f64(v: &DomainValue) -> Option<f64> {
    match v {
        DomainValue::Integer(i) => Some(*i as f64),
        DomainValue::Decimal(s) => s.parse().ok(),
        _ => None,
    }
}

fn end_reason(v: &DomainValue) -> Option<EndRequest> {
    match v {
        DomainValue::Text(t) if t == "non-convergence" => Some(EndRequest::NonConvergence),
        DomainValue::Text(t) if t == "actor-fault" => Some(EndRequest::ActorFault),
        _ => None,
    }
}

impl TestDomain {
    pub fn new() -> Self {
        Self::default()
    }
}

/// `0.25` as a threshold on `u64`: `floor(loss * 2^64)`, exactly.
fn threshold(loss: &Decimal) -> u64 {
    if loss.mantissa == 0 {
        return 0;
    }
    let scaled = loss.mantissa.checked_mul(1u128 << 64);
    let divisor = 10u128.checked_pow(loss.exponent.unsigned_abs());
    let q = match (loss.exponent <= 0, scaled, divisor) {
        (true, Some(s), Some(d)) => s / d,
        _ => u128::from(u64::MAX),
    };
    q.min(u128::from(u64::MAX)) as u64
}

fn loss_of(v: &DomainValue) -> Option<Decimal> {
    match v {
        DomainValue::Integer(0) => Some(Decimal::parse("0")?),
        DomainValue::Decimal(s) => {
            let d = Decimal::parse(s)?;
            (d.mantissa == 0 || (d.exponent < 0 && d.normalised().shortest().starts_with("0.")))
                .then_some(d)
        }
        _ => None,
    }
}

fn delay_of(v: &DomainValue, tick: florn_model::TickLength) -> Option<u64> {
    let DomainValue::Text(s) = v else {
        return None;
    };
    Quantity::parse(s).ok()?.to_ticks(tick).ok()?
}

fn wrong(field: String, expected: &'static str) -> Notification {
    refusal(
        "manifest.type",
        field.clone(),
        vec![
            ("field", Param::Text(field)),
            ("expected", Param::Enum(expected)),
        ],
    )
}

fn invalid(field: &str) -> Notification {
    Notification::new(
        "control.invalid",
        Severity::Error,
        Scope::run(),
        Ends::Command,
    )
    .with("command", Value::Name("intervene".into()))
    .with("reason", Value::Enum("wrong-type".into()))
    .with("field", Value::Text(field.into()))
}

fn observed(event: &str, target: Identity, detail: String) -> Action {
    Action::Notify(
        Notification::new("domain.event", Severity::Info, Scope::run(), Ends::Nothing)
            .with("domain", Value::Name(NAME.into()))
            .with("event", Value::Name(event.into()))
            .with("target", Value::Identity(target))
            .with("detail", Value::Text(detail)),
    )
}

impl TestDomain {
    /// Set a wire's attributes from a change or an intervention (one shape for both, RFC 008
    /// rule 4), scheduling `wire-state` events when `enabled` changes.
    fn set(&mut self, target: Identity, payload: &DomainMap) -> Vec<Action> {
        let Some(w) = self.wires.get_mut(&target) else {
            return Vec::new();
        };
        let mut actions = Vec::new();
        let mut toggled = None;
        for (k, v) in payload {
            match (k.as_str(), v) {
                ("enabled", DomainValue::Boolean(b)) => {
                    if w.enabled != *b {
                        w.enabled = *b;
                        toggled = Some(*b);
                    }
                }
                ("loss", v) => {
                    if let Some(l) = loss_of(v) {
                        w.threshold = threshold(&l);
                    }
                }
                ("delay", DomainValue::Integer(t)) => w.delay = *t as u64,
                _ => {}
            }
        }
        if let Some(up) = toggled {
            for (actor, port) in &w.ports {
                let mut payload = port.to_le_bytes().to_vec();
                payload.push(u8::from(up));
                actions.push(Action::Schedule {
                    origin: target,
                    delay: 0,
                    class: CLASS_WIRE_STATE,
                    target: *actor,
                    payload,
                });
            }
        }
        actions.push(observed(
            "wire-changed",
            target,
            format!(
                "enabled={} delay={} threshold={}",
                w.enabled, w.delay, w.threshold
            ),
        ));
        actions
    }

    fn check_payload(&self, field: &str, payload: &DomainMap) -> Result<(), Box<Notification>> {
        for (k, v) in payload {
            let ok = match (k.as_str(), v) {
                ("enabled", DomainValue::Boolean(_)) => true,
                ("loss", v) => loss_of(v).is_some(),
                ("delay", DomainValue::Integer(t)) => *t >= 0,
                _ => false,
            };
            if !ok {
                return Err(Box::new(invalid(&format!("{field}.{k}"))));
            }
        }
        Ok(())
    }
}

impl Domain for TestDomain {
    fn name(&self) -> &'static str {
        NAME
    }
    fn connection_types(&self) -> Vec<&'static str> {
        vec![WIRE]
    }
    fn port_types(&self) -> Vec<&'static str> {
        vec![PIN]
    }
    fn builtin_kinds(&self) -> Vec<&'static str> {
        vec![SINK]
    }
    fn builtin_channels(&self, _: &str) -> Vec<u32> {
        vec![1]
    }
    fn intervention_kinds(&self) -> Vec<&'static str> {
        vec![CUT, steps::STEP, END]
    }
    fn classes(&self) -> Vec<(u8, &'static str)> {
        vec![
            (CLASS_WIRE_STATE, "wire-state"),
            (CLASS_FRAME, "frame"),
            (CLASS_DROPPED, "wire-dropped"),
            (steps::CLASS_STEP, "step"),
        ]
    }
    fn units(&self) -> Vec<UnitDef> {
        // The test domain borrows two dimensions of RFC 007's set for its own units, so that the
        // dimension a refusal names is one the catalogue has a text for: `hop` and its prefixes
        // are of `resistance`, `pps` is of `frequency`.
        let unit = |symbol, factor: &str, dimension| UnitDef {
            symbol,
            factor: Decimal::parse(factor).expect("a decimal"),
            dimension,
        };
        vec![
            unit("hop", "1", "resistance"),
            unit("khop", "1000", "resistance"),
            unit("mhop", "0.001", "resistance"),
            unit("uhop", "0.000001", "resistance"),
            unit("nhop", "0.000000001", "resistance"),
            unit("pps", "1", "frequency"),
        ]
    }
    fn worlds(&self) -> Vec<DomainWorld> {
        vec![DomainWorld {
            name: WORLD.to_string(),
            imports_ids: vec![PROBE_MODULE.to_string()],
            exports_ids: vec![EVENTS_ID.to_string(), steps::STEPS_ID.to_string()],
            imports: vec![(
                PROBE_MODULE.to_string(),
                vec![Signature {
                    name: "probe",
                    params: vec![("port", Ty::U32)],
                    result: Some(Ty::U32),
                }],
            )],
            exports: vec![
                (
                    EVENTS_ID.to_string(),
                    vec![Signature {
                        name: "on-wire",
                        params: vec![("now", Ty::U64), ("port", Ty::U32), ("up", Ty::Bool)],
                        result: None,
                    }],
                ),
                (steps::STEPS_ID.to_string(), steps::signatures()),
            ],
            compounds: vec![steps::compound()],
            records: vec![],
        }]
    }

    fn validate(&self, scenario: &Manifest) -> Vec<Notification> {
        let mut out = Vec::new();
        let tick = scenario.time.tick;
        for (i, c) in scenario.connections.iter().enumerate() {
            if scenario.connection_domain(c) != NAME {
                continue;
            }
            let path = format!("connections[{i}]");
            if c.r#type.is_none() {
                out.push(refusal(
                    "manifest.missing-field",
                    format!("{path}.type"),
                    vec![("field", Param::Text(format!("{path}.type")))],
                ));
            }
            for (k, v) in &c.attrs {
                let field = format!("{path}.attrs.{k}");
                match k.as_str() {
                    "delay" => {
                        if delay_of(v, tick).is_none() {
                            out.push(wrong(field, "quantity"));
                        }
                    }
                    "loss" => {
                        if loss_of(v).is_none() {
                            out.push(refusal(
                                "quantity.out-of-range",
                                field.clone(),
                                vec![
                                    ("field", Param::Text(field)),
                                    ("value", Param::Quantity(format!("{v:?}"), String::new())),
                                ],
                            ));
                        }
                    }
                    "enabled" => {
                        if !matches!(v, DomainValue::Boolean(_)) {
                            out.push(wrong(field, "boolean"));
                        }
                    }
                    // A resistance, read by the Host's helper as a parameter's quantity is
                    // (RFC 012 rule 5, finding F-13): the value is not used, it is checked.
                    "r" => match v {
                        DomainValue::Text(text) => {
                            if let Err(refused) =
                                quantity_si(&self.units(), &field, text, "resistance")
                            {
                                out.push(*refused);
                            }
                        }
                        _ => out.push(wrong(field, "quantity")),
                    },
                    _ => out.push(refusal(
                        "manifest.unknown-key",
                        field,
                        vec![("key", Param::Text(k.clone()))],
                    )),
                }
            }
            for (pr, m) in &c.per_port {
                for (k, v) in m {
                    let field = format!("{path}.attrs.per-port.{}.{}.{k}", pr.actor, pr.port);
                    match (k.as_str(), v) {
                        ("cost", DomainValue::Integer(n)) if *n >= 0 => {}
                        ("cost", _) => out.push(wrong(field, "unsigned-integer")),
                        _ => out.push(refusal(
                            "manifest.unknown-key",
                            field,
                            vec![("key", Param::Text(k.clone()))],
                        )),
                    }
                }
            }
        }
        for (i, a) in scenario.actors.iter().enumerate() {
            if a.kind != ActorKind::Builtin(SINK.to_string()) {
                continue;
            }
            for (j, p) in a.ports.iter().enumerate() {
                if p.r#type != PIN {
                    out.push(refusal(
                        "module.port-type-unknown",
                        format!("actors[{i}].ports[{j}].type"),
                        vec![
                            ("actor", Param::Identity(a.id)),
                            ("port", Param::Name(p.name.clone())),
                            ("type", Param::Text(p.r#type.clone())),
                        ],
                    ));
                }
            }
        }
        if let Some(table) = scenario.domain_tables.get(NAME) {
            for (k, v) in table {
                let field = format!("{NAME}.{k}");
                match (k.as_str(), v) {
                    ("note", _) => {}
                    ("refuse-start", DomainValue::Boolean(_)) => {}
                    ("refuse-start", _) => out.push(wrong(field, "boolean")),
                    ("start-script", DomainValue::Text(_)) => {}
                    ("start-script", _) => out.push(wrong(field, "text")),
                    ("start-actor", DomainValue::Integer(n)) if *n >= 0 => {}
                    ("start-actor", _) => out.push(wrong(field, "unsigned-integer")),
                    ("faulted-policy", DomainValue::Text(t)) if t == "drop" || t == "stop-run" => {}
                    ("faulted-policy", _) => out.push(wrong(field, "text")),
                    ("end-at-start" | "end-reason", v) if end_reason(v).is_some() => {}
                    ("end-at-start" | "end-reason", _) => out.push(wrong(field, "text")),
                    ("end-from-tick", DomainValue::Integer(n)) if *n >= 0 => {}
                    ("end-from-tick", _) => out.push(wrong(field, "unsigned-integer")),
                    (
                        "inspect-guests" | "list-guests" | "warn-validate",
                        DomainValue::Boolean(_),
                    ) => {}
                    ("inspect-guests" | "list-guests" | "warn-validate", _) => {
                        out.push(wrong(field, "boolean"));
                    }
                    ("atol" | "rtol", v) if decimal_f64(v).is_some_and(|x| x >= 0.0) => {}
                    ("atol" | "rtol", _) => out.push(wrong(field, "quantity")),
                    ("continuous-actors", DomainValue::List(l))
                        if l.iter()
                            .all(|v| matches!(v, DomainValue::Integer(n) if *n >= 0)) => {}
                    ("continuous-actors", _) => out.push(wrong(field, "unsigned-integer")),
                    _ => out.push(refusal(
                        "manifest.unknown-key",
                        field,
                        vec![("key", Param::Text(k.clone()))],
                    )),
                }
            }
            if matches!(table.get("warn-validate"), Some(DomainValue::Boolean(true))) {
                out.push(
                    Notification::new(
                        "host.capacity-warning",
                        Severity::Warning,
                        Scope::run(),
                        Ends::Nothing,
                    )
                    .with("used", Value::Count(1))
                    .with("capacity", Value::Count(1))
                    .with("what", Value::Enum("instances".into())),
                );
            }
        }
        out
    }

    fn compose(&mut self, scenario: &Manifest, root_seed: u64) {
        let tick = scenario.time.tick;
        if let Some(table) = scenario.domain_tables.get(NAME) {
            self.refuse_start =
                matches!(table.get("refuse-start"), Some(DomainValue::Boolean(true)));
            if let (Some(DomainValue::Integer(a)), Some(DomainValue::Text(s))) =
                (table.get("start-actor"), table.get("start-script"))
            {
                self.start = Some((*a as Identity, s.clone()));
            }
            self.policy = match table.get("faulted-policy") {
                Some(DomainValue::Text(t)) if t == "stop-run" => Some(FaultPolicy::StopRun),
                Some(DomainValue::Text(_)) => Some(FaultPolicy::Drop),
                _ => None,
            };
            self.inspect_guests = matches!(
                table.get("inspect-guests"),
                Some(DomainValue::Boolean(true))
            );
            self.list_guests = matches!(table.get("list-guests"), Some(DomainValue::Boolean(true)));
            self.tolerance = match (table.get("atol"), table.get("rtol")) {
                (Some(a), Some(r)) => decimal_f64(a)
                    .zip(decimal_f64(r))
                    .map(|(atol, rtol)| Tolerance { atol, rtol }),
                _ => None,
            };
            self.continuous_actors = match table.get("continuous-actors") {
                Some(DomainValue::List(l)) => l
                    .iter()
                    .filter_map(|v| match v {
                        DomainValue::Integer(i) => Some(*i as Identity),
                        _ => None,
                    })
                    .collect(),
                _ => BTreeSet::new(),
            };
            self.end_at_start = table.get("end-at-start").and_then(end_reason);
            if let (Some(DomainValue::Integer(from)), Some(r)) = (
                table.get("end-from-tick"),
                table.get("end-reason").and_then(end_reason),
            ) {
                self.end_from = Some((*from as u64, r));
            }
        }
        for c in &scenario.connections {
            if scenario.connection_domain(c) != NAME {
                continue;
            }
            let mut w = Wire {
                ports: c.ports.iter().map(|p| (p.actor, p.port)).collect(),
                delay: 0,
                threshold: 0,
                enabled: true,
                cost: BTreeMap::new(),
                // RFC 001 rules 28 and 29: the wire's own stream, by its identity and purpose.
                stream: Stream::new(StreamKey::derive(root_seed, c.id, "loss")),
            };
            for (k, v) in &c.attrs {
                match k.as_str() {
                    "delay" => w.delay = delay_of(v, tick).unwrap_or(0),
                    "loss" => w.threshold = loss_of(v).map(|l| threshold(&l)).unwrap_or(0),
                    "enabled" => {
                        if let DomainValue::Boolean(b) = v {
                            w.enabled = *b;
                        }
                    }
                    _ => {}
                }
            }
            for (pr, m) in &c.per_port {
                if let Some(DomainValue::Integer(n)) = m.get("cost") {
                    w.cost.insert((pr.actor, pr.port), *n as u32);
                }
            }
            for p in &w.ports {
                self.by_pin.insert(*p, c.id);
            }
            self.wires.insert(c.id, w);
        }
        for a in &scenario.actors {
            if a.kind == ActorKind::Builtin(SINK.to_string()) {
                self.sinks.insert(a.id, 0);
            }
        }
    }

    fn deliver(
        &mut self,
        now: Instant,
        origin: Identity,
        _seq: u64,
        port: u32,
        payload: &[u8],
        dead: &BTreeSet<Identity>,
    ) -> Vec<Delivery> {
        // What a delivery finds, it cannot return as an action: the flag is read after the commit.
        if let Some((from, request)) = self.end_from
            && now.tick >= from
        {
            self.pending_end.get_or_insert(request);
        }
        let Some(id) = self.by_pin.get(&(origin, port)).copied() else {
            return Vec::new();
        };
        let w = self.wires.get_mut(&id).expect("a pin's wire");
        let dropped = |reason: &str| Delivery {
            target: id,
            class: CLASS_DROPPED,
            delay: 0,
            kind: EventKind::Domain {
                payload: reason.as_bytes().to_vec(),
            },
        };
        if !w.enabled {
            return vec![dropped("down")];
        }
        let mut out = Vec::new();
        // Receivers in ascending identity order (RFC 008 rule 10), one draw each.
        let mut receivers: Vec<(Identity, u32)> = w
            .ports
            .iter()
            .copied()
            .filter(|p| *p != (origin, port))
            .collect();
        receivers.sort_unstable();
        for (actor, their_port) in receivers {
            if dead.contains(&actor) {
                out.push(dropped("faulted"));
                continue;
            }
            if w.threshold > 0 && w.stream.next_u64() < w.threshold {
                out.push(dropped("loss"));
                continue;
            }
            out.push(Delivery {
                target: actor,
                class: CLASS_FRAME,
                delay: w.delay,
                kind: EventKind::Message {
                    port: their_port,
                    payload: payload.to_vec(),
                },
            });
        }
        out
    }

    fn change(&mut self, _now: Instant, target: Identity, payload: &DomainMap) -> Vec<Action> {
        self.set(target, payload)
    }

    fn check_intervention(
        &self,
        target: Identity,
        kind: &str,
        content: &DomainMap,
    ) -> Result<(), Box<Notification>> {
        if kind != CUT && kind != steps::STEP && kind != END {
            return Err(Box::new(invalid("kind")));
        }
        if !self.wires.contains_key(&target) {
            return Err(Box::new(invalid("target")));
        }
        if kind == END {
            return match content.get("reason").and_then(end_reason) {
                Some(_) => Ok(()),
                None => Err(Box::new(invalid("content"))),
            };
        }
        if kind == steps::STEP {
            return match (content.get("actor"), content.get("script")) {
                (Some(DomainValue::Integer(a)), Some(DomainValue::Text(_))) if *a >= 0 => Ok(()),
                _ => Err(Box::new(invalid("content"))),
            };
        }
        self.check_payload("content", content)
    }

    fn intervene(
        &mut self,
        _now: Instant,
        target: Identity,
        kind: &str,
        content: &DomainMap,
    ) -> Vec<Action> {
        if kind == END {
            return content
                .get("reason")
                .and_then(end_reason)
                .map(|r| vec![Action::EndRun(r)])
                .unwrap_or_default();
        }
        if kind == steps::STEP {
            let (Some(DomainValue::Integer(actor)), Some(DomainValue::Text(script))) =
                (content.get("actor"), content.get("script"))
            else {
                return Vec::new();
            };
            // The script is handled as an event of its own, a microstep on.
            return vec![Action::Schedule {
                origin: target,
                delay: 0,
                class: steps::CLASS_STEP,
                target: *actor as Identity,
                payload: script.as_bytes().to_vec(),
            }];
        }
        self.set(target, content)
    }

    fn event_calling(
        &mut self,
        event: &DomainEvent,
        alive: bool,
        calls: &mut dyn CallIn,
    ) -> Vec<Action> {
        if event.key.class != steps::CLASS_STEP {
            return self.event(event, alive);
        }
        let script = String::from_utf8_lossy(&event.payload).into_owned();
        let report = steps::play(&script, event.target, event.key.tick, calls);
        vec![observed("compound", event.target, report)]
    }

    /// The start hook (RFC 012 finding F-15): refuse the run when the `[test]` table says so, else
    /// play its `start-script` on its `start-actor` through the call-in handle.
    fn start_calling(
        &mut self,
        calls: &mut dyn CallIn,
        guests: &[Guest],
    ) -> Result<Vec<Action>, Box<Notification>> {
        if self.refuse_start {
            return Err(Box::new(refusal(
                "manifest.invalid",
                format!("{NAME}.refuse-start"),
                vec![(
                    "reason",
                    Param::Text("the test domain refuses to start".into()),
                )],
            )));
        }
        let mut actions = Vec::new();
        if self.list_guests {
            let detail = guests
                .iter()
                .map(|g| format!("{}={}@{}", g.actor, g.module, g.world()))
                .collect::<Vec<_>>()
                .join(" ");
            actions.push(observed("guests", 0, detail));
        }
        if let Some((actor, script)) = self.start.clone() {
            let report = steps::play(&script, actor, 0, calls);
            actions.push(observed("start", actor, report));
        }
        if let Some(request) = self.end_at_start {
            actions.push(Action::EndRun(request));
        }
        Ok(actions)
    }

    fn faulted_policy(&self) -> FaultPolicy {
        self.policy.unwrap_or(FaultPolicy::Drop)
    }

    fn tolerance(&self) -> Option<Tolerance> {
        self.tolerance
    }

    fn continuous_actor(&self, actor: Identity) -> bool {
        self.continuous_actors.contains(&actor)
    }

    fn numeric_fields(&self, _: Identity, site: Site, payload: &[u8]) -> Option<Numeric> {
        let (Site::Report { channel: 1 }, true) = (site, self.tolerance.is_some()) else {
            return None;
        };
        let bytes: [u8; 16] = payload.try_into().ok()?;
        Some(Numeric {
            fields: vec![NumericField {
                name: "level".into(),
                unit: "khop".into(),
                factor: 1000.0,
                value: f64::from_le_bytes(bytes[..8].try_into().ok()?),
            }],
            exact: bytes[8..].to_vec(),
        })
    }

    /// A report of 16 bytes on channel 1 is a `level` (an `f64`, the first and only `f64` field of
    /// the test domain's reports): one series, index 0.
    fn display_series(&self, _: Identity, channel: u32, payload: &[u8]) -> Vec<(u8, f64)> {
        match (channel, <[u8; 16]>::try_from(payload)) {
            (1, Ok(b)) if self.tolerance.is_some() => {
                vec![(
                    0,
                    f64::from_le_bytes(b[..8].try_into().expect("eight bytes")),
                )]
            }
            _ => Vec::new(),
        }
    }

    /// A wire, a sink and the domain's world (RFC 012 finding F-12): what the kit's inspectors read.
    fn inspect(&self, target: Identity) -> Option<Vec<InspectField>> {
        let text = |name: &str, value: String| InspectField {
            name: name.into(),
            value: InspectValue::Text(value),
        };
        if let Some(w) = self.wires.get(&target) {
            return Some(vec![
                text("enabled", w.enabled.to_string()),
                text("pins", w.ports.len().to_string()),
                InspectField {
                    name: "delay".into(),
                    value: InspectValue::Real {
                        value: w.delay as f64,
                        unit: "ticks".into(),
                    },
                },
            ]);
        }
        if let Some(frames) = self.sinks.get(&target) {
            return Some(vec![text("frames", frames.to_string())]);
        }
        // The domain's world has an identity beyond the reserved one (RFC 004 rule 8).
        (self.inspect_guests || target >= florn_model::manifest::IDENTITY_RESERVED_FROM).then(
            || {
                vec![
                    text("wires", self.wires.len().to_string()),
                    text("dropped", self.dropped.to_string()),
                ]
            },
        )
    }

    fn end_requested(&mut self) -> Option<EndRequest> {
        self.pending_end.take()
    }

    fn event(&mut self, event: &DomainEvent, alive: bool) -> Vec<Action> {
        match event.key.class {
            CLASS_WIRE_STATE => {
                let (Some(port), Some(up)) = (
                    event
                        .payload
                        .get(0..4)
                        .map(|b| u32::from_le_bytes(b.try_into().expect("four bytes"))),
                    event.payload.get(4).map(|b| *b != 0),
                ) else {
                    return Vec::new();
                };
                if !alive {
                    return Vec::new();
                }
                vec![Action::Call {
                    actor: event.target,
                    export: EXPORT_ON_WIRE.to_string(),
                    values: vec![
                        florn_abi::Val::U64(event.key.tick),
                        florn_abi::Val::U32(port),
                        florn_abi::Val::Bool(up),
                    ],
                }]
            }
            CLASS_DROPPED => {
                self.dropped += 1;
                let reason = String::from_utf8_lossy(&event.payload).into_owned();
                vec![observed("wire-dropped", event.target, reason)]
            }
            _ => Vec::new(),
        }
    }

    fn undelivered(&mut self, event: &DomainEvent) -> Vec<Action> {
        self.dropped += 1;
        vec![observed("wire-dropped", event.target, "faulted".into())]
    }

    fn answer(
        &mut self,
        actor: Identity,
        function: &str,
        args: &[florn_abi::Val],
    ) -> Option<florn_abi::Val> {
        match (function, args) {
            ("probe", [florn_abi::Val::U32(port)]) => {
                let cost = self
                    .by_pin
                    .get(&(actor, *port))
                    .and_then(|id| self.wires.get(id))
                    .and_then(|w| w.cost.get(&(actor, *port)).copied())
                    .unwrap_or(1);
                Some(florn_abi::Val::U32(cost))
            }
            _ => None,
        }
    }

    fn builtin(&mut self, actor: Identity, _kind: &str, event: &BuiltinEvent<'_>, sink: &mut Sink) {
        match event {
            BuiltinEvent::Init { .. } => {
                let _ = sink.report(0, b"sink ready".to_vec());
            }
            BuiltinEvent::Message { .. } => {
                let n = self.sinks.entry(actor).or_insert(0);
                *n += 1;
                let _ = sink.report(1, n.to_le_bytes().to_vec());
            }
            BuiltinEvent::Timer { .. } | BuiltinEvent::Domain(_) => {}
        }
    }

    fn decode(&self, protocol: &str, payload: &[u8]) -> Option<Vec<(String, String)>> {
        if protocol != "probe-proto" || payload.len() != 4 {
            return None;
        }
        let v = u32::from_le_bytes(payload.try_into().expect("four bytes"));
        Some(vec![("value".to_string(), v.to_string())])
    }

    fn telemetry(&self) -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
        vec![("count", vec![("n", "u64")])]
    }

    fn export(&self) -> Vec<u8> {
        let mut e = Encoder::default();
        e.u32(self.wires.len() as u32);
        for (id, w) in &self.wires {
            e.u32(*id);
            e.u8(u8::from(w.enabled));
            e.u64(w.delay);
            e.u64(w.threshold);
            e.u64(w.stream.drawn());
        }
        e.u32(self.sinks.len() as u32);
        for (id, n) in &self.sinks {
            e.u32(*id);
            e.u64(*n);
        }
        e.u64(self.dropped);
        e.finish()
    }

    fn restore(&mut self, bytes: &[u8]) -> bool {
        if bytes.is_empty() {
            return true;
        }
        let mut d = Decoder::new(bytes);
        let Some(n) = d.u32() else { return false };
        for _ in 0..n {
            let (Some(id), Some(enabled), Some(delay), Some(threshold), Some(drawn)) =
                (d.u32(), d.u8(), d.u64(), d.u64(), d.u64())
            else {
                return false;
            };
            let Some(w) = self.wires.get_mut(&id) else {
                return false;
            };
            w.enabled = enabled != 0;
            w.delay = delay;
            w.threshold = threshold;
            w.stream = Stream::at(w.stream.key(), drawn);
        }
        let Some(n) = d.u32() else { return false };
        for _ in 0..n {
            let (Some(id), Some(count)) = (d.u32(), d.u64()) else {
                return false;
            };
            self.sinks.insert(id, count);
        }
        match d.u64() {
            Some(dropped) => self.dropped = dropped,
            None => return false,
        }
        d.done()
    }
}

/// The Host's domains for the kit's tests: the core and the test domain.
pub fn domains() -> Domains {
    Domains::core().with(TestDomain::new)
}

/// The prober guest of the test world, hand-written: on its timer it sends `probe(0)`'s answer
/// through pin 0 and re-arms; a frame is reported on channel 1; `on-wire` reports the state on
/// channel 2 and `probe(port)` on channel 3.
pub fn guest_wat() -> String {
    r#"
(module
  (import "florn:core/host@0.1.0" "send" (func $send (param i32 i32 i32)))
  (import "florn:core/host@0.1.0" "set-timer" (func $set_timer (param i32 i64)))
  (import "florn:core/host@0.1.0" "cancel-timer" (func $cancel (param i32)))
  (import "florn:core/host@0.1.0" "random" (func $random (result i64)))
  (import "florn:core/host@0.1.0" "report" (func $report (param i32 i32 i32)))
  (import "test:kit/probe@0.1.0" "probe" (func $probe (param i32) (result i32)))
  (memory (export "memory") 2)
  (global $bump (mut i32) (i32.const 4096))
  (global $period (mut i64) (i64.const 5))
  (global $me (mut i32) (i32.const 0))
  (func $realloc (export "cabi_realloc") (param i32 i32 i32 i32) (result i32)
    (local $p i32)
    (local.set $p (i32.and (i32.add (global.get $bump) (i32.sub (local.get 2) (i32.const 1)))
                           (i32.sub (i32.const 0) (local.get 2))))
    (global.set $bump (i32.add (local.get $p) (local.get 3)))
    (block $ok (loop $grow
      (br_if $ok (i32.le_u (global.get $bump) (i32.mul (memory.size) (i32.const 65536))))
      (drop (memory.grow (i32.const 1)))
      (br $grow)))
    (local.get $p))
  (func (export "florn:core/actor@0.1.0#init") (param i64 i32 i32 i32 i32 i32)
    (global.set $me (local.get 1))
    (if (i32.ge_u (local.get 5) (i32.const 8))
      (then (global.set $period (i64.load (local.get 4)))))
    (call $set_timer (i32.const 0) (i64.const 1)))
  (func (export "florn:core/actor@0.1.0#on-message") (param i64 i32 i32 i32)
    (call $report (i32.const 1) (local.get 2) (local.get 3)))
  (func (export "florn:core/actor@0.1.0#on-timer") (param i64 i32)
    (i32.store (i32.const 64) (call $probe (i32.const 0)))
    (call $send (i32.const 0) (i32.const 64) (i32.const 4))
    (call $set_timer (i32.const 0) (global.get $period)))
  (func (export "test:kit/events@0.1.0#on-wire") (param i64 i32 i32)
    (i32.store (i32.const 72) (local.get 2))
    (call $report (i32.const 2) (i32.const 72) (i32.const 4))
    (i32.store (i32.const 76) (call $probe (local.get 1)))
    (call $report (i32.const 3) (i32.const 76) (i32.const 4)))
"#
    .to_string()
        + steps::GUEST_FUNCTIONS
        + ")"
}

pub fn guest_manifest() -> String {
    guest_manifest_of(WORLD)
}

/// The prober's module manifest naming `world`.
pub fn guest_manifest_of(world: &str) -> String {
    format!(
        r#"
florn-module = 1
world = "{world}"
kind = "prober"

[[ports]]
kind = "pin"
type = "{PIN}"
min = 0
max = 8

[[params]]
name = "period"
type = "u64"
default = 5

[[params]]
name = "weight"
type = "real"
unit = "hop"
default = "1hop"

[[telemetry]]
channel = 1
name = "frame"
fields = [{{ name = "v", type = "u64" }}]

[[telemetry]]
channel = 2
name = "wire"
fields = [{{ name = "up", type = "u64" }}]

[[telemetry]]
channel = 3
name = "cost"
fields = [{{ name = "cost", type = "u64" }}]

[resources]
memory = "128KiB"
"#
    )
}

/// The contract-identity section of the test world, from both contracts of record.
pub fn contract_section() -> Vec<u8> {
    contract_section_of("guest")
}

/// The contract-identity section of a world of `wit/test-world.wit` (`guest`, `pinger` or
/// `both`), over the core contract of record.
pub fn contract_section_of(world_name: &str) -> Vec<u8> {
    let core = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../contracts/florn-core/wit/core.wit"
    ))
    .expect("the core contract");
    let test = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/wit/test-world.wit"))
        .expect("the test world");
    let mut resolve = wit_parser::Resolve::default();
    resolve
        .push_str("core.wit", &core)
        .expect("the core contract parses");
    let pkg = resolve
        .push_str("test-world.wit", &test)
        .expect("the test world parses");
    let world = resolve
        .select_world(&[pkg], Some(world_name))
        .unwrap_or_else(|e| panic!("world {world_name}: {e}"));
    wit_component::metadata::encode(
        &resolve,
        world,
        wit_component::StringEncoding::UTF8,
        None,
        false,
    )
    .expect("encoded")
}

/// The prober's bytes with both custom sections (RFC 002 rule 11).
pub fn guest_bytes() -> Vec<u8> {
    guest_bytes_of(&guest_wat())
}

/// A module written in text, with the prober's custom sections: the prober itself, or a variant of
/// it that exports or lacks something, for admission's refusals.
pub fn guest_bytes_of(wat_text: &str) -> Vec<u8> {
    guest_bytes_in(wat_text, "guest", WORLD)
}

/// A module written in text, of the world `world_name` of `wit/test-world.wit`, named
/// `world_id` in its manifest (`second::COMPOSED` for `both`).
pub fn guest_bytes_in(wat_text: &str, world_name: &str, world_id: &str) -> Vec<u8> {
    let mut bytes = wat::parse_str(wat_text).expect("the module parses");
    CustomSection {
        name: format!("component-type:test-kit-{world_name}").into(),
        data: contract_section_of(world_name).into(),
    }
    .append_to(&mut bytes);
    CustomSection {
        name: "florn.manifest".into(),
        data: guest_manifest_of(world_id).into_bytes().into(),
    }
    .append_to(&mut bytes);
    bytes
}

/// A scenario of the test domain: the header, the prober module, then the body (actors,
/// connections, changes, the `[test]` table) as RFC 004 text.
pub fn text(name: &str, end: u64, seed: u64, body: &str) -> String {
    let hash = florn_host::run::sha256_hex(&guest_bytes());
    format!(
        "florn = 1\nname = \"{name}\"\ndomains = [\"{NAME}\"]\n\n[time]\ntick = \"1ns\"\nend = \"{end}ns\"\n\n[order]\ntie = \"by-identity\"\n\n[seed]\nroot = {seed}\n\n[limits.default]\nmemory = \"2MiB\"\n\n[modules.prober]\nsha256 = \"{hash}\"\ncontract = \"{WORLD}\"\n{body}"
    )
}

/// A prober actor with `n` pins.
pub fn prober(id: u32, n: u32) -> String {
    let ports: Vec<String> = (0..n)
        .map(|p| format!("{{ id = {p}, name = \"p{p}\", type = \"{PIN}\" }}"))
        .collect();
    format!(
        "\n[[actors]]\nid = {id}\nname = \"a{id}\"\nkind = \"module\"\nmodule = \"prober\"\nports = [{}]\n",
        ports.join(", ")
    )
}

/// A sink built-in with one pin.
pub fn sink(id: u32) -> String {
    format!(
        "\n[[actors]]\nid = {id}\nname = \"a{id}\"\nkind = \"builtin\"\nbuiltin = \"{SINK}\"\nports = [{{ id = 0, name = \"p0\", type = \"{PIN}\" }}]\n"
    )
}

/// A wire over (actor, pin) pairs with attribute lines (`delay = \"2ns\"`).
pub fn wire(id: u32, pins: &[(u32, u32)], attrs: &str) -> String {
    let ports: Vec<String> = pins.iter().map(|(a, p)| format!("\"a{a}.p{p}\"")).collect();
    let mut t = format!(
        "\n[[connections]]\nid = {id}\nname = \"w{id}\"\ntype = \"{WIRE}\"\nports = [{}]\n",
        ports.join(", ")
    );
    if !attrs.is_empty() {
        t.push_str("[connections.attrs]\n");
        t.push_str(attrs);
        t.push('\n');
    }
    t
}

pub fn modules() -> BTreeMap<String, Vec<u8>> {
    [("prober".to_string(), guest_bytes())]
        .into_iter()
        .collect()
}

pub fn manifest(text: &str) -> Manifest {
    Manifest::parse(text, &HostMaxima::native())
        .unwrap_or_else(|e| panic!("{:?}\n{text}", e.codes()))
}

/// Start a run of the text under the Host with the test domain.
pub fn try_start(text: &str, options: Options) -> Result<Host, HostError> {
    try_start_with(host::engine(), &domains(), text, options)
}

pub fn try_start_with(
    engine: WasmtimeEngine,
    domains: &Domains,
    text: &str,
    options: Options,
) -> Result<Host, HostError> {
    let manifest = manifest(text);
    let seed = manifest.seed;
    Run::new(
        engine,
        Admission::default(),
        &mut FixedPlatform::default(),
        domains,
        manifest,
        &modules(),
        Seed::Given(seed),
        options,
    )
}

pub fn start(text: &str) -> Host {
    try_start(text, host::options()).unwrap_or_else(|e| panic!("start: {e}"))
}

pub fn run(text: &str) -> Host {
    let mut r = start(text);
    r.run_to_end();
    r
}
