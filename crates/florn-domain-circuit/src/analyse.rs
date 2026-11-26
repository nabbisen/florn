//! The scenario read once, completely (RFC 012 rule 28: every code in one pass): the nets and the
//! signals with their attributes and units, the pins and the ports of every built-in, the
//! parameters, the floating nodes, the size of the plant. When nothing is an error the result also
//! holds the [`Plant`] the solver runs; `validate` and `compose` are two views of the same reading.

use std::collections::BTreeMap;

use florn_admission::config::{UnitDef, Units};
use florn_control::{Code, Notification, Scope, Severity, Value};
use florn_host::domain::refusal;
use florn_model::manifest::{Actor, ActorKind, Connection, DomainValue};
use florn_model::{Decimal, Manifest, Param, Quantity};

use crate::build::{self, Built};
use crate::kinds::{self, Kind, Role};
use crate::params::{self, Params};
use crate::plant::{Net, Plant, Signal};
use crate::profile::{self, Limits, Profile};
use crate::read::{missing_unit, note};
use crate::units::CircuitUnits;
use crate::{NET, SIGNAL};

/// What the reading found.
pub struct Analysis {
    pub notes: Vec<Notification>,
    pub plant: Option<Plant>,
    pub profile: Profile,
}

/// A built-in actor with its kind and its parameters, in ascending actor identity.
pub(crate) struct Member<'a> {
    pub index: usize,
    pub actor: &'a Actor,
    pub kind: &'static Kind,
    pub params: Params,
}

/// A signal as the scenario writes it, before its unit is resolved.
struct Wire<'a> {
    index: usize,
    c: &'a Connection,
    producer: Option<(u32, u32)>,
    consumers: Vec<(u32, u32)>,
    declared: Option<UnitDef>,
}

fn name(s: &str) -> Value {
    Value::Name(s.to_string())
}

/// The number of a quantity as written, with its sign, rounded once (no unit conversion).
pub(crate) fn as_written(q: &Quantity) -> Option<f64> {
    let x = q
        .number
        .times_as_f64(Decimal {
            mantissa: 1,
            exponent: 0,
        })
        .ok()?;
    Some(if q.negative { -x } else { x })
}

/// A quantity written in `from`, in the unit `to`: exact decimal arithmetic, rounded once.
pub(crate) fn converted(q: &Quantity, from: &UnitDef, to: &UnitDef) -> Option<f64> {
    let shift = Decimal {
        mantissa: 1,
        exponent: from.factor.exponent - to.factor.exponent,
    };
    let x = q.number.times_as_f64(shift).ok()?;
    Some(if q.negative { -x } else { x })
}

/// The unit's factor to the SI unit as an f64 (a power of ten rounded once).
pub(crate) fn factor_of(u: &UnitDef) -> f64 {
    u.factor
        .times_as_f64(Decimal {
            mantissa: 1,
            exponent: 0,
        })
        .unwrap_or(1.0)
}

