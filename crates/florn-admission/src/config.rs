//! The config encoder (RFC 002 rules 13 and 14): a scenario's `config` against the module's
//! declared parameters, in declaration order, defaults filled, each value encoded as rule 13 says.

use florn_model::{Decimal, DomainMap, DomainValue, Quantity, QuantityRefusal, TickLength};

use crate::error::{Error, Errors, Kind};
use crate::manifest::{ModuleManifest, Param, ParamType, value_fits};

/// A unit a domain registers (RFC 004 rule 27; RFC 012 rule 5 and finding F-13): its symbol, the
/// factor from it to the SI unit of its dimension, and the dimension's name (a value of the
/// enumeration `dimension`, RFC 007 section 4.5). A prefixed symbol (`kohm`) is a unit of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitDef {
    pub symbol: &'static str,
    pub factor: Decimal,
    pub dimension: &'static str,
}

/// The units the domains register. The core world registers none.
pub trait Units {
    /// `kohm` → `1000` of `resistance`; an unknown unit → `None`.
    fn unit(&self, symbol: &str) -> Option<UnitDef>;
}

/// No units: every `real` parameter with a unit is `quantity.unknown-unit`.
pub struct NoUnits;

impl Units for NoUnits {
    fn unit(&self, _: &str) -> Option<UnitDef> {
        None
    }
}

/// A number as written, cut to what a message may carry (RFC 004 rule 23: nothing is built from a
/// number the input chose).
fn shown(text: &str) -> String {
    match text.char_indices().nth(64) {
        Some((i, _)) => format!("{}…", &text[..i]),
        None => text.to_string(),
    }
}

fn model_error(path: &str, kind: florn_model::Kind) -> Error {
    Error::at(Kind::Model(Box::new(kind)), path)
}

fn boxed_error(path: &str, kind: Box<florn_model::Kind>) -> Error {
    Error::at(Kind::Model(kind), path)
}

/// An `f64` parameter written as a number: a value that is not finite, or that is not zero and
/// becomes zero, is `quantity.out-of-range` and never a parameter (finding F-13); the text is
/// read as one decimal number and rounded once.
fn f64_of_number(text: &str, path: &str) -> Result<f64, Error> {
    let out_of_range = || {
        model_error(
            path,
            florn_model::Kind::QuantityOutOfRange {
                field: path.to_string(),
                value: (shown(text), String::new()),
            },
        )
    };
    let f: f64 = text.parse().map_err(|_| out_of_range())?;
    let mantissa = text.split(['e', 'E']).next().unwrap_or(text);
    let written_zero = !mantissa.bytes().any(|b| matches!(b, b'1'..=b'9'));
    if !f.is_finite() || (f == 0.0 && !written_zero) {
        return Err(out_of_range());
    }
    Ok(f)
}

impl Units for Vec<UnitDef> {
    fn unit(&self, symbol: &str) -> Option<UnitDef> {
        self.iter().find(|u| u.symbol == symbol).copied()
    }
}

/// A unit of the core (RFC 004 rules 25 and 26): a time or a number of bytes. A domain's field of
/// another dimension written in one is of the wrong dimension, not in a unit nobody knows.
fn is_core_unit(unit: &str) -> bool {
    Quantity::time_unit_femto_exponent(unit).is_some()
        || matches!(unit, "B" | "KiB" | "MiB" | "GiB")
}

/// The SI value of a quantity written for a field of the dimension `expected` (RFC 012 rule 5,
/// finding F-13, F-17): the unit must be one a domain registers, of that dimension; the number and
/// the unit's factor are converted by exact decimal arithmetic and rounded once, and the sign is
/// applied after, so that `-x` is exactly the negation of `x`. A value that would be infinity, or
/// that is not zero and would be zero, is `quantity.out-of-range`.
pub fn convert_quantity(
    units: &dyn Units,
    field: &str,
    q: &Quantity,
    expected: &'static str,
) -> Result<f64, Box<florn_model::Kind>> {
    use florn_model::Kind;
    let wrong_dimension = || {
        Box::new(Kind::QuantityDimension {
            field: field.to_string(),
            value: q.as_pair(),
            expected,
        })
    };
    let Some(written) = units.unit(&q.unit) else {
        return Err(if is_core_unit(&q.unit) {
            wrong_dimension()
        } else {
            Box::new(Kind::QuantityUnknownUnit {
                field: field.to_string(),
                unit: q.unit.clone(),
            })
        });
    };
    if written.dimension != expected {
        return Err(wrong_dimension());
    }
    let magnitude =
        q.number
            .times_as_f64(written.factor)
            .map_err(|_| Kind::QuantityOutOfRange {
                field: field.to_string(),
                value: {
                    let (number, unit) = q.as_pair();
                    (shown(&number), shown(&unit))
                },
            })?;
    Ok(if q.negative { -magnitude } else { magnitude })
}

