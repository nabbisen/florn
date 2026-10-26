//! The network domain (RFC 008 §3 to §9) over the domain interface of `florn-host`: `network/link`
//! and `network/lan` connections between `network/interface` ports; frames delivered with
//! delay, serialisation, jitter and loss from the connection's own streams; the `link` and
//! `node` interventions; the `link-info` import and the `on-link` export of the router contract;
//! the `network/blackhole` built-in; the codes of rule 18; the telemetry layouts the view reads;
//! the decoders of the ladder's protocols.
//!
//! The kernel knows nothing of this crate (ARCH-006); it reaches no adapter (RFC 005 rule 3).
//! Its randomness is the kernel's streams, derived for each connection (RFC 001 rules 28 to 31).
//!
//! Implemented by PR-16 of `rfcs/handoffs/008-network-domain/task-breakdown-pr-plan.md`.
#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

use florn_abi::Ty;
use florn_abi::core_world::Signature;
use florn_admission::manifest::{ModuleManifest, ParamType};
use florn_control::{Ends, Notification, Scope, Severity, Value};
use florn_host::domain::{Action, BuiltinEvent, Domain, DomainEvent, DomainWorld, refusal};
use florn_kernel::{Delivery, EventKind, Identity, Instant, Sink, Stream, StreamKey};
use florn_model::canonical::{Decoder, Encoder};
use florn_model::manifest::{ActorKind, DomainMap, DomainValue};
use florn_model::{Decimal, Manifest, Param, Quantity, TickLength};

pub mod decode;

pub const NAME: &str = "network";
pub const INTERFACE: &str = "network/interface";
pub const LINK: &str = "network/link";
pub const LAN: &str = "network/lan";
pub const BLACKHOLE: &str = "network/blackhole";
pub const WORLD: &str = "florn:network/router@0.1.0";
pub const LINK_MODULE: &str = "florn:network/link@0.1.0";
pub const EVENTS_ID: &str = "florn:network/router-events@0.1.0";
pub const EXPORT_ON_LINK: &str = "florn:network/router-events@0.1.0#on-link";
pub const EXPORT_ON_MESSAGE: &str = "florn:core/actor@0.1.0#on-message";
/// The classes of rule 11.
pub const CLASS_LINK_STATE: u8 = 3;
pub const CLASS_FRAME: u8 = 4;
pub const CLASS_FRAME_DROPPED: u8 = 5;
/// The default `mtu` (rule 9), in bytes.
pub const DEFAULT_MTU: u32 = 1500;
/// The blackhole's channel, named `dropped` (rule 19).
pub const BLACKHOLE_CHANNEL: u32 = 1;

/// A connection of the domain as the run holds it.
#[derive(Clone, Debug)]
struct Link {
    lan: bool,
    ports: Vec<(Identity, u32)>,
    delay: u64,
    /// Bits per second; zero when unlimited.
    bandwidth: u64,
    jitter: u64,
    /// `loss` as a threshold on a 64-bit draw: dropped when the draw is below it.
    loss: u64,
    mtu: u32,
    enabled: bool,
    cost: BTreeMap<(Identity, u32), u32>,
    port_mtu: BTreeMap<(Identity, u32), u32>,
    loss_stream: Stream,
    jitter_stream: Stream,
}

/// The network domain's state for one run.
#[derive(Clone, Debug)]
pub struct NetworkDomain {
    links: BTreeMap<Identity, Link>,
    by_interface: BTreeMap<(Identity, u32), Identity>,
    /// Nodes taken down by the `node` intervention (rule 13).
    down: BTreeSet<Identity>,
    blackholes: BTreeMap<Identity, u64>,
    tick: TickLength,
}

impl Default for NetworkDomain {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkDomain {
    pub fn new() -> Self {
        Self {
            links: BTreeMap::new(),
            by_interface: BTreeMap::new(),
            down: BTreeSet::new(),
            blackholes: BTreeMap::new(),
            tick: TickLength::Ns,
        }
    }
}

/// One attribute of a connection (rule 9), parsed from a manifest value or a change payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Attr {
    Delay(u64),
    Bandwidth(u64),
    Jitter(u64),
    /// The threshold on a 64-bit draw.
    Loss(u64),
    Mtu(u32),
    Enabled(bool),
}

