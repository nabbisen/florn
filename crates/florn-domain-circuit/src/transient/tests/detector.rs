//! RFC 012 rule 13 and RFC 013 rule 15: the detector at the solver's level, over an RC network and a
//! `comparator` across the capacitor (RC = 1 ms).

use crate::dc::initial_solve;
use crate::plant::Plant;
use crate::tests::Deck;
use crate::transient::{Found, Transient};

fn plant(profile: &str, threshold: &str, hysteresis: &str) -> Plant {
    Deck::new()
        .builtin(1, "vs", "voltage-source", "v = \"1V\"")
        .builtin(2, "r", "resistor", "r = \"1kohm\"")
        .builtin(3, "c", "capacitor", "c = \"1uF\"\nic = \"0V\"")
        .builtin(
            4,
            "cmp",
            "comparator",
            &format!("threshold = \"{threshold}\"\nhysteresis = \"{hysteresis}\""),
        )
        .builtin(5, "vm", "voltmeter", "period = \"100us\"")
        .net("in", &["vs.p", "r.a"])
        .net("cap", &["r.b", "c.a", "cmp.p", "vm.p"])
        .ground("gnd", &["vs.n", "c.b", "cmp.n", "vm.n"])
        .profile(profile)
        .plant()
}

fn started(profile: &str, threshold: &str, hysteresis: &str) -> Transient {
    let p = plant(profile, threshold, hysteresis);
    Transient::new(&p, &initial_solve(&p).unwrap())
}

/// The advance of a span, as the domain's `advance` event makes it.
fn advance(t: &mut Transient, until: u64) -> Vec<Found> {
    t.begin_advance();
    assert!(t.advance(until).stuck.is_none());
    t.found()
}

#[test]
fn a_crossing_is_a_fact_about_the_end_points_of_an_accepted_step() {
    let mut t = started("control = \"fixed\"\nstep = \"10us\"", "0.5V", "0V");
    assert!(advance(&mut t, 500_000).is_empty(), "0.39 V at 0.5 ms");
    // The capacitor passes 0.5 V at RC·ln 2 = 693,147 ticks: the advance to 800 us finds it.
    let found = advance(&mut t, 800_000);
    assert_eq!(found.len(), 1);
    assert!(found[0].output);
    let exact = 693_147.0;
    assert!(
        (found[0].located as f64 - exact).abs() <= 10_000.0,
        "{found:?}"
    );
    // The step that crossed holds the located tick: the step begins no later and ends no earlier.
    assert!(found[0].located <= 800_000);
    // Nothing more is found until it crosses again, and no step was rejected for the crossing.
    assert!(advance(&mut t, 1_500_000).is_empty());
}

#[test]
fn the_output_goes_high_above_the_threshold_plus_half_the_hysteresis_and_low_below_its_minus() {
    let mut t = started("control = \"fixed\"\nstep = \"10us\"", "0.5V", "0.2V");
    // Not at 0.5 V, which is the threshold and not the level the hysteresis puts above it: at 0.7 ms
    // the capacitor is at 0.50 V and the output is still low.
    assert!(advance(&mut t, 700_000).is_empty());
    assert!(!t.detectors()[0].output);
    // High at 0.6 V: RC·ln 2.5 = 916,291 ticks.
    let rising = advance(&mut t, 1_000_000);
    assert_eq!(rising.len(), 1);
    assert!(
        (rising[0].located as f64 - 916_291.0).abs() <= 10_000.0,
        "{rising:?}"
    );
    // The source goes to 0 V: low at 0.4 V, a time `RC·ln(V/0.4)` later.
    let v = 1.0 - crate::math::exp(-1.0);
    t.set(1, 0.0);
    t.cut();
    let start = t.now();
    let falling = advance(&mut t, start + 2_000_000);
    assert_eq!(falling.len(), 1);
    assert!(!falling[0].output);
    let exact = start as f64 + crate::math::ln(v / 0.4) * 1e6;
    assert!(
        (falling[0].located as f64 - exact).abs() <= 10_000.0,
        "{falling:?} for {exact}"
    );
}

