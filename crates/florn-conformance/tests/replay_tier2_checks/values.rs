//! The numeric field: the bound, the units, the infinities, the NaNs, the worst field.

use super::*;

// ---- B4: a last-bit difference ---------------------------------------------------------------

pub(super) fn last_bit(x: f64) -> f64 {
    f64::from_bits(x.to_bits() ^ 1)
}

#[test]
fn b4_a_last_bit_difference_is_within_tolerance_across_hosts_and_diverged_on_one_build() {
    let mut d = base();
    let idx = level_records(&d.trace)[1];
    d.set_level(idx, last_bit(2.0));
    d.flip_chain();
    let one = compare(&d);
    assert_eq!(one.kind(), "diverged", "{:?}", one.codes());
    d.across();
    let o = compare(&d);
    expect!(o, "within-tolerance", ["replay.within-tolerance"]);
    assert_eq!(o.text("replay.within-tolerance", "differing"), "1");
    assert_eq!(o.text("replay.within-tolerance", "worst"), "level");
    // `at` is the tick of the record that differs (the delivery at tick 200).
    println!(
        "within-tolerance params: compared={} at={} largest={:?}",
        o.text("replay.within-tolerance", "compared"),
        o.text("replay.within-tolerance", "at"),
        o.param("replay.within-tolerance", "largest")
    );
}

// ---- B6, B7, B16, B20, B22: the tolerance ------------------------------------------------------

#[test]
fn b6_a_difference_beyond_the_tolerance_is_diverged_naming_the_record_and_both_values() {
    let mut d = base();
    let l = level_records(&d.trace)[2];
    d.set_level(l, 3.5);
    d.flip_chain();
    d.across();
    let o = compare(&d);
    expect!(o, "diverged", ["replay.value-differs"]);
    assert_eq!(o.text("replay.value-differs", "field"), "level");
    println!(
        "value-differs: record={} (index {}) recorded={:?} replayed={:?}",
        o.text("replay.value-differs", "record"),
        l,
        o.param("replay.value-differs", "recorded"),
        o.param("replay.value-differs", "replayed")
    );
    let r: usize = o.text("replay.value-differs", "record").parse().unwrap();
    // Ruling: `record` counts from 1, as the stored points count records.
    assert_eq!(
        r,
        l + 1,
        "record {r} is not the one-based number of index {l}"
    );
}

#[test]
fn b7_the_bound_is_inclusive_and_one_ulp_beyond_it_diverges() {
    // atol = 125 * 2^-17 exactly, rtol = 0; the level is 0.0 in the replay; 2^-20 khop is 1000 * 2^-20 = atol in SI.
    let text = scenario("9.5367431640625e-4", "0");
    let mut d = record_with(&text, [0.0, 0.0, 0.0, 0.0]);
    let l = level_records(&d.trace)[0];
    let edge = (-20f64).exp2();
    d.set_level(l, edge);
    d.flip_chain();
    d.across();
    let o = compare(&d);
    assert_eq!(
        o.kind(),
        "within-tolerance",
        "on the bound: {:?}",
        o.codes()
    );
    let mut d2 = d.clone();
    d2.set_level(l, f64::from_bits(edge.to_bits() + 1));
    let o = compare(&d2);
    assert_eq!(o.kind(), "diverged", "one ulp beyond: {:?}", o.codes());
    assert_eq!(o.codes(), ["replay.value-differs"]);
}

