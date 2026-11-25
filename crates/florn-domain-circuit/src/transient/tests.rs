//! RFC 013 rules 6, 7 and 10 to 14 at the plant: the goldens of rule 21 (`rc-step` within
//! `4·ltol·V` under both methods), the order of the methods, the start-up and the restarts, the
//! step control, and the failures. Each expectation is the RFC's own figure, not the code's.

use super::*;
use crate::dc::initial_solve;
use crate::tests::Deck;

const RC: f64 = 1e-3;

/// `rc-step` of RFC 012 rule 27: 1 kohm and 1 uF with `ic = 0V` on a source of 1 V, a voltmeter
/// across the capacitor, the tick 1 ns. `profile` is the `[circuit]` table.
fn rc(profile: &str) -> Transient {
    let deck = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"1V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "c1", "capacitor", "c = \"1uF\"\nic = \"0V\"")
        .builtin(4, "vm", "voltmeter", "period = \"100us\"")
        .net("top", &["v1.p", "r1.a"])
        .net("cap", &["r1.b", "c1.a", "vm.p"])
        .ground("gnd", &["v1.n", "c1.b", "vm.n"])
        .profile(profile);
    let plant = deck.plant();
    let solution = initial_solve(&plant).expect("the operating point");
    Transient::new(&plant, &solution)
}

/// The analytic voltage across the capacitor at `t` seconds when the source is stepped from 1 V to
/// 0 V at `fall` (no fall: `f64::INFINITY`).
fn analytic(t: f64, fall: f64) -> f64 {
    if t <= fall {
        1.0 - crate::math::exp(-t / RC)
    } else {
        (1.0 - crate::math::exp(-fall / RC)) * crate::math::exp(-(t - fall) / RC)
    }
}

/// The largest error against the analytic solution over the voltmeter's samples (every 100 us up
/// to 5 ms), the source stepped to 0 V at 2 ms with the history cut there (a breakpoint).
fn worst_error(t: &mut Transient) -> f64 {
    let mut worst: f64 = 0.0;
    for k in 1..=50u64 {
        let tick = k * 100_000;
        let out = t.advance(tick);
        assert!(out.stuck.is_none(), "{out:?}");
        let v = t.voltage("cap").unwrap();
        worst = crate::math::fmax(worst, (v - analytic(tick as f64 * 1e-9, 2e-3)).abs());
        if k == 20 {
            assert!(t.set(1, 0.0));
            t.cut();
        }
    }
    worst
}

#[test]
fn rc_step_is_within_four_ltol_v_under_both_methods() {
    for method in ["trapezoidal", "gear2"] {
        let mut t = rc(&format!("method = \"{method}\""));
        let e = worst_error(&mut t);
        assert!(e <= 4.0 * 1e-3, "{method}: {e}");
        // And with `max-step = RC/10`, the second row of RFC 013 section 11.
        let mut t = rc(&format!("method = \"{method}\"\nmax-step = \"100us\""));
        let e = worst_error(&mut t);
        assert!(e <= 4.0 * 1e-3, "{method} with max-step = RC/10: {e}");
    }
}

/// The error at the sample at 2 ms of a fixed step of `ticks`, with no change and no sample in
/// the way (RFC 013 section 11, Order).
fn error_at_2ms(method: &str, ticks: u64) -> f64 {
    let mut t = rc(&format!(
        "method = \"{method}\"\ncontrol = \"fixed\"\nstep = \"{ticks}ns\"\nmin-step = \"{ticks}ns\"\nmax-step = \"{}ns\"",
        2 * ticks
    ));
    let out = t.advance(2_000_000);
    assert!(out.stuck.is_none(), "{out:?}");
    (t.voltage("cap").unwrap() - analytic(2e-3, f64::INFINITY)).abs()
}

