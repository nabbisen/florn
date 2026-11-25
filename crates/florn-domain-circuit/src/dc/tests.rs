//! RFC 013 rule 9 and RFC 012 rule 30: the operating point at tick 0 of small circuits against
//! their hand solutions, the stages of the initial solve and what refuses a run.

use super::*;
use crate::plant::UnknownKind;
use crate::tests::{Deck, divider};

fn solved(deck: &Deck) -> (Plant, Solution) {
    let plant = deck.plant();
    let s = initial_solve(&plant).unwrap_or_else(|e| panic!("no operating point: {e:?}"));
    (plant, s)
}

/// The voltage of a net by name, and the current or state of an element by its actor's name.
fn unknown(p: &Plant, s: &Solution, name: &str) -> f64 {
    let i = p
        .unknowns
        .iter()
        .position(|u| u.name == name)
        .unwrap_or_else(|| panic!("{name}"));
    s.x[i]
}

fn near(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "{a} vs {b} (tolerance {tol})");
}

#[test]
fn a_resistor_divider_has_its_hand_solution() {
    let (p, s) = solved(&divider());
    assert_eq!(s.stage, Stage::Plain);
    near(unknown(&p, &s, "top"), 10.0, 1e-7);
    near(unknown(&p, &s, "mid"), 7.5, 1e-7);
    // The source supplies the current: its branch current is from p through it to n.
    near(unknown(&p, &s, "v1"), -2.5e-3, 1e-10);
}

#[test]
fn a_current_source_into_a_resistor_gives_i_times_r() {
    let deck = Deck::new()
        .builtin(1, "i1", "current-source", "i = \"1mA\"")
        .builtin(2, "r1", "resistor", "r = \"2kohm\"")
        .net("a", &["i1.n", "r1.a"])
        .ground("gnd", &["i1.p", "r1.b"])
        .observed();
    let (p, s) = solved(&deck);
    near(unknown(&p, &s, "a"), 2.0, 1e-7);
}

#[test]
fn a_capacitor_without_an_ic_is_open_at_dc() {
    let deck = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"5V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "c1", "capacitor", "c = \"1uF\"")
        .net("top", &["v1.p", "r1.a"])
        .net("a", &["r1.b", "c1.a"])
        .ground("gnd", &["v1.n", "c1.b"])
        .observed();
    let (p, s) = solved(&deck);
    near(unknown(&p, &s, "a"), 5.0, 1e-7);
    // No current flows: the source's branch current is zero.
    near(unknown(&p, &s, "v1"), 0.0, 1e-10);
}

#[test]
fn a_capacitor_with_an_ic_is_a_voltage_source_of_that_value() {
    let deck = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"5V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "c1", "capacitor", "c = \"1uF\"\nic = \"2V\"")
        .net("top", &["v1.p", "r1.a"])
        .net("a", &["r1.b", "c1.a"])
        .ground("gnd", &["v1.n", "c1.b"])
        .observed();
    let (p, s) = solved(&deck);
    near(unknown(&p, &s, "a"), 2.0, 1e-7);
    // 3 V across 1 kohm: 3 mA from the source; the capacitor's branch current takes it.
    near(unknown(&p, &s, "v1"), -3e-3, 1e-10);
    near(unknown(&p, &s, "c1"), 3e-3, 1e-10);
    // The extra unknown exists in the initial solve only (rule 3 counts three).
    assert_eq!(p.transient_unknowns, 3);
    assert_eq!(p.size(), 4);
}

#[test]
fn an_inductor_is_a_short_without_an_ic_and_a_current_source_with_one() {
    let short = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"5V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "l1", "inductor", "l = \"1mH\"")
        .net("top", &["v1.p", "r1.a"])
        .net("a", &["r1.b", "l1.a"])
        .ground("gnd", &["v1.n", "l1.b"])
        .observed();
    let (p, s) = solved(&short);
    near(unknown(&p, &s, "a"), 0.0, 1e-7);
    near(unknown(&p, &s, "l1"), 5e-3, 1e-10);
    let with = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"5V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "l1", "inductor", "l = \"1mH\"\nic = \"1mA\"")
        .net("top", &["v1.p", "r1.a"])
        .net("a", &["r1.b", "l1.a"])
        .ground("gnd", &["v1.n", "l1.b"])
        .observed();
    let (p, s) = solved(&with);
    near(unknown(&p, &s, "a"), 4.0, 1e-7);
    near(unknown(&p, &s, "l1"), 1e-3, 1e-10);
}