#[test]
fn b7b_the_relative_term_uses_the_larger_magnitude() {
    // replay level 1e6 khop (1e9 SI): bound = atol + rtol*1e9 = 1e-9 + 1e3 with rtol 1e-6.
    let text = scenario("1e-9", "1e-6");
    let mut d = record_with(&text, [1.0e6, 0.0, 0.0, 0.0]);
    let l = level_records(&d.trace)[0];
    d.flip_chain();
    d.across();
    // recorded 1e6 + 0.9 khop: SI diff 900 <= 1e3 ; and 1e6 + 1.1: 1100 > 1e3.
    d.set_level(l, 1.0e6 + 0.9);
    assert_eq!(compare(&d).kind(), "within-tolerance");
    d.set_level(l, 1.0e6 + 1.1);
    assert_eq!(compare(&d).kind(), "diverged");
    // the same pair swapped (recorded larger magnitude) is symmetric
    let mut e = record_with(&text, [1.0e6 + 1.1, 0.0, 0.0, 0.0]);
    let l = level_records(&e.trace)[0];
    e.set_level(l, 1.0e6);
    e.flip_chain();
    e.across();
    assert_eq!(compare(&e).kind(), "diverged");
}

#[test]
fn b16_the_first_record_that_does_not_agree_is_named_not_the_first_that_differs() {
    let mut d = base();
    let ls = level_records(&d.trace);
    d.set_level(ls[0], last_bit(1.0)); // within tolerance
    d.set_level(ls[2], 99.0); // beyond
    d.flip_chain();
    d.across();
    let o = compare(&d);
    expect!(o, "diverged", ["replay.value-differs"]);
    let r: usize = o.text("replay.value-differs", "record").parse().unwrap();
    // Ruling: `record` counts from 1.
    assert_eq!(
        r,
        ls[2] + 1,
        "named {r}; the record beyond tolerance is index {}",
        ls[2]
    );
}

#[test]
fn b20_the_tolerance_is_applied_in_si_units() {
    // khop factor 1000: replay 0.0, recorded 1e-5 khop = 1e-2 SI, atol 1e-3 SI: beyond in SI, inside if read in khop.
    let text = scenario("1e-3", "0");
    let mut d = record_with(&text, [0.0, 0.0, 0.0, 0.0]);
    let l = level_records(&d.trace)[0];
    d.set_level(l, 1e-5);
    d.flip_chain();
    d.across();
    let o = compare(&d);
    expect!(o, "diverged", ["replay.value-differs"]);
    // and inside in SI: 1e-7 khop = 1e-4 SI
    d.set_level(l, 1e-7);
    assert_eq!(compare(&d).kind(), "within-tolerance");
}

#[test]
fn b22_the_tolerance_is_the_profiles() {
    let mk = |atol: &str| {
        let mut d = record_with(&scenario(atol, "0"), [0.0, 0.0, 0.0, 0.0]);
        let l = level_records(&d.trace)[0];
        d.set_level(l, 1e-6); // 1e-3 SI
        d.flip_chain();
        d.across();
        compare(&d).kind()
    };
    assert_eq!(mk("1e-2"), "within-tolerance");
    assert_eq!(mk("1e-4"), "diverged");
}

#[test]
fn b21_worst_is_the_largest_share_of_the_tolerance() {
    // record 0: SI diff 0.1 on a value near 1e6 SI (bound ~1), share 0.1; record 1: SI diff 5e-10 at zero, bound 1e-9, share 0.5.
    let text = scenario("1e-9", "1e-6");
    let mut d = record_with(&text, [1000.0, 0.0, 0.0, 0.0]);
    let ls = level_records(&d.trace);
    d.set_level(ls[0], 1000.0 + 1e-4);
    d.set_level(ls[1], 0.5e-12);
    d.flip_chain();
    d.across();
    let o = compare(&d);
    assert_eq!(o.kind(), "within-tolerance", "{:?}", o.codes());
    let at = o.text("replay.within-tolerance", "at");
    println!(
        "worst at={at} (record 1 is at tick {}), largest={:?}, differing={}, compared={}",
        TICKS[1],
        o.param("replay.within-tolerance", "largest"),
        o.text("replay.within-tolerance", "differing"),
        o.text("replay.within-tolerance", "compared")
    );
    let want = match &d.trace[ls[1]] {
        Record::Handled { key, .. } => key.tick,
        _ => unreachable!(),
    };
    assert_eq!(
        at,
        want.to_string(),
        "the worst share is at the second delivery (tick {want})"
    );
}