#[test]
fn halving_a_fixed_step_divides_the_error_at_2ms_by_four_under_both_methods() {
    // The steps RC/8 to RC/64; the figures of RFC 013 rule 6 are 3.89 to 3.97 and 4.01 to 4.11.
    for method in ["trapezoidal", "gear2"] {
        let errors: Vec<f64> = [125_000, 62_500, 31_250, 15_625]
            .iter()
            .map(|&n| error_at_2ms(method, n))
            .collect();
        for pair in errors.windows(2) {
            let ratio = pair[0] / pair[1];
            assert!(
                (3.8..=4.2).contains(&ratio),
                "{method}: {ratio} from {errors:?}"
            );
        }
    }
}

/// A fixed step pattern: the steps of `pattern` times `base` ticks, repeated `cycles` times, each
/// exactly that long (`fixed` with a `step` above every one of them takes `d` when `d` is shorter).
fn prescribed(method: &str, base: u64, pattern: &[f64], cycles: u64) -> (f64, u64) {
    let mut t = rc(&format!(
        "method = \"{method}\"\ncontrol = \"fixed\"\nstep = \"900000ns\"\nmax-step = \"1800000ns\""
    ));
    for _ in 0..cycles {
        for m in pattern {
            let n = (base as f64 * m) as u64;
            assert_eq!(n as f64, base as f64 * m, "whole ticks");
            let out = t.advance(t.now() + n);
            assert!(out.stuck.is_none(), "{out:?}");
        }
    }
    let at = t.now();
    (
        (t.voltage("cap").unwrap() - analytic(at as f64 * 1e-9, f64::INFINITY)).abs(),
        at,
    )
}

#[test]
fn a_prescribed_step_pattern_of_ratios_from_half_to_two_keeps_the_order_under_gear2() {
    // Steps of 2, 4, 2 and 1 times a unit in turn: consecutive ratios of 2, 1/2, 1/2 and 2, the
    // whole range the controller allows. Halving the unit and doubling the cycles ends every run
    // at the same instant, 1.8 ms.
    let pattern = [2.0, 4.0, 2.0, 1.0];
    let runs: Vec<(f64, u64)> = [50_000u64, 25_000, 12_500, 6_250, 3_125]
        .iter()
        .zip([4u64, 8, 16, 32, 64])
        .map(|(&u, c)| prescribed("gear2", u, &pattern, c))
        .collect();
    assert!(
        runs.iter().all(|r| r.1 == 1_800_000),
        "one instant: {runs:?}"
    );
    for pair in runs.windows(2) {
        let ratio = pair[0].0 / pair[1].0;
        assert!((3.8..=4.2).contains(&ratio), "{ratio} from {runs:?}");
    }
}

fn formulas(t: &mut Transient) -> Vec<(Formula, bool)> {
    t.take_trace()
        .iter()
        .map(|r| (r.formula, r.ratio.is_some()))
        .collect()
}

#[test]
fn after_a_cut_two_backward_euler_steps_then_one_with_the_order_one_test_then_the_method() {
    let be = Formula::BackwardEuler;
    let mut t = rc("");
    t.trace_steps();
    let out = t.advance(10_000_000);
    assert!(out.stuck.is_none());
    let f = formulas(&mut t);
    // Steps 1 and 2: no test; step 3: backward Euler with the test; from step 4 the method, tested.
    assert_eq!(
        &f[..4],
        [
            (be, false),
            (be, false),
            (be, true),
            (Formula::Trapezoidal, true)
        ]
    );
    assert!(f[3..].iter().all(|x| *x == (Formula::Trapezoidal, true)));
    // The first two steps are the profile's `step`, 1 us = 1000 ticks, with no growth between.
    let trace = {
        let mut t = rc("");
        t.trace_steps();
        t.advance(1_000_000);
        t.take_trace()
    };
    assert_eq!((trace[0].ticks, trace[1].ticks), (1000, 1000));
    // A cut again, mid-run, restarts the same three.
    let mut t = rc("method = \"gear2\"");
    t.advance(3_000_000);
    t.trace_steps();
    assert!(t.set(1, 0.0));
    t.cut();
    t.advance(6_000_000);
    let f = formulas(&mut t);
    assert_eq!(
        &f[..4],
        [(be, false), (be, false), (be, true), (Formula::Gear2, true)]
    );
}

