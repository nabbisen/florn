//! A settable value set from outside the plant (RFC 012 rule 15): a scheduled change addressed to
//! the world and an intervention of kind `source` carry the same content, `actor` (an identity or a
//! name) and the value's field typed as the built-in's parameter: `v` of a `voltage-source`, `i` of
//! a `current-source`, `value` of a `constant`, `threshold` of a `comparator`. An intervention of
//! kind `signal` freezes or releases a signal's held value.

use florn_admission::config::Units;
use florn_control::{Code, Notification, Scope, Severity, Value};
use florn_host::domain::quantity_si;
use florn_model::Quantity;
use florn_model::manifest::{DomainMap, DomainValue};

use crate::analyse::{as_written, converted};
use crate::inputs::Input;
use crate::plant::{Element, Plant};
use crate::read::{core, missing_unit, note};
use crate::units::CircuitUnits;

/// What a settable value change comes to: the plant's input, its value (SI, or in the signal's
/// unit for a `constant`), and for a `constant` the signal whose held value it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Source {
    pub actor: u32,
    pub input: Input,
    pub value: f64,
    pub signal: Option<usize>,
}

/// Why a change could not be read.
#[derive(Debug)]
pub enum Fault {
    /// No actor of that identity or name, or none given.
    Actor(String),
    /// A key the actor's kind does not set.
    Key {
        actor: String,
        key: String,
        expected: &'static str,
    },
    /// A value of the wrong type for the key.
    Type {
        actor: String,
        key: String,
        expected: &'static str,
    },
    /// The core's refusal of a quantity.
    Value(Box<Notification>),
}

impl Fault {
    /// The notification a scenario's validation gives, `field` being the path of the payload.
    pub fn at_field(self, field: &str) -> Notification {
        match self {
            Fault::Actor(text) => *core(
                &format!("{field}.actor"),
                florn_model::Kind::ReferenceUnresolved {
                    reference: text,
                    kind: "actor",
                },
            ),
            Fault::Key {
                actor,
                key,
                expected,
            } => note(
                Code::CircuitParamUnknown,
                Scope::field(format!("{field}.{key}")),
                Some(&format!("{field}.{key}")),
                vec![
                    Value::Name(actor),
                    Value::Name(key),
                    Value::Text(expected.to_string()),
                ],
            ),
            Fault::Type {
                actor,
                key,
                expected,
            } => note(
                Code::CircuitParamType,
                Scope::field(format!("{field}.{key}")),
                Some(&format!("{field}.{key}")),
                vec![
                    Value::Name(actor),
                    Value::Name(key),
                    Value::Text(expected.to_string()),
                ],
            ),
            Fault::Value(n) => *n,
        }
    }

    /// The refusal of an intervention: `control.invalid` naming the field (RFC 005 rule 26).
    pub fn as_intervention(self) -> Box<Notification> {
        let field = match &self {
            Fault::Actor(_) => "content.actor".to_string(),
            Fault::Key { key, .. } | Fault::Type { key, .. } => format!("content.{key}"),
            Fault::Value(n) => n.scope.field.clone().unwrap_or_else(|| "content".into()),
        };
        invalid(&field)
    }
}

/// `control.invalid` for an intervention's field.
pub fn invalid(field: &str) -> Box<Notification> {
    Box::new(
        Notification::new(
            "control.invalid",
            Severity::Error,
            Scope::run(),
            florn_control::Ends::Command,
        )
        .with("command", Value::Name("intervene".into()))
        .with("reason", Value::Enum("wrong-type".into()))
        .with("field", Value::Text(field.into())),
    )
}

/// The field a kind sets, with the dimension its quantity has (`None`: a `constant`'s value, in the
/// unit of its signal).
fn settable(element: &Element) -> Option<(&'static str, Option<&'static str>)> {
    match element {
        Element::VoltageSource { .. } => Some(("v", Some("voltage"))),
        Element::CurrentSource { .. } => Some(("i", Some("current"))),
        Element::Comparator { .. } => Some(("threshold", Some("voltage"))),
        Element::Constant { .. } => Some(("value", None)),
        _ => None,
    }
}

/// The text of the keys a payload may hold, for the `expected` of a refusal.
const KEYS: &str = "actor, and one of v, i, value, threshold";

