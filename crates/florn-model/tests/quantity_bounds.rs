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
    scenario_at("1ns", end, memory)
}

/// The same with the tick length as given.
fn scenario_at(tick: &str, end: &str, memory: &str) -> String {
    format!(
        "florn = 1\nname = \"bounds\"\ndomains = [\"core\"]\n\n[time]\ntick = \"{tick}\"\nend = \"{end}\"\n\n\
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

/// RFC 004 rule 23, a number is exact and bounded: a whole number of more than 39 digits does not
/// fit the mantissa; it is a number out of range and not a text that is no number.
#[test]
fn a_whole_number_of_more_than_39_digits_is_out_of_range() {
    // The largest `u128` is 39 digits and is read; the next is not.
    let largest = u128::MAX.to_string();
    assert_eq!(largest.len(), 39);
    assert_eq!(
        Decimal::parse(&largest).unwrap().shortest(),
        largest,
        "the largest mantissa is read"
    );
    assert!(Decimal::parse("340282366920938463463374607431768211456").is_none());
    assert!(Decimal::parse(&"9".repeat(40)).is_none());
    // Leading zeros are not digits of the value; a fraction counts its digits all the same.
    assert!(Decimal::parse(&format!("{}1", "0".repeat(60))).is_some());
    assert!(Decimal::parse(&format!("0.{}", "9".repeat(40))).is_none());

    for end in [
        format!("{}s", "9".repeat(40)),
        format!("{}ns", "9".repeat(5_000)),
        format!("0.{}s", "9".repeat(40)),
    ] {
        let (code, field, texts) = only_error(&scenario(&end, "1MiB"));
        assert_eq!(code, "quantity.out-of-range", "{}", &end[..20]);
        assert_eq!(field.as_deref(), Some("time.end"));
        assert!(texts.iter().all(|t| t.chars().count() <= 65), "{texts:?}");
        assert_eq!(texts.len(), 2);
    }
    // A byte quantity too.
    let (code, field, _) = only_error(&scenario("1s", &format!("{}B", "9".repeat(40))));
    assert_eq!(code, "quantity.out-of-range");
    assert_eq!(field.as_deref(), Some("limits.default.memory"));
}

/// A duration beyond what the arithmetic holds is out of range; it is not "not whole", and no
/// smallest tick is made up for it. A value that is truly not whole keeps its code.
#[test]
fn a_duration_beyond_u128_ticks_is_out_of_range_and_not_not_whole() {
    // At 1 fs, 1e30 s is 1e45 ticks: whole, and beyond a `u128` (3.4e38).
    for (tick, end) in [
        ("1fs", "1e30s"),
        ("1fs", "1e50s"),
        ("1fs", "1e24s"),
        ("1ns", "1e50s"),
        ("1ns", "1e999s"),
        ("1ps", "3e25s"),
    ] {
        let (code, field, texts) = only_error(&scenario_at(tick, end, "1MiB"));
        assert_eq!(code, "quantity.out-of-range", "{end} at {tick}");
        assert_eq!(field.as_deref(), Some("time.end"));
        assert_eq!(texts.len(), 2, "{end}: the value is named");
    }
    // Between u64 and u128 was out of range before, and stays so.
    let (code, _, _) = only_error(&scenario_at("1fs", "1e10s", "1MiB"));
    assert_eq!(code, "quantity.out-of-range", "1e25 ticks");
    // Not whole is not out of range: the smallest tick is named when one makes it whole.
    let (code, _, _) = only_error(&scenario_at("1ns", "1.5ns", "1MiB"));
    assert_eq!(code, "quantity.not-whole-ticks");
    let (code, _, _) = only_error(&scenario_at("1fs", "1e-30s", "1MiB"));
    assert_eq!(code, "quantity.not-whole-ticks", "below a femtosecond");
}

/// The value is not whole at the tick, and whole at a finer one only beyond a `u128`: the finer
/// tick would not make it valid, so it is out of range and none is named.
#[test]
fn a_value_not_whole_that_a_finer_tick_cannot_hold_is_out_of_range() {
    // 38 digits and a half nanosecond: 1.2e38 × 100 > u128 at 1 ps.
    let end = "12345678901234567890123456789012345678.5ns";
    let q = Quantity::parse(end).unwrap();
    assert_eq!(q.to_ticks(TickLength::Ns), Ok(None));
    let (code, _, _) = only_error(&scenario_at("1ns", end, "1MiB"));
    assert_eq!(code, "quantity.out-of-range");
}

#[test]
fn to_ticks_and_to_bytes_say_too_large_the_same_way_as_beyond_u64() {
    let q = |t: &str| Quantity::parse(t).unwrap();
    // u64 range: Ok(Some); beyond u64 but within u128: Ok(None); beyond u128: Ok(None) too.
    assert_eq!(
        q("1s").to_ticks(TickLength::Fs),
        Ok(Some(1_000_000_000_000_000))
    );
    assert_eq!(q("1e10s").to_ticks(TickLength::Fs), Ok(None));
    assert_eq!(q("1e30s").to_ticks(TickLength::Fs), Ok(None));
    assert_eq!(q("1e999s").to_ticks(TickLength::Fs), Ok(None));
    assert_eq!(
        q("1e-999s").to_ticks(TickLength::Fs),
        Err(Some(TickLength::Fs))
    );
    assert_eq!(q("600y").to_ticks(TickLength::Fs), Err(None));
    assert_eq!(q("1e20B").to_bytes(), Ok(None));
    assert_eq!(q("1e50B").to_bytes(), Ok(None));
    assert_eq!(q("1e999MiB").to_bytes(), Ok(None));
    assert_eq!(q("1.5B").to_bytes(), Err(true));
    assert_eq!(q("1e-999B").to_bytes(), Err(true));
    assert_eq!(q("1s").to_bytes(), Err(false));
    assert_eq!(q("4MiB").to_bytes(), Ok(Some(4 * 1024 * 1024)));
    // The work does not grow with the exponent: the largest accepted one at once.
    assert_eq!(q("1e1000s").to_ticks(TickLength::Fs), Ok(None));
    assert_eq!(
        q("1e-1000s").to_ticks(TickLength::Fs),
        Err(Some(TickLength::Fs))
    );
}

#[test]
fn a_byte_quantity_beyond_u128_is_out_of_range_and_not_not_whole() {
    let (code, field, _) = only_error(&scenario("1s", "1e50B"));
    assert_eq!(code, "quantity.out-of-range");
    assert_eq!(field.as_deref(), Some("limits.default.memory"));
    let (code, _, _) = only_error(&scenario("1s", "0.5B"));
    assert_eq!(code, "quantity.not-whole-unit");
}

/// RFC 004 rule 25: a duration is refused only when it is not whole in ticks; zero is whole in
/// every tick length, however it is written. (It was refused as "not whole", with the smallest
/// tick `1fs` named, because the normalised zero has the exponent 0.)
#[test]
fn zero_is_a_whole_number_of_ticks_in_every_tick_length() {
    for tick in [TickLength::Ns, TickLength::Ps, TickLength::Fs] {
        for text in ["0s", "0ns", "0fs", "0.000ms", "0e5s", "0e-5us", "000ps"] {
            let q = Quantity::parse(text).unwrap();
            assert_eq!(q.to_ticks(tick), Ok(Some(0)), "{text} at {tick:?}");
        }
    }
    assert_eq!(Quantity::parse("0B").unwrap().to_bytes(), Ok(Some(0)));
    assert_eq!(Quantity::parse("0.0MiB").unwrap().to_bytes(), Ok(Some(0)));
    assert_eq!(Decimal::parse("0").unwrap().as_integer_at(6), Some(0));
    // A zero in a scenario's fields.
    assert!(parse(&scenario_at("1ns", "0s", "1MiB")).is_ok());
    assert!(parse(&scenario_at("1fs", "0e30s", "1MiB")).is_ok());
    // Not zero, and not whole, keeps its code; a unit that is no time unit keeps its own.
    let (code, _, _) = only_error(&scenario_at("1ns", "0.5ns", "1MiB"));
    assert_eq!(code, "quantity.not-whole-ticks");
    let (code, _, _) = only_error(&scenario_at("1ns", "0y", "1MiB"));
    assert_eq!(code, "quantity.unknown-unit");
}