#[test]
fn the_first_two_steps_are_the_step_or_the_distance_to_the_limit_if_shorter() {
    let mut t = rc("");
    t.trace_steps();
    t.advance(1_500);
    let ticks: Vec<u64> = t.take_trace().iter().map(|r| r.ticks).collect();
    assert_eq!(ticks, [1000, 500]);
    // A distance below the step is the whole step, below the minimum or not.
    let mut t = rc("min-step = \"10ns\"");
    t.trace_steps();
    t.advance(5);
    assert_eq!(t.take_trace()[0].ticks, 5);
}

#[test]
fn under_fixed_the_method_starts_at_the_third_step_with_no_test() {
    let mut t = rc("control = \"fixed\"\nmethod = \"gear2\"");
    t.trace_steps();
    t.advance(10_000);
    let f = formulas(&mut t);
    let be = Formula::BackwardEuler;
    assert_eq!(
        &f[..4],
        [
            (be, false),
            (be, false),
            (Formula::Gear2, false),
            (Formula::Gear2, false)
        ]
    );
    assert!(f.iter().all(|x| !x.1), "no error test under fixed");
}

/// A simple deterministic stream of numbers, for limits that fall anywhere.
fn lcg(seed: &mut u64) -> u64 {
    *seed = seed
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *seed >> 33
}

#[test]
fn no_step_is_below_the_minimum_except_the_whole_distance_to_a_limit() {
    for method in ["trapezoidal", "gear2"] {
        let mut t = rc(&format!("method = \"{method}\"\nmin-step = \"700ns\""));
        t.trace_steps();
        let mut seed = 7;
        for _ in 0..200 {
            let until = t.now() + 1 + lcg(&mut seed) % 400_000;
            let out = t.advance(until);
            assert!(out.stuck.is_none(), "{out:?}");
            assert_eq!(t.now(), until, "every advance ends on its limit");
        }
        for r in t.take_trace() {
            let whole_distance = r.start + r.ticks == r.limit && r.ticks < 700;
            assert!(r.ticks >= 700 || whole_distance, "{method}: {r:?}");
            assert!(r.ticks <= r.limit - r.start);
        }
    }
}

#[test]
fn the_last_step_before_a_limit_is_the_minimum_when_the_remainder_would_be_less() {
    // The proposal grows to 100 us; a limit 100.4 us ahead would leave 0.4 us: the step before it is
    // shortened so that the last is exactly the minimum, 1 us here.
    let mut t = rc("min-step = \"1000ns\"\nmax-step = \"100us\"\nstep = \"100us\"");
    t.advance(2_000_000);
    t.trace_steps();
    let start = t.now();
    t.advance(start + 100_400);
    let ticks: Vec<u64> = t.take_trace().iter().map(|r| r.ticks).collect();
    assert_eq!(ticks.iter().sum::<u64>(), 100_400);
    assert_eq!(*ticks.last().unwrap(), 1000, "{ticks:?}");
}

#[test]
fn under_gear2_the_next_step_is_at_most_twice_the_one_just_taken() {
    let mut t = rc("method = \"gear2\"");
    t.trace_steps();
    let mut seed = 11;
    for _ in 0..100 {
        let until = t.now() + 1 + lcg(&mut seed) % 300_000;
        assert!(t.advance(until).stuck.is_none());
    }
    let trace = t.take_trace();
    for pair in trace.windows(2) {
        if pair[1].formula == Formula::Gear2 && pair[1].start == pair[0].start + pair[0].ticks {
            assert!(pair[1].ticks <= 2 * pair[0].ticks, "{pair:?}");
        }
    }
}

