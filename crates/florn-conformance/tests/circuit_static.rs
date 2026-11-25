//! PR-44 (checklist B-44): the circuit domain and the static plant under the Host. RFC 012 rules 1
//! to 6, 23, 24, 28 and 30; RFC 013 rules 1 to 5, 8, 9, 16, 18, 19 and 22. Each code of rule 28 the
//! plant can produce without a guest is produced by a scenario built to provoke it, and the run is
//! refused or warned as the rule says; the operating point at tick 0 of small circuits is read
//! through the Host's inspector and compared with the hand solution; a run with no operating point
//! does not start.
use florn_conformance::circuit::{self, Deck, refusals, start, try_start, try_start_in};
use florn_control::message::InspectValue;
use florn_control::{Severity, Value};
use florn_domain_circuit::{CHANNEL_CONDITION, CHANNEL_PROBE, CircuitDomain, Limits};
use florn_host::domain::Domain;
use florn_host::{EndReason, HostError};

/// The codes a deck is refused with.
fn codes(deck: &Deck) -> Vec<String> {
    refusals(try_start(deck))
        .iter()
        .map(|n| n.code.clone())
        .collect()
}

fn voltage(run: &circuit::Host, net: u32) -> f64 {
    let i = run.inspect(net).unwrap_or_else(|| panic!("net {net}"));
    for f in &i.fields {
        if let ("voltage", InspectValue::Real { value, unit }) = (f.name.as_str(), &f.value) {
            assert_eq!(unit, "V");
            return *value;
        }
    }
    panic!("no voltage in {i:?}")
}

