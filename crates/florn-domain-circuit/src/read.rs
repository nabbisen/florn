//! Reading the values a scenario writes for the domain: the notification of a code of RFC 012
//! rule 28, the refusals of the core for a quantity (RFC 007 §4.1), a time in whole ticks, a plain
//! number. A domain reads its own fields (RFC 004 rule 32), so these are the one place where the
//! text of a value becomes a number.

use florn_admission::config::Units;
use florn_control::{Code, Notification, Scope, Value};
use florn_host::domain::refusal;
use florn_model::manifest::DomainValue;
use florn_model::{Error, Kind, Quantity, TickLength};

/// A notification of a code of the catalogue, at a scope and with a field path.
pub fn note(code: Code, scope: Scope, field: Option<&str>, values: Vec<Value>) -> Notification {
    let mut n = code.notification(scope, values);
    n.scope.field = field.map(str::to_string);
    n
}

/// The core's refusal for a value of a field of the scenario.
pub fn core(field: &str, kind: Kind) -> Box<Notification> {
    let error = Error::new(kind);
    Box::new(refusal(error.code(), field, error.params()))
}

/// A bare number written where a quantity is needed.
pub fn missing_unit(field: &str, dimension: &'static str) -> Box<Notification> {
    core(
        field,
        Kind::QuantityMissingUnit {
            field: field.to_string(),
            dimension,
        },
    )
}

/// A plain number: an integer, or a decimal in its shortest text (`DomainValue`).
pub fn number(v: &DomainValue) -> Option<f64> {
    match v {
        DomainValue::Integer(i) => Some(*i as f64),
        DomainValue::Decimal(t) => t.parse::<f64>().ok().filter(|x| x.is_finite()),
        _ => None,
    }
}

/// A time quantity in whole ticks (RFC 004 rule 25), with the core's refusals: a bare number is
/// `quantity.missing-unit`, a unit that is no time unit is `quantity.dimension` or
/// `quantity.unknown-unit`, a value that is not whole is `quantity.not-whole-ticks`.
pub fn time_ticks(
    units: &dyn Units,
    field: &str,
    v: &DomainValue,
    tick: TickLength,
) -> Result<u64, Box<Notification>> {
    let text = match v {
        DomainValue::Text(t) => t,
        DomainValue::Integer(_) | DomainValue::Decimal(_) => {
            return Err(missing_unit(field, "time"));
        }
        _ => {
            return Err(core(
                field,
                Kind::ManifestType {
                    field: field.to_string(),
                    expected: "quantity",
                },
            ));
        }
    };
    let q = match Quantity::parse(text) {
        Ok(q) => q,
        Err(florn_model::quantity::QuantityError::MissingUnit) => {
            return Err(missing_unit(field, "time"));
        }
        Err(_) => {
            return Err(core(
                field,
                Kind::ManifestType {
                    field: field.to_string(),
                    expected: "quantity",
                },
            ));
        }
    };
    match q.to_ticks(tick) {
        Ok(Some(t)) => Ok(t),
        Ok(None) => Err(core(
            field,
            Kind::QuantityOutOfRange {
                field: field.to_string(),
                value: q.as_pair(),
            },
        )),
        Err(None) => {
            let known =
                units.unit(&q.unit).is_some() || matches!(q.unit.as_str(), "B" | "KiB" | "MiB");
            Err(core(
                field,
                if known {
                    Kind::QuantityDimension {
                        field: field.to_string(),
                        value: q.as_pair(),
                        expected: "time",
                    }
                } else {
                    Kind::QuantityUnknownUnit {
                        field: field.to_string(),
                        unit: q.unit.clone(),
                    }
                },
            ))
        }
        Err(Some(smallest)) => Err(core(
            field,
            Kind::QuantityNotWholeTicks {
                field: field.to_string(),
                value: q.as_pair(),
                tick: tick.as_quantity(),
                smallest: smallest.as_quantity(),
            },
        )),
    }
}

#[cfg(test)]
mod tests;
