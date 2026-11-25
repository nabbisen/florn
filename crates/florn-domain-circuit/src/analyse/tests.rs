//! RFC 012 rule 28: each validation code produced by a scenario built to provoke it, in one pass,
//! and the plant a clean scenario gives (rules 3 and 4 of RFC 013).

use florn_control::{Severity, Value};

use crate::plant::Element;
use crate::tests::{Deck, divider};

fn has(deck: &Deck, code: &str) -> bool {
    deck.codes().iter().any(|c| c == code)
}

fn param(deck: &Deck, code: &str, key: &str) -> Value {
    let notes = deck.notes();
    let n = notes.iter().find(|n| n.code == code).unwrap_or_else(|| {
        panic!(
            "{code} in {:?}",
            notes.iter().map(|n| &n.code).collect::<Vec<_>>()
        )
    });
    n.params
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| panic!("{key}"))
}

fn name(s: &str) -> Value {
    Value::Name(s.into())
}

#[test]
fn a_clean_scenario_has_no_notification_and_a_plant() {
    let a = divider().analyse();
    assert!(a.notes.is_empty(), "{:?}", a.notes);
    let plant = a.plant.unwrap();
    // The nets but the ground in ascending identity, then the source's current.
    assert_eq!(
        plant
            .nets
            .iter()
            .map(|n| n.name.as_str())
            .collect::<Vec<_>>(),
        ["top", "mid"]
    );
    assert_eq!(
        plant
            .unknowns
            .iter()
            .map(|u| u.name.as_str())
            .collect::<Vec<_>>(),
        ["top", "mid", "v1"]
    );
    assert_eq!(plant.transient_unknowns, 3);
    // The devices are in ascending actor identity (RFC 013 rule 5).
    assert_eq!(
        plant.devices.iter().map(|d| d.actor).collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
}

#[test]
fn circuit_no_ground() {
    let deck = Deck::new()
        .builtin(1, "r1", "resistor", "r = \"1kohm\"")
        .builtin(2, "r2", "resistor", "r = \"1kohm\"")
        .net("a", &["r1.a", "r2.a"])
        .net("b", &["r1.b", "r2.b"]);
    assert!(has(&deck, "circuit.no-ground"), "{:?}", deck.codes());
    assert!(deck.analyse().plant.is_none());
    // No nets, no ground needed.
    let deck = Deck::new().builtin(1, "k", "constant", "value = 1");
    assert!(!has(&deck, "circuit.no-ground"));
}

#[test]
fn circuit_two_grounds_names_the_second() {
    let deck = Deck::new()
        .builtin(1, "r1", "resistor", "r = \"1kohm\"")
        .builtin(2, "r2", "resistor", "r = \"1kohm\"")
        .ground("g1", &["r1.a", "r2.a"])
        .ground("g2", &["r1.b", "r2.b"]);
    assert_eq!(
        param(&deck, "circuit.two-grounds", "connection"),
        name("g2")
    );
    assert!(!has(&deck, "circuit.no-ground"));
}

#[test]
fn circuit_pin_open() {
    let deck = Deck::new()
        .builtin(1, "r1", "resistor", "r = \"1kohm\"")
        .builtin(2, "r2", "resistor", "r = \"1kohm\"")
        .ground("g", &["r1.a", "r2.a"])
        .net("n", &["r2.b", "r1.b"]);
    assert!(!has(&deck, "circuit.pin-open"));
    // `r1.b` is on no net.
    let deck = Deck::new()
        .builtin(1, "r1", "resistor", "r = \"1kohm\"")
        .builtin(2, "r2", "resistor", "r = \"1kohm\"")
        .ground("g", &["r1.a", "r2.a", "r2.b"]);
    assert_eq!(param(&deck, "circuit.pin-open", "actor"), name("r1"));
    assert_eq!(param(&deck, "circuit.pin-open", "port"), name("b"));
}

#[test]
fn a_pin_the_kind_has_and_the_actor_lacks_is_an_open_pin() {
    let deck = Deck::new()
        .builtin_with(
            1,
            "r1",
            "resistor",
            &[("a", "circuit/pin")],
            "r = \"1kohm\"",
        )
        .builtin(2, "r2", "resistor", "r = \"1kohm\"")
        .ground("g", &["r1.a", "r2.a", "r2.b"]);
    assert_eq!(param(&deck, "circuit.pin-open", "port"), name("b"));
}

#[test]
fn circuit_port_type() {
    // A signal port on a net.
    let deck = Deck::new()
        .builtin(1, "vm", "voltmeter", "period = \"1ms\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .ground("g", &["vm.p", "vm.n", "r1.a", "r1.b"])
        .net("bad", &["vm.v", "r1.a"]);
    assert!(has(&deck, "circuit.port-type"));
    assert_eq!(param(&deck, "circuit.port-type", "connection"), name("bad"));
    assert_eq!(param(&deck, "circuit.port-type", "port"), name("v"));
    assert_eq!(
        param(&deck, "circuit.port-type", "type"),
        Value::Text("circuit/signal-out".into())
    );
    // A pin on a signal.
    let deck = divider().signal("oops", &["vm.v", "r1.a"], "");
    assert!(has(&deck, "circuit.port-type"));
}

#[test]
fn circuit_signal_ports() {
    // Two outputs.
    let deck = Deck::new()
        .builtin(1, "k1", "constant", "value = 1")
        .builtin(2, "k2", "constant", "value = 1")
        .builtin(3, "s1", "switch", "ron = \"1ohm\"\nroff = \"1Mohm\"")
        .ground("g", &["s1.a", "s1.b"])
        .signal("two", &["k1.y", "k2.y", "s1.ctl"], "");
    assert_eq!(
        param(&deck, "circuit.signal-ports", "outs"),
        Value::Count(2)
    );
    // No output.
    let deck = Deck::new()
        .builtin(1, "s1", "switch", "ron = \"1ohm\"\nroff = \"1Mohm\"")
        .builtin(2, "s2", "switch", "ron = \"1ohm\"\nroff = \"1Mohm\"")
        .ground("g", &["s1.a", "s1.b", "s2.a", "s2.b"])
        .signal("none", &["s1.ctl", "s2.ctl"], "");
    assert_eq!(
        param(&deck, "circuit.signal-ports", "outs"),
        Value::Count(0)
    );
}

/// A sensor's voltage on a guest's `signal-in`, which declares no unit and is not checked
/// (RFC 012 rule 4).
fn volts_into_a_guest(attrs: &str) -> Deck {
    Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"1V\"")
        .builtin(2, "vm", "voltmeter", "period = \"1ms\"")
        .builtin(3, "r1", "resistor", "r = \"1kohm\"")
        .guest(10, "ctl", &[("in", "circuit/signal-in")])
        .net("top", &["v1.p", "vm.p", "r1.a"])
        .ground("g", &["v1.n", "vm.n", "r1.b"])
        .signal("volts", &["vm.v", "ctl.in"], attrs)
}

/// A sensor's voltage on a switch's `ctl`, which has no unit: refused (rule 4).
fn volts_into_a_switch(attrs: &str) -> Deck {
    Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"1V\"")
        .builtin(2, "vm", "voltmeter", "period = \"1ms\"")
        .builtin(3, "s1", "switch", "ron = \"1ohm\"\nroff = \"1Mohm\"")
        .net("top", &["v1.p", "vm.p", "s1.a"])
        .ground("g", &["v1.n", "vm.n", "s1.b"])
        .signal("volts", &["vm.v", "s1.ctl"], attrs)
}

