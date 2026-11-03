//! A quantity in a module's `config` is bounded as one in the scenario is (SEC-005; RFC 004 rule
//! 23): a number beyond what a quantity holds is `quantity.out-of-range`, naming the parameter,
//! and not `config.type`, which says the text is no quantity at all.
use florn_admission::config::{NoUnits, Units, encode_config};
use florn_admission::manifest::parse_manifest;
use florn_model::{Decimal, DomainMap, DomainValue, TickLength};

struct Ohms;
impl Units for Ohms {
    fn to_si(&self, unit: &str) -> Option<Decimal> {
        (unit == "ohm").then(|| Decimal::parse("1").unwrap())
    }
}

fn module(ty: &str, unit: &str) -> florn_admission::manifest::ModuleManifest {
    let unit = if unit.is_empty() {
        String::new()
    } else {
        format!("unit = \"{unit}\"\n")
    };
    parse_manifest(&format!(
        "florn-module = 1\nworld = \"w\"\nkind = \"k\"\n[[params]]\nname = \"p\"\ntype = \"{ty}\"\n{unit}"
    ))
    .unwrap()
}

/// The refusals of one parameter given as text: (code, field, the value's two texts when the
/// refusal carries them).
fn refusals(
    ty: &str,
    unit: &str,
    value: &str,
    units: &dyn Units,
) -> Vec<(&'static str, Option<String>, Vec<String>)> {
    let config: DomainMap = [("p".to_string(), DomainValue::Text(value.to_string()))]
        .into_iter()
        .collect();
    let e =
        encode_config(1, &module(ty, unit), &config, TickLength::Ns, units).expect_err("refused");
    e.0.iter()
        .map(|err| {
            let texts = match &err.kind {
                florn_admission::Kind::Model(k) => match &**k {
                    florn_model::Kind::QuantityOutOfRange { value, .. } => {
                        vec![value.0.clone(), value.1.clone()]
                    }
                    _ => Vec::new(),
                },
                _ => Vec::new(),
            };
            (err.code(), err.field.clone(), texts)
        })
        .collect()
}

#[test]
fn a_real_beyond_the_exponent_bound_is_out_of_range_not_the_wrong_type() {
    for value in [
        "1e2000000000ohm",
        "1e1001ohm",
        "1e-1001ohm",
        "1e99999999999999999999ohm",
    ] {
        let r = refusals("real", "ohm", value, &Ohms);
        assert_eq!(r.len(), 1, "{value}: {r:?}");
        assert_eq!(r[0].0, "quantity.out-of-range", "{value}");
        assert_eq!(r[0].1.as_deref(), Some("config.p"), "{value}");
        assert!(r[0].2.iter().all(|t| t.chars().count() <= 65), "{r:?}");
    }
    // The same through a registry that knows no unit: the number is judged before the unit.
    let r = refusals("real", "ohm", "1e2000000000ohm", &NoUnits);
    assert_eq!(r[0].0, "quantity.out-of-range");
}

#[test]
fn ticks_and_integer_parameters_are_bounded_too() {
    for (ty, unit, value) in [
        ("ticks", "", "1e2000000000s"),
        ("integer", "0.01USD", "1e2000000000USD"),
        // A whole number of more than 39 digits, in each type
        ("ticks", "", "340282366920938463463374607431768211456ns"),
        (
            "integer",
            "0.01USD",
            "340282366920938463463374607431768211456USD",
        ),
        ("real", "ohm", "340282366920938463463374607431768211456ohm"),
    ] {
        let r = refusals(ty, unit, value, &Ohms);
        assert_eq!(r.len(), 1, "{ty} {value}: {r:?}");
        assert_eq!(r[0].0, "quantity.out-of-range", "{ty} {value}");
        assert_eq!(r[0].1.as_deref(), Some("config.p"), "{ty} {value}");
    }
}

#[test]
fn the_value_in_the_refusal_is_cut_not_built() {
    let long = format!("0.{}1ohm", "0".repeat(100_000));
    let r = refusals("real", "ohm", &long, &Ohms);
    assert_eq!(r[0].0, "quantity.out-of-range");
    assert_eq!(
        r[0].2[0].chars().count(),
        65,
        "64 characters and the cut mark"
    );
    assert!(r[0].2[0].starts_with("0.000") && r[0].2[0].ends_with('…'));
    assert_eq!(r[0].2[1], "ohm");
}

#[test]
fn what_is_no_quantity_is_still_the_wrong_type() {
    for (ty, unit, value) in [
        ("real", "ohm", "ten"),
        ("real", "ohm", "1e+ohm"),
        ("ticks", "", "1e5"),
        ("integer", "0.01USD", "USD"),
    ] {
        let r = refusals(ty, unit, value, &Ohms);
        assert_eq!(r[0].0, "config.type", "{ty} {value}");
    }
}

#[test]
fn a_value_in_range_is_encoded_as_before() {
    let config: DomainMap = [("p".to_string(), DomainValue::Text("1.5ohm".to_string()))]
        .into_iter()
        .collect();
    let bytes = encode_config(1, &module("real", "ohm"), &config, TickLength::Ns, &Ohms).unwrap();
    assert_eq!(bytes, 1.5f64.to_le_bytes());
}