/// A quantity in the text of a field a domain reads itself, an attribute (RFC 012 rule 5: "a
/// parameter **or an attribute**"), converted as a parameter is: the SI value, or the refusal the
/// core would give a scenario's field: `quantity.missing-unit`, `manifest.type` for a text that is
/// no quantity, `quantity.unknown-unit`, `quantity.dimension`, `quantity.out-of-range`. A domain
/// calls it from `validate` with its own units and the dimension the field expects; the sign is
/// allowed, and a field that cannot be negative is the domain's to check on the value.
pub fn quantity_si(
    units: &dyn Units,
    field: &str,
    text: &str,
    expected: &'static str,
) -> Result<f64, Box<florn_model::Kind>> {
    use florn_model::Kind;
    match Quantity::classify_signed(text) {
        Ok(q) => convert_quantity(units, field, &q, expected),
        Err(QuantityRefusal::OutOfRange { number, unit }) => {
            Err(Box::new(Kind::QuantityOutOfRange {
                field: field.to_string(),
                value: (number, unit),
            }))
        }
        Err(QuantityRefusal::MissingUnit) => Err(Box::new(Kind::QuantityMissingUnit {
            field: field.to_string(),
            dimension: expected,
        })),
        Err(_) => Err(Box::new(Kind::ManifestType {
            field: field.to_string(),
            expected: "quantity",
        })),
    }
}

/// `value / unit` as a whole number, or `None` if it is not whole or does not fit.
fn whole_quotient(value: &Decimal, unit: &Decimal) -> Option<i64> {
    let e = value.exponent.min(unit.exponent);
    let scale = |d: &Decimal| -> Option<u128> {
        10u128
            .checked_pow(u32::try_from(d.exponent - e).ok()?)
            .and_then(|p| d.mantissa.checked_mul(p))
    };
    let (a, b) = (scale(value)?, scale(unit)?);
    if b == 0 || a % b != 0 {
        return None;
    }
    i64::try_from(a / b).ok()
}

