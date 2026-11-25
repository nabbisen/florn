use super::*;
use florn_host::domain::quantity_si;

fn si(text: &str, dimension: &'static str) -> f64 {
    quantity_si(&CircuitUnits, "f", text, dimension).unwrap_or_else(|n| panic!("{text}: {n:?}"))
}

#[test]
fn the_table_has_every_symbol_of_rule_5_with_its_dimension() {
    let all = all();
    assert_eq!(all.len(), 10 * 9 + 5);
    let of = |s: &str| all.iter().find(|u| u.symbol == s).map(|u| u.dimension);
    for (symbol, dimension) in [
        ("V", "voltage"),
        ("mA", "current"),
        ("kohm", "resistance"),
        ("kΩ", "resistance"),
        ("uF", "capacitance"),
        ("µF", "capacitance"),
        ("mH", "inductance"),
        ("pS", "conductance"),
        ("W", "power"),
        ("MHz", "frequency"),
        ("mNm", "torque"),
        ("rad/s", "angular-speed"),
        ("kgm2", "inertia"),
        ("Nms", "friction"),
        ("Nm/A", "torque-per-current"),
        ("Vs/rad", "voltage-per-speed"),
    ] {
        assert_eq!(of(symbol), Some(dimension), "{symbol}");
    }
    // The composite units take no prefix; the core's time units are not here.
    assert_eq!(of("mrad/s"), None);
    assert_eq!(of("ms"), None);
    let mut symbols: Vec<&str> = all.iter().map(|u| u.symbol).collect();
    symbols.sort_unstable();
    symbols.dedup();
    assert_eq!(symbols.len(), all.len(), "no symbol twice");
}

#[test]
fn every_dimension_is_in_the_closed_set_of_rfc_007() {
    for u in all() {
        assert!(
            florn_control::catalogue::enumeration("dimension")
                .unwrap()
                .contains(&u.dimension),
            "{}",
            u.symbol
        );
    }
}

#[test]
fn a_quantity_converts_by_exact_decimal_arithmetic_rounded_once() {
    assert_eq!(si("1.2kohm", "resistance"), 1200.0);
    assert_eq!(si("1.2kΩ", "resistance"), 1200.0);
    assert_eq!(si("1uF", "capacitance"), 1e-6);
    assert_eq!(si("1µF", "capacitance"), 1e-6);
    assert_eq!(si("0.5mH", "inductance"), 5e-4);
    assert_eq!(si("-12V", "voltage"), -12.0);
    assert_eq!(si("150rad/s", "angular-speed"), 150.0);
    // 0.1 mA is the f64 nearest to 1e-4 and not 0.1 × 1e-3 computed in floats.
    assert_eq!(si("0.1mA", "current").to_bits(), 1e-4f64.to_bits());
    assert_eq!(si("1pS", "conductance").to_bits(), 1e-12f64.to_bits());
}

#[test]
fn a_quantity_of_another_dimension_is_the_cores_refusal() {
    let n = quantity_si(&CircuitUnits, "actors[0].config.r", "5V", "resistance").unwrap_err();
    assert_eq!(n.code, "quantity.dimension");
    let n = quantity_si(&CircuitUnits, "f", "5furlong", "voltage").unwrap_err();
    assert_eq!(n.code, "quantity.unknown-unit");
}

#[test]
fn each_prefix_has_its_power_of_ten() {
    let table: [(&str, f64); 9] = [
        ("pV", 1e-12),
        ("nV", 1e-9),
        ("uV", 1e-6),
        ("µV", 1e-6),
        ("mV", 1e-3),
        ("V", 1.0),
        ("kV", 1e3),
        ("MV", 1e6),
        ("GV", 1e9),
    ];
    for (symbol, want) in table {
        assert_eq!(
            si(&format!("1{symbol}"), "voltage").to_bits(),
            want.to_bits(),
            "{symbol}"
        );
    }
}