/// `0.25` as a threshold on `u64`: `floor(loss * 2^64)`, exactly.
fn threshold(loss: &Decimal) -> u64 {
    if loss.mantissa == 0 {
        return 0;
    }
    let scaled = loss.mantissa.checked_mul(1u128 << 64);
    let divisor = 10u128.checked_pow(loss.exponent.unsigned_abs());
    match (loss.exponent <= 0, scaled, divisor) {
        (true, Some(s), Some(d)) => (s / d).min(u128::from(u64::MAX)) as u64,
        _ => u64::MAX,
    }
}

/// A real in `[0, 1)`.
fn loss_of(v: &DomainValue) -> Option<Decimal> {
    match v {
        DomainValue::Integer(0) => Decimal::parse("0"),
        DomainValue::Decimal(s) => {
            let d = Decimal::parse(s)?;
            let below_one = d.mantissa == 0
                || (d.exponent < 0 && {
                    let digits = d.mantissa.to_string().len() as i32;
                    digits + d.exponent <= 0
                });
            below_one.then_some(d)
        }
        _ => None,
    }
}

/// The units the domain registers (rules 1 and 9): bits per second and its decimal multiples.
pub fn bandwidth_units() -> Vec<(&'static str, Decimal)> {
    vec![
        ("bit/s", Decimal::parse("1").expect("a decimal")),
        ("kbit/s", Decimal::parse("1000").expect("a decimal")),
        ("Mbit/s", Decimal::parse("1000000").expect("a decimal")),
        ("Gbit/s", Decimal::parse("1000000000").expect("a decimal")),
    ]
}

/// A bandwidth as whole bits per second.
fn bandwidth_of(v: &DomainValue) -> Option<u64> {
    let DomainValue::Text(s) = v else {
        return None;
    };
    let q = Quantity::parse(s).ok()?;
    let factor = bandwidth_units().into_iter().find(|(u, _)| *u == q.unit)?.1;
    let bits = Decimal {
        mantissa: q.number.mantissa,
        exponent: q.number.exponent + factor.normalised().exponent,
    };
    let bits = Decimal {
        mantissa: bits.mantissa.checked_mul(factor.normalised().mantissa)?,
        exponent: bits.exponent,
    };
    let n = bits.as_integer_at(0)?;
    u64::try_from(n).ok().filter(|n| *n > 0)
}

fn ticks_of(v: &DomainValue, tick: TickLength) -> Option<u64> {
    let DomainValue::Text(s) = v else {
        return None;
    };
    Quantity::parse(s).ok()?.to_ticks(tick).ok()?
}

/// Parse one attribute; `Err` names the type expected (rule 18, `network.attr-type`).
fn attr(key: &str, v: &DomainValue, tick: TickLength) -> Result<Attr, &'static str> {
    match key {
        "delay" => ticks_of(v, tick)
            .map(Attr::Delay)
            .ok_or("a time quantity in whole ticks"),
        "jitter" => ticks_of(v, tick)
            .map(Attr::Jitter)
            .ok_or("a time quantity in whole ticks"),
        "bandwidth" => bandwidth_of(v)
            .map(Attr::Bandwidth)
            .ok_or("a quantity in bit/s, kbit/s, Mbit/s or Gbit/s"),
        "loss" => loss_of(v)
            .map(|l| Attr::Loss(threshold(&l)))
            .ok_or("a real in [0, 1)"),
        "mtu" => match v {
            DomainValue::Integer(n) if (1..=i64::from(u32::MAX)).contains(n) => {
                Ok(Attr::Mtu(*n as u32))
            }
            _ => Err("a whole number of bytes"),
        },
        "enabled" => match v {
            DomainValue::Boolean(b) => Ok(Attr::Enabled(*b)),
            _ => Err("a boolean"),
        },
        _ => Err("an attribute of rule 9"),
    }
}

/// The serialisation time of `bytes` at `bandwidth` bits per second, in whole ticks, rounded
/// up (rule 9): `ceil(8n / bandwidth)` seconds over the tick length.
pub fn serialisation_ticks(bytes: usize, bandwidth: u64, tick: TickLength) -> u64 {
    if bandwidth == 0 {
        return 0;
    }
    let bits = (bytes as u128) * 8;
    // seconds = bits / bandwidth; ticks = seconds * 10^15 / 10^tick_fs
    let scale = 10u128.pow(15 - tick.femto_exponent());
    let num = bits * scale;
    let ticks = num.div_ceil(u128::from(bandwidth));
    u64::try_from(ticks).unwrap_or(u64::MAX)
}

