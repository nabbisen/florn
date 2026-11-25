//! The diode's junction (RFC 013 rule 8): the Shockley characteristic continued linearly beyond
//! `v/vte = 80`, and SPICE3's `pnjlim`, written out in the rule with its literals so that every
//! Host has the same bits. The limiter lives inside the device: only the device knows its
//! junction, its `is` and its `n`, and the solver changes no trial value.

use crate::math::{exp, ln};

/// `VT = 0.025852` V, a literal so that every Host has the same bits.
pub const VT: f64 = 0.025852;
/// Where `exp` stops: beyond `v/vte = 80` the characteristic continues with its slope there.
const KNEE: f64 = 80.0;

/// The junction's constants: `vte = n·VT` and `vcrit = vte·ln(vte / (√2·is))`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Junction {
    pub is: f64,
    pub gmin: f64,
    pub vte: f64,
    pub vcrit: f64,
}

impl Junction {
    pub fn new(is: f64, n: f64, gmin: f64) -> Self {
        let vte = n * VT;
        let vcrit = vte * ln(vte / (core::f64::consts::SQRT_2 * is));
        Self {
            is,
            gmin,
            vte,
            vcrit,
        }
    }

    /// SPICE3's `pnjlim(v_new, v_old)` in the `DEVpnjlim` form of SPICE3f5 and ngspice (RFC 013
    /// rule 8): the limited voltage, and whether it limited.
    pub fn limit(&self, v_new: f64, v_old: f64) -> (f64, bool) {
        if v_new > self.vcrit && (v_new - v_old).abs() > 2.0 * self.vte {
            let v = if v_old > 0.0 {
                let a = (v_new - v_old) / self.vte;
                if a > 0.0 {
                    v_old + self.vte * (2.0 + ln(a - 2.0))
                } else {
                    v_old - self.vte * (2.0 + ln(2.0 - a))
                }
            } else {
                self.vte * ln(v_new / self.vte)
            };
            (v, true)
        } else {
            (v_new, false)
        }
    }

    /// The current and the conductance at a junction voltage: `is·(exp(v/vte) − 1) + gmin·v`, and
    /// its slope, continued linearly beyond the knee so that `exp` never overflows.
    pub fn characteristic(&self, v: f64) -> (f64, f64) {
        let x = v / self.vte;
        if x <= KNEE {
            let e = exp(x);
            (
                self.is * (e - 1.0) + self.gmin * v,
                self.is * e / self.vte + self.gmin,
            )
        } else {
            let e = exp(KNEE);
            let v_knee = KNEE * self.vte;
            let i = self.is * (e - 1.0) + self.gmin * v_knee;
            let g = self.is * e / self.vte + self.gmin;
            (i + g * (v - v_knee), g)
        }
    }

    /// The device's answer at a trial voltage given the junction voltage it used last time:
    /// the current and the conductance at the limited voltage, the current extended linearly to
    /// the trial (`i(v_lim) + g(v_lim)·(v − v_lim)`), the junction voltage used, and whether it
    /// limited.
    pub fn evaluate(&self, v: f64, v_old: f64) -> Evaluation {
        let (v_lim, limited) = self.limit(v, v_old);
        let (i, g) = self.characteristic(v_lim);
        Evaluation {
            i: i + g * (v - v_lim),
            g,
            v_used: v_lim,
            limited,
        }
    }
}

/// A diode's answer in one evaluation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Evaluation {
    pub i: f64,
    pub g: f64,
    pub v_used: f64,
    pub limited: bool,
}

#[cfg(test)]
mod tests;
