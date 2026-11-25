//! A built-in's `config` read against its kind (RFC 012 rule 23): every parameter in the SI unit
//! (a quantity converted by exact decimal arithmetic and rounded once, rule 5), a time in whole
//! ticks, a plain number. Every error of the actor is found in one pass: an unknown key, a value
//! of the wrong type or outside its range, a missing parameter (`circuit.param-*`), a sensor
//! without a period or with one of zero ticks (`circuit.no-period`, `circuit.period-zero`).

use std::collections::BTreeMap;

use florn_admission::config::Units;
use florn_control::{Code, Notification, Scope, Value};
use florn_host::domain::quantity_si;
use florn_model::TickLength;
use florn_model::manifest::{Actor, DomainValue};

use crate::kinds::{Kind, ParamSpec, Ty};
use crate::read::{missing_unit, note, number, time_ticks};

/// What a built-in's `config` says, once read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Params {
    pub reals: BTreeMap<&'static str, f64>,
    pub ticks: BTreeMap<&'static str, u64>,
    /// The `constant`'s `value` as written, resolved once its signal's unit is known (rule 23).
    pub held: Option<DomainValue>,
}

impl Params {
    /// A real parameter the kind declares and the actor has or defaults.
    pub fn real(&self, name: &str) -> f64 {
        self.reals.get(name).copied().unwrap_or(0.0)
    }
    pub fn opt(&self, name: &str) -> Option<f64> {
        self.reals.get(name).copied()
    }
    pub fn ticks(&self, name: &str) -> u64 {
        self.ticks.get(name).copied().unwrap_or(0)
    }
}

/// What a parameter of a type is, for the `expected` of the codes.
pub fn describe(ty: Ty) -> String {
    match ty {
        Ty::Real(dimension, range) => format!("a quantity of {dimension}{}", range.words()),
        Ty::Number(range) => format!("a number{}", range.words()),
        Ty::Ticks => "a time in whole ticks".to_string(),
        Ty::Held => "a quantity in the unit of its signal, or a number".to_string(),
    }
}

/// Read the `config` of `actor` (the `index`-th of the scenario) against its kind.
pub fn read(
    kind: &Kind,
    index: usize,
    actor: &Actor,
    units: &dyn Units,
    tick: TickLength,
) -> (Params, Vec<Notification>) {
    let mut out = Vec::new();
    let mut params = Params::default();
    let name = || Value::Name(actor.name.clone());
    let at = |key: &str| format!("actors[{index}].config.{key}");
    for key in actor.config.keys() {
        if kind.param(key).is_none() {
            out.push(note(
                Code::CircuitParamUnknown,
                Scope::field(at(key)),
                Some(&at(key)),
                vec![
                    name(),
                    Value::Name(key.clone()),
                    Value::Text(kind.param_names()),
                ],
            ));
        }
    }
    for spec in kind.params {
        let path = at(spec.name);
        match actor.config.get(spec.name) {
            Some(v) => read_one(spec, v, &path, actor, units, tick, &mut params, &mut out),
            None => absent(spec, actor, &path, &mut params, &mut out),
        }
    }
    (params, out)
}

fn absent(
    spec: &ParamSpec,
    actor: &Actor,
    path: &str,
    params: &mut Params,
    out: &mut Vec<Notification>,
) {
    if let Some(d) = spec.default {
        match spec.ty {
            Ty::Ticks => {
                params.ticks.insert(spec.name, d as u64);
            }
            Ty::Held => {}
            _ => {
                params.reals.insert(spec.name, d);
            }
        }
        return;
    }
    if spec.optional {
        return;
    }
    if spec.name == "period" {
        out.push(note(
            Code::CircuitNoPeriod,
            Scope::actor(actor.id),
            Some(path),
            vec![Value::Name(actor.name.clone())],
        ));
        return;
    }
    out.push(note(
        Code::CircuitParamMissing,
        Scope::field(path),
        Some(path),
        vec![
            Value::Name(actor.name.clone()),
            Value::Name(spec.name.to_string()),
            Value::Text(describe(spec.ty)),
        ],
    ));
}

#[allow(clippy::too_many_arguments)]
fn read_one(
    spec: &ParamSpec,
    v: &DomainValue,
    path: &str,
    actor: &Actor,
    units: &dyn Units,
    tick: TickLength,
    params: &mut Params,
    out: &mut Vec<Notification>,
) {
    let wrong = |out: &mut Vec<Notification>, ty: Ty| {
        out.push(note(
            Code::CircuitParamType,
            Scope::field(path.to_string()),
            Some(path),
            vec![
                Value::Name(actor.name.clone()),
                Value::Name(spec.name.to_string()),
                Value::Text(describe(ty)),
            ],
        ));
    };
    match spec.ty {
        Ty::Real(dimension, range) => match v {
            DomainValue::Text(text) => match quantity_si(units, path, text, dimension) {
                Ok(x) if range.holds(x) => {
                    params.reals.insert(spec.name, x);
                }
                Ok(_) => wrong(out, spec.ty),
                Err(refused) => out.push(*refused),
            },
            DomainValue::Integer(_) | DomainValue::Decimal(_) => {
                out.push(*missing_unit(path, dimension));
            }
            _ => wrong(out, spec.ty),
        },
        Ty::Number(range) => match number(v) {
            Some(x) if range.holds(x) => {
                params.reals.insert(spec.name, x);
            }
            _ => wrong(out, spec.ty),
        },
        Ty::Ticks => match time_ticks(units, path, v, tick) {
            Ok(0) if spec.name == "period" => out.push(note(
                Code::CircuitPeriodZero,
                Scope::actor(actor.id),
                Some(path),
                vec![Value::Name(actor.name.clone())],
            )),
            Ok(t) => {
                params.ticks.insert(spec.name, t);
            }
            Err(refused) => out.push(*refused),
        },
        Ty::Held => match v {
            DomainValue::Text(_) | DomainValue::Integer(_) | DomainValue::Decimal(_) => {
                params.held = Some(v.clone());
            }
            _ => wrong(out, spec.ty),
        },
    }
}

#[cfg(test)]
mod tests;
