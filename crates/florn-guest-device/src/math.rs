//! What a device model computes with floats (RFC 013 rules 8 and 19): `exp` and `ln` from `libm`,
//! the same implementation the Host's solver calls, so that a guest and a built-in diode compute the
//! same bits, and SPICE3's junction limiter with the solver's literals. **Every call of `libm` is in
//! this file**, and no float method outside the allowed operations is called at all
//! (`clippy.toml`): the lint cannot be allowed away here.
// RFC 013 rule 19, control 1: no `allow` of the lint can hide a call of a float method the rule
// disallows. (The crate's root cannot forbid it: the generated bindings allow every lint.)
#![forbid(clippy::disallowed_methods)]

/// `e^x`.
pub fn exp(x: f64) -> f64 {
    libm::exp(x)
}

/// The natural logarithm.
pub fn ln(x: f64) -> f64 {
    libm::log(x)
}

/// `VT = 0.025852` V, the thermal voltage as the solver's literal (RFC 013 rule 8), so that every
/// Host and every guest has the same bits.
pub const VT: f64 = 0.025852;

/// `vcrit = vte·ln(vte / (√2·is))` of a junction of saturation current `is` and thermal voltage
/// `vte = n·VT`.
pub fn vcrit(vte: f64, is: f64) -> f64 {
    vte * ln(vte / (core::f64::consts::SQRT_2 * is))
}

/// SPICE3's `pnjlim(v_new, v_old)` in the `DEVpnjlim` form of SPICE3f5 and ngspice, as RFC 013
/// rule 8 writes it: the limited junction voltage, and whether it limited. A device that limits
/// reports it in the last entry of `evaluate`'s result.
pub fn pnjlim(v_new: f64, v_old: f64, vte: f64, vcrit: f64) -> (f64, bool) {
    if v_new > vcrit && (v_new - v_old).abs() > 2.0 * vte {
        let v = if v_old > 0.0 {
            let a = (v_new - v_old) / vte;
            if a > 0.0 {
                v_old + vte * (2.0 + ln(a - 2.0))
            } else {
                v_old - vte * (2.0 + ln(2.0 - a))
            }
        } else {
            vte * ln(v_new / vte)
        };
        (v, true)
    } else {
        (v_new, false)
    }
}