#[test]
fn a_step_shortened_to_a_limit_does_not_shrink_the_proposal() {
    let mut t = rc("");
    t.advance(1_500_000);
    let before = t.proposal();
    assert!(before > 1000, "the proposal has grown: {before}");
    // A limit seven ticks ahead is one step of seven: the proposal after it is not below.
    t.advance(t.now() + 7);
    assert!(t.proposal() >= before, "{} after {before}", t.proposal());
}

#[test]
fn an_advance_with_rejected_attempts_counts_them_and_names_the_first_cause() {
    // 1 us time constant at the default step of 1 us: the order-1 test of the third step fails.
    let deck = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"1V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "c1", "capacitor", "c = \"1nF\"\nic = \"0V\"")
        .builtin(4, "vm", "voltmeter", "period = \"100us\"")
        .net("top", &["v1.p", "r1.a"])
        .net("cap", &["r1.b", "c1.a", "vm.p"])
        .ground("gnd", &["v1.n", "c1.b", "vm.n"]);
    let plant = deck.plant();
    let mut t = Transient::new(&plant, &initial_solve(&plant).unwrap());
    t.trace_steps();
    let out = t.advance(100_000);
    assert!(out.stuck.is_none(), "{out:?}");
    let r = &out.report;
    assert!(r.rejected > 0, "{r:?}");
    assert_eq!(r.first_cause, Some(Cause::Error));
    assert_eq!(r.attempts, r.steps + r.rejected);
    assert!(r.minimum.is_empty());
    // A retried step is a step whose record says how many attempts it took.
    assert!(t.take_trace().iter().any(|s| s.attempts > 1));
}

#[test]
fn an_error_test_that_fails_at_the_minimum_step_is_accepted_and_reported() {
    // `min-step` equal to the step: nothing smaller is allowed.
    let deck = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"1V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "c1", "capacitor", "c = \"1nF\"\nic = \"0V\"")
        .builtin(4, "vm", "voltmeter", "period = \"100us\"")
        .net("top", &["v1.p", "r1.a"])
        .net("cap", &["r1.b", "c1.a", "vm.p"])
        .ground("gnd", &["v1.n", "c1.b", "vm.n"])
        .profile("min-step = \"1us\"");
    let plant = deck.plant();
    let mut t = Transient::new(&plant, &initial_solve(&plant).unwrap());
    let out = t.advance(50_000);
    assert!(out.stuck.is_none(), "{out:?}");
    assert!(!out.report.minimum.is_empty(), "{:?}", out.report);
    assert!(out.report.minimum.iter().all(|m| m.step == 1000));
    assert_eq!(out.report.rejected, 0, "nothing smaller was attempted");
}

#[test]
fn a_plant_with_no_dynamic_element_grows_its_step_to_the_maximum() {
    let deck = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"5V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "vm", "voltmeter", "period = \"1ms\"")
        .net("a", &["v1.p", "r1.a", "vm.p"])
        .ground("g", &["v1.n", "r1.b", "vm.n"]);
    let plant = deck.plant();
    let mut t = Transient::new(&plant, &initial_solve(&plant).unwrap());
    t.trace_steps();
    t.advance(2_000_000);
    let ticks: Vec<u64> = t.take_trace().iter().map(|r| r.ticks).collect();
    // Two steps of the profile's step, then the proposal doubles up to `max-step` = 100 us.
    assert_eq!(&ticks[..5], [1000, 1000, 1000, 2000, 4000]);
    assert!(ticks.contains(&100_000), "{ticks:?}");
    assert!(ticks.iter().all(|&n| n <= 100_000));
}

/// A diode with a resistor, a capacitor and a source stepped from 5 V to −5 V and back.
fn clipper() -> Transient {
    clipper_with("1kohm")
}

