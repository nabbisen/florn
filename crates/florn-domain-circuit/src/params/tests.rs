use super::*;
use crate::kinds::by_builtin;
use crate::units::CircuitUnits;
use florn_model::ActorKind;
use florn_model::manifest::DomainMap;

fn actor(config: &[(&str, DomainValue)]) -> Actor {
    Actor {
        id: 7,
        name: "x".into(),
        kind: ActorKind::Builtin("circuit/resistor".into()),
        class: "default".into(),
        ports: Vec::new(),
        config: config
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect::<DomainMap>(),
    }
}

fn text(s: &str) -> DomainValue {
    DomainValue::Text(s.into())
}

fn read_kind(kind: &str, config: &[(&str, DomainValue)]) -> (Params, Vec<String>) {
    let k = by_builtin(&format!("circuit/{kind}")).unwrap();
    let (p, n) = read(k, 0, &actor(config), &CircuitUnits, TickLength::Ns);
    (p, n.iter().map(|n| n.code.clone()).collect())
}

#[test]
fn a_resistor_reads_its_resistance_in_ohms() {
    let (p, codes) = read_kind("resistor", &[("r", text("1.2kohm"))]);
    assert!(codes.is_empty(), "{codes:?}");
    assert_eq!(p.real("r"), 1200.0);
}

#[test]
fn defaults_fill_what_a_scenario_leaves_out() {
    let (p, codes) = read_kind("diode", &[]);
    assert!(codes.is_empty(), "{codes:?}");
    assert_eq!(p.real("is"), 1e-14);
    assert_eq!(p.real("n"), 1.0);
    assert_eq!(p.real("gmin"), 1e-12);
    let (p, _) = read_kind("voltage-source", &[]);
    assert_eq!((p.real("v"), p.real("gain")), (0.0, 1.0));
    let (p, _) = read_kind("switch", &[("ron", text("1ohm")), ("roff", text("1Mohm"))]);
    assert_eq!(p.real("threshold"), 0.5);
}

#[test]
fn an_optional_initial_condition_is_absent_or_signed() {
    let (p, _) = read_kind("capacitor", &[("c", text("1uF"))]);
    assert_eq!(p.opt("ic"), None);
    let (p, codes) = read_kind("capacitor", &[("c", text("1uF")), ("ic", text("-2V"))]);
    assert!(codes.is_empty());
    assert_eq!(p.opt("ic"), Some(-2.0));
}

#[test]
fn every_error_of_an_actor_is_found_in_one_pass() {
    let (_, codes) = read_kind(
        "switch",
        &[
            ("ron", text("5V")),
            ("threshold", DomainValue::Boolean(true)),
            ("colour", text("red")),
        ],
    );
    assert_eq!(
        codes,
        [
            "circuit.param-unknown",
            "quantity.dimension",
            "circuit.param-missing",
            "circuit.param-type"
        ]
    );
}

#[test]
fn a_value_outside_its_range_is_a_wrong_type_naming_the_expectation() {
    let k = by_builtin("circuit/resistor").unwrap();
    let (_, n) = read(
        k,
        0,
        &actor(&[("r", text("-5ohm"))]),
        &CircuitUnits,
        TickLength::Ns,
    );
    assert_eq!(n[0].code, "circuit.param-type");
    let expected = n[0].params.iter().find(|(k, _)| k == "expected").unwrap();
    assert_eq!(
        expected.1,
        Value::Text("a quantity of resistance above zero".into())
    );
    let (_, n) = read(
        k,
        0,
        &actor(&[("r", text("0ohm"))]),
        &CircuitUnits,
        TickLength::Ns,
    );
    assert_eq!(n[0].code, "circuit.param-type");
}

#[test]
fn a_bare_number_for_a_quantity_is_the_cores_missing_unit() {
    let (_, codes) = read_kind("resistor", &[("r", DomainValue::Integer(100))]);
    assert_eq!(codes, ["quantity.missing-unit"]);
}

#[test]
fn a_sensors_period_is_required_and_at_least_one_tick() {
    let (_, codes) = read_kind("voltmeter", &[]);
    assert_eq!(codes, ["circuit.no-period"]);
    let (_, codes) = read_kind("voltmeter", &[("period", text("0ns"))]);
    assert_eq!(codes, ["circuit.period-zero"]);
    let (p, codes) = read_kind(
        "voltmeter",
        &[("period", text("100us")), ("phase", text("5us"))],
    );
    assert!(codes.is_empty());
    assert_eq!((p.ticks("period"), p.ticks("phase")), (100_000, 5_000));
    let (_, codes) = read_kind("voltmeter", &[("period", text("0.5ns"))]);
    assert_eq!(codes, ["quantity.not-whole-ticks"]);
}

#[test]
fn a_held_value_is_kept_as_written_for_its_signal() {
    let (p, codes) = read_kind("constant", &[("value", text("5V"))]);
    assert!(codes.is_empty());
    assert_eq!(p.held, Some(text("5V")));
    let (p, _) = read_kind("constant", &[]);
    assert_eq!(p.held, None);
    let (_, codes) = read_kind("constant", &[("value", DomainValue::Boolean(false))]);
    assert_eq!(codes, ["circuit.param-type"]);
}