#[test]
fn circuit_signal_dimension() {
    // The voltmeter's `v` is a voltage: a signal declared in amperes is refused, naming it.
    let deck = volts_into_a_guest("unit = \"A\"");
    assert!(has(&deck, "circuit.signal-dimension"));
    assert_eq!(
        param(&deck, "circuit.signal-dimension", "expected"),
        Value::Enum("voltage".into())
    );
    assert_eq!(param(&deck, "circuit.signal-dimension", "port"), name("v"));
    assert_eq!(
        param(&deck, "circuit.signal-dimension", "connection"),
        name("volts")
    );
    // The switch's `ctl` has no unit: a signal that has one is refused with `dimensionless`.
    let deck = volts_into_a_switch("");
    assert_eq!(
        param(&deck, "circuit.signal-dimension", "expected"),
        Value::Enum("dimensionless".into())
    );
    assert_eq!(
        param(&deck, "circuit.signal-dimension", "port"),
        name("ctl")
    );
    // A unit of the same dimension is converted, not refused; a guest's port is not checked.
    let ok = volts_into_a_guest("unit = \"mV\"");
    assert!(!has(&ok, "circuit.signal-dimension"), "{:?}", ok.codes());
    let plant = ok.plant();
    let s = &plant.signals[0];
    assert_eq!(
        (s.unit.as_deref(), s.dimension, s.factor),
        (Some("mV"), Some("voltage"), 1e-3)
    );
    // Undeclared, a sensor's signal is in the sensor's unit.
    let plant = volts_into_a_guest("").plant();
    assert_eq!(plant.signals[0].unit.as_deref(), Some("V"));
}

