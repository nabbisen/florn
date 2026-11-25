//! RFC 013 rule 8: the characteristic, the limiter's formula and its figures: a scalar model of a
//! diode and a 1 kohm resistor stepped from 0 V to 5 V needs 59 iterations without the limiter,
//! more than the default bound of 50, and 8 with it.

use super::*;
use crate::math::{exp, fmax, ln};

fn junction() -> Junction {
    Junction::new(1e-14, 1.0, 1e-12)
}

#[test]
fn the_constants_are_the_literals_of_the_rule() {
    let j = junction();
    assert_eq!(VT, 0.025852);
    assert_eq!(j.vte, 0.025852);
    // vcrit = vte·ln(vte / (√2·is)) ≈ 0.73 V for the defaults.
    assert!(
        (j.vcrit - 0.025852 * ln(0.025852 / (core::f64::consts::SQRT_2 * 1e-14))).abs() < 1e-12
    );
    assert!(j.vcrit > 0.7 && j.vcrit < 0.8, "{}", j.vcrit);
    // `n` scales the thermal voltage.
    assert_eq!(Junction::new(1e-14, 2.0, 1e-12).vte, 2.0 * 0.025852);
}

#[test]
fn the_characteristic_is_shockley_plus_gmin() {
    let j = junction();
    let (i, g) = j.characteristic(0.5);
    let e = exp(0.5f64 / 0.025852);
    assert!((i - (1e-14 * (e - 1.0) + 1e-12 * 0.5)).abs() <= 1e-12 * i.abs());
    assert!((g - (1e-14 * e / 0.025852 + 1e-12)).abs() <= 1e-12 * g.abs());
    // At zero volts the current is zero and the conductance is is/vte + gmin.
    let (i0, g0) = j.characteristic(0.0);
    assert_eq!(i0, 0.0);
    assert!((g0 - (1e-14 / 0.025852 + 1e-12)).abs() < 1e-24);
}

#[test]
fn beyond_the_knee_the_characteristic_continues_linearly_and_never_overflows() {
    let j = junction();
    let knee = 80.0 * j.vte;
    let (i80, g80) = j.characteristic(knee);
    let (i, g) = j.characteristic(knee + 1.0);
    assert!(i.is_finite() && g.is_finite());
    assert!(
        (g - g80).abs() <= 1e-12 * g80,
        "the slope is the one at the knee"
    );
    assert!((i - (i80 + g80 * 1.0)).abs() <= 1e-9 * i.abs());
    let (huge, _) = j.characteristic(1e6);
    assert!(huge.is_finite());
    // Continuous at the knee, to the last bit.
    let (below, _) = j.characteristic(knee * (1.0 - 1e-15));
    assert!((below - i80).abs() <= 1e-9 * i80.abs());
}

#[test]
fn a_step_within_two_vte_or_below_vcrit_is_not_limited() {
    let j = junction();
    assert_eq!(j.limit(0.1, 0.0), (0.1, false), "below vcrit");
    let v = j.vcrit + 0.01;
    assert_eq!(
        j.limit(v, v - 0.01),
        (v, false),
        "within 2·vte of the old voltage"
    );
    assert_eq!(j.limit(-3.0, 0.5), (-3.0, false), "a fall is not limited");
}

#[test]
fn a_large_rise_from_a_positive_old_voltage_follows_the_logarithm() {
    let j = junction();
    let (old, new) = (0.65, 5.0);
    let a = (new - old) / j.vte;
    let want = old + j.vte * (2.0 + ln(a - 2.0));
    let (v, limited) = j.limit(new, old);
    assert!(limited);
    assert!((v - want).abs() < 1e-12, "{v} vs {want}");
    assert!(v < new && v > old);
}

#[test]
fn a_rise_from_a_non_positive_old_voltage_is_vte_ln_of_the_ratio() {
    let j = junction();
    let (v, limited) = j.limit(5.0, 0.0);
    assert!(limited);
    assert!((v - j.vte * ln(5.0 / j.vte)).abs() < 1e-12);
    let (v, limited) = j.limit(5.0, -1.0);
    assert!(limited);
    assert!((v - j.vte * ln(5.0 / j.vte)).abs() < 1e-12);
}

#[test]
fn the_evaluation_extends_the_current_linearly_to_the_trial_voltage() {
    let j = junction();
    let e = j.evaluate(5.0, 0.0);
    assert!(e.limited);
    let (i, g) = j.characteristic(e.v_used);
    assert_eq!(e.g, g);
    assert_eq!(e.i, i + g * (5.0 - e.v_used));
    // Not limited: the characteristic at the trial itself.
    let e = j.evaluate(0.3, 0.29);
    assert!(!e.limited);
    assert_eq!(e.v_used, 0.3);
    assert_eq!((e.i, e.g), j.characteristic(0.3));
}

/// The scalar model of rule 8: a diode and a 1 kohm resistor on 5 V, the unknown the junction
/// voltage, started from the previous solution (0 V, the source stepped from 0 V). Returns the
/// iterations until the criteria of the rule hold, or `None` above the bound.
fn scalar_iterations(limit: bool, bound: u32) -> Option<(u32, f64)> {
    let j = junction();
    let (abstol, reltol, vntol) = (1e-12, 1e-6, 1e-9);
    let (mut v, mut old) = (0.0f64, 0.0f64);
    for iteration in 1..=bound {
        let (used, limited) = if limit { j.limit(v, old) } else { (v, false) };
        let (i_at, g) = j.characteristic(used);
        let i = i_at + g * (v - used);
        let f = (v - 5.0) / 1000.0 + i;
        let jac = 1.0 / 1000.0 + g;
        let dx = -f / jac;
        old = used;
        let new = v + dx;
        let scale = ((v - 5.0) / 1000.0).abs() + i.abs();
        let converged = dx.abs() <= reltol * fmax(v.abs(), new.abs()) + vntol
            && f.abs() <= abstol + reltol * scale
            && !limited;
        v = new;
        if converged {
            return Some((iteration, v));
        }
    }
    None
}

#[test]
fn without_the_limiter_the_step_needs_59_iterations_and_with_it_8() {
    let (plain, v_plain) = scalar_iterations(false, 1000).unwrap();
    let (limited, v_limited) = scalar_iterations(true, 1000).unwrap();
    assert_eq!(plain, 59);
    assert_eq!(limited, 8);
    // The same answer.
    assert!(
        (v_plain - v_limited).abs() < 1e-9,
        "{v_plain} vs {v_limited}"
    );
    // The default bound of 50 is not enough without the limiter, and enough with it.
    assert_eq!(scalar_iterations(false, 50), None);
    assert!(scalar_iterations(true, 50).is_some());
}

#[test]
fn the_limiter_acts_beyond_two_vte_of_the_old_voltage_and_not_within_it() {
    let j = junction();
    let old = j.vcrit;
    // Just above 2·vte and 2.5·vte above the old voltage, both above vcrit: limited.
    assert!(j.limit(old + 2.1 * j.vte, old).1);
    assert!(j.limit(old + 2.5 * j.vte, old).1);
    // Within 2·vte: the trial stands.
    assert_eq!(j.limit(old + 1.9 * j.vte, old), (old + 1.9 * j.vte, false));
}