fn clipper_with(r: &str) -> Transient {
    let deck = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"5V\"")
        .builtin(2, "r1", "resistor", &format!("r = \"{r}\""))
        .builtin(3, "d1", "diode", "")
        .builtin(4, "c1", "capacitor", "c = \"10nF\"")
        .builtin(5, "vm", "voltmeter", "period = \"100us\"")
        .net("top", &["v1.p", "r1.a"])
        .net("a", &["r1.b", "d1.a", "c1.a", "vm.p"])
        .ground("g", &["v1.n", "d1.k", "c1.b", "vm.n"]);
    let plant = deck.plant();
    Transient::new(&plant, &initial_solve(&plant).unwrap())
}

#[test]
fn a_diode_clipper_converges_at_every_step_through_its_restarts() {
    let mut t = clipper();
    let mut iterations = 0;
    for (k, v) in [-5.0, 5.0, -5.0, 5.0].iter().enumerate() {
        assert!(t.set(1, *v));
        t.cut();
        let out = t.advance((k as u64 + 1) * 500_000);
        assert!(out.stuck.is_none(), "{out:?}");
        iterations += out.report.iterations;
        // Settled: the clipped level at +5 V is the diode's knee; at −5 V the reverse-biased
        // diode leaves the source.
        let a = t.voltage("a").unwrap();
        if *v > 0.0 {
            assert!((0.6..0.75).contains(&a), "{a}");
        } else {
            assert!((a + 5.0).abs() < 1e-6, "{a}");
        }
    }
    assert!(iterations > 0);
}

#[test]
fn a_motor_started_from_rest_settles_at_its_direct_current_steady_state() {
    let motor = "r = \"1ohm\"\nl = \"1mH\"\nkt = \"0.1Nm/A\"\nke = \"0.1Vs/rad\"\nj = \"0.001kgm2\"\nb = \"0.0001Nms\"\nperiod = \"1ms\"";
    let build = |v: &str| {
        Deck::new()
            .builtin(1, "v1", "voltage-source", &format!("v = \"{v}\""))
            .builtin(2, "m1", "dc-motor", motor)
            .net("top", &["v1.p", "m1.a+"])
            .ground("g", &["v1.n", "m1.a-"])
            .plant()
    };
    // The steady state of 12 V, from the initial solve of a plant that starts there.
    let target = initial_solve(&build("12V")).unwrap();
    let rest = build("0V");
    let mut t = Transient::new(&rest, &initial_solve(&rest).unwrap());
    assert!(t.x().iter().all(|x| x.abs() < 1e-6), "at rest: {:?}", t.x());
    assert!(t.set(1, 12.0));
    t.cut();
    let out = t.advance(2_000_000_000);
    assert!(out.stuck.is_none(), "{out:?}");
    let (speed, wanted) = (t.x()[2], target.x[2]);
    assert!(
        (speed - wanted).abs() <= 1e-3 * wanted.abs(),
        "{speed} vs {wanted}"
    );
    let (current, wanted) = (t.x()[1], target.x[1]);
    assert!(
        (current - wanted).abs() <= 1e-2 * wanted.abs() + 1e-6,
        "{current} vs {wanted}"
    );
}

/// A circuit Newton cannot solve in the iterations it is given: a diode stepped to 5 V with three.
fn hostile(profile: &str) -> Transient {
    let deck = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"0V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "d1", "diode", "")
        .net("top", &["v1.p", "r1.a"])
        .net("a", &["r1.b", "d1.a"])
        .ground("g", &["v1.n", "d1.k"])
        .observed()
        .profile(&format!("newton.max-iterations = 3\n{profile}"));
    let plant = deck.plant();
    let mut t = Transient::new(&plant, &initial_solve(&plant).unwrap());
    assert!(t.set(1, 5.0));
    t.cut();
    t
}