#[test]
fn chatter_inside_one_advance_that_leaves_the_output_as_it_was_publishes_nothing() {
    // Over one advance the output goes high and low again: at its end it is what it was at its start.
    let mut t = started("control = \"fixed\"\nstep = \"10us\"", "0.5V", "0V");
    t.begin_advance();
    t.set(1, 5.0);
    t.cut();
    assert!(t.advance(300_000).stuck.is_none());
    assert!(t.detectors()[0].output, "high by 0.3 ms at 5 V");
    t.set(1, 0.0);
    t.cut();
    assert!(t.advance(2_000_000).stuck.is_none());
    assert!(!t.detectors()[0].output, "low again");
    assert!(
        t.found().is_empty(),
        "the same output as at the start of the advance"
    );
}

#[test]
fn the_located_crossing_of_a_change_is_that_of_the_last_one() {
    // High, low, high inside one advance: the output differs from the start, and the tick located is
    // the last change's.
    let mut t = started("control = \"fixed\"\nstep = \"10us\"", "0.5V", "0V");
    t.begin_advance();
    t.set(1, 5.0);
    t.cut();
    t.advance(300_000);
    let first = t.detectors()[0].located;
    t.set(1, 0.0);
    t.cut();
    t.advance(2_000_000);
    t.set(1, 5.0);
    t.cut();
    t.advance(2_400_000);
    let found = t.found();
    assert_eq!(found.len(), 1);
    assert!(found[0].output);
    assert!(
        found[0].located > 1_000_000 && found[0].located > first,
        "{found:?}"
    );
}

#[test]
fn a_condition_already_held_at_the_first_step_is_located_at_its_start() {
    // The threshold is below the operating point's 0 V... the first step ends above it: φ = 0 where
    // there is no step before (rule 15), the start of the step.
    let mut t = started("control = \"fixed\"\nstep = \"10us\"", "-1V", "0V");
    let found = advance(&mut t, 100_000);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].located, 0);
}

#[test]
fn a_threshold_moved_by_a_breakpoint_is_located_at_the_start_of_the_next_step() {
    let mut t = started("control = \"fixed\"\nstep = \"10us\"", "0.9V", "0V");
    assert!(advance(&mut t, 500_000).is_empty(), "0.39 V is under 0.9 V");
    t.begin_advance();
    // The threshold falls to 0.1 V: the condition holds at once, and the detector has no step
    // before the breakpoint to interpolate from.
    assert!(t.set(4, 0.1));
    t.cut();
    let at = t.now();
    assert!(t.advance(at + 100_000).stuck.is_none());
    let found = t.found();
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].located, at,
        "the start of the step that follows the breakpoint"
    );
    // Another comparator's threshold is not this one's: `set` of a source leaves it alone.
    assert!(t.detectors()[0].previous.is_some());
    t.set(1, 0.5);
    assert!(t.detectors()[0].previous.is_some());
}

#[test]
fn the_interpolation_is_clamped_and_a_flat_step_is_located_at_its_start() {
    // d = d_prev: φ = 0. Hysteresis 0 and a threshold exactly at the operating point: nothing moves
    // while the source holds the capacitor at its start.
    let mut t = started("control = \"fixed\"\nstep = \"10us\"", "0V", "0V");
    t.set(1, 0.0);
    t.cut();
    let found = advance(&mut t, 200_000);
    assert!(
        found.is_empty(),
        "d stays 0 and is not above 0 V: {found:?}"
    );
}