/// Read the scenario.
pub fn analyse(scenario: &Manifest, limits: Limits) -> Analysis {
    let units = CircuitUnits;
    let tick = scenario.time.tick;
    let (profile, mut notes) =
        profile::read(scenario.domain_tables.get("circuit"), tick, limits, &units);

    // The connections, in ascending identity: nets and signals.
    let mut nets: Vec<Net> = Vec::new();
    let mut net_of_port: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    let mut wires: Vec<Wire> = Vec::new();
    for (i, c) in scenario.connections.iter().enumerate() {
        if scenario.connection_domain(c) != crate::NAME {
            continue;
        }
        let path = format!("connections[{i}]");
        match c.r#type.as_deref() {
            Some(NET) => read_net(scenario, c, &path, &mut nets, &mut net_of_port, &mut notes),
            Some(SIGNAL) => {
                if let Some(w) = read_signal(scenario, c, i, &path, &units, &mut notes) {
                    wires.push(w);
                }
            }
            Some(_) => {}
            None => notes.push(refusal(
                "manifest.missing-field",
                format!("{path}.type"),
                vec![("field", Param::Text(format!("{path}.type")))],
            )),
        }
    }
    let grounds: Vec<&Net> = nets.iter().filter(|n| n.ground).collect();
    if !nets.is_empty() && grounds.is_empty() {
        notes.push(note(
            Code::CircuitNoGround,
            Scope::file(&scenario.name),
            None,
            vec![],
        ));
    }
    for n in grounds.iter().skip(1) {
        notes.push(note(
            Code::CircuitTwoGrounds,
            Scope::connection(n.id),
            None,
            vec![name(&n.name)],
        ));
    }

    // The built-ins, in ascending identity: ports, pins, parameters.
    let mut members: Vec<Member> = Vec::new();
    for (i, a) in scenario.actors.iter().enumerate() {
        match &a.kind {
            ActorKind::Builtin(b) => {
                if let Some(kind) = kinds::by_builtin(b) {
                    let (params, n) = params::read(kind, i, a, &units, tick);
                    notes.extend(n);
                    check_ports(i, a, kind, &mut notes);
                    members.push(Member {
                        index: i,
                        actor: a,
                        kind,
                        params,
                    });
                }
            }
            ActorKind::Module(_) => {}
        }
    }
    // A pin that is on no net, a guest's included (rule 4).
    for a in &scenario.actors {
        for p in a.ports.iter().filter(|p| p.r#type == crate::PIN) {
            if !net_of_port.contains_key(&(a.id, p.id)) {
                notes.push(note(
                    Code::CircuitPinOpen,
                    Scope::port(a.id, p.id),
                    None,
                    vec![name(&a.name), name(&p.name)],
                ));
            }
        }
    }
    for m in &members {
        for spec in m.kind.ports.iter().filter(|s| s.role == Role::Pin) {
            if !m.actor.ports.iter().any(|p| p.name == spec.name) {
                notes.push(note(
                    Code::CircuitPinOpen,
                    Scope::actor(m.actor.id),
                    None,
                    vec![name(&m.actor.name), name(spec.name)],
                ));
            }
        }
    }

    // The signals: their units, their dimensions, their held values (rules 4 and 5).
    let (signals, signal_of_port) =
        resolve_signals(scenario, &wires, &mut members, &units, &mut notes);
    for m in &members {
        for p in m
            .actor
            .ports
            .iter()
            .filter(|p| p.r#type == crate::SIGNAL_IN)
        {
            if m.kind.port(&p.name).is_some() && !signal_of_port.contains_key(&(m.actor.id, p.id)) {
                notes.push(note(
                    Code::CircuitSignalUnconnected,
                    Scope::port(m.actor.id, p.id),
                    None,
                    vec![name(&m.actor.name), name(&p.name)],
                ));
            }
        }
    }

    // The size of the plant, and whether anything observes it.
    let built = build::unknowns(&nets, &members);
    let n = (built.transient) as u64;
    if n > profile.max_unknowns {
        notes.push(note(
            Code::CircuitTooLarge,
            Scope::file(&scenario.name),
            None,
            vec![
                Value::Count(n),
                Value::Count(profile.max_unknowns),
                Value::Bytes(Plant::estimate(n, profile.max_breakpoints)),
            ],
        ));
    }
    let settable = members.iter().any(|m| m.kind.settable.is_some());
    let observed = members.iter().any(|m| {
        matches!(
            m.kind.name,
            "voltmeter" | "ammeter" | "dc-motor" | "comparator"
        )
    });
    if settable && !observed {
        notes.push(note(
            Code::CircuitPlantUnobserved,
            Scope::file(&scenario.name),
            None,
            vec![],
        ));
    }

    let mut plant = None;
    if !notes.iter().any(|n| n.severity == Severity::Error) {
        let wiring = build::Wiring {
            scenario,
            nets: &nets,
            net_of_port: &net_of_port,
            signal_of_port: &signal_of_port,
        };
        let Built { plant: p, floating } =
            build::plant(&wiring, &members, signals, profile.clone());
        for id in floating {
            let net = nets
                .iter()
                .find(|n| n.id == id)
                .map_or("", |n| n.name.as_str());
            notes.push(note(
                Code::CircuitFloatingNode,
                Scope::connection(id),
                None,
                vec![name(net)],
            ));
        }
        plant = Some(p);
    }
    Analysis {
        notes,
        plant,
        profile,
    }
}