#[test]
fn a_convergence_failure_at_the_minimum_step_is_non_convergence_with_the_state_kept() {
    let mut t = hostile("");
    let before = t.x().to_vec();
    let out = t.advance(10_000);
    let stuck = out.stuck.expect("it gives up");
    assert_eq!((stuck.why, stuck.tick), (Why::AtMinimumStep, 0));
    assert!(stuck.iterations <= 3 && out.report.rejected > 0);
    // 1000 → 250 → 62 → 15 → 3 → 1: five rejections reach the minimum of one tick.
    assert_eq!(out.report.rejected, 5, "{:?}", out.report);
    assert_eq!((t.now(), t.x()), (0, &before[..]), "the last accepted step");
}

#[test]
fn under_fixed_a_convergence_failure_is_non_convergence_at_once() {
    let mut t = hostile("control = \"fixed\"");
    let out = t.advance(10_000);
    assert_eq!(out.stuck.map(|s| s.why), Some(Why::Fixed));
    assert_eq!((out.report.attempts, out.report.rejected), (1, 0));
}

#[test]
fn more_than_max_rejections_in_a_row_is_non_convergence() {
    let mut t = hostile("max-rejections = 2");
    let out = t.advance(10_000);
    assert_eq!(out.stuck.map(|s| s.why), Some(Why::TooManyRejections));
    assert_eq!(
        out.report.attempts, 3,
        "two rejections allowed, the third gives up"
    );
}

#[test]
fn the_error_ratio_uses_the_spans_of_unequal_steps() {
    // After steps of 2, 2, 4 and 2 units the next step of 1 unit is tested by the divided
    // differences of the last four points, whose gaps differ by a factor of 4 (the last three
    // accepted points and the candidate). The estimate `h^3 |dd3| / 2` is then `h^3 |x'''| / 12`,
    // and the third derivative of the capacitor's voltage is known: `x''' = -e^(-t/RC) / RC^3`,
    // taken in the middle of the window.
    let unit = 20_000u64;
    let mut t = rc("control = \"fixed\"\nstep = \"900000ns\"\nmax-step = \"1800000ns\"");
    for k in [2, 2, 4, 2] {
        assert!(t.advance(t.now() + k * unit).stuck.is_none());
    }
    let (n, tick) = (unit, 1e-9);
    let h = n as f64 * tick;
    let start = t.now();
    // The candidate is the solver's own: the estimate is of the numerical points, whose smooth
    // error is negligible against the third derivative.
    let Tried::Done(candidate) = t.attempt(n, Formula::Trapezoidal, true) else {
        panic!("the step converges")
    };
    let ratio = candidate.ratio.expect("a tested step");
    let values = candidate.values;
    // The window runs from the third point back (8 units before `start`) to the candidate.
    let middle = (start as f64 - 8.0 * unit as f64 + (start + n) as f64) / 2.0 * tick;
    let third = crate::math::exp(-middle / RC) / (RC * RC * RC);
    let p = &t.plant().profile;
    let scale = p.ltol * crate::math::fmax(values[0].abs(), p.vntol) + p.vntol;
    let predicted = h * h * h * third / 12.0 / scale;
    assert!(
        (ratio / predicted - 1.0).abs() < 0.15,
        "the ratio {ratio} against {predicted}"
    );
}

#[test]
fn a_diode_starts_a_step_from_the_junction_voltage_it_had_so_a_small_change_is_not_limited() {
    // 5 V over 100 ohm carries 44 mA, above the current of `vcrit`, so the junction is where the
    // limiter acts. A source change of 0.1 V moves it by about a millivolt: with the old junction
    // voltage in the device's memory (rule 8: the start is the previous accepted solution) nothing
    // is limited and Newton needs a handful of iterations; started from zero memory the limiter
    // would act at the first one.
    let mut t = clipper_with("100ohm");
    assert!(t.set(1, 4.9));
    t.cut();
    let out = t.advance(1000);
    assert!(out.stuck.is_none());
    assert!(out.report.iterations <= 3, "{:?}", out.report);
}
