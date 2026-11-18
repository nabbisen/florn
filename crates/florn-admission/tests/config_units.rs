//! A quantity of a `real` parameter (RFC 012 rule 5 and finding F-13; RFC 004 rule 27): the unit's
//! dimension is the parameter's, the value is the number and the unit's factor converted exactly
//! and rounded once, and what would be infinity, or zero from a number that is not zero, is
//! refused. Non-finite `f64` parameters are refused too.
use florn_admission::config::{NoUnits, UnitDef, Units, encode_config};
use florn_admission::manifest::{ModuleManifest, parse_manifest};
use florn_model::{Decimal, DomainMap, DomainValue, TickLength};

/// A registry of three dimensions, with prefixes the way a domain writes them out.
struct Registry;
impl Units for Registry {
    fn unit(&self, symbol: &str) -> Option<UnitDef> {
        let (symbol, factor, dimension) = match symbol {
            "ohm" => ("ohm", "1", "resistance"),
            "kohm" => ("kohm", "1000", "resistance"),
            "F" => ("F", "1", "capacitance"),
            "nF" => ("nF", "0.000000001", "capacitance"),
            "uF" => ("uF", "0.000001", "capacitance"),
            // Two units whose factors make the old arithmetic round three times.
            "tF" => ("tF", "0.00000000001", "capacitance"),
            "V" => ("V", "1", "voltage"),
            _ => return None,
        };
        Some(UnitDef {
            symbol,
            factor: Decimal::parse(factor)?,
            dimension,
        })
    }
}