fn read_net(
    scenario: &Manifest,
    c: &Connection,
    path: &str,
    nets: &mut Vec<Net>,
    net_of_port: &mut BTreeMap<(u32, u32), usize>,
    notes: &mut Vec<Notification>,
) {
    let index = nets.len();
    let mut ground = false;
    for (j, pr) in c.ports.iter().enumerate() {
        let Some(p) = scenario.port(pr.actor, pr.port) else {
            continue;
        };
        if p.r#type == crate::PIN {
            net_of_port.insert((pr.actor, pr.port), index);
        } else {
            notes.push(port_type(
                c,
                &format!("{path}.ports[{j}]"),
                &p.name,
                &p.r#type,
            ));
        }
    }
    for (k, v) in &c.attrs {
        let field = format!("{path}.attrs.{k}");
        match (k.as_str(), v) {
            ("ground", DomainValue::Boolean(g)) => ground = *g,
            ("ground", _) => notes.push(attr(Code::CircuitAttrType, c, &field, k, "a boolean")),
            _ => notes.push(attr(Code::CircuitAttrUnknown, c, &field, k, "ground")),
        }
    }
    per_port(c, path, notes);
    nets.push(Net {
        id: c.id,
        name: c.name.clone(),
        ground,
    });
}

fn per_port(c: &Connection, path: &str, notes: &mut Vec<Notification>) {
    if !c.per_port.is_empty() {
        notes.push(attr(
            Code::CircuitAttrUnknown,
            c,
            &format!("{path}.attrs.per-port"),
            "per-port",
            "ground, unit, initial",
        ));
    }
}

fn port_type(c: &Connection, field: &str, port: &str, ty: &str) -> Notification {
    note(
        Code::CircuitPortType,
        Scope::field(field.to_string()),
        Some(field),
        vec![name(&c.name), name(port), Value::Text(ty.to_string())],
    )
}

fn attr(code: Code, c: &Connection, field: &str, key: &str, expected: &str) -> Notification {
    note(
        code,
        Scope::field(field.to_string()),
        Some(field),
        vec![name(&c.name), name(key), Value::Text(expected.to_string())],
    )
}

fn read_signal<'a>(
    scenario: &Manifest,
    c: &'a Connection,
    index: usize,
    path: &str,
    units: &dyn Units,
    notes: &mut Vec<Notification>,
) -> Option<Wire<'a>> {
    let mut wire = Wire {
        index,
        c,
        producer: None,
        consumers: Vec::new(),
        declared: None,
    };
    let mut outs = 0u64;
    for (j, pr) in c.ports.iter().enumerate() {
        let Some(p) = scenario.port(pr.actor, pr.port) else {
            continue;
        };
        match p.r#type.as_str() {
            crate::SIGNAL_OUT => {
                outs += 1;
                wire.producer.get_or_insert((pr.actor, pr.port));
            }
            crate::SIGNAL_IN => wire.consumers.push((pr.actor, pr.port)),
            _ => notes.push(port_type(
                c,
                &format!("{path}.ports[{j}]"),
                &p.name,
                &p.r#type,
            )),
        }
    }
    if outs != 1 {
        notes.push(note(
            Code::CircuitSignalPorts,
            Scope::connection(c.id),
            None,
            vec![name(&c.name), Value::Count(outs)],
        ));
    }
    for (k, v) in &c.attrs {
        let field = format!("{path}.attrs.{k}");
        match (k.as_str(), v) {
            ("unit", DomainValue::Text(sym)) => match units.unit(sym) {
                Some(u) => wire.declared = Some(u),
                None => notes.push(attr(
                    Code::CircuitAttrType,
                    c,
                    &field,
                    k,
                    "a unit of the circuit domain",
                )),
            },
            ("unit", _) => notes.push(attr(
                Code::CircuitAttrType,
                c,
                &field,
                k,
                "a unit of the circuit domain",
            )),
            ("initial", _) => {}
            _ => notes.push(attr(
                Code::CircuitAttrUnknown,
                c,
                &field,
                k,
                "unit, initial",
            )),
        }
    }
    per_port(c, path, notes);
    Some(wire)
}

