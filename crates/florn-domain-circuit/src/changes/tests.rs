//! RFC 012 rule 15: a settable value changed from outside the plant, read against the plant.

use super::*;
use crate::tests::Deck;

fn map(entries: &[(&str, DomainValue)]) -> DomainMap {
    entries
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

fn text(s: &str) -> DomainValue {
    DomainValue::Text(s.to_string())
}

fn plant() -> crate::plant::Plant {
    Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"1V\"")
        .builtin(2, "i1", "current-source", "i = \"1mA\"")
        .builtin(3, "k1", "constant", "value = \"5Nm\"")
        .builtin(4, "r1", "resistor", "r = \"1kohm\"")
        .builtin(5, "vm", "voltmeter", "period = \"1ms\"")
        .builtin_opt(
            6,
            "m1",
            "dc-motor",
            "r = \"1ohm\"\nl = \"1mH\"\nkt = \"0.1Nm/A\"\nke = \"0.1Vs/rad\"\nj = \"0.01kgm2\"\nb = \"0.001Nms\"\nperiod = \"1ms\"",
        )
        .net("top", &["v1.p", "r1.a", "i1.n", "m1.a+"])
        .net("mid", &["r1.b", "vm.p"])
        .ground("g", &["v1.n", "i1.p", "vm.n", "m1.a-"])
        .signal("load", &["k1.y", "m1.load"], "unit = \"mNm\"")
        .plant()
}

#[test]
fn a_source_is_read_in_si_by_identity_or_by_name() {
    let p = plant();
    let a = source(
        &p,
        &map(&[("actor", DomainValue::Integer(1)), ("v", text("2.5V"))]),
        "x",
    )
    .unwrap();
    assert_eq!(
        (a.actor, a.input, a.value, a.signal),
        (1, Input::Source(1), 2.5, None)
    );
    let b = source(&p, &map(&[("actor", text("i1")), ("i", text("-3mA"))]), "x").unwrap();
    assert_eq!((b.input, b.value), (Input::Source(2), -0.003));
}

#[test]
fn a_constants_value_is_converted_to_the_unit_of_its_signal_once() {
    let p = plant();
    // The signal is in mNm: 0.5 Nm is 500 mNm, a bare number is refused.
    let c = source(
        &p,
        &map(&[("actor", text("k1")), ("value", text("0.5Nm"))]),
        "x",
    )
    .unwrap();
    assert_eq!((c.value, c.signal.is_some()), (500.0, true));
    assert!(matches!(c.input, Input::Signal(_)));
    let bare = source(
        &p,
        &map(&[("actor", text("k1")), ("value", DomainValue::Integer(3))]),
        "x",
    );
    assert!(matches!(bare, Err(Fault::Value(n)) if n.code == "quantity.missing-unit"));
    let wrong = source(
        &p,
        &map(&[("actor", text("k1")), ("value", text("3V"))]),
        "x",
    );
    assert!(matches!(wrong, Err(Fault::Value(n)) if n.code == "quantity.dimension"));
}

#[test]
fn what_cannot_be_set_is_refused_naming_the_actor_and_the_key() {
    let p = plant();
    let none = source(
        &p,
        &map(&[("actor", DomainValue::Integer(99)), ("v", text("1V"))]),
        "x",
    );
    assert!(matches!(none, Err(Fault::Actor(t)) if t == "99"));
    let resistor = source(
        &p,
        &map(&[("actor", text("r1")), ("r", text("1kohm"))]),
        "x",
    );
    assert!(matches!(resistor, Err(Fault::Key { ref actor, .. }) if actor == "r1"));
    let other = source(&p, &map(&[("actor", text("v1")), ("i", text("1A"))]), "x");
    assert!(matches!(other, Err(Fault::Key { ref key, expected: "v", .. }) if key == "i"));
    let n = other.err().unwrap().at_field("changes[0].payload");
    assert_eq!(n.code, "circuit.param-unknown");
    assert_eq!(n.scope.field.as_deref(), Some("changes[0].payload.i"));
}

#[test]
fn a_signal_intervention_names_a_connection_and_a_boolean() {
    let p = plant();
    let id = p.signals[0].id;
    assert_eq!(
        signal(&p, id, &map(&[("held", DomainValue::Boolean(true))])).unwrap(),
        (0, true)
    );
    assert!(signal(&p, id + 99, &map(&[("held", DomainValue::Boolean(true))])).is_err());
    assert!(signal(&p, id, &map(&[])).is_err());
    assert!(signal(&p, id, &map(&[("held", text("yes"))])).is_err());
}