/// Uniform on `0..n` by the SDK's algorithm for `below` (RFC 001 rule 30), so that every Host
/// draws the same jitter.
fn below(stream: &mut Stream, n: u64) -> u64 {
    if n <= 1 {
        return 0;
    }
    let threshold = n.wrapping_neg() % n;
    loop {
        let x = stream.next_u64();
        let m = (x as u128) * (n as u128);
        if (m as u64) >= threshold {
            return (m >> 64) as u64;
        }
    }
}

fn code(name: &str, field: String, params: Vec<(&'static str, Param)>) -> Notification {
    refusal(name, field, params)
}

fn warning(name: &str, module: &str, params: Vec<(&'static str, Value)>) -> Notification {
    let mut n = Notification::new(name, Severity::Warning, Scope::file(module), Ends::Nothing);
    for (k, v) in params {
        n = n.with(k, v);
    }
    n
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

fn invalid(field: &str) -> Box<Notification> {
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
}

/// A link-state event's payload: the interface's port and the new state.
fn link_state_payload(port: u32, up: bool) -> Vec<u8> {
    let mut p = port.to_le_bytes().to_vec();
    p.push(u8::from(up));
    p
}

/// A frame event's payload: the receiving port, then the frame.
fn frame_payload(port: u32, frame: &[u8]) -> Vec<u8> {
    let mut p = port.to_le_bytes().to_vec();
    p.extend_from_slice(frame);
    p
}

impl NetworkDomain {
    fn link_of(&self, actor: Identity, port: u32) -> Option<&Link> {
        self.by_interface
            .get(&(actor, port))
            .and_then(|id| self.links.get(id))
    }

    /// `link-info.up` for an interface: the link enabled and the node up (rule 15).
    fn up(&self, actor: Identity, port: u32) -> bool {
        self.link_of(actor, port)
            .is_some_and(|l| l.enabled && !self.down.contains(&actor))
    }

    /// Schedule `on-link` for every interface of a link, with each interface's own state.
    fn link_states(&self, id: Identity, actions: &mut Vec<Action>) {
        let Some(l) = self.links.get(&id) else {
            return;
        };
        for (actor, port) in &l.ports {
            actions.push(Action::Schedule {
                origin: id,
                delay: 0,
                class: CLASS_LINK_STATE,
                target: *actor,
                payload: link_state_payload(*port, l.enabled && !self.down.contains(actor)),
            });
        }
    }

    /// Set a link's attributes from a change or a `link` intervention (rule 12).
    fn set_link(&mut self, id: Identity, payload: &DomainMap) -> Vec<Action> {
        let tick = self.tick;
        let Some(l) = self.links.get_mut(&id) else {
            return Vec::new();
        };
        let mut toggled = false;
        for (k, v) in payload {
            match attr(k, v, tick) {
                Ok(Attr::Delay(t)) => l.delay = t,
                Ok(Attr::Bandwidth(b)) => l.bandwidth = b,
                Ok(Attr::Jitter(j)) => l.jitter = j,
                Ok(Attr::Loss(t)) => l.loss = t,
                Ok(Attr::Mtu(m)) => l.mtu = m,
                Ok(Attr::Enabled(e)) => {
                    toggled |= l.enabled != e;
                    l.enabled = e;
                }
                Err(_) => {}
            }
        }
        let detail = format!(
            "enabled={} delay={} bandwidth={} jitter={} mtu={}",
            l.enabled, l.delay, l.bandwidth, l.jitter, l.mtu
        );
        let mut actions = Vec::new();
        if toggled {
            self.link_states(id, &mut actions);
        }
        actions.push(observed("link-changed", id, detail));
        actions
    }

    /// Take a node down or up (rule 13): its own interfaces and, on point-to-point links,
    /// its neighbours learn the state.
    fn set_node(&mut self, actor: Identity, up: bool) -> Vec<Action> {
        let was_down = self.down.contains(&actor);
        if up {
            self.down.remove(&actor);
        } else {
            self.down.insert(actor);
        }
        let mut actions = Vec::new();
        if was_down != !up {
            let links: Vec<Identity> = self
                .by_interface
                .iter()
                .filter(|((a, _), _)| *a == actor)
                .map(|(_, id)| *id)
                .collect();
            for id in links {
                let Some(l) = self.links.get(&id) else {
                    continue;
                };
                for (a, port) in &l.ports {
                    if *a != actor && l.lan {
                        continue;
                    }
                    actions.push(Action::Schedule {
                        origin: id,
                        delay: 0,
                        class: CLASS_LINK_STATE,
                        target: *a,
                        payload: link_state_payload(
                            *port,
                            l.enabled && !self.down.contains(a) && (*a == actor || up),
                        ),
                    });
                }
            }
        }
        actions.push(observed("node-changed", actor, format!("up={up}")));
        actions
    }

    fn check_link_payload(&self, payload: &DomainMap) -> Result<(), Box<Notification>> {
        for (k, v) in payload {
            if attr(k, v, self.tick).is_err() {
                return Err(invalid(&format!("content.{k}")));
            }
        }
        Ok(())
    }
}

impl Domain for NetworkDomain {
    fn name(&self) -> &'static str {
        NAME
    }
    fn connection_types(&self) -> Vec<&'static str> {
        vec![LINK, LAN]
    }
    fn port_types(&self) -> Vec<&'static str> {
        vec![INTERFACE]
    }
    fn builtin_kinds(&self) -> Vec<&'static str> {
        vec![BLACKHOLE]
    }
    fn builtin_channels(&self, _: &str) -> Vec<u32> {
        vec![BLACKHOLE_CHANNEL]
    }
    fn intervention_kinds(&self) -> Vec<&'static str> {
        vec!["link", "node"]
    }
    fn classes(&self) -> Vec<(u8, &'static str)> {
        vec![
            (CLASS_LINK_STATE, "link-state"),
            (CLASS_FRAME, "frame"),
            (CLASS_FRAME_DROPPED, "frame-dropped"),
        ]
    }
    fn units(&self) -> Vec<(&'static str, Decimal)> {
        bandwidth_units()
    }
    fn world(&self) -> Option<DomainWorld> {
        Some(DomainWorld {
            name: WORLD.to_string(),
            imports_ids: vec![LINK_MODULE.to_string()],
            exports_ids: vec![EVENTS_ID.to_string()],
            imports: vec![(
                LINK_MODULE.to_string(),
                vec![Signature {
                    name: "link-info",
                    params: vec![("port", Ty::U32)],
                    result: Some(Ty::record(&[
                        ("up", Ty::Bool),
                        ("cost", Ty::U32),
                        ("mtu", Ty::U32),
                        ("bandwidth", Ty::U64),
                    ])),
                }],
            )],
            exports: vec![(
                EVENTS_ID.to_string(),
                vec![Signature {
                    name: "on-link",
                    params: vec![("now", Ty::U64), ("port", Ty::U32), ("up", Ty::Bool)],
                    result: None,
                }],
            )],
        })
    }

    fn validate(&self, scenario: &Manifest) -> Vec<Notification> {
        let mut out = Vec::new();
        let tick = scenario.time.tick;
        for (i, c) in scenario.connections.iter().enumerate() {
            if scenario.connection_domain(c) != NAME {
                continue;
            }
            let path = format!("connections[{i}]");
            let name = || Param::Name(c.name.clone());
            match c.r#type.as_deref() {
                Some(LINK) if c.ports.len() != 2 => out.push(code(
                    "network.link-ports",
                    format!("{path}.ports"),
                    vec![
                        ("connection", name()),
                        ("found", Param::Count(c.ports.len() as u64)),
                    ],
                )),
                Some(LINK) | Some(LAN) => {}
                Some(_) => {}
                None => out.push(code(
                    "manifest.missing-field",
                    format!("{path}.type"),
                    vec![("field", Param::Text(format!("{path}.type")))],
                )),
            }
            for (j, pr) in c.ports.iter().enumerate() {
                if let Some(p) = scenario.port(pr.actor, pr.port)
                    && p.r#type != INTERFACE
                {
                    out.push(code(
                        "network.port-type",
                        format!("{path}.ports[{j}]"),
                        vec![
                            ("connection", name()),
                            ("port", Param::Name(p.name.clone())),
                            ("type", Param::Text(p.r#type.clone())),
                        ],
                    ));
                }
            }
            for (k, v) in &c.attrs {
                let field = format!("{path}.attrs.{k}");
                match attr(k, v, tick) {
                    Ok(_) => {}
                    Err("an attribute of rule 9") => out.push(code(
                        "network.attr-unknown",
                        field,
                        vec![("connection", name()), ("key", Param::Name(k.clone()))],
                    )),
                    Err(expected) => out.push(code(
                        "network.attr-type",
                        field,
                        vec![
                            ("connection", name()),
                            ("key", Param::Name(k.clone())),
                            ("expected", Param::Text(expected.into())),
                        ],
                    )),
                }
            }
            for (pr, m) in &c.per_port {
                for (k, v) in m {
                    let field = format!("{path}.attrs.per-port.{}.{}.{k}", pr.actor, pr.port);
                    match (k.as_str(), v) {
                        ("cost", DomainValue::Integer(n))
                            if (0..=i64::from(u32::MAX)).contains(n) => {}
                        ("mtu", DomainValue::Integer(n))
                            if (1..=i64::from(u32::MAX)).contains(n) => {}
                        ("cost", _) | ("mtu", _) => out.push(code(
                            "network.attr-type",
                            field,
                            vec![
                                ("connection", name()),
                                ("key", Param::Name(k.clone())),
                                ("expected", Param::Text("a whole number".into())),
                            ],
                        )),
                        _ => out.push(code(
                            "network.attr-unknown",
                            field,
                            vec![("connection", name()), ("key", Param::Name(k.clone()))],
                        )),
                    }
                }
            }
        }
        for (i, a) in scenario.actors.iter().enumerate() {
            if a.kind != ActorKind::Builtin(BLACKHOLE.to_string()) {
                continue;
            }
            for (j, p) in a.ports.iter().enumerate() {
                if p.r#type != INTERFACE {
                    out.push(code(
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
            for k in table.keys() {
                out.push(code(
                    "manifest.unknown-key",
                    format!("{NAME}.{k}"),
                    vec![("key", Param::Text(k.clone()))],
                ));
            }
        }
        out
    }

    fn validate_module(&self, module: &str, manifest: &ModuleManifest) -> Vec<Notification> {
        let mut out = Vec::new();
        if let Some(table) = manifest.domain_tables.get(NAME) {
            for (k, v) in table {
                match (k.as_str(), v) {
                    ("protocol", DomainValue::Text(p)) => {
                        if !decode::PROTOCOLS.contains(&p.as_str()) {
                            out.push(warning(
                                "network.protocol-unknown",
                                module,
                                vec![
                                    ("module", Value::Name(module.into())),
                                    ("protocol", Value::Text(p.clone())),
                                ],
                            ));
                        }
                    }
                    _ => out.push(
                        Notification::new(
                            "manifest.unknown-key",
                            Severity::Error,
                            Scope::file(module),
                            Ends::Command,
                        )
                        .with("key", Value::Text(format!("{NAME}.{k}"))),
                    ),
                }
            }
        }
        // The layouts the view reads (rule 17), by channel name.
        for ch in &manifest.telemetry {
            let expected: Option<&[(&str, ParamType)]> = match ch.name.as_str() {
                "neighbour" => Some(&[
                    ("port", ParamType::U64),
                    ("neighbour", ParamType::U64),
                    ("state", ParamType::Text),
                ]),
                "route" => Some(&[
                    ("prefix", ParamType::Text),
                    ("next-hop", ParamType::U64),
                    ("metric", ParamType::U64),
                    ("action", ParamType::Text),
                ]),
                _ => None,
            };
            if let Some(expected) = expected {
                let same = ch.fields.len() == expected.len()
                    && ch
                        .fields
                        .iter()
                        .zip(expected)
                        .all(|((n, t), (en, et))| n == en && t == et);
                if !same {
                    out.push(warning(
                        "network.telemetry-layout",
                        module,
                        vec![
                            ("module", Value::Name(module.into())),
                            ("channel", Value::Name(ch.name.clone())),
                        ],
                    ));
                }
            }
        }
        out
    }

    fn compose(&mut self, scenario: &Manifest, root_seed: u64) {
        self.tick = scenario.time.tick;
        for c in &scenario.connections {
            if scenario.connection_domain(c) != NAME {
                continue;
            }
            let mut l = Link {
                lan: c.r#type.as_deref() == Some(LAN),
                ports: c.ports.iter().map(|p| (p.actor, p.port)).collect(),
                delay: 0,
                bandwidth: 0,
                jitter: 0,
                loss: 0,
                mtu: DEFAULT_MTU,
                enabled: true,
                cost: BTreeMap::new(),
                port_mtu: BTreeMap::new(),
                // RFC 001 rules 28 and 29; RFC 008 rule 3: the connection's own streams.
                loss_stream: Stream::new(StreamKey::derive(root_seed, c.id, "loss")),
                jitter_stream: Stream::new(StreamKey::derive(root_seed, c.id, "jitter")),
            };
            for (k, v) in &c.attrs {
                match attr(k, v, self.tick) {
                    Ok(Attr::Delay(t)) => l.delay = t,
                    Ok(Attr::Bandwidth(b)) => l.bandwidth = b,
                    Ok(Attr::Jitter(j)) => l.jitter = j,
                    Ok(Attr::Loss(t)) => l.loss = t,
                    Ok(Attr::Mtu(m)) => l.mtu = m,
                    Ok(Attr::Enabled(e)) => l.enabled = e,
                    Err(_) => {}
                }
            }
            for (pr, m) in &c.per_port {
                if let Some(DomainValue::Integer(n)) = m.get("cost") {
                    l.cost.insert((pr.actor, pr.port), *n as u32);
                }
                if let Some(DomainValue::Integer(n)) = m.get("mtu") {
                    l.port_mtu.insert((pr.actor, pr.port), *n as u32);
                }
            }
            for p in &l.ports {
                self.by_interface.insert(*p, c.id);
            }
            self.links.insert(c.id, l);
        }
        for a in &scenario.actors {
            if a.kind == ActorKind::Builtin(BLACKHOLE.to_string()) {
                self.blackholes.insert(a.id, 0);
            }
        }
    }

    fn deliver(
        &mut self,
        _now: Instant,
        origin: Identity,
        _seq: u64,
        port: u32,
        payload: &[u8],
        _dead: &BTreeSet<Identity>,
    ) -> Vec<Delivery> {
        let Some(id) = self.by_interface.get(&(origin, port)).copied() else {
            return Vec::new();
        };
        let node_down = self.down.contains(&origin);
        let tick = self.tick;
        let l = self.links.get_mut(&id).expect("an interface's link");
        let dropped = |reason: &str| Delivery {
            target: id,
            class: CLASS_FRAME_DROPPED,
            delay: 0,
            kind: EventKind::Domain {
                payload: reason.as_bytes().to_vec(),
            },
        };
        // Rule 10, step 1.
        if !l.enabled || node_down {
            return vec![dropped("down")];
        }
        let mtu = l.port_mtu.get(&(origin, port)).copied().unwrap_or(l.mtu);
        if payload.len() > mtu as usize {
            return vec![dropped("too-large")];
        }
        let serialisation = serialisation_ticks(payload.len(), l.bandwidth, tick);
        let mut receivers: Vec<(Identity, u32)> = l
            .ports
            .iter()
            .copied()
            .filter(|p| *p != (origin, port))
            .collect();
        receivers.sort_unstable();
        let mut out = Vec::new();
        for (actor, their_port) in receivers {
            // Steps 2 to 4, one draw each from the connection's streams.
            if l.loss > 0 && l.loss_stream.next_u64() < l.loss {
                out.push(dropped(&format!("loss {actor}")));
                continue;
            }
            let j = if l.jitter > 0 {
                below(&mut l.jitter_stream, l.jitter + 1)
            } else {
                0
            };
            out.push(Delivery {
                target: actor,
                class: CLASS_FRAME,
                delay: l.delay.saturating_add(serialisation).saturating_add(j),
                kind: EventKind::Domain {
                    payload: frame_payload(their_port, payload),
                },
            });
        }
        out
    }

    fn change(&mut self, _now: Instant, target: Identity, payload: &DomainMap) -> Vec<Action> {
        self.set_link(target, payload)
    }

    fn check_intervention(
        &self,
        target: Identity,
        kind: &str,
        content: &DomainMap,
    ) -> Result<(), Box<Notification>> {
        match kind {
            "link" => {
                if !self.links.contains_key(&target) {
                    return Err(invalid("target"));
                }
                self.check_link_payload(content)
            }
            "node" => {
                if !self.by_interface.keys().any(|(a, _)| *a == target) {
                    return Err(invalid("target"));
                }
                for (k, v) in content {
                    match (k.as_str(), v) {
                        ("up", DomainValue::Boolean(_)) => {}
                        ("actor", DomainValue::Integer(a)) if *a == i64::from(target) => {}
                        _ => return Err(invalid(&format!("content.{k}"))),
                    }
                }
                if !content.contains_key("up") {
                    return Err(invalid("content.up"));
                }
                Ok(())
            }
            _ => Err(invalid("kind")),
        }
    }

    fn intervene(
        &mut self,
        _now: Instant,
        target: Identity,
        kind: &str,
        content: &DomainMap,
    ) -> Vec<Action> {
        match kind {
            "link" => self.set_link(target, content),
            "node" => match content.get("up") {
                Some(DomainValue::Boolean(up)) => self.set_node(target, *up),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    fn event(&mut self, event: &DomainEvent, alive: bool) -> Vec<Action> {
        match event.key.class {
            CLASS_FRAME => {
                let Some((port, frame)) = event
                    .payload
                    .split_at_checked(4)
                    .map(|(p, f)| (u32::from_le_bytes(p.try_into().expect("four bytes")), f))
                else {
                    return Vec::new();
                };
                let link = self.by_interface.get(&(event.target, port)).copied();
                // Rule 5 and rule 13: dropped at delivery, with the reason.
                let reason = if !alive {
                    Some("faulted")
                } else if self.down.contains(&event.target) {
                    Some("down")
                } else {
                    None
                };
                if let Some(reason) = reason {
                    return vec![observed(
                        "frame-dropped",
                        link.unwrap_or(event.target),
                        reason.into(),
                    )];
                }
                vec![Action::Call {
                    actor: event.target,
                    export: EXPORT_ON_MESSAGE.to_string(),
                    values: vec![
                        florn_abi::Val::U64(event.key.tick),
                        florn_abi::Val::U32(port),
                        florn_abi::Val::bytes(frame),
                    ],
                }]
            }
            CLASS_LINK_STATE => {
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
                    export: EXPORT_ON_LINK.to_string(),
                    values: vec![
                        florn_abi::Val::U64(event.key.tick),
                        florn_abi::Val::U32(port),
                        florn_abi::Val::Bool(up),
                    ],
                }]
            }
            CLASS_FRAME_DROPPED => {
                let reason = String::from_utf8_lossy(&event.payload).into_owned();
                vec![observed("frame-dropped", event.target, reason)]
            }
            _ => Vec::new(),
        }
    }

    fn answer(
        &mut self,
        actor: Identity,
        function: &str,
        args: &[florn_abi::Val],
    ) -> Option<florn_abi::Val> {
        match (function, args) {
            ("link-info", [florn_abi::Val::U32(port)]) => {
                let up = self.up(actor, *port);
                let l = self.link_of(actor, *port);
                let cost = l
                    .and_then(|l| l.cost.get(&(actor, *port)).copied())
                    .unwrap_or(1);
                let mtu = l
                    .map(|l| l.port_mtu.get(&(actor, *port)).copied().unwrap_or(l.mtu))
                    .unwrap_or(DEFAULT_MTU);
                let bandwidth = l.map_or(0, |l| l.bandwidth);
                Some(florn_abi::Val::Record(vec![
                    florn_abi::Val::Bool(up),
                    florn_abi::Val::U32(cost),
                    florn_abi::Val::U32(mtu),
                    florn_abi::Val::U64(bandwidth),
                ]))
            }
            _ => None,
        }
    }

    fn builtin(&mut self, actor: Identity, _kind: &str, event: &BuiltinEvent<'_>, sink: &mut Sink) {
        if let BuiltinEvent::Domain(ev) = event
            && ev.key.class == CLASS_FRAME
        {
            let n = self.blackholes.entry(actor).or_insert(0);
            *n += 1;
            let _ = sink.report(BLACKHOLE_CHANNEL, n.to_le_bytes().to_vec());
        }
    }

    fn decode(&self, protocol: &str, payload: &[u8]) -> Option<Vec<(String, String)>> {
        decode::decode(protocol, payload)
    }

    fn telemetry(&self) -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
        vec![
            (
                "neighbour",
                vec![("port", "u64"), ("neighbour", "u64"), ("state", "text")],
            ),
            (
                "route",
                vec![
                    ("prefix", "text"),
                    ("next-hop", "u64"),
                    ("metric", "u64"),
                    ("action", "text"),
                ],
            ),
            ("dropped", vec![("count", "u64")]),
        ]
    }

    fn export(&self) -> Vec<u8> {
        let mut e = Encoder::default();
        e.u32(self.links.len() as u32);
        for (id, l) in &self.links {
            e.u32(*id);
            e.u8(u8::from(l.enabled));
            e.u64(l.delay);
            e.u64(l.bandwidth);
            e.u64(l.jitter);
            e.u64(l.loss);
            e.u32(l.mtu);
            e.u64(l.loss_stream.drawn());
            e.u64(l.jitter_stream.drawn());
        }
        e.list(&self.down.iter().copied().collect::<Vec<_>>(), |e, a| {
            e.u32(*a)
        });
        e.u32(self.blackholes.len() as u32);
        for (id, n) in &self.blackholes {
            e.u32(*id);
            e.u64(*n);
        }
        e.finish()
    }

    fn restore(&mut self, bytes: &[u8]) -> bool {
        if bytes.is_empty() {
            return true;
        }
        let mut d = Decoder::new(bytes);
        let Some(n) = d.u32() else { return false };
        for _ in 0..n {
            let (Some(id), Some(enabled)) = (d.u32(), d.u8()) else {
                return false;
            };
            let (Some(delay), Some(bandwidth), Some(jitter), Some(loss)) =
                (d.u64(), d.u64(), d.u64(), d.u64())
            else {
                return false;
            };
            let (Some(mtu), Some(loss_drawn), Some(jitter_drawn)) = (d.u32(), d.u64(), d.u64())
            else {
                return false;
            };
            let Some(l) = self.links.get_mut(&id) else {
                return false;
            };
            l.enabled = enabled != 0;
            l.delay = delay;
            l.bandwidth = bandwidth;
            l.jitter = jitter;
            l.loss = loss;
            l.mtu = mtu;
            l.loss_stream = Stream::at(l.loss_stream.key(), loss_drawn);
            l.jitter_stream = Stream::at(l.jitter_stream.key(), jitter_drawn);
        }
        let Some(down) = d.list(Decoder::u32) else {
            return false;
        };
        self.down = down.into_iter().collect();
        let Some(n) = d.u32() else { return false };
        for _ in 0..n {
            let (Some(id), Some(count)) = (d.u32(), d.u64()) else {
                return false;
            };
            self.blackholes.insert(id, count);
        }
        d.done()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialisation_rounds_up_over_the_tick() {
        // 1,500 bytes at 1 Mbit/s take 12 ms: 12,000,000 ns ticks.
        assert_eq!(
            serialisation_ticks(1500, 1_000_000, TickLength::Ns),
            12_000_000
        );
        // At 1000 Gbit/s: 12 ns.
        assert_eq!(
            serialisation_ticks(1500, 1_000_000_000_000, TickLength::Ns),
            12
        );
        assert_eq!(serialisation_ticks(1, 1_000_000_000_000, TickLength::Ns), 1);
        assert_eq!(serialisation_ticks(1500, 0, TickLength::Ns), 0);
    }

    #[test]
    fn attributes_parse_by_rule_9() {
        let t = TickLength::Ns;
        assert_eq!(
            attr("delay", &DomainValue::Text("20ns".into()), t),
            Ok(Attr::Delay(20))
        );
        assert_eq!(
            attr("bandwidth", &DomainValue::Text("1Mbit/s".into()), t),
            Ok(Attr::Bandwidth(1_000_000))
        );
        assert_eq!(
            attr("bandwidth", &DomainValue::Text("2.5Gbit/s".into()), t),
            Ok(Attr::Bandwidth(2_500_000_000))
        );
        assert_eq!(
            attr("loss", &DomainValue::Decimal("0.5".into()), t),
            Ok(Attr::Loss(1 << 63))
        );
        assert_eq!(attr("loss", &DomainValue::Integer(0), t), Ok(Attr::Loss(0)));
        assert!(attr("loss", &DomainValue::Decimal("1.0".into()), t).is_err());
        assert!(attr("loss", &DomainValue::Integer(1), t).is_err());
        assert_eq!(
            attr("mtu", &DomainValue::Integer(9000), t),
            Ok(Attr::Mtu(9000))
        );
        assert_eq!(
            attr("enabled", &DomainValue::Boolean(false), t),
            Ok(Attr::Enabled(false))
        );
        assert_eq!(
            attr("colour", &DomainValue::Integer(1), t),
            Err("an attribute of rule 9")
        );
        assert!(attr("delay", &DomainValue::Text("1.5ns".into()), t).is_err());
    }

    #[test]
    fn below_matches_the_sdk() {
        let mut s = Stream::new(StreamKey::derive(1, 2, "jitter"));
        for _ in 0..1000 {
            assert!(below(&mut s, 5) < 5);
        }
        assert_eq!(below(&mut s, 1), 0);
    }
}