#[test]
fn a_negative_ic_is_a_negative_source() {
    let deck = Deck::new()
        .builtin(1, "c1", "capacitor", "c = \"1uF\"\nic = \"-3V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .net("a", &["c1.a", "r1.a"])
        .ground("gnd", &["c1.b", "r1.b"])
        .observed();
    let (p, s) = solved(&deck);
    near(unknown(&p, &s, "a"), -3.0, 1e-7);
}

#[test]
fn a_diode_in_series_with_a_resistor_finds_the_junction_voltage() {
    let deck = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"5V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "d1", "diode", "")
        .net("top", &["v1.p", "r1.a"])
        .net("a", &["r1.b", "d1.a"])
        .ground("gnd", &["v1.n", "d1.k"])
        .observed();
    let (p, s) = solved(&deck);
    let v = unknown(&p, &s, "a");
    // The scalar model of rule 8 gives 0.6925436…: the diode's current equals the resistor's.
    near(v, 0.692_543_633_176_399, 1e-7);
    let i = (5.0 - v) / 1000.0;
    let junction = crate::diode::Junction::new(1e-14, 1.0, 1e-12);
    near(junction.characteristic(v).0, i, 1e-10);
    // The limiter was needed: plain Newton from zero does it in a handful of iterations.
    assert!(s.iterations <= 12, "{}", s.iterations);
}

#[test]
fn a_reversed_diode_blocks() {
    let deck = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"-5V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "d1", "diode", "")
        .net("top", &["v1.p", "r1.a"])
        .net("a", &["r1.b", "d1.a"])
        .ground("gnd", &["v1.n", "d1.k"])
        .observed();
    let (p, s) = solved(&deck);
    // The diode takes the whole −5 V; the leak is gmin and is-sized.
    near(unknown(&p, &s, "a"), -5.0, 1e-6);
}

#[test]
fn a_switch_is_ron_when_its_control_is_above_the_threshold_and_roff_below() {
    let on = |ctl: &str| {
        Deck::new()
            .builtin(1, "v1", "voltage-source", "v = \"10V\"")
            .builtin(2, "s1", "switch", "ron = \"1ohm\"\nroff = \"1Mohm\"")
            .builtin(3, "r1", "resistor", "r = \"999ohm\"")
            .builtin(4, "k", "constant", &format!("value = {ctl}"))
            .net("top", &["v1.p", "s1.a"])
            .net("mid", &["s1.b", "r1.a"])
            .ground("gnd", &["v1.n", "r1.b"])
            .signal("ctl", &["k.y", "s1.ctl"], "")
            .observed()
    };
    let (p, s) = solved(&on("1"));
    // 10 V over 1 + 999 ohm: 0.999 × 10 V at the middle.
    near(unknown(&p, &s, "mid"), 9.99, 1e-7);
    let (p, s) = solved(&on("0"));
    // 10 V over 1 Mohm + 999 ohm: 999/1000999 of 10 V.
    near(unknown(&p, &s, "mid"), 10.0 * 999.0 / 1_000_999.0, 1e-7);
}

#[test]
fn a_source_with_a_command_adds_gain_times_the_held_value() {
    let deck = Deck::new()
        .builtin_opt(1, "v1", "voltage-source", "v = \"1V\"\ngain = \"3V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "k", "constant", "value = 2")
        .net("top", &["v1.p", "r1.a"])
        .ground("gnd", &["v1.n", "r1.b"])
        .signal("cmd", &["k.y", "v1.cmd"], "")
        .observed();
    let (p, s) = solved(&deck);
    near(unknown(&p, &s, "top"), 7.0, 1e-7);
}

#[test]
fn an_ammeter_is_a_zero_volt_source_that_reads_the_current() {
    let deck = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"5V\"")
        .builtin(2, "am", "ammeter", "period = \"1ms\"")
        .builtin(3, "r1", "resistor", "r = \"500ohm\"")
        .net("top", &["v1.p", "am.a"])
        .net("mid", &["am.b", "r1.a"])
        .ground("gnd", &["v1.n", "r1.b"]);
    let (p, s) = solved(&deck);
    near(unknown(&p, &s, "mid"), 5.0, 1e-7);
    near(unknown(&p, &s, "am"), 10e-3, 1e-10);
}

fn motor(load: Option<&str>, b: &str) -> Deck {
    let mut deck = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"12V\"")
        .builtin_opt(
            2,
            "m1",
            "dc-motor",
            &format!(
                "r = \"1ohm\"\nl = \"1mH\"\nkt = 0.1Nm/A\nke = 0.1Vs/rad\nj = \"0.01kgm2\"\nb = \"{b}\"\nperiod = \"1ms\""
            )
            .replace("0.1Nm/A", "\"0.1Nm/A\"")
            .replace("0.1Vs/rad", "\"0.1Vs/rad\""),
        )
        .net("top", &["v1.p", "m1.a+"])
        .ground("gnd", &["v1.n", "m1.a\u{2212}"]);
    if let Some(load) = load {
        deck = deck
            .builtin(3, "tl", "constant", &format!("value = \"{load}\""))
            .signal("load", &["tl.y", "m1.load"], "");
    }
    deck
}

