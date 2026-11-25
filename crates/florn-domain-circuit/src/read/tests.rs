use super::*;
use crate::units::CircuitUnits;

fn text(s: &str) -> DomainValue {
    DomainValue::Text(s.into())
}

#[test]
fn a_time_is_whole_ticks_or_the_cores_refusal() {
    let t = |v: &DomainValue| time_ticks(&CircuitUnits, "f", v, TickLength::Ns);
    assert_eq!(t(&text("100us")).unwrap(), 100_000);
    assert_eq!(t(&text("1ns")).unwrap(), 1);
    assert_eq!(t(&text("0ns")).unwrap(), 0);
    assert_eq!(
        t(&text("0.5ns")).unwrap_err().code,
        "quantity.not-whole-ticks"
    );
    assert_eq!(
        t(&DomainValue::Integer(5)).unwrap_err().code,
        "quantity.missing-unit"
    );
    assert_eq!(t(&text("5V")).unwrap_err().code, "quantity.dimension");
    assert_eq!(
        t(&text("5parsec")).unwrap_err().code,
        "quantity.unknown-unit"
    );
    assert_eq!(
        t(&DomainValue::Boolean(true)).unwrap_err().code,
        "manifest.type"
    );
}

#[test]
fn a_number_is_an_integer_or_a_decimal() {
    assert_eq!(number(&DomainValue::Integer(3)), Some(3.0));
    assert_eq!(number(&DomainValue::Decimal("0.001".into())), Some(0.001));
    assert_eq!(number(&text("1")), None);
    assert_eq!(number(&DomainValue::Boolean(true)), None);
}

#[test]
fn a_note_carries_the_catalogues_severity_and_ends_and_the_field() {
    let n = note(
        Code::CircuitPinOpen,
        Scope::port(3, 1),
        Some("actors[2].ports[1]"),
        vec![Value::Name("r1".into()), Value::Name("a".into())],
    );
    assert_eq!(n.code, "circuit.pin-open");
    assert_eq!(n.scope.field.as_deref(), Some("actors[2].ports[1]"));
    assert_eq!((n.scope.actor, n.scope.port), (Some(3), Some(1)));
    assert!(n.check().is_ok(), "{:?}", n.check());
}
