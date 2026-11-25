//! The solver profile (RFC 013 rules 1 and 2): the scenario's `[circuit]` table, the only place a
//! learner tunes the solver. Every field is optional and has its default; an unknown key is
//! `circuit.profile-unknown`, a value of the wrong type or outside its range is
//! `circuit.profile-value` naming the expectation, a quantity of the wrong dimension is the core's
//! `quantity.dimension`. A value above the Host's maximum (`max-unknowns`, `max-breakpoints`) is
//! `circuit.profile-value` too (rule 18).

use florn_admission::config::Units;
use florn_control::{Code, Notification, Scope, Value};
use florn_host::FaultPolicy;
use florn_host::domain::quantity_si;
use florn_model::TickLength;
use florn_model::manifest::{DomainMap, DomainValue};

use crate::read::{note, number, time_ticks};

/// The Host's maxima of the profile's bounds (RFC 013 rule 18), given to the domain when the Host
/// builds it. A scenario may lower them and cannot raise them (RFC 001 rule 13). The figures are
/// the starting figures of the rule; a measurement may lower them and only an amendment raises them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_unknowns: u64,
    pub max_breakpoints: u64,
}

impl Limits {
    /// The browser Host: 512 unknowns (2 MiB of matrix), 262,144 breakpoints (8 MiB).
    pub const fn browser() -> Self {
        Self {
            max_unknowns: 512,
            max_breakpoints: 262_144,
        }
    }
    /// The native Host: 1,024 unknowns (8 MiB), 1,000,000 breakpoints (32 MB).
    pub const fn native() -> Self {
        Self {
            max_unknowns: 1024,
            max_breakpoints: 1_000_000,
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self::browser()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Trapezoidal,
    Gear2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Fixed,
    Adaptive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnNonConvergence {
    StopRun,
    Hold,
}

/// The profile with every field read (rule 2). Times are in ticks, everything else in SI units.
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub method: Method,
    pub step: u64,
    pub control: Control,
    pub min_step: u64,
    pub max_step: u64,
    pub ltol: f64,
    pub max_iterations: u32,
    pub abstol: f64,
    pub reltol: f64,
    pub vntol: f64,
    pub gmin: f64,
    pub max_rejections: u32,
    pub on_non_convergence: OnNonConvergence,
    pub faulted_policy: FaultPolicy,
    pub detect_interval: u64,
    pub max_breakpoints: u64,
    pub max_unknowns: u64,
    pub atol: f64,
    pub rtol: f64,
}

/// `1 µs` in ticks, the default `step`: the tick is never longer than a nanosecond.
fn microsecond(tick: TickLength) -> u64 {
    10u64.pow(9 - tick.femto_exponent())
}

impl Profile {
    /// The profile of a scenario that declares the domain without the table.
    pub fn defaults(tick: TickLength, limits: Limits) -> Self {
        let step = microsecond(tick);
        Self {
            method: Method::Trapezoidal,
            step,
            control: Control::Adaptive,
            min_step: 1,
            max_step: 100 * step,
            ltol: 1e-3,
            max_iterations: 50,
            abstol: 1e-12,
            reltol: 1e-6,
            vntol: 1e-9,
            gmin: 1e-12,
            max_rejections: 60,
            on_non_convergence: OnNonConvergence::StopRun,
            faulted_policy: FaultPolicy::Drop,
            detect_interval: 100 * step,
            max_breakpoints: 65_536.min(limits.max_breakpoints),
            max_unknowns: 256.min(limits.max_unknowns),
            atol: 1e-9,
            rtol: 1e-6,
        }
    }
}

/// The keys of the table, for the `expected` of `circuit.profile-unknown`.
pub const KEYS: &str = "method, step, control, min-step, max-step, ltol, newton.max-iterations, \
newton.abstol, newton.reltol, newton.vntol, gmin, max-rejections, on-non-convergence, \
faulted-policy, detect-interval, max-breakpoints, max-unknowns, tolerance.atol, tolerance.rtol";

struct Reader<'a> {
    units: &'a dyn Units,
    tick: TickLength,
    out: Vec<Notification>,
    /// A time field was refused: the relations between the steps are not checked then.
    time_failed: bool,
}

impl Reader<'_> {
    fn value(&mut self, key: &str, expected: &str) {
        let field = format!("circuit.{key}");
        self.out.push(note(
            Code::CircuitProfileValue,
            Scope::field(field.clone()),
            Some(&field),
            vec![
                Value::Name(key.to_string()),
                Value::Text(expected.to_string()),
            ],
        ));
    }