fn real(run: &circuit::Host, id: u32, name: &str) -> f64 {
    let i = run.inspect(id).unwrap_or_else(|| panic!("element {id}"));
    i.fields
        .iter()
        .find_map(|f| match (&f.value, f.name == name) {
            (InspectValue::Real { value, .. }, true) => Some(*value),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no {name} in {i:?}"))
}

fn net(n: usize) -> u32 {
    Deck::connection_id(n)
}

/// A divider with a voltmeter, in which one thing is wrong: the deck is `divider` with `spoil`
/// applied to its text.
fn divider() -> Deck {
    Deck::new("divider", "1ms")
        .part(1, "v1", "voltage-source", &[], "v = \"10V\"")
        .part(2, "r1", "resistor", &[], "r = \"1kohm\"")
        .part(3, "r2", "resistor", &[], "r = \"3kohm\"")
        .part(4, "vm", "voltmeter", &[], "period = \"1ms\"")
        .net("top", &["v1.p", "r1.a"])
        .net("mid", &["r1.b", "r2.a", "vm.p"])
        .ground("gnd", &["v1.n", "r2.b", "vm.n"])
}

#[test]
fn a_clean_circuit_starts_and_runs_to_its_end() {
    let mut run = start(&divider());
    run.run_to_end();
    assert_eq!(run.ended(), Some(EndReason::EndReached));
    // Nothing is advanced yet (the plant advances on demand, PR-46): the run is the end alone.
    let again = {
        let mut r = start(&divider());
        r.run_to_end();
        r
    };
    assert_eq!(run.chain_hex(), again.chain_hex());
}

#[test]
fn the_operating_point_at_tick_zero_is_the_hand_solution() {
    let run = start(&divider());
    // The gmin of RFC 013 rule 4 (1 pS from every net to ground) moves a divider by about gmin·R.
    assert!((voltage(&run, net(0)) - 10.0).abs() < 1e-7);
    assert!((voltage(&run, net(1)) - 7.5).abs() < 1e-7);
    // The source supplies 2.5 mA: its current, from p through it to n, is negative.
    assert!((real(&run, 1, "current") + 2.5e-3).abs() < 1e-10);
    // The resistor between top and mid carries 2.5 mA.
    assert!((real(&run, 2, "current") - 2.5e-3).abs() < 1e-10);
    // The world says how the operating point was found and how many unknowns there are.
    let world = run
        .inspect(0xF000_0010 + 1)
        .expect("the world's inspection");
    assert!(world.fields.iter().any(|f| f.name == "unknowns"));
}

#[test]
fn rc_step_at_dc_has_the_capacitors_ic_across_it() {
    // The capacitor has `ic = 0V`: at tick 0 the state is the learner's, and the source's 1 V
    // appears across the resistor alone.
    let run = start(&circuit::rc_step());
    assert!((voltage(&run, net(0)) - 1.0).abs() < 1e-9, "in");
    assert!(voltage(&run, net(1)).abs() < 1e-12, "cap");
    // 1 V across 1 kohm: 1 mA from the source, none of it returned by anything but the capacitor.
    assert!((real(&run, 1, "current") + 1e-3).abs() < 1e-12);
}

#[test]
fn a_diode_in_series_with_a_resistor_finds_its_junction_voltage_through_the_host() {
    let deck = Deck::new("diode-dc", "1ms")
        .part(1, "v1", "voltage-source", &[], "v = \"5V\"")
        .part(2, "r1", "resistor", &[], "r = \"1kohm\"")
        .part(3, "d1", "diode", &[], "")
        .part(4, "vm", "voltmeter", &[], "period = \"1ms\"")
        .net("top", &["v1.p", "r1.a"])
        .net("a", &["r1.b", "d1.a", "vm.p"])
        .ground("gnd", &["v1.n", "d1.k", "vm.n"]);
    let run = start(&deck);
    assert!((voltage(&run, net(1)) - 0.692_543_633_176_399).abs() < 1e-7);
}

#[test]
fn a_motor_starts_in_its_dc_steady_state_through_the_host() {
    // r·i + ke·ω = 12 V and kt·i = b·ω: ω = 12 / (r·b/kt + ke) = 109.09 rad/s.
    let deck = Deck::new("motor-dc", "1ms")
        .part(1, "v1", "voltage-source", &[], "v = \"12V\"")
        .part(
            2,
            "m1",
            "dc-motor",
            &[],
            "r = \"1ohm\"\nl = \"1mH\"\nkt = \"0.1Nm/A\"\nke = \"0.1Vs/rad\"\nj = \"0.01kgm2\"\nb = \"0.001Nms\"\nperiod = \"1ms\"",
        )
        .net("top", &["v1.p", "m1.a+"])
        .ground("gnd", &["v1.n", "m1.a\u{2212}"]);
    let run = start(&deck);
    assert!((real(&run, 2, "speed") - 12.0 / 0.11).abs() < 1e-5);
    assert!((real(&run, 2, "current") - 0.001 * (12.0 / 0.11) / 0.1).abs() < 1e-8);
}

#[test]
fn each_code_of_rule_28_the_plant_can_produce_is_produced_by_a_scenario_built_for_it() {
    // (the code, a scenario that provokes it). The scenarios share a divider's shape.
    let cases: Vec<(&str, Deck)> = vec![
        (
            "circuit.no-ground",
            Deck::new("d", "1ms")
                .part(1, "r1", "resistor", &[], "r = \"1kohm\"")
                .part(2, "r2", "resistor", &[], "r = \"1kohm\"")
                .net("a", &["r1.a", "r2.a"])
                .net("b", &["r1.b", "r2.b"]),
        ),
        (
            "circuit.two-grounds",
            Deck::new("d", "1ms")
                .part(1, "r1", "resistor", &[], "r = \"1kohm\"")
                .part(2, "r2", "resistor", &[], "r = \"1kohm\"")
                .ground("a", &["r1.a", "r2.a"])
                .ground("b", &["r1.b", "r2.b"]),
        ),
        (
            "circuit.pin-open",
            Deck::new("d", "1ms")
                .part(1, "r1", "resistor", &[], "r = \"1kohm\"")
                .part(2, "r2", "resistor", &[], "r = \"1kohm\"")
                .ground("g", &["r1.a", "r2.a", "r2.b"]),
        ),
        (
            "circuit.port-type",
            Deck::new("d", "1ms")
                .part(1, "vm", "voltmeter", &[], "period = \"1ms\"")
                .part(2, "r1", "resistor", &[], "r = \"1kohm\"")
                .ground("g", &["vm.p", "vm.n", "r1.b"])
                .net("n", &["vm.v", "r1.a"]),
        ),
        (
            "circuit.signal-ports",
            Deck::new("d", "1ms")
                .part(1, "s1", "switch", &[], "ron = \"1ohm\"\nroff = \"1Mohm\"")
                .part(2, "s2", "switch", &[], "ron = \"1ohm\"\nroff = \"1Mohm\"")
                .ground("g", &["s1.a", "s1.b", "s2.a", "s2.b"])
                .signal("none", &["s1.ctl", "s2.ctl"], ""),
        ),
        (
            "circuit.signal-dimension",
            Deck::new("d", "1ms")
                .part(1, "v1", "voltage-source", &[], "v = \"1V\"")
                .part(2, "vm", "voltmeter", &[], "period = \"1ms\"")
                .part(3, "s1", "switch", &[], "ron = \"1ohm\"\nroff = \"1Mohm\"")
                .net("top", &["v1.p", "vm.p", "s1.a"])
                .ground("g", &["v1.n", "vm.n", "s1.b"])
                .signal("volts", &["vm.v", "s1.ctl"], "unit = \"A\""),
        ),
        (
            "circuit.attr-unknown",
            divider().signal("s", &["vm.v", "r1.a"], "colour = 1"),
        ),
        (
            "circuit.attr-type",
            Deck::new("d", "1ms")
                .part(1, "r1", "resistor", &[], "r = \"1kohm\"")
                .part(2, "r2", "resistor", &[], "r = \"1kohm\"")
                .connection_with_attrs("g", &["r1.a", "r2.a"], "ground = \"yes\"")
                .net("n", &["r1.b", "r2.b"]),
        ),
        (
            "circuit.param-unknown",
            Deck::new("d", "1ms")
                .part(1, "r1", "resistor", &[], "r = \"1kohm\"\ncolour = 1")
                .part(2, "r2", "resistor", &[], "r = \"1kohm\"")
                .ground("g", &["r1.a", "r2.a", "r1.b", "r2.b"]),
        ),
        (
            "circuit.param-type",
            Deck::new("d", "1ms")
                .part(1, "r1", "resistor", &[], "r = true")
                .part(2, "r2", "resistor", &[], "r = \"1kohm\"")
                .ground("g", &["r1.a", "r2.a", "r1.b", "r2.b"]),
        ),
        (
            "circuit.param-missing",
            Deck::new("d", "1ms")
                .part(1, "r1", "resistor", &[], "")
                .part(2, "r2", "resistor", &[], "r = \"1kohm\"")
                .ground("g", &["r1.a", "r2.a", "r1.b", "r2.b"]),
        ),
        ("circuit.profile-unknown", divider().profile("colour = 1")),
        ("circuit.profile-value", divider().profile("ltol = 5")),
        (
            "circuit.no-period",
            Deck::new("d", "1ms")
                .part(1, "vm", "voltmeter", &[], "")
                .part(2, "r1", "resistor", &[], "r = \"1kohm\"")
                .net("a", &["vm.p", "r1.a"])
                .ground("g", &["vm.n", "r1.b"]),
        ),
        (
            "circuit.period-zero",
            Deck::new("d", "1ms")
                .part(1, "vm", "voltmeter", &[], "period = \"0ns\"")
                .part(2, "r1", "resistor", &[], "r = \"1kohm\"")
                .net("a", &["vm.p", "r1.a"])
                .ground("g", &["vm.n", "r1.b"]),
        ),
        (
            "circuit.plant-unobserved",
            Deck::new("d", "1ms")
                .part(1, "v1", "voltage-source", &[], "v = \"1V\"")
                .part(2, "r1", "resistor", &[], "r = \"1kohm\"")
                .net("a", &["v1.p", "r1.a"])
                .ground("g", &["v1.n", "r1.b"]),
        ),
        ("circuit.too-large", divider().profile("max-unknowns = 2")),
        (
            "circuit.no-operating-point",
            Deck::new("d", "1ms")
                .part(1, "v1", "voltage-source", &[], "v = \"5V\"")
                .part(2, "c1", "capacitor", &[], "c = \"1uF\"\nic = \"2V\"")
                .part(3, "vm", "voltmeter", &[], "period = \"1ms\"")
                .net("top", &["v1.p", "c1.a", "vm.p"])
                .ground("g", &["v1.n", "c1.b", "vm.n"]),
        ),
    ];
    for (code, deck) in cases {
        let got = codes(&deck);
        assert!(
            got.iter().any(|c| c == code),
            "{code}: refused with {got:?}\n{}",
            deck.text()
        );
        // The notification is one the catalogue knows, with its parameters.
        for n in refusals(try_start(&deck)) {
            assert!(n.check().is_ok(), "{}: {:?}", n.code, n.check());
        }
    }
}

#[test]
fn a_scenario_with_many_faults_reports_them_all_in_one_pass() {
    let deck = Deck::new("d", "1ms")
        .profile("colour = 1\nltol = 5")
        .part(1, "r1", "resistor", &[], "colour = \"red\"")
        .part(2, "vm", "voltmeter", &[], "")
        .part(3, "v1", "voltage-source", &[], "v = \"1V\"")
        .net("a", &["r1.a", "vm.p", "v1.p"])
        .connection_with_attrs("b", &["vm.n", "v1.n"], "ground = 1");
    let got = codes(&deck);
    for want in [
        "circuit.profile-unknown",
        "circuit.profile-value",
        "circuit.param-unknown",
        "circuit.param-missing",
        "circuit.no-period",
        "circuit.attr-type",
        "circuit.pin-open",
        "circuit.no-ground",
    ] {
        assert!(got.iter().any(|c| c == want), "{want} in {got:?}");
    }
}

#[test]
fn the_codes_carry_their_parameters_and_the_run_does_not_start() {
    let n = refusals(try_start(
        &Deck::new("d", "1ms")
            .part(1, "r1", "resistor", &[], "r = \"1kohm\"")
            .part(2, "r2", "resistor", &[], "r = \"1kohm\"")
            .ground("a", &["r1.a", "r2.a"])
            .ground("b", &["r1.b", "r2.b"]),
    ));
    let two = n.iter().find(|n| n.code == "circuit.two-grounds").unwrap();
    assert_eq!(
        two.params,
        [("connection".to_string(), Value::Name("b".into()))]
    );
    assert_eq!(two.severity, Severity::Error);
}

#[test]
fn a_capacitor_with_an_ic_across_a_voltage_source_is_refused_naming_the_unknown() {
    let deck = Deck::new("no-point", "1ms")
        .part(1, "v1", "voltage-source", &[], "v = \"5V\"")
        .part(2, "c1", "capacitor", &[], "c = \"1uF\"\nic = \"2V\"")
        .part(3, "vm", "voltmeter", &[], "period = \"1ms\"")
        .net("top", &["v1.p", "c1.a", "vm.p"])
        .ground("g", &["v1.n", "c1.b", "vm.n"]);
    match try_start(&deck) {
        Err(HostError::Domain(n)) => {
            assert_eq!(n.len(), 1);
            assert_eq!(n[0].code, "circuit.no-operating-point");
            assert_eq!(n[0].severity, Severity::Error);
            assert!(n[0].check().is_ok(), "{:?}", n[0].check());
            let get = |k: &str| n[0].params.iter().find(|(p, _)| p == k).unwrap().1.clone();
            assert_eq!(get("kind"), Value::Enum("current".into()));
            assert!(matches!(get("unknown"), Value::Name(ref s) if s == "v1" || s == "c1"));
        }
        Err(other) => panic!("{other}"),
        Ok(_) => panic!("the run started"),
    }
}

#[test]
fn an_inductor_without_an_ic_across_a_voltage_source_is_refused() {
    let deck = Deck::new("no-point-l", "1ms")
        .part(1, "v1", "voltage-source", &[], "v = \"5V\"")
        .part(2, "l1", "inductor", &[], "l = \"1mH\"")
        .part(3, "vm", "voltmeter", &[], "period = \"1ms\"")
        .net("top", &["v1.p", "l1.a", "vm.p"])
        .ground("g", &["v1.n", "l1.b", "vm.n"]);
    assert_eq!(codes(&deck), ["circuit.no-operating-point"]);
}

#[test]
fn a_current_source_into_a_capacitor_without_an_ic_warns_and_starts() {
    let deck = Deck::new("floating", "1ms")
        .part(1, "i1", "current-source", &[], "i = \"1mA\"")
        .part(2, "c1", "capacitor", &[], "c = \"1uF\"")
        .part(3, "vm", "voltmeter", &[], "period = \"1ms\"")
        .net("a", &["i1.n", "c1.a", "vm.p"])
        .ground("g", &["i1.p", "c1.b", "vm.n"]);
    let run = start(&deck);
    let entry = run
        .log()
        .entries()
        .iter()
        .find(|e| e.notification.code == "circuit.floating-node")
        .expect("the warning is in the log");
    assert_eq!(entry.notification.severity, Severity::Warning);
    assert_eq!(entry.notification.params[0].1, Value::Name("a".into()));
    // A warning refuses nothing.
    assert!(run.ended().is_none());
}

#[test]
fn an_input_on_no_signal_warns_and_reads_zero() {
    let deck = Deck::new("unconnected", "1ms")
        .part(1, "v1", "voltage-source", &[], "v = \"10V\"")
        .part(2, "s1", "switch", &[], "ron = \"1ohm\"\nroff = \"1Mohm\"")
        .part(3, "r1", "resistor", &[], "r = \"999ohm\"")
        .part(4, "vm", "voltmeter", &[], "period = \"1ms\"")
        .net("top", &["v1.p", "s1.a"])
        .net("mid", &["s1.b", "r1.a", "vm.p"])
        .ground("g", &["v1.n", "r1.b", "vm.n"]);
    let run = start(&deck);
    assert!(run.log().codes().contains(&"circuit.signal-unconnected"));
    // The control reads 0, below the threshold: the switch is open (roff).
    assert!((voltage(&run, net(1)) - 10.0 * 999.0 / 1_000_999.0).abs() < 1e-7);
}

#[test]
fn a_profile_above_a_host_maximum_is_refused_and_the_native_host_takes_it() {
    let deck = divider().profile("max-unknowns = 513");
    let browser = refusals(try_start_in(
        &circuit::browser_domains(),
        &deck,
        florn_conformance::host::options(),
    ));
    assert_eq!(
        browser.iter().map(|n| n.code.as_str()).collect::<Vec<_>>(),
        ["circuit.profile-value"]
    );
    assert_eq!(browser[0].params[0].1, Value::Name("max-unknowns".into()));
    assert!(
        try_start(&deck).is_ok(),
        "the native Host's maximum is 1,024"
    );
    let deck = divider().profile("max-breakpoints = 262145");
    assert!(
        try_start_in(
            &circuit::browser_domains(),
            &deck,
            florn_conformance::host::options()
        )
        .is_err()
    );
    assert!(try_start(&deck).is_ok());
    // The native maximum is a maximum too.
    assert!(try_start(&divider().profile("max-unknowns = 1025")).is_err());
}

#[test]
fn the_channel_numbers_of_the_builtins_are_frozen() {
    // RFC 012 rule 24: `probe` is channel 1 and `condition` channel 2 for every kind that has them.
    // The numbers are part of the trace; a change here is a format change.
    assert_eq!((CHANNEL_PROBE, CHANNEL_CONDITION), (1, 2));
    let d = CircuitDomain::new(Limits::native());
    for kind in ["voltmeter", "ammeter", "dc-motor"] {
        assert_eq!(
            d.builtin_channels(&format!("circuit/{kind}")),
            [1],
            "{kind}"
        );
    }
    assert_eq!(d.builtin_channels("circuit/comparator"), [2]);
    for kind in [
        "resistor",
        "capacitor",
        "inductor",
        "voltage-source",
        "current-source",
        "diode",
        "switch",
        "constant",
    ] {
        assert!(
            d.builtin_channels(&format!("circuit/{kind}")).is_empty(),
            "{kind}"
        );
    }
    // The layouts, by name, in the order the table of rule 24 gives them.
    assert_eq!(
        d.telemetry(),
        vec![
            ("probe", vec![("value", "f64")]),
            (
                "control",
                vec![("input", "f64"), ("output", "f64"), ("error", "f64")]
            ),
            ("condition", vec![("output", "f64"), ("located", "u64")]),
        ]
    );
}

#[test]
fn the_classes_and_the_world_identity_are_those_of_the_rfc() {
    let d = CircuitDomain::new(Limits::native());
    assert_eq!(
        d.classes(),
        [(6, "advance"), (7, "sample"), (8, "condition")]
    );
    // The domain's world has the identity of its place among the declared domains.
    let m = divider().manifest();
    assert_eq!(m.world_identity("circuit"), Some(0xF000_0010 + 1));
}

#[test]
fn display_series_numbers_each_f64_field_among_the_actors_declared_ones() {
    // RFC 014 rule 3: the index counts the actor's declared `f64` fields, in channel order and then
    // field order. A voltmeter declares `probe` (one f64: index 0); a comparator declares
    // `condition` (an f64 then a u64: its output is index 0 and the tick is not drawn).
    let manifest = divider().manifest();
    let mut d = CircuitDomain::new(Limits::native());
    d.compose(&manifest, 1);
    assert_eq!(
        d.display_series(4, CHANNEL_PROBE, &2.5f64.to_le_bytes()),
        [(0, 2.5)]
    );
    assert!(
        d.display_series(4, CHANNEL_CONDITION, &2.5f64.to_le_bytes())
            .is_empty()
    );
    assert!(
        d.display_series(2, CHANNEL_PROBE, &2.5f64.to_le_bytes())
            .is_empty(),
        "a resistor reports nothing"
    );
    let cmp = Deck::new("cmp", "1ms")
        .part(1, "v1", "voltage-source", &[], "v = \"1V\"")
        .part(2, "c1", "comparator", &[], "")
        .net("a", &["v1.p", "c1.p"])
        .ground("g", &["v1.n", "c1.n"]);
    let mut d = CircuitDomain::new(Limits::native());
    d.compose(&cmp.manifest(), 1);
    let mut payload = 1.0f64.to_le_bytes().to_vec();
    payload.extend(99u64.to_le_bytes());
    assert_eq!(d.display_series(2, CHANNEL_CONDITION, &payload), [(0, 1.0)]);
}

#[test]
fn a_clean_scenario_has_no_refusal_and_the_same_digest_every_time() {
    let digest = |d: &Deck| {
        let mut r = start(d);
        r.run_to_end();
        r.chain_hex()
    };
    assert_eq!(digest(&divider()), digest(&divider()));
    // Nothing is advanced yet, so the trace is the run's end alone: the digest does not tell two
    // plants apart before the transient (PR-46); the operating point is read through the inspector.
}
