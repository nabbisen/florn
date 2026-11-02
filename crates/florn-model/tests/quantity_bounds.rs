//! A number in a scenario is bounded (SEC-005; RFC 004 rule 39: nothing is allocated from a size
//! the input chose; rules 23 to 25: a quantity is a number with a unit, a duration whole in
//! ticks). A decimal's exponent, once normalised, is at most `Decimal::MAX_EXPONENT` in
//! magnitude; a quantity beyond it is refused as `quantity.out-of-range`, naming its field, with
//! the value as written and cut, not built into a text of its own size.
use florn_model::*;

const MAX: i32 = Decimal::MAX_EXPONENT;

fn parse(text: &str) -> Result<Manifest, Errors> {
    Manifest::parse(text, &HostMaxima::browser())
}

/// A scenario with `end` and a default memory limit as given; everything else minimal.
fn scenario(end: &str, memory: &str) -> String {
    format!(
        "florn = 1\nname = \"bounds\"\ndomains = [\"core\"]\n\n[time]\ntick = \"1ns\"\nend = \"{end}\"\n\n\
         [order]\ntie = \"by-identity\"\n\n[seed]\nroot = 1\n\n[limits.default]\nmemory = \"{memory}\"\n"
    )
}

/// The one error a scenario is expected to give: its code, its field and the value's two texts.
fn only_error(text: &str) -> (&'static str, Option<String>, Vec<String>) {
    let e = parse(text).unwrap_err();
    assert_eq!(e.0.len(), 1, "one error: {:?}", e.codes());
    let err = &e.0[0];
    let texts = match &err.kind {
        Kind::QuantityOutOfRange { value, .. } => vec![value.0.clone(), value.1.clone()],
        _ => Vec::new(),
    };
    (err.code(), err.field.clone(), texts)
}

#[test]
fn the_exponent_is_accepted_up_to_the_bound_and_refused_beyond_it() {
    // At the bound, in both directions, whatever the mantissa's trailing zeros.
    assert!(Decimal::parse(&format!("1e{MAX}")).is_some());
    assert!(Decimal::parse(&format!("1e-{MAX}")).is_some());
    assert!(
        Decimal::parse(&format!("10e{}", MAX - 1)).is_some(),
        "10e999 normalises to 1e1000"
    );
    assert!(Decimal::parse(&format!("0.{}1", "0".repeat(MAX as usize - 1))).is_some());
    // One beyond.
    assert!(Decimal::parse(&format!("1e{}", MAX + 1)).is_none());
    assert!(Decimal::parse(&format!("1e-{}", MAX + 1)).is_none());
    assert!(Decimal::parse(&format!("0.{}1", "0".repeat(MAX as usize))).is_none());
    // Far beyond, and beyond what an `i32` holds.
    assert!(Decimal::parse("1e2000000000").is_none());
    assert!(Decimal::parse("1e99999999999999999999999999").is_none());
    assert!(Decimal::parse("1e-99999999999999999999999999").is_none());
    // Zero has no scale, however it is written.
    assert_eq!(Decimal::parse("0e2000000000").unwrap().shortest(), "0");
    assert_eq!(
        Decimal::parse("0.000e-99999999999999").unwrap().shortest(),
        "0"
    );
}

#[test]
fn what_was_accepted_before_is_read_as_before() {
    for (text, shortest) in [
        ("1.5", "1.5"),
        ("0.001", "0.001"),
        ("2.50E6", "2500000"),
        ("1e+3", "1000"),
        ("000", "0"),
        ("1e-3", "0.001"),
    ] {
        assert_eq!(Decimal::parse(text).unwrap().shortest(), shortest, "{text}");
    }
}

#[test]
fn a_malformed_exponent_is_still_malformed() {
    for text in ["1e", "1e+", "1e-", "1e1.5", "1ex", "e5", "1e 5", "1e--5"] {
        assert!(Decimal::parse(text).is_none(), "{text}");
    }
}

#[test]
fn shortest_is_bounded_for_a_value_no_parser_makes() {
    let huge = Decimal {
        mantissa: 1,
        exponent: 2_000_000_000,
    };
    assert_eq!(huge.shortest(), "1e2000000000");
    let tiny = Decimal {
        mantissa: 12345,
        exponent: -2_000_000_000,
    };
    assert_eq!(tiny.shortest(), "12345e-2000000000");
    // At the bound the plain form is kept: a thousand zeros, not an exponent.
    let at = Decimal::parse(&format!("1e{MAX}")).unwrap().shortest();
    assert_eq!(at.len(), MAX as usize + 1);
}

#[test]
fn an_end_with_an_enormous_exponent_is_out_of_range_and_names_its_field() {
    for end in [
        "1e2000000000s",
        "1e1001s",
        "1e-1001s",
        "1E+2000000000s",
        // beyond an i32, and a fraction that shifts the exponent below the bound
        "1e99999999999999999999s",
        "1e-99999999999999999999s",
    ] {
        let (code, field, texts) = only_error(&scenario(end, "1MiB"));
        assert_eq!(code, "quantity.out-of-range", "{end}");
        assert_eq!(field.as_deref(), Some("time.end"), "{end}");
        assert!(
            texts.iter().all(|t| t.chars().count() <= 70),
            "{end}: {texts:?}"
        );
    }
}

#[test]
fn the_value_in_the_error_is_what_was_written_cut_not_built() {
    // A fraction of a hundred thousand zeros: the exponent is -100,001.
    let long = format!("0.{}1s", "0".repeat(100_000));
    let (code, _, texts) = only_error(&scenario(&long, "1MiB"));
    assert_eq!(code, "quantity.out-of-range");
    assert_eq!(texts.len(), 2);
    assert!(
        texts[0].starts_with("0.000") && texts[0].ends_with('…'),
        "{:?}",
        texts[0]
    );
    assert_eq!(
        texts[0].chars().count(),
        65,
        "64 characters and the cut mark"
    );
    assert_eq!(texts[1], "s");
    // A unit of a hundred thousand letters is cut too.
    let unit = format!("1e2000{}", "x".repeat(100_000));
    let (_, _, texts) = only_error(&scenario(&unit, "1MiB"));
    assert!(texts.iter().all(|t| t.chars().count() <= 65), "{texts:?}");
}

#[test]
fn a_byte_quantity_is_bounded_the_same_way() {
    let (code, field, texts) = only_error(&scenario("1s", "1e2000000000MiB"));
    assert_eq!(code, "quantity.out-of-range");
    assert_eq!(field.as_deref(), Some("limits.default.memory"));
    assert!(texts.iter().all(|t| t.chars().count() <= 70), "{texts:?}");
}

#[test]
fn the_other_refusals_keep_their_codes() {
    // A bare number is a missing unit, whatever its exponent.
    let (code, _, _) = only_error(&scenario("1e5000", "1MiB"));
    assert_eq!(code, "quantity.missing-unit");
    // Not a number at all.
    let (code, _, _) = only_error(&scenario("1e+s", "1MiB"));
    assert_eq!(code, "manifest.type");
    // Within the bound but not a duration the core knows, and not whole in ticks.
    let (code, _, _) = only_error(&scenario("1e500y", "1MiB"));
    assert_eq!(code, "quantity.unknown-unit");
    let (code, _, _) = only_error(&scenario("1e-500s", "1MiB"));
    assert_eq!(code, "quantity.not-whole-ticks");
}