    fn word<T: Copy>(&mut self, key: &str, v: &DomainValue, words: &[(&str, T)]) -> Option<T> {
        if let DomainValue::Text(t) = v
            && let Some((_, x)) = words.iter().find(|(w, _)| w == t)
        {
            return Some(*x);
        }
        let list: Vec<String> = words.iter().map(|(w, _)| format!("\"{w}\"")).collect();
        self.value(key, &format!("one of {}", list.join(", ")));
        None
    }

    fn whole(&mut self, key: &str, v: &DomainValue, low: u64, high: u64) -> Option<u64> {
        match v {
            DomainValue::Integer(i) if (low as i64..=high as i64).contains(i) => Some(*i as u64),
            _ => {
                self.value(key, &format!("a whole number from {low} to {high}"));
                None
            }
        }
    }

    fn real(
        &mut self,
        key: &str,
        v: &DomainValue,
        low: f64,
        high: f64,
        open: (bool, bool),
    ) -> Option<f64> {
        let ok = |x: f64| {
            (if open.0 { x > low } else { x >= low }) && (if open.1 { x < high } else { x <= high })
        };
        match number(v) {
            Some(x) if ok(x) => Some(x),
            _ => {
                let (l, r) = (
                    if open.0 { "(" } else { "[" },
                    if open.1 { ")" } else { "]" },
                );
                self.value(key, &format!("a number in {l}{low}, {high}{r}"));
                None
            }
        }
    }

    /// A quantity of a dimension, above zero or from zero, at most `high` (SI); the core's
    /// refusal for a wrong unit or dimension.
    fn quantity(
        &mut self,
        key: &str,
        v: &DomainValue,
        dimension: &'static str,
        zero_ok: bool,
        high: f64,
    ) -> Option<f64> {
        let field = format!("circuit.{key}");
        let words = if zero_ok { "from zero" } else { "above zero" };
        match v {
            DomainValue::Text(t) => match quantity_si(self.units, &field, t, dimension) {
                Ok(x) if (if zero_ok { x >= 0.0 } else { x > 0.0 }) && x <= high => Some(x),
                Ok(_) => {
                    self.value(
                        key,
                        &format!("a quantity of {dimension} {words} and at most {high}"),
                    );
                    None
                }
                Err(n) => {
                    self.out.push(*n);
                    None
                }
            },
            DomainValue::Integer(_) | DomainValue::Decimal(_) => {
                self.out.push(*crate::read::missing_unit(&field, dimension));
                None
            }
            _ => {
                self.value(key, &format!("a quantity of {dimension} {words}"));
                None
            }
        }
    }

    fn time(&mut self, key: &str, v: &DomainValue) -> Option<u64> {
        let field = format!("circuit.{key}");
        match time_ticks(self.units, &field, v, self.tick) {
            Ok(t) if t >= 1 => Some(t),
            Ok(_) => {
                self.time_failed = true;
                self.value(key, "a time of at least 1 tick");
                None
            }
            Err(n) => {
                self.time_failed = true;
                self.out.push(*n);
                None
            }
        }
    }
}

