//! The step controller's arithmetic (RFC 013 rules 10 to 13), as functions of whole ticks and of
//! the error ratio `r`: how a step is shaped to end at a limit, how the proposal grows after an
//! accepted step and shrinks after a rejected one. Nothing here knows the plant.
//!
//! The only inexact functions are the square root (exact in IEEE) and the cube root (`libm`'s,
//! rule 19).

use crate::math::{cbrt, fmax, fmin};

/// The step actually taken (rule 13). `n` is the controller's proposal in ticks, `d` the ticks to
/// the nearest limit (a breakpoint or the advance's tick), `min` the smallest step. A step is a
/// whole number of ticks from `min` up, except where it ends at a limit:
///
/// - `n ≥ d`: the step is `d`, and ends at the limit;
/// - else, if the remainder `d − n` is below `min`, the step is `d − min` when `d ≥ 2 × min`, so
///   that the last step before the limit is exactly `min`, and `d` when it is not;
/// - else the step is `n`.
///
/// A step is below `min` only when `d` is: it is then the whole remaining distance.
pub fn shape(n: u64, d: u64, min: u64) -> u64 {
    if n >= d {
        d
    } else if d - n < min {
        if d >= 2 * min { d - min } else { d }
    } else {
        n
    }
}

/// `r^e` for the order of the estimate (rule 12): `1/sqrt(r)` for order 1 (`e = −1/2`) and
/// `1/cbrt(r)` for order 2 (`e = −1/3`).
fn power(r: f64, order: u8) -> f64 {
    if order == 1 {
        1.0 / r.sqrt()
    } else {
        1.0 / cbrt(r)
    }
}

/// A ratio the controller can reason with: a NaN (an estimate that overflowed) is as bad as can be.
fn clean(r: f64) -> f64 {
    if r.is_nan() { f64::INFINITY } else { r }
}

/// The ticks of the number `x` rounded down, as a step: `floor` of a non-negative `f64` cast.
fn floor_ticks(x: f64) -> u64 {
    x.floor() as u64
}

/// The proposal after an **accepted** step of `n` ticks whose error ratio was `r ≤ 1` (rule 12):
/// `floor(n·f)` with `f = min(2, 0.9·r^e)` (`r = 0` gives `f = 2`), clamped to `[min, max]`.
pub fn grow(n: u64, r: f64, order: u8, min: u64, max: u64) -> u64 {
    let r = clean(r);
    let f = if r == 0.0 {
        2.0
    } else {
        fmin(2.0, 0.9 * power(r, order))
    };
    clamp(floor_ticks(n as f64 * f), min, max)
}

/// The step to attempt again after a **rejected** attempt of `n` ticks with error ratio `r > 1`
/// (rule 12): `floor(n·clamp(0.9·r^e, 0.25, 0.9))`, at least `min`.
pub fn shrink(n: u64, r: f64, order: u8, min: u64) -> u64 {
    let r = clean(r);
    let f = fmin(fmax(0.9 * power(r, order), 0.25), 0.9);
    floor_ticks(n as f64 * f).max(min)
}

/// The step to attempt again after a convergence failure: `floor(n/4)`, at least `min`.
pub fn quarter(n: u64, min: u64) -> u64 {
    (n / 4).max(min)
}

fn clamp(n: u64, min: u64, max: u64) -> u64 {
    n.max(min).min(max)
}

#[cfg(test)]
mod tests;
