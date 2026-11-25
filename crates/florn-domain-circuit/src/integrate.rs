//! The integration formulas of RFC 013 rules 6 and 11: for every dynamic state variable of an
//! element the coefficients of `x'_{n+1} = α·x_{n+1} − history` under backward Euler, the
//! trapezoidal rule and variable-step Gear 2, and the local error estimates from divided
//! differences of the accepted points since the last cut, the candidate included.
//!
//! The arithmetic is IEEE `+ − × ÷` in the order written and nothing else (rule 19); the one
//! function that is not exact is the cube root of the controller, which is `libm`'s
//! (`control.rs`). A step of `n` ticks is `h = n × tick` seconds (rule 14).

use crate::plant::{Element, Plant, UnknownKind};
use crate::profile::Method;

/// Which formula a step uses (rules 6 and 7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Formula {
    BackwardEuler,
    Trapezoidal,
    Gear2,
}

impl Formula {
    pub fn of(method: Method) -> Self {
        match method {
            Method::Trapezoidal => Self::Trapezoidal,
            Method::Gear2 => Self::Gear2,
        }
    }

    /// The order of the error estimate: 1 for backward Euler, 2 for the others.
    pub fn order(self) -> u8 {
        match self {
            Self::BackwardEuler => 1,
            Self::Trapezoidal | Self::Gear2 => 2,
        }
    }
}

/// Where a dynamic state variable is read from the unknowns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Read {
    /// The voltage across a capacitor, `x[a] − x[b]` (a node is the ground when `None`).
    Across(Option<usize>, Option<usize>),
    /// An unknown: an inductor's or a motor's current, the motor's speed.
    Unknown(usize),
}

/// A dynamic state variable (rule 6): what it is, and the tolerance kind of rule 11 (`vntol` for a
/// voltage or a speed, `abstol` for a current).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dynamic {
    pub read: Read,
    pub kind: UnknownKind,
}

impl Dynamic {
    pub fn value(&self, x: &[f64]) -> f64 {
        match self.read {
            Read::Across(a, b) => a.map_or(0.0, |k| x[k]) - b.map_or(0.0, |k| x[k]),
            Read::Unknown(k) => x[k],
        }
    }
}

/// The dynamic state variables of the plant, in the order the equations use them: the devices in
/// ascending identity, a capacitor's voltage, an inductor's current, a motor's current and then its
/// speed.
pub fn dynamics(plant: &Plant) -> Vec<Dynamic> {
    let mut out = Vec::new();
    for d in &plant.devices {
        match &d.element {
            Element::Capacitor { a, b, .. } => out.push(Dynamic {
                read: Read::Across(*a, *b),
                kind: UnknownKind::Voltage,
            }),
            Element::Inductor { branch, .. } => out.push(Dynamic {
                read: Read::Unknown(*branch),
                kind: UnknownKind::Current,
            }),
            Element::Motor { branch, state, .. } => {
                out.push(Dynamic {
                    read: Read::Unknown(*branch),
                    kind: UnknownKind::Current,
                });
                out.push(Dynamic {
                    read: Read::Unknown(*state),
                    kind: UnknownKind::State,
                });
            }
            _ => {}
        }
    }
    out
}

/// `α` for a step of `h` seconds after a step of `h_prev` (rule 6's table). `h_prev` matters for
/// Gear 2 only.
pub fn alpha(formula: Formula, h: f64, h_prev: f64) -> f64 {
    match formula {
        Formula::BackwardEuler => 1.0 / h,
        Formula::Trapezoidal => 2.0 / h,
        Formula::Gear2 => {
            let w = h / h_prev;
            (1.0 + 2.0 * w) / ((1.0 + w) * h)
        }
    }
}

/// The history term of one state variable (rule 6's table): `x_n` its value at the last accepted
/// point, `x_prev` the one before (Gear 2), `dx_n` its derivative at the last point
/// (trapezoidal).
pub fn history(formula: Formula, h: f64, h_prev: f64, x_n: f64, x_prev: f64, dx_n: f64) -> f64 {
    match formula {
        Formula::BackwardEuler => x_n / h,
        Formula::Trapezoidal => 2.0 * x_n / h + dx_n,
        Formula::Gear2 => {
            let w = h / h_prev;
            ((1.0 + w) * x_n - (w * w / (1.0 + w)) * x_prev) / h
        }
    }
}

/// The second divided difference of three points whose gaps are `h1` and `h2` seconds, the span
/// `h1 + h2` given as `span` (rule 11: defined for unequal spacing).
pub fn dd2(x0: f64, x1: f64, x2: f64, h1: f64, h2: f64, span: f64) -> f64 {
    ((x2 - x1) / h2 - (x1 - x0) / h1) / span
}

/// The third divided difference of four points whose gaps are `h1`, `h2` and `h3`; the spans of
/// the two triples and of the four are given.
pub fn dd3(x: [f64; 4], h: [f64; 3], span_first: f64, span_second: f64, span_all: f64) -> f64 {
    let first = dd2(x[0], x[1], x[2], h[0], h[1], span_first);
    let second = dd2(x[1], x[2], x[3], h[1], h[2], span_second);
    (second - first) / span_all
}

/// The error estimate of one step for one state variable (rule 11's table), given `h`, the
/// previous step `h_prev` (Gear 2), and the divided difference of the order the formula needs:
/// `dd2` for backward Euler, `dd3` for the others.
pub fn estimate(formula: Formula, h: f64, h_prev: f64, divided: f64) -> f64 {
    match formula {
        Formula::BackwardEuler => h * h * divided.abs(),
        Formula::Trapezoidal => h * h * h * divided.abs() / 2.0,
        Formula::Gear2 => {
            let w = h / h_prev;
            (1.0 + w) * (1.0 + w) / (w * (1.0 + 2.0 * w)) * h * h * h * divided.abs()
        }
    }
}

#[cfg(test)]
mod tests;