#[test]
fn a_consumer_with_a_dimension_needs_a_signal_of_it() {
    // The motor's `load` is a torque: a constant written in volts is refused.
    let deck = motor_deck("value = \"5V\"");
    assert!(has(&deck, "circuit.signal-dimension"), "{:?}", deck.codes());
    assert_eq!(
        param(&deck, "circuit.signal-dimension", "port"),
        name("load")
    );
    assert_eq!(
        param(&deck, "circuit.signal-dimension", "expected"),
        Value::Enum("torque".into())
    );
    // A plain number is dimensionless, also refused for a torque.
    assert!(has(&motor_deck("value = 5"), "circuit.signal-dimension"));
    // A torque is taken, in whatever prefix.
    assert!(!has(
        &motor_deck("value = \"500mNm\""),
        "circuit.signal-dimension"
    ));
}

fn motor_deck(constant: &str) -> Deck {
    motor_signal(constant, "")
}

/// A motor whose `load` is on a signal from a constant, with the signal's attributes.
fn motor_signal(constant: &str, attrs: &str) -> Deck {
    Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"12V\"")
        .builtin_opt(
            2,
            "m1",
            "dc-motor",
            "r = \"1ohm\"\nl = \"1mH\"\nkt = \"0.1Nm/A\"\nke = \"0.1Vs/rad\"\nj = \"0.01kgm2\"\nb = \"0.001Nms\"\nperiod = \"1ms\"",
        )
        .builtin(3, "tl", "constant", constant)
        .net("top", &["v1.p", "m1.a+"])
        .ground("g", &["v1.n", "m1.a-"])
        .signal("load", &["tl.y", "m1.load"], attrs)
}

#[test]
fn a_constant_written_with_a_unit_gives_an_undeclared_signal_that_unit() {
    let plant = motor_deck("value = \"500mNm\"").plant();
    let s = &plant.signals[0];
    assert_eq!(
        (s.unit.as_deref(), s.dimension),
        (Some("mNm"), Some("torque"))
    );
    assert_eq!(s.factor, 1e-3);
    // The value is held in the signal's own unit, as written.
    assert_eq!(s.held, 500.0);
}

