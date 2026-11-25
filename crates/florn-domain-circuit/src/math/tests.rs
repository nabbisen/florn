use super::*;

#[test]
fn fmin_and_fmax_are_by_comparison() {
    assert_eq!(fmin(1.0, 2.0), 1.0);
    assert_eq!(fmin(2.0, 1.0), 1.0);
    assert_eq!(fmax(1.0, 2.0), 2.0);
    assert_eq!(fmax(2.0, 1.0), 2.0);
    // A pair of signed zeros gives the first, whatever the target: the rule that makes the
    // result the same on every Host.
    assert_eq!(fmin(0.0, -0.0).to_bits(), 0.0f64.to_bits());
    assert_eq!(fmin(-0.0, 0.0).to_bits(), (-0.0f64).to_bits());
    assert_eq!(fmax(0.0, -0.0).to_bits(), 0.0f64.to_bits());
    assert_eq!(fmax(-0.0, 0.0).to_bits(), (-0.0f64).to_bits());
}

#[test]
fn exp_and_ln_are_libm_and_agree_with_the_known_values() {
    assert_eq!(exp(0.0), 1.0);
    assert_eq!(ln(1.0), 0.0);
    assert!((exp(1.0) - core::f64::consts::E).abs() < 1e-15);
    assert!((ln(core::f64::consts::E) - 1.0).abs() < 1e-15);
    assert!(ln(-1.0).is_nan());
    assert_eq!(exp(f64::NEG_INFINITY), 0.0);
}

#[test]
fn every_nan_that_leaves_is_one_nan() {
    let other = f64::from_bits(0x7ff8_0000_0000_0001);
    assert!(other.is_nan());
    assert_eq!(canonical(other).to_bits(), 0x7ff8_0000_0000_0000);
    assert_eq!(canonical(1.5), 1.5);
}