#[test]
fn the_detectors_state_is_in_the_solvers_snapshot() {
    use florn_model::canonical::{Decoder, Encoder};
    let p = plant("control = \"fixed\"\nstep = \"10us\"", "0.5V", "0.2V");
    let mut t = Transient::new(&p, &initial_solve(&p).unwrap());
    advance(&mut t, 1_000_000);
    assert!(t.detectors()[0].output);
    let mut e = Encoder::default();
    t.encode(&mut e);
    let bytes = e.finish();
    let back = Transient::decode(&p, &mut Decoder::new(&bytes)).expect("it fits the plant");
    assert!(back.detectors()[0].output);
    assert_eq!(back.detectors()[0].previous, t.detectors()[0].previous);
    // Continued from the restored state, the same crossing back down is found the same way.
    let (mut a, mut b) = (t, back);
    for x in [&mut a, &mut b] {
        x.set(1, 0.0);
        x.cut();
    }
    let (fa, fb) = (advance(&mut a, 3_000_000), advance(&mut b, 3_000_000));
    assert_eq!(fa, fb);
    assert_eq!(fa.len(), 1);
}

/// A 1 mA source into the capacitor (a ramp of 1 V/ms, exactly linear under every method) and a
/// comparator across it.
fn ramp(threshold: &str, profile: &str) -> Transient {
    let p = Deck::new()
        .builtin(1, "is", "current-source", "i = \"1mA\"")
        .builtin(3, "c", "capacitor", "c = \"1uF\"\nic = \"0V\"")
        .builtin(
            4,
            "cmp",
            "comparator",
            &format!("threshold = \"{threshold}\"\nhysteresis = \"0V\""),
        )
        .builtin(5, "vm", "voltmeter", "period = \"100us\"")
        .net("cap", &["is.n", "c.a", "cmp.p", "vm.p"])
        .ground("gnd", &["is.p", "c.b", "cmp.n", "vm.n"])
        .profile(profile)
        .plant();
    Transient::new(&p, &initial_solve(&p).unwrap())
}

#[test]
fn the_located_tick_of_an_exactly_linear_crossing_is_the_ceiling_of_the_crossing() {
    // 1 V/ms: the capacitor is at 0.5000002 V at 500,000.2 ticks, which a step of 10 us contains:
    // `start + ceil(φ·n)` is 500,001, not 500,000 and not the start of the step (500,000 or less).
    let mut t = ramp("0.5000002V", "control = \"fixed\"\nstep = \"10us\"");
    let found = advance(&mut t, 600_000);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].located, 500_001, "{found:?}");
    // And with a step of 100 us the crossing is no nearer the end of its step than it is.
    let mut t = ramp("0.4500001V", "control = \"fixed\"\nstep = \"100us\"");
    assert_eq!(advance(&mut t, 600_000)[0].located, 450_001);
}

#[test]
fn a_breakpoint_that_moves_the_threshold_forgets_the_step_before_it() {
    // A divider whose output jumps with its source: the step before the breakpoint is not a sample
    // of the curve the new step follows, so a crossing there is located at the start of its step.
    let p = Deck::new()
        .builtin(1, "vs", "voltage-source", "v = \"0V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "r2", "resistor", "r = \"1kohm\"")
        .builtin(
            4,
            "cmp",
            "comparator",
            "threshold = \"10V\"\nhysteresis = \"0V\"",
        )
        .builtin(5, "vm", "voltmeter", "period = \"100us\"")
        .net("top", &["vs.p", "r1.a"])
        .net("mid", &["r1.b", "r2.a", "cmp.p", "vm.p"])
        .ground("gnd", &["vs.n", "r2.b", "cmp.n", "vm.n"])
        .profile("control = \"fixed\"\nstep = \"10us\"")
        .plant();
    let mut t = Transient::new(&p, &initial_solve(&p).unwrap());
    assert!(advance(&mut t, 100_000).is_empty());
    t.begin_advance();
    // The source jumps to 2 V (the divider's middle to 1 V) and the threshold to 0.9 V, together.
    t.set(1, 2.0);
    assert!(t.set(4, 0.9));
    t.cut();
    assert!(t.advance(200_000).stuck.is_none());
    let found = t.found();
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].located, 100_000,
        "the start of the first step after the breakpoint"
    );
}