fn encode_one(
    out: &mut Vec<u8>,
    actor: u32,
    param: &Param,
    value: &DomainValue,
    tick: TickLength,
    units: &dyn Units,
    path: &str,
) -> Result<(), Error> {
    let wrong = || {
        Error::at(
            Kind::ConfigType {
                actor,
                parameter: param.name.clone(),
                expected: param.ty.name(),
            },
            path,
        )
    };
    // A number beyond what a quantity holds is out of range, not of the wrong type (RFC 004 rule
    // 23; SEC-005): the value as written, cut, and the parameter's path.
    if let (ParamType::Ticks | ParamType::Integer | ParamType::Real, DomainValue::Text(s)) =
        (param.ty, value)
        && let Err(QuantityRefusal::OutOfRange { number, unit }) = match param.ty {
            // Only a `real` may be negative (RFC 004 rule 23); a sign elsewhere is out of range.
            ParamType::Real => Quantity::classify_signed(s),
            _ => Quantity::classify(s),
        }
    {
        return Err(Error::at(
            Kind::Model(Box::new(florn_model::Kind::QuantityOutOfRange {
                field: path.to_string(),
                value: (number, unit),
            })),
            path,
        ));
    }
    if !value_fits(param.ty, value) {
        return Err(wrong());
    }
    match (param.ty, value) {
        (ParamType::Bool, DomainValue::Boolean(b)) => out.push(*b as u8),
        (ParamType::U64, DomainValue::Integer(i)) => {
            out.extend_from_slice(&(*i as u64).to_le_bytes())
        }
        (ParamType::S64, DomainValue::Integer(i)) => out.extend_from_slice(&i.to_le_bytes()),
        (ParamType::F64, DomainValue::Integer(i)) => {
            out.extend_from_slice(&florn_abi::canonical_f64(*i as f64).to_le_bytes())
        }
        (ParamType::F64, DomainValue::Decimal(s)) => {
            let f = f64_of_number(s, path)?;
            out.extend_from_slice(&florn_abi::canonical_f64(f).to_le_bytes());
        }
        (ParamType::Text, DomainValue::Text(s)) => {
            out.extend_from_slice(&(u32::try_from(s.len()).map_err(|_| wrong())?).to_le_bytes());
            out.extend_from_slice(s.as_bytes());
        }
        (ParamType::Bytes, DomainValue::Text(s)) => {
            // `value_fits` has checked an even number of hexadecimal digits; the pairs are read
            // by bytes and by digits all the same, so that no slice depends on that check.
            let bytes: Vec<u8> = s
                .as_bytes()
                .chunks(2)
                .map(|p| {
                    let hi = char::from(p[0]).to_digit(16)?;
                    let lo = char::from(*p.get(1)?).to_digit(16)?;
                    Some((hi * 16 + lo) as u8)
                })
                .collect::<Option<_>>()
                .ok_or_else(wrong)?;
            out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(&bytes);
        }
        (ParamType::Ticks, DomainValue::Text(s)) => {
            let q = Quantity::parse(s).map_err(|_| wrong())?;
            let ticks = match q.to_ticks(tick) {
                Ok(Some(t)) => t,
                Ok(None) => {
                    return Err(Error::at(
                        Kind::Model(Box::new(florn_model::Kind::QuantityOutOfRange {
                            field: path.to_string(),
                            value: q.as_pair(),
                        })),
                        path,
                    ));
                }
                // Not a time unit: a unit a domain registers, or one of the core's own (bytes), is
                // of another dimension; any other is unknown.
                Err(None) => {
                    let known = units.unit(&q.unit).is_some() || is_core_unit(&q.unit);
                    let kind = if known {
                        florn_model::Kind::QuantityDimension {
                            field: path.to_string(),
                            value: q.as_pair(),
                            expected: "time",
                        }
                    } else {
                        florn_model::Kind::QuantityUnknownUnit {
                            field: path.to_string(),
                            unit: q.unit.clone(),
                        }
                    };
                    return Err(model_error(path, kind));
                }
                Err(Some(smallest)) => {
                    return Err(Error::at(
                        Kind::Model(Box::new(florn_model::Kind::QuantityNotWholeTicks {
                            field: path.to_string(),
                            value: q.as_pair(),
                            tick: tick.as_quantity(),
                            smallest: smallest.as_quantity(),
                        })),
                        path,
                    ));
                }
            };
            out.extend_from_slice(&ticks.to_le_bytes());
        }
        (ParamType::Integer, DomainValue::Text(s)) => {
            let q = Quantity::parse(s).map_err(|_| wrong())?;
            let delivery = param
                .unit
                .as_deref()
                .and_then(|u| Quantity::parse(u).ok())
                .ok_or_else(wrong)?;
            if q.unit != delivery.unit {
                return Err(Error::at(
                    Kind::Model(Box::new(florn_model::Kind::QuantityUnknownUnit {
                        field: path.to_string(),
                        unit: q.unit.clone(),
                    })),
                    path,
                ));
            }
            let n = whole_quotient(&q.number, &delivery.number).ok_or_else(|| {
                Error::at(
                    Kind::Model(Box::new(florn_model::Kind::QuantityNotWholeUnit {
                        field: path.to_string(),
                        value: q.as_pair(),
                        unit: delivery.as_pair().0 + &delivery.unit,
                    })),
                    path,
                )
            })?;
            out.extend_from_slice(&n.to_le_bytes());
        }
        (ParamType::Real, DomainValue::Text(s)) => {
            let q = Quantity::parse_signed(s).map_err(|_| wrong())?;
            let declared_symbol = param.unit.as_deref().unwrap_or("");
            // The declared unit is the module's: one the registry does not know is named as such.
            let declared = units.unit(declared_symbol).ok_or_else(|| {
                model_error(
                    path,
                    florn_model::Kind::QuantityUnknownUnit {
                        field: path.to_string(),
                        unit: declared_symbol.to_string(),
                    },
                )
            })?;
            let f = convert_quantity(units, path, &q, declared.dimension)
                .map_err(|kind| boxed_error(path, kind))?;
            out.extend_from_slice(&florn_abi::canonical_f64(f).to_le_bytes());
        }
        _ => return Err(wrong()),
    }
    Ok(())
}

/// Encode an actor's `config` for `init` (rule 14). Every error is reported; the bytes are only
/// returned when there is none.
pub fn encode_config(
    actor: u32,
    manifest: &ModuleManifest,
    config: &DomainMap,
    tick: TickLength,
    units: &dyn Units,
) -> Result<Vec<u8>, Errors> {
    let mut errors = Errors::default();
    let mut out = Vec::new();
    for key in config.keys() {
        if manifest.param(key).is_none() {
            errors.push(Error::at(
                Kind::ConfigUnknownParameter {
                    actor,
                    parameter: key.clone(),
                },
                format!("config.{key}"),
            ));
        }
    }
    for param in &manifest.params {
        let path = format!("config.{}", param.name);
        let value = match (config.get(&param.name), &param.default) {
            (Some(v), _) => v,
            (None, Some(d)) => d,
            (None, None) if param.required => {
                errors.push(Error::at(
                    Kind::ConfigMissingParameter {
                        actor,
                        parameter: param.name.clone(),
                    },
                    &path,
                ));
                continue;
            }
            // Not required, no default: the zero value of the type is delivered.
            (None, None) => {
                match param.ty {
                    ParamType::Bool => out.push(0),
                    ParamType::Text | ParamType::Bytes => {
                        out.extend_from_slice(&0u32.to_le_bytes())
                    }
                    _ => out.extend_from_slice(&0u64.to_le_bytes()),
                }
                continue;
            }
        };
        if let Err(e) = encode_one(&mut out, actor, param, value, tick, units, &path) {
            errors.push(e);
        }
    }
    errors.into_result(out)
}
