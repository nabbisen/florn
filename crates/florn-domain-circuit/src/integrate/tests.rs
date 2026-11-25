//! RFC 013 rules 6 and 11: the coefficients of the three formulas are exact on the polynomials
//! they are built for, and the estimates have the constants of the rule.

use super::*;

/// `x(t) = 1 + 2t + 3t²`, and its derivative.
fn quadratic(t: f64) -> (f64, f64) {
    (1.0 + 2.0 * t + 3.0 * t * t, 2.0 + 6.0 * t)
}

fn near(a: f64, b: f64) {
    assert!(
        (a - b).abs() <= 1e-9 * (1.0 + crate::math::fmax(a.abs(), b.abs())),
        "{a} vs {b}"
    );
}

#[test]
fn backward_euler_is_exact_on_a_line() {
    let (h, line) = (0.25, |t: f64| 5.0 - 4.0 * t);
    let (x0, x1) = (line(1.0), line(1.0 + h));
    let hist = history(Formula::BackwardEuler, h, h, x0, 0.0, 0.0);
    near(alpha(Formula::BackwardEuler, h, h) * x1 - hist, -4.0);
}

#[test]
fn the_trapezoidal_rule_is_exact_on_a_quadratic() {
    let (h, t0) = (0.3, 0.7);
    let ((x0, d0), (x1, d1)) = (quadratic(t0), quadratic(t0 + h));
    let hist = history(Formula::Trapezoidal, h, h, x0, 0.0, d0);
    near(alpha(Formula::Trapezoidal, h, h) * x1 - hist, d1);
}

#[test]
fn variable_step_gear2_is_exact_on_a_quadratic_at_the_ratios_of_the_rule() {
    // ω = 1/2, 1, 3/2 and 2: the coefficients are exact on quadratics.
    let t1 = 0.4;
    for w in [0.5, 1.0, 1.5, 2.0] {
        let (h_prev, h) = (0.2, 0.2 * w);
        let ((xp, _), (x0, _), (x1, d1)) =
            (quadratic(t1 - h_prev), quadratic(t1), quadratic(t1 + h));
        let hist = history(Formula::Gear2, h, h_prev, x0, xp, 0.0);
        near(alpha(Formula::Gear2, h, h_prev) * x1 - hist, d1);
    }
    // At a constant step it reduces to 3/(2h) and (4·x_n − x_{n−1})/(2h).
    let h = 0.125;
    near(alpha(Formula::Gear2, h, h), 3.0 / (2.0 * h));
    near(
        history(Formula::Gear2, h, h, 5.0, 3.0, 0.0),
        (4.0 * 5.0 - 3.0) / (2.0 * h),
    );
}

#[test]
fn divided_differences_of_unequal_spacing_give_the_leading_coefficient() {
    // x = 2 + t − 4t² + 5t³ at t = 0, 0.3, 0.7, 1.5.
    let x = |t: f64| 2.0 + t - 4.0 * t * t + 5.0 * t * t * t;
    let (t0, t1, t2, t3) = (0.0, 0.3, 0.7, 1.5);
    let q = |a: f64, b: f64, c: f64| dd2(x(a), x(b), x(c), b - a, c - b, c - a);
    // The second difference of a cubic is not constant; of a quadratic it is its leading coefficient.
    let quad = |t: f64| 7.0 - t + 3.0 * t * t;
    near(
        dd2(quad(t0), quad(t1), quad(t2), t1 - t0, t2 - t1, t2 - t0),
        3.0,
    );
    near(
        dd3(
            [x(t0), x(t1), x(t2), x(t3)],
            [t1 - t0, t2 - t1, t3 - t2],
            t2 - t0,
            t3 - t1,
            t3 - t0,
        ),
        5.0,
    );
    assert!(q(t0, t1, t2).is_finite());
}

#[test]
fn the_estimates_have_the_constants_of_rule_11() {
    let (h, d) = (0.5, 2.0);
    // Backward Euler: h²·|dd2|. Trapezoidal: h³·|dd3|/2.
    near(estimate(Formula::BackwardEuler, h, h, d), 0.25 * 2.0);
    near(estimate(Formula::Trapezoidal, h, h, -d), 0.125 * 2.0 / 2.0);
    // Gear 2: 6·C(ω)·h³·|dd3| with C = 2/9 at ω = 1, 3/8 at ω = 1/2 and 3/20 at ω = 2.
    for (w, c) in [(1.0, 2.0 / 9.0), (0.5, 3.0 / 8.0), (2.0, 3.0 / 20.0)] {
        near(
            estimate(Formula::Gear2, h * w, h, d),
            6.0 * c * (h * w) * (h * w) * (h * w) * d,
        );
    }
}

#[test]
fn the_dynamic_state_variables_follow_the_devices() {
    use crate::dc::initial_solve;
    use crate::tests::Deck;
    let deck = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"12V\"")
        .builtin(2, "l1", "inductor", "l = \"1mH\"")
        .builtin(3, "c1", "capacitor", "c = \"1uF\"")
        .builtin_opt(
            4,
            "m1",
            "dc-motor",
            "r = \"1ohm\"\nl = \"1mH\"\nkt = \"0.1Nm/A\"\nke = \"0.1Vs/rad\"\nj = \"0.01kgm2\"\nb = \"0.001Nms\"\nperiod = \"1ms\"",
        )
        .builtin(5, "r1", "resistor", "r = \"1ohm\"")
        .net("top", &["v1.p", "l1.a", "m1.a+"])
        .net("mid", &["l1.b", "c1.a", "r1.a"])
        .ground("g", &["v1.n", "c1.b", "r1.b", "m1.a-"])
        .observed();
    let plant = deck.plant();
    let dynamics = dynamics(&plant);
    // The inductor's current, the capacitor's voltage, the motor's current and then its speed, in
    // ascending identity: 2, 3, 4 (two).
    let kinds: Vec<_> = dynamics.iter().map(|d| d.kind).collect();
    assert_eq!(
        kinds,
        [
            UnknownKind::Current,
            UnknownKind::Voltage,
            UnknownKind::Current,
            UnknownKind::State
        ]
    );
    let x = initial_solve(&plant).unwrap().x;
    // The capacitor's voltage is the difference of its nets; the others are unknowns.
    assert_eq!(dynamics[1].value(&x), x[1]);
    assert_eq!(dynamics[0].value(&x), x[plant.nets.len() + 1]);
}