/// Read the `[circuit]` table (absent: every default) with the Host's maxima. Every error is
/// found in one pass; a field that is refused keeps its default.
pub fn read(
    table: Option<&DomainMap>,
    tick: TickLength,
    limits: Limits,
    units: &dyn Units,
) -> (Profile, Vec<Notification>) {
    let mut p = Profile::defaults(tick, limits);
    let mut r = Reader {
        units,
        tick,
        out: Vec::new(),
        time_failed: false,
    };
    let Some(table) = table else {
        return (p, r.out);
    };
    let (mut step, mut min_step, mut max_step, mut detect) = (None, None, None, None);
    let mut flat: Vec<(String, &DomainValue)> = Vec::new();
    for (k, v) in table {
        match (k.as_str(), v) {
            ("newton" | "tolerance", DomainValue::Map(m)) => {
                flat.extend(m.iter().map(|(s, v)| (format!("{k}.{s}"), v)));
            }
            _ => flat.push((k.clone(), v)),
        }
    }
    for (key, v) in flat {
        match key.as_str() {
            "method" => {
                if let Some(m) = r.word(
                    &key,
                    v,
                    &[
                        ("trapezoidal", Method::Trapezoidal),
                        ("gear2", Method::Gear2),
                    ],
                ) {
                    p.method = m;
                }
            }
            "step" => step = r.time(&key, v),
            "min-step" => min_step = r.time(&key, v),
            "max-step" => max_step = r.time(&key, v),
            "detect-interval" => detect = r.time(&key, v),
            "control" => {
                if let Some(c) = r.word(
                    &key,
                    v,
                    &[("fixed", Control::Fixed), ("adaptive", Control::Adaptive)],
                ) {
                    p.control = c;
                }
            }
            "ltol" => p.ltol = r.real(&key, v, 0.0, 1.0, (true, false)).unwrap_or(p.ltol),
            "newton.max-iterations" => {
                p.max_iterations = r
                    .whole(&key, v, 1, 1000)
                    .map_or(p.max_iterations, |x| x as u32);
            }
            "newton.abstol" => {
                p.abstol = r
                    .quantity(&key, v, "current", false, f64::MAX)
                    .unwrap_or(p.abstol)
            }
            "newton.reltol" => {
                p.reltol = r.real(&key, v, 0.0, 1.0, (true, true)).unwrap_or(p.reltol)
            }
            "newton.vntol" => {
                p.vntol = r
                    .quantity(&key, v, "voltage", false, f64::MAX)
                    .unwrap_or(p.vntol)
            }
            "gmin" => {
                p.gmin = r
                    .quantity(&key, v, "conductance", true, 1e-3)
                    .unwrap_or(p.gmin)
            }
            "max-rejections" => {
                p.max_rejections = r
                    .whole(&key, v, 1, 1000)
                    .map_or(p.max_rejections, |x| x as u32);
            }
            "on-non-convergence" => {
                if let Some(x) = r.word(
                    &key,
                    v,
                    &[
                        ("stop-run", OnNonConvergence::StopRun),
                        ("hold", OnNonConvergence::Hold),
                    ],
                ) {
                    p.on_non_convergence = x;
                }
            }
            "faulted-policy" => {
                if let Some(x) = r.word(
                    &key,
                    v,
                    &[
                        ("drop", FaultPolicy::Drop),
                        ("stop-run", FaultPolicy::StopRun),
                    ],
                ) {
                    p.faulted_policy = x;
                }
            }
            "max-breakpoints" => {
                let high = limits.max_breakpoints;
                p.max_breakpoints = r.whole(&key, v, 1, high).unwrap_or(p.max_breakpoints);
            }
            "max-unknowns" => {
                let high = limits.max_unknowns;
                p.max_unknowns = r.whole(&key, v, 1, high).unwrap_or(p.max_unknowns);
            }
            "tolerance.atol" => {
                p.atol = r
                    .real(&key, v, 0.0, f64::MAX, (false, false))
                    .unwrap_or(p.atol)
            }
            "tolerance.rtol" => p.rtol = r.real(&key, v, 0.0, 1.0, (false, true)).unwrap_or(p.rtol),
            _ => {
                let field = format!("circuit.{key}");
                r.out.push(note(
                    Code::CircuitProfileUnknown,
                    Scope::field(field.clone()),
                    Some(&field),
                    vec![Value::Name(key.clone()), Value::Text(KEYS.to_string())],
                ));
            }
        }
    }
    // The steps, and their order (rule 2): `min-step ≤ step ≤ max-step` and `max-step ≥ 2 × min-step`;
    // the defaults of the fields not written follow the ones that were.
    let step_written = step.is_some();
    p.step = step.unwrap_or(p.step);
    p.min_step = min_step.unwrap_or(p.min_step);
    p.max_step = max_step.unwrap_or(if step_written {
        100 * p.step
    } else {
        p.max_step
    });
    p.detect_interval = detect.unwrap_or(p.max_step);
    if !r.time_failed {
        if p.min_step > p.step {
            r.value("min-step", "at most step");
        }
        if p.max_step < p.step {
            r.value("max-step", "at least step");
        } else if p.max_step < p.min_step.saturating_mul(2) {
            r.value("max-step", "at least twice min-step");
        }
    }
    (p, r.out)
}

#[cfg(test)]
mod tests;