/// The ports of a built-in against its kind's row: a port the kind lacks, or of another type, is
/// the core's `module.port-type-unknown` (the form the network domain uses for the same fault).
fn check_ports(index: usize, a: &Actor, kind: &Kind, notes: &mut Vec<Notification>) {
    for (j, p) in a.ports.iter().enumerate() {
        let fits = kind
            .port(&p.name)
            .is_some_and(|s| s.role.port_type() == p.r#type);
        if !fits {
            notes.push(refusal(
                "module.port-type-unknown",
                format!("actors[{index}].ports[{j}].type"),
                vec![
                    ("actor", Param::Identity(a.id)),
                    ("port", Param::Name(p.name.clone())),
                    ("type", Param::Text(p.r#type.clone())),
                ],
            ));
        }
    }
}

/// The default unit of a port whose signal declares none (rule 4): the unit of the dimension.
fn base_unit(dimension: &str) -> Option<&'static str> {
    Some(match dimension {
        "voltage" => "V",
        "current" => "A",
        "angular-speed" => "rad/s",
        "torque" => "Nm",
        _ => return None,
    })
}

type PortSignals = BTreeMap<(u32, u32), usize>;

/// Each signal's unit, dimension, factor and starting held value, with the checks of rules 4 and 5.
fn resolve_signals(
    scenario: &Manifest,
    wires: &[Wire],
    members: &mut [Member],
    units: &dyn Units,
    notes: &mut Vec<Notification>,
) -> (Vec<Signal>, PortSignals) {
    let mut signals = Vec::new();
    let mut of_port = PortSignals::new();
    for wire in wires {
        let c = wire.c;
        let idx = signals.len();
        let producer = wire.producer.and_then(|(a, p)| {
            let m = members.iter().position(|m| m.actor.id == a)?;
            let port = scenario.port(a, p)?;
            Some((m, port.name.clone()))
        });
        let spec = producer
            .as_ref()
            .and_then(|(m, pname)| members[*m].kind.port(pname));
        // The unit and the value of a `constant`'s output (rule 23).
        let mut unit: Option<UnitDef> = wire.declared;
        let mut held = 0.0;
        let mut pdim = spec.and_then(|s| s.dimension);
        if let Some((m, pname)) = &producer {
            let member = &members[*m];
            if member.kind.name == "constant" {
                let field = format!("actors[{}].config.value", member.index);
                match member.params.held.clone() {
                    Some(DomainValue::Text(t)) => match Quantity::parse_signed(&t) {
                        Ok(q) => match units.unit(&q.unit) {
                            Some(w) => {
                                pdim = Some(w.dimension);
                                match wire.declared {
                                    Some(d) if d.dimension != w.dimension => notes.push(note(
                                        Code::CircuitSignalDimension,
                                        Scope::connection(c.id),
                                        None,
                                        vec![
                                            name(&c.name),
                                            name(pname),
                                            Value::Enum(d.dimension.to_string()),
                                        ],
                                    )),
                                    Some(d) => held = converted(&q, &w, &d).unwrap_or(0.0),
                                    None => {
                                        unit = Some(w);
                                        held = as_written(&q).unwrap_or(0.0);
                                    }
                                }
                            }
                            None => notes.push(*crate::read::core(
                                &field,
                                florn_model::Kind::QuantityUnknownUnit {
                                    field: field.clone(),
                                    unit: q.unit.clone(),
                                },
                            )),
                        },
                        Err(_) => notes.push(*missing_unit(&field, "voltage")),
                    },
                    Some(v) => {
                        if let Some(x) = crate::read::number(&v) {
                            held = x;
                        }
                    }
                    None => {}
                }
            } else if let Some(dim) = pdim {
                match wire.declared {
                    Some(d) if d.dimension != dim => notes.push(note(
                        Code::CircuitSignalDimension,
                        Scope::connection(c.id),
                        None,
                        vec![name(&c.name), name(pname), Value::Enum(dim.to_string())],
                    )),
                    Some(_) => {}
                    None => unit = base_unit(dim).and_then(|s| units.unit(s)),
                }
            }
        }
        // A consumer that declares a dimension needs a signal of it (the motor's `load`).
        for &(a, p) in &wire.consumers {
            let Some(m) = members.iter().find(|m| m.actor.id == a) else {
                continue;
            };
            let Some(port) = scenario.port(a, p) else {
                continue;
            };
            of_port.insert((a, p), idx);
            // A consumer with a dimension needs a signal of it, one without (`cmd`, `ctl`) a
            // signal with no unit: the other way round is the mistake RFC 012 rule 4 names, with
            // the dimension `dimensionless` when the consumer is the one without.
            if let Some(spec) = m.kind.port(&port.name) {
                let expected = spec.dimension.unwrap_or("dimensionless");
                let have = unit.map(|u| u.dimension);
                if have != spec.dimension {
                    notes.push(note(
                        Code::CircuitSignalDimension,
                        Scope::connection(c.id),
                        None,
                        vec![
                            name(&c.name),
                            name(&port.name),
                            Value::Enum(expected.to_string()),
                        ],
                    ));
                }
            }
        }
        for &(a, p) in &wire.consumers {
            of_port.insert((a, p), idx);
        }
        if let Some((a, p)) = wire.producer {
            of_port.insert((a, p), idx);
        }
        // `initial`: a quantity of the signal's dimension, or a plain number when it has none.
        let field = format!("connections[{}].attrs.initial", wire.index);
        let is_constant = producer
            .as_ref()
            .is_some_and(|(m, _)| members[*m].kind.name == "constant");
        let mut initial = 0.0;
        if let Some(v) = c.attrs.get("initial") {
            match (v, unit) {
                (DomainValue::Text(t), Some(u)) => match Quantity::parse_signed(t) {
                    Ok(q) => match units.unit(&q.unit) {
                        Some(w) if w.dimension == u.dimension => {
                            initial = converted(&q, &w, &u).unwrap_or(0.0)
                        }
                        Some(_) => notes.push(*crate::read::core(
                            &field,
                            florn_model::Kind::QuantityDimension {
                                field: field.clone(),
                                value: q.as_pair(),
                                expected: u.dimension,
                            },
                        )),
                        None => notes.push(*crate::read::core(
                            &field,
                            florn_model::Kind::QuantityUnknownUnit {
                                field: field.clone(),
                                unit: q.unit.clone(),
                            },
                        )),
                    },
                    Err(_) => notes.push(attr(
                        Code::CircuitAttrType,
                        c,
                        &field,
                        "initial",
                        "a quantity of the signal's dimension",
                    )),
                },
                (DomainValue::Integer(_) | DomainValue::Decimal(_), Some(u)) => {
                    notes.push(*missing_unit(&field, u.dimension));
                }
                (v, None) => match crate::read::number(v) {
                    Some(x) => initial = x,
                    None => notes.push(attr(
                        Code::CircuitAttrType,
                        c,
                        &field,
                        "initial",
                        "a number",
                    )),
                },
                _ => notes.push(attr(
                    Code::CircuitAttrType,
                    c,
                    &field,
                    "initial",
                    "a quantity of the signal's dimension",
                )),
            }
        }
        let held = if is_constant { held } else { initial };
        // The comparator's output starts at 0 (rule 30); a sensor's signal holds `initial`.
        let held = match producer.as_ref().map(|(m, _)| members[*m].kind.name) {
            Some("comparator") => 0.0,
            _ => held,
        };
        signals.push(Signal {
            id: c.id,
            name: c.name.clone(),
            unit: unit.map(|u| u.symbol.to_string()),
            dimension: unit.map(|u| u.dimension),
            factor: unit.map_or(1.0, |u| factor_of(&u)),
            held,
            producer: wire
                .producer
                .and_then(|(a, p)| scenario.port(a, p).map(|port| (a, port.name.clone()))),
        });
    }
    (signals, of_port)
}

#[cfg(test)]
mod tests;
