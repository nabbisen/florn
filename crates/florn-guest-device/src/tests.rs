//! The helpers of the device SDK, on the host: `Trial` keeps a rejected step from touching the
//! committed state, and `pnjlim` is RFC 013 rule 8's limiter.
extern crate std;

use super::*;

#[test]
fn a_rejected_step_leaves_the_committed_state_as_it_was() {
    let mut t = Trial::new(0.25f64);
    assert_eq!(*t.committed(), 0.25);
    t.begin();
    *t.trial() = 0.75;
    t.reject();
    assert_eq!((*t.committed(), *t.trial()), (0.25, 0.25));
    // The next step begins from the committed state, not from the dropped one.
    t.begin();
    assert_eq!(*t.trial(), 0.25);
    *t.trial() = 0.5;
    t.accept();
    assert_eq!(*t.committed(), 0.5);
    t.begin();
    *t.trial() = 9.0;
    t.reject();
    assert_eq!(*t.committed(), 0.5);
}

#[test]
fn the_trial_state_of_a_step_survives_its_iterations_and_is_dropped_by_the_next_begin() {
    let mut t = Trial::new((0.0f64, 0u32));
    t.begin();
    t.trial().0 = 0.6;
    t.trial().1 += 1;
    t.trial().1 += 1;
    assert_eq!(*t.trial(), (0.6, 2));
    t.begin();
    assert_eq!(*t.trial(), (0.0, 0));
}

#[test]
fn pnjlim_is_the_solvers_limiter_with_its_literals() {
    let vte = VT;
    let is = 1e-14;
    let vc = vcrit(vte, is);
    // vcrit = vte·ln(vte / (√2·is)).
    assert!((vc - vte * math::ln(vte / (core::f64::consts::SQRT_2 * is))).abs() < 1e-12);
    // Below vcrit, and a step within two vte, stand.
    assert_eq!(pnjlim(0.3, 0.0, vte, vc), (0.3, false));
    assert_eq!(pnjlim(vc + vte, vc, vte, vc), (vc + vte, false));
    // A jump from a reverse or zero junction voltage is limited to vte·ln(v_new / vte).
    let (v, limited) = pnjlim(5.0, 0.0, vte, vc);
    assert!(limited);
    assert_eq!(v.to_bits(), (vte * math::ln(5.0 / vte)).to_bits());
    // A jump from a forward junction voltage is limited to v_old + vte·(2 + ln(a − 2)).
    let (v, limited) = pnjlim(5.0, 0.6, vte, vc);
    assert!(limited);
    let a = (5.0 - 0.6) / vte;
    assert_eq!(
        v.to_bits(),
        (0.6 + vte * (2.0 + math::ln(a - 2.0))).to_bits()
    );
    // A jump down from above is the other branch.
    let (v, limited) = pnjlim(vc + 1.0, vc + 10.0, vte, vc);
    assert!(limited);
    let a = (vc + 1.0 - (vc + 10.0)) / vte;
    assert_eq!(
        v.to_bits(),
        ((vc + 10.0) - vte * (2.0 + math::ln(2.0 - a))).to_bits()
    );
}

#[test]
fn the_math_is_the_solvers_exp_and_ln() {
    // Known bits of libm's answers (the solver's, RFC 013 rule 19): `exp(1)` is `e` rounded, `ln(e)`
    // is 1, `exp(0)` is 1 and `ln(1)` is 0.
    // (`libm`'s `exp(1)` is one unit in the last place from the rounded `e`.)
    assert_eq!(math::exp(1.0).to_bits(), 0x4005_bf0a_8b14_576a);
    assert_eq!(math::ln(f64::from_bits(0x4005_bf0a_8b14_576a)), 1.0);
    assert_eq!(math::exp(0.0), 1.0);
    assert_eq!(math::ln(1.0), 0.0);
    assert!(math::exp(710.0).is_infinite() && math::ln(-1.0).is_nan());
}