fn module(ty: &str, unit: &str) -> ModuleManifest {
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

fn config(value: DomainValue) -> DomainMap {
    [("p".to_string(), value)].into_iter().collect()
}

fn encode(ty: &str, unit: &str, value: DomainValue) -> Result<Vec<u8>, Vec<Refusal>> {
    encode_config(
        1,
        &module(ty, unit),
        &config(value),
        TickLength::Ns,
        &Registry,
    )
    .map_err(|e| {
        e.0.iter()
            .map(|err| Refusal {
                code: err.code(),
                field: err.field.clone(),
                params: err
                    .params()
                    .into_iter()
                    .map(|(n, v)| format!("{n}={v:?}"))
                    .collect(),
            })
            .collect()
    })
}

#[derive(Debug)]
struct Refusal {
    code: &'static str,
    field: Option<String>,
    params: Vec<String>,
}

fn real(text: &str) -> Result<f64, Vec<Refusal>> {
    let unit = if text.ends_with("F") { "F" } else { "ohm" };
    encode("real", unit, DomainValue::Text(text.into()))
        .map(|b| f64::from_le_bytes(b.try_into().expect("eight bytes")))
}

#[test]
fn a_number_and_a_unit_are_rounded_once_0_1_and_0_3() {
    // `0.1nF` and `0.3` of a unit of 1e-11: the old arithmetic multiplied the two floats and gave
    // 1.0000000000000002e-10 and 2.9999999999999997e-12. The exact product is rounded once.
    assert_eq!(real("0.1nF").unwrap().to_bits(), 1e-10f64.to_bits());
    assert_eq!(real("0.3tF").unwrap().to_bits(), 3e-12f64.to_bits());
    assert_eq!(real("3.3uF").unwrap().to_bits(), 3.3e-6f64.to_bits());
    assert_eq!(real("1.5kohm").unwrap(), 1500.0);
    assert_eq!(real("0ohm").unwrap().to_bits(), 0.0f64.to_bits());
    // A prefix the registry lacks is refused, not guessed.
    assert_eq!(
        real("100pF").expect_err("pF")[0].code,
        "quantity.unknown-unit"
    );
    // The same value in another prefix is the same f64 (the value is the value).
    assert_eq!(real("0.0001uF").unwrap().to_bits(), 1e-10f64.to_bits());
}

#[test]
fn a_quantity_of_another_dimension_is_refused_naming_the_field() {
    let r = encode("real", "ohm", DomainValue::Text("5V".into())).expect_err("refused");
    assert_eq!(r.len(), 1, "{r:?}");
    assert_eq!(r[0].code, "quantity.dimension");
    assert_eq!(r[0].field.as_deref(), Some("config.p"));
    assert!(r[0].params[0].contains("config.p"), "{:?}", r[0].params);
    assert!(r[0].params[1].contains("5") && r[0].params[1].contains("V"));
    assert!(r[0].params[2].contains("resistance"), "{:?}", r[0].params);
    // A unit of the right dimension in another prefix is accepted; the same unit symbol named by
    // the declaration but not registered is named as unknown, not the written one.
    assert!(real("2kohm").is_ok());
    let r = encode("real", "ohm", DomainValue::Text("5parsec".into())).expect_err("refused");
    assert_eq!(r[0].code, "quantity.unknown-unit");
    let r = encode("real", "mho", DomainValue::Text("5ohm".into())).expect_err("refused");
    assert_eq!(r[0].code, "quantity.unknown-unit");
    assert!(r[0].params[1].contains("mho"), "{:?}", r[0].params);
}

#[test]
fn infinity_and_zero_from_a_nonzero_number_are_refused() {
    // Today `1e400ohm` encodes infinity and `1e-400ohm` encodes 0.
    for text in ["1e400ohm", "1e-400ohm", "1e308kohm"] {
        let r = real(text).expect_err(text);
        assert_eq!(r[0].code, "quantity.out-of-range", "{text}");
        assert_eq!(r[0].field.as_deref(), Some("config.p"), "{text}");
    }
    // The edge is a value, not a refusal.
    assert_eq!(real("1.7976931348623157e308ohm").unwrap(), f64::MAX);
    assert!(real("1e-310ohm").unwrap() > 0.0);
}

#[test]
fn an_f64_parameter_that_is_not_finite_is_refused() {
    let f64_of = |v: DomainValue| encode("f64", "", v);
    for text in ["nan", "inf", "-inf", "NaN", "infinity"] {
        let r = f64_of(DomainValue::Decimal(text.into())).expect_err(text);
        assert_eq!(r[0].code, "quantity.out-of-range", "{text}");
        assert_eq!(r[0].field.as_deref(), Some("config.p"), "{text}");
    }
    // A number beyond the f64 range, and one that is not zero and reads as zero.
    for text in [
        format!("1{}", "0".repeat(400)),
        format!("0.{}1", "0".repeat(400)),
    ] {
        let r = f64_of(DomainValue::Decimal(text.clone())).expect_err("beyond the range");
        assert_eq!(r[0].code, "quantity.out-of-range");
        assert!(
            r[0].params[1].len() < 120,
            "the value in the refusal is cut: {:?}",
            r[0].params
        );
    }
    // Values: read once, zero is zero, an integer is exact.
    assert_eq!(
        f64_of(DomainValue::Decimal("0.1".into())).unwrap(),
        0.1f64.to_le_bytes()
    );
    assert_eq!(
        f64_of(DomainValue::Decimal("0.0".into())).unwrap(),
        0.0f64.to_le_bytes()
    );
    assert_eq!(
        f64_of(DomainValue::Integer(3)).unwrap(),
        3.0f64.to_le_bytes()
    );
}

#[test]
fn a_time_parameter_written_in_a_unit_of_a_domain_is_of_the_wrong_dimension() {
    let r = encode("ticks", "", DomainValue::Text("5ohm".into())).expect_err("refused");
    assert_eq!(r[0].code, "quantity.dimension");
    assert!(r[0].params[2].contains("time"), "{:?}", r[0].params);
    // A unit no domain knows stays unknown.
    let r = encode("ticks", "", DomainValue::Text("5parsec".into())).expect_err("refused");
    assert_eq!(r[0].code, "quantity.unknown-unit");
    // And with no domain at all, a domain's unit is unknown.
    let none = encode_config(
        1,
        &module("ticks", ""),
        &config(DomainValue::Text("5ohm".into())),
        TickLength::Ns,
        &NoUnits,
    )
    .expect_err("refused");
    assert_eq!(none.codes(), ["quantity.unknown-unit"]);
}