#[test]
fn a_declared_unit_converts_a_constants_value_once() {
    let deck = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"12V\"")
        .builtin_opt(2, "m1", "dc-motor", "r = \"1ohm\"\nl = \"1mH\"\nkt = \"0.1Nm/A\"\nke = \"0.1Vs/rad\"\nj = \"0.01kgm2\"\nb = \"0.001Nms\"\nperiod = \"1ms\"")
        .builtin(3, "tl", "constant", "value = \"0.5Nm\"")
        .net("top", &["v1.p", "m1.a+"])
        .ground("g", &["v1.n", "m1.a-"])
        .signal("load", &["tl.y", "m1.load"], "unit = \"mNm\"");
    let plant = deck.plant();
    assert_eq!(plant.signals[0].held, 500.0);
    assert_eq!(plant.signals[0].unit.as_deref(), Some("mNm"));
    // Another dimension than the declared unit.
    let bad = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"12V\"")
        .builtin_opt(2, "m1", "dc-motor", "r = \"1ohm\"\nl = \"1mH\"\nkt = \"0.1Nm/A\"\nke = \"0.1Vs/rad\"\nj = \"0.01kgm2\"\nb = \"0.001Nms\"\nperiod = \"1ms\"")
        .builtin(3, "tl", "constant", "value = \"5V\"")
        .net("top", &["v1.p", "m1.a+"])
        .ground("g", &["v1.n", "m1.a-"])
        .signal("load", &["tl.y", "m1.load"], "unit = \"Nm\"");
    assert!(has(&bad, "circuit.signal-dimension"));
}

#[test]
fn a_signals_initial_is_a_quantity_of_its_dimension_or_a_plain_number() {
    // Held before the sensor's first sample, converted to the signal's unit, rounded once.
    let plant = volts_into_a_guest("unit = \"mV\"\ninitial = \"2V\"").plant();
    assert_eq!(plant.signals[0].held, 2000.0);
    let plant = volts_into_a_guest("initial = \"-1.5V\"").plant();
    assert_eq!(plant.signals[0].held, -1.5);
    // A bare number for a signal with a unit, and a quantity of another dimension: the core's.
    let deck = volts_into_a_guest("initial = 2");
    assert!(has(&deck, "quantity.missing-unit"), "{:?}", deck.codes());
    let deck = volts_into_a_guest("initial = \"2A\"");
    assert!(has(&deck, "quantity.dimension"), "{:?}", deck.codes());
    // A dimensionless signal takes a plain number only.
    let dimensionless = |attrs: &str| {
        Deck::new()
            .builtin(1, "k", "constant", "value = 1")
            .builtin(2, "s1", "switch", "ron = \"1ohm\"\nroff = \"1Mohm\"")
            .builtin(3, "r1", "resistor", "r = \"1kohm\"")
            .net("n", &["r1.a", "s1.a"])
            .ground("g", &["r1.b", "s1.b"])
            .signal("s", &["k.y", "s1.ctl"], attrs)
            .observed()
    };
    assert_eq!(dimensionless("initial = 0.25").plant().signals[0].held, 1.0);
    assert!(has(&dimensionless("initial = \"3V\""), "circuit.attr-type"));
}

#[test]
fn circuit_signal_unconnected_is_a_warning_and_does_not_refuse() {
    let deck = Deck::new()
        .builtin(1, "s1", "switch", "ron = \"1ohm\"\nroff = \"1Mohm\"")
        .builtin(2, "vm", "voltmeter", "period = \"1ms\"")
        .builtin(3, "v1", "voltage-source", "v = \"1V\"")
        .net("a", &["v1.p", "s1.a", "vm.p"])
        .ground("g", &["v1.n", "s1.b", "vm.n"]);
    let notes = deck.notes();
    let n = notes
        .iter()
        .find(|n| n.code == "circuit.signal-unconnected")
        .expect("the warning");
    assert_eq!(n.severity, Severity::Warning);
    assert_eq!(
        n.params.iter().find(|(k, _)| k == "port").unwrap().1,
        name("ctl")
    );
    assert!(deck.analyse().plant.is_some(), "a warning refuses nothing");
}

