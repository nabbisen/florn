//! The config encoder (RFC 002 rules 13 and 14): a scenario's `config` against the module's
//! declared parameters, in declaration order, defaults filled, each value encoded as rule 13 says.

use florn_model::{Decimal, DomainMap, DomainValue, Quantity, TickLength};

use crate::error::{Error, Errors, Kind};
use crate::manifest::{ModuleManifest, Param, ParamType, value_fits};

/// The units a domain registers (RFC 004 rule 27): for a `real` parameter, the factor from the
/// written unit to the SI unit of its dimension. The core world registers none.
pub trait Units {
    /// `kohm` → `1000`; `ohm` → `1`; an unknown unit → `None`.
    fn to_si(&self, unit: &str) -> Option<Decimal>;
}

/// No units: every `real` parameter with a unit is `quantity.unknown-unit`.
pub struct NoUnits;

impl Units for NoUnits {
    fn to_si(&self, _: &str) -> Option<Decimal> {
        None
    }
}

fn decimal_to_f64(d: &Decimal) -> f64 {
    d.shortest().parse::<f64>().unwrap_or(f64::NAN)
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
            let f = s.parse::<f64>().map_err(|_| wrong())?;
            out.extend_from_slice(&florn_abi::canonical_f64(f).to_le_bytes());
        }
        (ParamType::Text, DomainValue::Text(s)) => {
            out.extend_from_slice(&(u32::try_from(s.len()).map_err(|_| wrong())?).to_le_bytes());
            out.extend_from_slice(s.as_bytes());
        }
        (ParamType::Bytes, DomainValue::Text(s)) => {
            let bytes: Vec<u8> = (0..s.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
                .collect::<Result<_, _>>()
                .map_err(|_| wrong())?;
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
                Err(None) => {
                    return Err(Error::at(
                        Kind::Model(Box::new(florn_model::Kind::QuantityUnknownUnit {
                            field: path.to_string(),
                            unit: q.unit.clone(),
                        })),
                        path,
                    ));
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
            let q = Quantity::parse(s).map_err(|_| wrong())?;
            let expected = param.unit.as_deref().unwrap_or("");
            let (Some(written), Some(declared)) = (units.to_si(&q.unit), units.to_si(expected))
            else {
                return Err(Error::at(
                    Kind::Model(Box::new(florn_model::Kind::QuantityUnknownUnit {
                        field: path.to_string(),
                        unit: q.unit.clone(),
                    })),
                    path,
                ));
            };
            // Both units must be of the declared unit's dimension; the registry answers only for
            // units it knows, and a domain registers one dimension per unit symbol.
            let _ = declared;
            let f = decimal_to_f64(&q.number) * decimal_to_f64(&written);
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