#[test]
fn a_motor_starts_in_its_dc_steady_state() {
    // r·i + ke·ω = 12 and kt·i = b·ω: ω = 12 / (r·b/kt + ke) = 12 / 0.11.
    let (p, s) = solved(&motor(None, "0.001Nms"));
    let i = 0.001 * (12.0 / 0.11) / 0.1;
    // Two unknowns are called "m1": the armature current, then the speed; the first is found.
    let currents: Vec<f64> = p
        .unknowns
        .iter()
        .zip(&s.x)
        .filter(|(u, _)| u.name == "m1")
        .map(|(_, x)| *x)
        .collect();
    near(currents[0], i, 1e-7);
    near(currents[1], 12.0 / 0.11, 1e-6);
}

#[test]
fn a_load_in_a_scaled_unit_is_converted_by_the_signals_factor() {
    // 500 mNm on `load` is 0.5 Nm: kt·i = b·ω + 0.5 and 12 = r·i + ke·ω.
    let (p, s) = solved(&motor(Some("500mNm"), "0.001Nms"));
    let speed = p
        .unknowns
        .iter()
        .zip(&s.x)
        .filter(|(u, _)| u.name == "m1" && u.kind == UnknownKind::State)
        .map(|(_, x)| *x)
        .next()
        .unwrap();
    // i = (b·ω + load)/kt; 12 = (b·ω + load)/kt + 0.1·ω → ω = (12 − load/kt) / (b/kt + ke).
    near(speed, (12.0 - 0.5 / 0.1) / (0.001 / 0.1 + 0.1), 1e-6);
}

#[test]
fn a_circuit_with_nothing_to_solve_has_an_empty_solution() {
    let deck = Deck::new()
        .builtin(1, "r1", "resistor", "r = \"1ohm\"")
        .ground("gnd", &["r1.a", "r1.b"]);
    let (_, s) = solved(&deck);
    assert!(s.x.is_empty());
}

#[test]
fn a_capacitor_with_an_ic_across_a_voltage_source_has_no_operating_point() {
    let deck = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"5V\"")
        .builtin(2, "c1", "capacitor", "c = \"1uF\"\nic = \"2V\"")
        .builtin(3, "vm", "voltmeter", "period = \"1ms\"")
        .net("top", &["v1.p", "c1.a", "vm.p"])
        .ground("gnd", &["v1.n", "c1.b", "vm.n"]);
    let e = initial_solve(&deck.plant()).unwrap_err();
    // Two voltage constraints on one pair of nets: the matrix is singular, at the second source.
    assert_eq!(e.kind, UnknownKind::Current);
    assert!(e.unknown == "c1" || e.unknown == "v1", "{e:?}");
}