#[test]
fn circuit_attr_unknown_and_attr_type() {
    let deck = Deck::new()
        .builtin(1, "r1", "resistor", "r = \"1kohm\"")
        .builtin(2, "r2", "resistor", "r = \"1kohm\"")
        .net_with("g", &["r1.a", "r2.a"], "ground = \"yes\"\ncolour = \"red\"")
        .net("n", &["r1.b", "r2.b"]);
    assert_eq!(param(&deck, "circuit.attr-type", "key"), name("ground"));
    assert_eq!(
        param(&deck, "circuit.attr-type", "expected"),
        Value::Text("a boolean".into())
    );
    assert_eq!(param(&deck, "circuit.attr-unknown", "key"), name("colour"));
    // A signal's unit that is no unit of the domain.
    let deck = Deck::new()
        .builtin(1, "k", "constant", "value = 1")
        .builtin(2, "s1", "switch", "ron = \"1ohm\"\nroff = \"1Mohm\"")
        .ground("g", &["s1.a", "s1.b"])
        .signal("s", &["k.y", "s1.ctl"], "unit = \"furlong\"");
    assert!(has(&deck, "circuit.attr-type"));
    let deck = Deck::new()
        .builtin(1, "k", "constant", "value = 1")
        .builtin(2, "s1", "switch", "ron = \"1ohm\"\nroff = \"1Mohm\"")
        .ground("g", &["s1.a", "s1.b"])
        .signal("s", &["k.y", "s1.ctl"], "colour = 1");
    assert_eq!(param(&deck, "circuit.attr-unknown", "key"), name("colour"));
}

#[test]
fn circuit_param_unknown_type_and_missing() {
    let deck = Deck::new()
        .builtin(1, "r1", "resistor", "colour = \"red\"")
        .builtin(2, "r2", "resistor", "r = true")
        .ground("g", &["r1.a", "r2.a", "r1.b", "r2.b"]);
    assert_eq!(param(&deck, "circuit.param-unknown", "key"), name("colour"));
    assert_eq!(param(&deck, "circuit.param-missing", "key"), name("r"));
    assert_eq!(param(&deck, "circuit.param-type", "actor"), name("r2"));
}

#[test]
fn a_parameter_of_another_dimension_is_the_cores_quantity_dimension() {
    let deck = Deck::new()
        .builtin(1, "r1", "resistor", "r = \"5V\"")
        .builtin(2, "r2", "resistor", "r = \"1kohm\"")
        .ground("g", &["r1.a", "r2.a", "r1.b", "r2.b"]);
    assert!(has(&deck, "quantity.dimension"));
    // `1.2kohm` is 1200 by exact decimal arithmetic.
    let ok = Deck::new()
        .builtin(1, "r1", "resistor", "r = \"1.2kohm\"")
        .builtin(2, "r2", "resistor", "r = \"1kohm\"")
        .ground("g", &["r1.a", "r2.a", "r1.b", "r2.b"]);
    let plant = ok.plant();
    assert_eq!(
        plant.devices[0].element,
        Element::Resistor {
            a: None,
            b: None,
            g: 1.0 / 1200.0
        }
    );
}

#[test]
fn circuit_profile_unknown_and_value() {
    let deck = divider().profile("colour = 1\nltol = 5");
    assert_eq!(
        param(&deck, "circuit.profile-unknown", "key"),
        name("colour")
    );
    assert_eq!(param(&deck, "circuit.profile-value", "key"), name("ltol"));
}

#[test]
fn a_profile_above_a_host_maximum_is_profile_value() {
    let mut deck = divider().profile("max-unknowns = 513");
    deck.limits = crate::profile::Limits::browser();
    assert_eq!(
        param(&deck, "circuit.profile-value", "key"),
        name("max-unknowns")
    );
    deck.limits = crate::profile::Limits::native();
    assert!(!has(&deck, "circuit.profile-value"));
}