// ---- B8: special values --------------------------------------------------------------------------

fn special(replayed: f64, recorded: f64) -> Out {
    let text = scenario("1e-9", "1e-6");
    let mut d = record_with(&text, [replayed, 0.0, 0.0, 0.0]);
    let l = level_records(&d.trace)[0];
    d.set_level(l, recorded);
    d.flip_chain();
    d.across();
    compare(&d)
}
fn beyond(o: Out, what: &str) {
    assert_eq!(o.kind(), "diverged", "{what}: {:?}", o.codes());
    assert_eq!(o.codes(), ["replay.value-differs"], "{what}");
}

#[test]
fn b8a_plus_infinity_against_minus_infinity_is_beyond_the_tolerance() {
    beyond(special(f64::INFINITY, f64::NEG_INFINITY), "+inf vs -inf")
}
#[test]
fn b8b_an_infinity_against_a_finite_value_is_beyond_the_tolerance() {
    beyond(special(f64::INFINITY, 1.0), "+inf vs 1");
    beyond(special(1.0, f64::INFINITY), "1 vs +inf")
}
#[test]
fn b8c_a_nan_against_a_finite_value_is_beyond_the_tolerance() {
    beyond(special(f64::NAN, 1.0), "nan vs 1");
    beyond(special(1.0, f64::NAN), "1 vs nan")
}
#[test]
fn b8d_signed_zeros_differ_in_their_bits_and_agree_in_value() {
    // RFC 013 rule 19 names the pair of signed zeros as the case where Hosts may differ; the pair agrees (|a-b| = 0).
    let o = special(-0.0, 0.0);
    assert_eq!(o.kind(), "within-tolerance", "-0 vs +0: {:?}", o.codes());
    assert_eq!(o.codes(), ["replay.within-tolerance"]);
}
#[test]
fn b8e_equal_infinities_agree_and_leave_the_unexplained_chain_diverged() {
    let o = special(f64::INFINITY, f64::INFINITY);
    assert_eq!(o.kind(), "diverged");
    assert_eq!(
        o.codes(),
        ["replay.diverged"],
        "equal infinities are not a value difference"
    );
    let o = special(f64::NEG_INFINITY, f64::NEG_INFINITY);
    assert_eq!(o.codes(), ["replay.diverged"]);
}
#[test]
fn b8f_two_nans_are_one_value() {
    let nan_a = f64::from_bits(0x7ff8_0000_0000_0001);
    let nan_b = f64::from_bits(0x7ff8_0000_0000_0000);
    let o = special(nan_a, nan_b);
    println!("NaN(1) vs NaN(0): {} {:?}", o.kind(), o.codes());
    assert!(
        !o.codes().contains(&"replay.value-differs".to_string()),
        "every NaN is one value: {:?}",
        o.codes()
    );
    let o = special(f64::NAN, f64::NAN);
    assert!(
        !o.codes().contains(&"replay.value-differs".to_string()),
        "{:?}",
        o.codes()
    );
}

#[test]
fn b7c_the_relative_term_is_taken_of_the_larger_magnitude() {
    // atol 0, rtol 0.6 (the test domain accepts any rtol >= 0): 1 SI against 2 SI differ by 1 <= 0.6 * 2, and > 0.6 * 1.
    let text = scenario("0", "0.6");
    for (replayed, recorded) in [(1e-3, 2e-3), (2e-3, 1e-3)] {
        let mut d = record_with(&text, [replayed, 0.0, 0.0, 0.0]);
        let l = level_records(&d.trace)[0];
        d.set_level(l, recorded);
        d.flip_chain();
        d.across();
        let o = compare(&d);
        assert_eq!(
            o.kind(),
            "within-tolerance",
            "{replayed} vs {recorded}: {:?}",
            o.codes()
        );
    }
}