/// Read a payload of a scheduled change or of a `source` intervention against the plant.
pub fn source(plant: &Plant, payload: &DomainMap, field: &str) -> Result<Source, Fault> {
    let device = match payload.get("actor") {
        Some(DomainValue::Integer(id)) => plant.devices.iter().find(|d| i64::from(d.actor) == *id),
        Some(DomainValue::Text(name)) => plant.devices.iter().find(|d| &d.name == name),
        _ => return Err(Fault::Actor(String::new())),
    };
    let Some(device) = device else {
        return Err(Fault::Actor(match payload.get("actor") {
            Some(DomainValue::Integer(id)) => id.to_string(),
            Some(DomainValue::Text(t)) => t.clone(),
            _ => String::new(),
        }));
    };
    let name = device.name.clone();
    let Some((key, dimension)) = settable(&device.element) else {
        return Err(Fault::Key {
            actor: name,
            key: "actor".into(),
            expected: KEYS,
        });
    };
    for k in payload.keys() {
        if k != "actor" && k != key {
            return Err(Fault::Key {
                actor: name,
                key: k.clone(),
                expected: key,
            });
        }
    }
    let Some(v) = payload.get(key) else {
        return Err(Fault::Type {
            actor: name,
            key: key.into(),
            expected: "a value",
        });
    };
    let path = format!("{field}.{key}");
    let units = CircuitUnits;
    match (&device.element, dimension) {
        (Element::Constant { out, .. }, _) => {
            let signal = *out;
            let value = match signal.map(|s| &plant.signals[s]) {
                Some(s) if s.unit.is_some() => {
                    let unit = units.unit(s.unit.as_deref().unwrap_or(""));
                    match (v, unit) {
                        (DomainValue::Text(t), Some(u)) => {
                            let q = Quantity::parse_signed(t).map_err(|_| Fault::Type {
                                actor: name.clone(),
                                key: key.into(),
                                expected: "a quantity in the unit of its signal",
                            })?;
                            let w = units.unit(&q.unit).ok_or_else(|| {
                                Fault::Value(core(
                                    &path,
                                    florn_model::Kind::QuantityUnknownUnit {
                                        field: path.clone(),
                                        unit: q.unit.clone(),
                                    },
                                ))
                            })?;
                            if w.dimension != u.dimension {
                                return Err(Fault::Value(core(
                                    &path,
                                    florn_model::Kind::QuantityDimension {
                                        field: path.clone(),
                                        value: q.as_pair(),
                                        expected: u.dimension,
                                    },
                                )));
                            }
                            converted(&q, &w, &u).unwrap_or(0.0)
                        }
                        (DomainValue::Integer(_) | DomainValue::Decimal(_), Some(u)) => {
                            return Err(Fault::Value(missing_unit(&path, u.dimension)));
                        }
                        _ => {
                            return Err(Fault::Type {
                                actor: name,
                                key: key.into(),
                                expected: "a quantity in the unit of its signal",
                            });
                        }
                    }
                }
                _ => match v {
                    // A dimensionless signal takes a plain number (or a quantity as written).
                    DomainValue::Text(t) => {
                        as_written(&Quantity::parse_signed(t).map_err(|_| Fault::Type {
                            actor: name.clone(),
                            key: key.into(),
                            expected: "a number",
                        })?)
                        .unwrap_or(0.0)
                    }
                    other => crate::read::number(other).ok_or(Fault::Type {
                        actor: name,
                        key: key.into(),
                        expected: "a number",
                    })?,
                },
            };
            Ok(Source {
                actor: device.actor,
                input: signal.map_or(Input::Source(device.actor), Input::Signal),
                value,
                signal,
            })
        }
        (_, Some(dimension)) => match v {
            DomainValue::Text(t) => {
                let value = quantity_si(&units, &path, t, dimension).map_err(Fault::Value)?;
                Ok(Source {
                    actor: device.actor,
                    input: Input::Source(device.actor),
                    value,
                    signal: None,
                })
            }
            DomainValue::Integer(_) | DomainValue::Decimal(_) => {
                Err(Fault::Value(missing_unit(&path, dimension)))
            }
            _ => Err(Fault::Type {
                actor: name,
                key: key.into(),
                expected: "a quantity",
            }),
        },
        _ => unreachable!("every settable kind has a field and a dimension or a held value"),
    }
}

/// An intervention of kind `signal`: the connection (the target) and `held`, a boolean; the content
/// may repeat the connection as `connection`.
pub fn signal(
    plant: &Plant,
    target: u32,
    content: &DomainMap,
) -> Result<(usize, bool), Box<Notification>> {
    let Some(sig) = plant.signals.iter().position(|s| s.id == target) else {
        return Err(invalid("target"));
    };
    let mut held = None;
    for (k, v) in content {
        match (k.as_str(), v) {
            ("held", DomainValue::Boolean(b)) => held = Some(*b),
            ("connection", DomainValue::Integer(c)) if *c == i64::from(target) => {}
            _ => return Err(invalid(&format!("content.{k}"))),
        }
    }
    held.map(|h| (sig, h))
        .ok_or_else(|| invalid("content.held"))
}

#[cfg(test)]
mod tests;