#[test]
fn circuit_no_period_and_period_zero() {
    let deck = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"1V\"")
        .builtin(2, "vm", "voltmeter", "")
        .builtin(3, "am", "ammeter", "period = \"0ns\"")
        .builtin(4, "r1", "resistor", "r = \"1kohm\"")
        .net("a", &["v1.p", "vm.p", "am.a"])
        .net("b", &["am.b", "r1.a"])
        .ground("g", &["v1.n", "vm.n", "r1.b"]);
    assert_eq!(param(&deck, "circuit.no-period", "actor"), name("vm"));
    assert_eq!(param(&deck, "circuit.period-zero", "actor"), name("am"));
    // A period that is not a whole number of ticks is the core's refusal.
    let deck = Deck::new()
        .builtin(1, "vm", "voltmeter", "period = \"0.5ns\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .net("a", &["vm.p", "r1.a"])
        .ground("g", &["vm.n", "r1.b"]);
    assert_eq!(deck.codes(), ["quantity.not-whole-ticks"]);
}

#[test]
fn circuit_plant_unobserved() {
    // A source and nothing that samples or detects.
    let deck = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"1V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .net("a", &["v1.p", "r1.a"])
        .ground("g", &["v1.n", "r1.b"]);
    assert!(has(&deck, "circuit.plant-unobserved"));
    // A sensor, or a detector, observes it.
    assert!(!has(&divider(), "circuit.plant-unobserved"));
    let cmp = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"1V\"")
        .builtin(2, "c1", "comparator", "")
        .net("a", &["v1.p", "c1.p"])
        .ground("g", &["v1.n", "c1.n"]);
    assert!(!has(&cmp, "circuit.plant-unobserved"), "{:?}", cmp.codes());
    // A circuit of passive parts alone has nothing to change, nothing to refuse.
    let passive = Deck::new()
        .builtin(1, "r1", "resistor", "r = \"1kohm\"")
        .builtin(2, "r2", "resistor", "r = \"1kohm\"")
        .ground("g", &["r1.a", "r2.a", "r1.b", "r2.b"]);
    assert!(!has(&passive, "circuit.plant-unobserved"));
}

#[test]
fn circuit_floating_node_is_a_warning_for_a_current_source_into_a_capacitor() {
    let deck = Deck::new()
        .builtin(1, "i1", "current-source", "i = \"1mA\"")
        .builtin(2, "c1", "capacitor", "c = \"1uF\"")
        .builtin(3, "vm", "voltmeter", "period = \"1ms\"")
        .net("a", &["i1.n", "c1.a", "vm.p"])
        .ground("g", &["i1.p", "c1.b", "vm.n"]);
    let notes = deck.notes();
    let n = notes
        .iter()
        .find(|n| n.code == "circuit.floating-node")
        .expect("the warning");
    assert_eq!(n.severity, Severity::Warning);
    assert_eq!(
        n.params.iter().find(|(k, _)| k == "connection").unwrap().1,
        name("a")
    );
    assert!(deck.analyse().plant.is_some());
    // An `ic` makes the capacitor a voltage source: a path to ground for DC.
    let fixed = Deck::new()
        .builtin(1, "i1", "current-source", "i = \"1mA\"")
        .builtin(2, "c1", "capacitor", "c = \"1uF\"\nic = \"0V\"")
        .builtin(3, "vm", "voltmeter", "period = \"1ms\"")
        .net("a", &["i1.n", "c1.a", "vm.p"])
        .ground("g", &["i1.p", "c1.b", "vm.n"]);
    assert!(!has(&fixed, "circuit.floating-node"));
}