#[test]
fn an_inductor_without_an_ic_across_a_voltage_source_has_no_operating_point() {
    let deck = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"5V\"")
        .builtin(2, "l1", "inductor", "l = \"1mH\"")
        .net("top", &["v1.p", "l1.a"])
        .ground("gnd", &["v1.n", "l1.b"])
        .observed();
    let e = initial_solve(&deck.plant()).unwrap_err();
    assert_eq!(e.kind, UnknownKind::Current);
}

#[test]
fn two_inductors_without_an_ic_in_parallel_have_no_operating_point() {
    let deck = Deck::new()
        .builtin(1, "l1", "inductor", "l = \"1mH\"")
        .builtin(2, "l2", "inductor", "l = \"1mH\"")
        .net("a", &["l1.a", "l2.a"])
        .ground("gnd", &["l1.b", "l2.b"]);
    let e = initial_solve(&deck.plant()).unwrap_err();
    assert_eq!(e.kind, UnknownKind::Current);
}

#[test]
fn gmin_levels_are_the_decades_above_both_gmin_and_1e_15_then_gmin_itself() {
    assert_eq!(
        gmin_levels(1e-12),
        [
            1e-3, 1e-4, 1e-5, 1e-6, 1e-7, 1e-8, 1e-9, 1e-10, 1e-11, 1e-12
        ]
    );
    assert_eq!(gmin_levels(1e-3), [1e-3]);
    // Zero is the last level, and the decades stop at 1e-14 (1e-15 is not above 1e-15).
    let zero = gmin_levels(0.0);
    assert_eq!(zero.len(), 13);
    assert_eq!(zero[11], 1e-14);
    assert_eq!(*zero.last().unwrap(), 0.0);
    assert_eq!(gmin_levels(1e-15).last(), Some(&1e-15));
}

/// A diode on 5 V through a resistor, the bound of Newton's iterations as given.
fn diode_stage(r: &str, bound: u32) -> Deck {
    Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"5V\"")
        .builtin(2, "r1", "resistor", &format!("r = \"{r}\""))
        .builtin(3, "d1", "diode", "")
        .net("top", &["v1.p", "r1.a"])
        .net("a", &["r1.b", "d1.a"])
        .ground("gnd", &["v1.n", "d1.k"])
        .observed()
        .profile(&format!("newton.max-iterations = {bound}"))
}

#[test]
fn the_stages_are_tried_in_order_and_each_finds_the_same_point() {
    // The plain solve, when it fits the bound (8 iterations with the limiter, rule 8).
    let (p, plain) = solved(&diode_stage("1kohm", 50));
    assert_eq!((plain.stage, plain.iterations), (Stage::Plain, 8));
    let v = unknown(&p, &plain, "a");
    // A bound of 7 fails the plain solve: `gmin` stepping or source stepping follows.
    let (p, source) = solved(&diode_stage("1kohm", 6));
    assert_eq!(source.stage, Stage::SourceStepping);
    near(unknown(&p, &source, "a"), v, 1e-9);
    // With a larger series resistance `gmin` stepping gets there first.
    let (p, plain100k) = solved(&diode_stage("100kohm", 50));
    assert_eq!(plain100k.stage, Stage::Plain);
    let (p2, gmin) = solved(&diode_stage("100kohm", 7));
    assert_eq!(gmin.stage, Stage::GminStepping);
    near(unknown(&p2, &gmin, "a"), unknown(&p, &plain100k, "a"), 1e-9);
    // More work than the plain solve, whichever stage.
    assert!(gmin.iterations > plain100k.iterations);
    assert!(source.iterations > plain.iterations);
}

#[test]
fn when_all_three_stages_fail_the_run_has_no_operating_point() {
    let deck = diode_stage("1kohm", 4);
    let e = initial_solve(&deck.plant()).unwrap_err();
    // The unknown furthest from the criteria at the last iteration is named, with its kind.
    assert!(!e.unknown.is_empty());
    assert!(e.iterations >= 4, "{e:?}");
    // A bound of one iteration cannot converge anything non-trivial either.
    assert!(initial_solve(&diode_stage("1kohm", 1).plant()).is_err());
}
