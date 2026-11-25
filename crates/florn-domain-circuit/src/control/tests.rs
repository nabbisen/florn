//! RFC 013 rules 10 to 13 as arithmetic on ticks: the shape of a step at a limit, the growth of the
//! proposal after an accepted step and its shrinking after a rejected one.

use super::*;

#[test]
fn a_step_that_reaches_the_limit_ends_there() {
    assert_eq!(shape(100, 100, 10), 100);
    assert_eq!(shape(500, 100, 10), 100);
    // A distance below the minimum is the whole distance, and nothing is notified.
    assert_eq!(shape(500, 7, 10), 7);
    assert_eq!(shape(1, 1, 10), 1);
}

#[test]
fn a_remainder_below_the_minimum_leaves_the_last_step_exactly_the_minimum() {
    // d = 105, n = 100: the remainder 5 is below the minimum 10, so the step is d − min = 95 and
    // the last step before the limit is 10.
    assert_eq!(shape(100, 105, 10), 95);
    assert_eq!(shape(100, 105, 10) + 10, 105);
    // The remainder equal to the minimum is fine; one less is not.
    assert_eq!(shape(100, 110, 10), 100);
    assert_eq!(shape(100, 109, 10), 99);
    // d below twice the minimum cannot be split: the whole distance.
    assert_eq!(shape(11, 19, 10), 19);
    assert_eq!(shape(11, 20, 10), 10);
}

#[test]
fn an_accepted_step_grows_by_the_factor_of_rule_12() {
    // r = 0 doubles; r = 1 gives f = 0.9; a small r is capped at 2.
    assert_eq!(grow(1000, 0.0, 2, 1, u64::MAX), 2000);
    assert_eq!(grow(1000, 1.0, 2, 1, u64::MAX), 900);
    assert_eq!(grow(1000, 1e-9, 1, 1, u64::MAX), 2000);
    // Order 1: 0.9/sqrt(0.25) = 1.8; order 2: 0.9/cbrt(0.125) = 1.8.
    assert_eq!(grow(1000, 0.25, 1, 1, u64::MAX), 1800);
    assert_eq!(grow(1000, 0.125, 2, 1, u64::MAX), 1800);
    // Rounded down, then clamped to the profile's bounds.
    assert_eq!(grow(7, 1.0, 2, 1, u64::MAX), 6);
    assert_eq!(grow(1000, 0.0, 2, 1, 1500), 1500);
    assert_eq!(grow(1, 1.0, 2, 3, 100), 3);
}

#[test]
fn a_rejected_step_shrinks_by_the_factor_of_rule_12_and_never_below_the_minimum() {
    // r = 4, order 1: 0.9/2 = 0.45; order 2: 0.9/cbrt(8) = 0.45.
    assert_eq!(shrink(1000, 4.0, 1, 1), 450);
    assert_eq!(shrink(1000, 8.0, 2, 1), 450);
    // The factor is at most 0.9 (just above 1) and at least 0.25 (far above).
    assert_eq!(shrink(1000, 1.0 + 1e-12, 2, 1), 899);
    assert_eq!(shrink(1000, 1e12, 1, 1), 250);
    assert_eq!(shrink(1000, f64::INFINITY, 1, 1), 250);
    assert_eq!(shrink(1000, f64::NAN, 2, 1), 250);
    assert_eq!(shrink(10, 1e12, 1, 5), 5);
}

#[test]
fn a_convergence_failure_quarters_the_step_to_the_minimum() {
    assert_eq!(quarter(1000, 1), 250);
    assert_eq!(quarter(7, 1), 1);
    assert_eq!(quarter(100, 40), 40);
}