/// `tail` reaches the rest of the circuit only through the element `x`; a voltmeter gives it its
/// second pin and conducts nothing; `head` is tied to the ground by a resistor.
fn behind(kind: &str, config: &str) -> Deck {
    let (a, b) = if kind == "dc-motor" {
        ("a+", "a-")
    } else {
        (
            crate::tests::pins_of(kind)[0],
            crate::tests::pins_of(kind)[1],
        )
    };
    let mut deck = Deck::new()
        .builtin_opt(1, "x", kind, config)
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "probe", "voltmeter", "period = \"1ms\"");
    if kind == "switch" {
        deck = deck
            .builtin(4, "k", "constant", "value = 1")
            .signal("c", &["k.y", "x.ctl"], "");
    }
    deck.net("head", &[&format!("x.{a}"), "r1.a"])
        .net("tail", &[&format!("x.{b}"), "probe.p"])
        .ground("g", &["r1.b", "probe.n"])
}

#[test]
fn floating_nodes_follow_the_list_of_conducting_elements() {
    // The elements that conduct direct current (RFC 013 rule 9): a net behind one has a path.
    for (kind, config) in [
        ("resistor", "r = \"1kohm\""),
        ("voltage-source", "v = \"1V\""),
        ("inductor", "l = \"1mH\""),
        ("ammeter", "period = \"1ms\""),
        ("diode", ""),
        ("switch", "ron = \"1ohm\"\nroff = \"1Mohm\""),
        (
            "dc-motor",
            "r = \"1ohm\"\nl = \"1mH\"\nkt = \"0.1Nm/A\"\nke = \"0.1Vs/rad\"\nj = \"0.01kgm2\"\nb = \"0.001Nms\"\nperiod = \"1ms\"",
        ),
        ("capacitor", "c = \"1uF\"\nic = \"1V\""),
    ] {
        let deck = behind(kind, config);
        assert!(
            !has(&deck, "circuit.floating-node"),
            "{kind}: {:?}",
            deck.codes()
        );
    }
    // Those that do not: a net behind one is floating, and the warning names it.
    for (kind, config) in [
        ("capacitor", "c = \"1uF\""),
        ("current-source", "i = \"1mA\""),
        ("inductor", "l = \"1mH\"\nic = \"1mA\""),
        ("voltmeter", "period = \"1ms\""),
        ("comparator", ""),
    ] {
        let deck = behind(kind, config);
        assert_eq!(
            param(&deck, "circuit.floating-node", "connection"),
            name("tail"),
            "{kind}"
        );
    }
}

#[test]
fn a_set_of_floating_nets_is_one_warning_naming_its_lowest_identity() {
    // `m` and `t` are joined by a resistor and reach the ground only through a capacitor and a
    // voltmeter, which conduct nothing: one set, one warning.
    let deck = Deck::new()
        .builtin(1, "c1", "capacitor", "c = \"1uF\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "vm", "voltmeter", "period = \"1ms\"")
        .net("m", &["c1.a", "r1.a"])
        .net("t", &["r1.b", "vm.p"])
        .ground("g", &["c1.b", "vm.n"]);
    let floating: Vec<String> = deck
        .codes()
        .into_iter()
        .filter(|c| c == "circuit.floating-node")
        .collect();
    assert_eq!(floating.len(), 1, "{:?}", deck.codes());
    assert_eq!(
        param(&deck, "circuit.floating-node", "connection"),
        name("m")
    );
}

#[test]
fn circuit_too_large_names_the_estimate() {
    let deck = divider().profile("max-unknowns = 2");
    assert_eq!(
        param(&deck, "circuit.too-large", "unknowns"),
        Value::Count(3)
    );
    assert_eq!(
        param(&deck, "circuit.too-large", "maximum"),
        Value::Count(2)
    );
    // 8·n² + 8·6n + 32 × max-breakpoints, with the default 65,536 breakpoints.
    assert_eq!(
        param(&deck, "circuit.too-large", "bytes"),
        Value::Bytes(8 * 9 + 48 * 3 + 32 * 65_536)
    );
    assert!(deck.analyse().plant.is_none());
    let ok = divider().profile("max-unknowns = 3");
    assert!(!has(&ok, "circuit.too-large"));
}

#[test]
fn every_error_is_reported_in_one_pass() {
    let deck = Deck::new()
        .builtin(1, "r1", "resistor", "colour = \"red\"")
        .builtin(2, "vm", "voltmeter", "")
        .builtin(3, "v1", "voltage-source", "v = \"1V\"")
        .net("a", &["r1.a", "vm.p", "v1.p"])
        .net_with("b", &["vm.n", "v1.n"], "ground = 1")
        .profile("colour = 1");
    let codes = deck.codes();
    for want in [
        "circuit.profile-unknown",
        "circuit.param-unknown",
        "circuit.param-missing",
        "circuit.no-period",
        "circuit.attr-type",
        "circuit.pin-open",
        "circuit.no-ground",
    ] {
        assert!(codes.iter().any(|c| c == want), "{want} in {codes:?}");
    }
    assert!(deck.analyse().plant.is_none());
}

#[test]
fn a_port_the_kind_lacks_or_of_another_type_is_the_cores_port_type_unknown() {
    let deck = Deck::new()
        .builtin_with(
            1,
            "r1",
            "resistor",
            &[
                ("a", "circuit/pin"),
                ("b", "circuit/pin"),
                ("c", "circuit/pin"),
            ],
            "r = \"1kohm\"",
        )
        .builtin_with(
            2,
            "r2",
            "resistor",
            &[("a", "circuit/signal-in"), ("b", "circuit/pin")],
            "r = \"1kohm\"",
        )
        .ground("g", &["r1.a", "r1.b", "r1.c", "r2.b"]);
    let n = deck
        .codes()
        .iter()
        .filter(|c| *c == "module.port-type-unknown")
        .count();
    assert_eq!(n, 2);
}

#[test]
fn the_unknowns_are_ordered_by_identity_whatever_the_order_written() {
    // RFC 013 rule 3: nets by connection identity, then the currents of the voltage sources,
    // inductors, ammeters and armatures in ascending actor identity, then the states.
    let deck = Deck::new()
        .builtin(7, "l1", "inductor", "l = \"1mH\"")
        .builtin(3, "v1", "voltage-source", "v = \"1V\"")
        .builtin(5, "am", "ammeter", "period = \"1ms\"")
        .net("zeta", &["v1.p", "am.a"])
        .net("alpha", &["am.b", "l1.a"])
        .ground("g", &["v1.n", "l1.b"]);
    let plant = deck.plant();
    let names: Vec<&str> = plant.unknowns.iter().map(|u| u.name.as_str()).collect();
    // `zeta` has the lower connection identity (declared first), so it is first.
    assert_eq!(names, ["zeta", "alpha", "v1", "am", "l1"]);
}

#[test]
fn the_motors_negative_pin_is_the_ascii_name_and_u2212_is_not_accepted() {
    let motor = "r = \"1ohm\"\nl = \"1mH\"\nkt = \"0.1Nm/A\"\nke = \"0.1Vs/rad\"\nj = \"0.01kgm2\"\nb = \"0.001Nms\"\nperiod = \"1ms\"";
    let deck = |negative: &str| {
        Deck::new()
            .builtin(1, "v1", "voltage-source", "v = \"12V\"")
            .builtin_with(
                2,
                "m1",
                "dc-motor",
                &[
                    ("a+", "circuit/pin"),
                    (negative, "circuit/pin"),
                    ("speed", "circuit/signal-out"),
                ],
                motor,
            )
            .net("top", &["v1.p", "m1.a+"])
            .ground("g", &["v1.n", &format!("m1.{negative}")])
    };
    // The ASCII name is the motor's pin.
    let ok = deck("a-");
    assert!(
        ok.notes().iter().all(|n| n.severity != Severity::Error),
        "{:?}",
        ok.codes()
    );
    // The character RFC 012 once wrote is another name, and the pin the motor needs is open.
    let bad = deck("a\u{2212}");
    assert!(has(&bad, "circuit.pin-open"), "{:?}", bad.codes());
}
