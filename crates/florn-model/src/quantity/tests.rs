use super::*;

#[test]
fn decimals() {
    assert_eq!(
        Decimal::parse("5").unwrap(),
        Decimal {
            mantissa: 5,
            exponent: 0
        }
    );
    assert_eq!(
        Decimal::parse("0.5").unwrap(),
        Decimal {
            mantissa: 5,
            exponent: -1
        }
    );
    assert_eq!(
        Decimal::parse("1e-3").unwrap(),
        Decimal {
            mantissa: 1,
            exponent: -3
        }
    );
    assert_eq!(
        Decimal::parse("2.50E6").unwrap(),
        Decimal {
            mantissa: 25,
            exponent: 5
        }
    );
    assert_eq!(Decimal::parse("1.5").unwrap().shortest(), "1.5");
    assert_eq!(Decimal::parse("0.001").unwrap().shortest(), "0.001");
    assert_eq!(Decimal::parse("2.50E6").unwrap().shortest(), "2500000");
    assert_eq!(Decimal::parse("000").unwrap().shortest(), "0");
    assert!(Decimal::parse("").is_none());
    assert!(Decimal::parse(".").is_none());
    assert!(Decimal::parse("1.2.3").is_none());
}

#[test]
fn quantities_and_ticks() {
    let q = Quantity::parse("5ms").unwrap();
    assert_eq!(q.to_ticks(TickLength::Ns), Ok(Some(5_000_000)));
    assert_eq!(
        Quantity::parse("1.5ns").unwrap().to_ticks(TickLength::Ns),
        Err(Some(TickLength::Ps))
    );
    assert_eq!(
        Quantity::parse("1.0000005ns")
            .unwrap()
            .to_ticks(TickLength::Ns),
        Err(Some(TickLength::Fs))
    );
    assert_eq!(
        Quantity::parse("60s").unwrap().to_ticks(TickLength::Ns),
        Ok(Some(60_000_000_000))
    );
    assert_eq!(
        Quantity::parse("1e-3s").unwrap().to_ticks(TickLength::Ps),
        Ok(Some(1_000_000_000))
    );
    assert_eq!(
        Quantity::parse("600y").unwrap().to_ticks(TickLength::Ns),
        Err(None)
    );
    assert_eq!(
        Quantity::parse("5").unwrap_err(),
        QuantityError::MissingUnit
    );
    assert_eq!(Quantity::parse("ms").unwrap_err(), QuantityError::Malformed);
    assert_eq!(
        Quantity::parse("4MiB").unwrap().to_bytes(),
        Ok(Some(4 * 1024 * 1024))
    );
    assert_eq!(Quantity::parse("0.5B").unwrap().to_bytes(), Err(true));
    assert_eq!(Quantity::parse("3eV").unwrap().unit, "eV");
    assert_eq!(
        Quantity::parse("1.2ohm").unwrap().as_pair(),
        ("1.2".to_string(), "ohm".to_string())
    );
    assert_eq!(
        Quantity::parse("0.01USD").unwrap().as_pair(),
        ("0.01".to_string(), "USD".to_string())
    );
}

fn dec(text: &str) -> Decimal {
    Decimal::parse(text).expect("a decimal")
}

#[test]
fn a_number_and_a_factor_are_rounded_once_not_three_times() {
    // RFC 012 finding F-13: the number and the factor were each rounded to an f64, and then their
    // product. `0.1 × 1e-9` came out as 1.0000000000000002e-10 and `0.3 × 1e-11` as
    // 2.9999999999999997e-12; the exact product rounded once is the f64 nearest to it, which is the
    // one the plain text of the result reads as.
    for (number, factor, expected) in [
        ("0.1", "1e-9", 1e-10f64),
        ("0.3", "1e-11", 3e-12),
        ("3.3", "1e-6", 3.3e-6),
        ("0.47", "1e-6", 4.7e-7),
        ("4.7", "1000", 4700.0),
        ("1.5", "1", 1.5),
    ] {
        let once = dec(number).times_as_f64(dec(factor)).unwrap();
        assert_eq!(once.to_bits(), expected.to_bits(), "{number} × {factor}");
    }
    // The old way differs on these, which is why the test is worth its place.
    let old = 0.1f64 * 1e-9f64;
    assert_ne!(old.to_bits(), 1e-10f64.to_bits());
}

#[test]
fn a_product_wider_than_a_mantissa_is_exact() {
    // Two 39-digit mantissas multiply to 78 digits; no step is a float.
    let a = dec("123456789012345678901234567890123456789");
    let b = dec("0.187654321098765432109876543210987654321");
    // The product, worked out apart (Python's `decimal` at 200 digits, then `float`).
    let got = a.times_as_f64(b).unwrap();
    assert_eq!(got.to_bits(), 2.3167199927145253e37f64.to_bits());
    assert_eq!(
        dec("1").times_as_f64(dec("0")).unwrap().to_bits(),
        0.0f64.to_bits(),
        "zero is zero"
    );
}

#[test]
fn infinity_and_a_nonzero_value_that_becomes_zero_are_refused() {
    // Today `1e400ohm` encodes infinity and `1e-400ohm` encodes 0 (finding F-13).
    assert_eq!(
        dec("1e400").times_as_f64(dec("1")),
        Err(FloatRefusal::Overflow)
    );
    assert_eq!(
        dec("1e200").times_as_f64(dec("1e200")),
        Err(FloatRefusal::Overflow)
    );
    assert_eq!(
        dec("1e-400").times_as_f64(dec("1")),
        Err(FloatRefusal::Underflow)
    );
    assert_eq!(
        dec("1e-200").times_as_f64(dec("1e-200")),
        Err(FloatRefusal::Underflow)
    );
    // The edges of the range are values: the largest f64 and a subnormal.
    assert_eq!(
        dec("1.7976931348623157e308").times_as_f64(dec("1")),
        Ok(f64::MAX)
    );
    assert!(dec("1e-310").times_as_f64(dec("1")).unwrap() > 0.0);
}

#[test]
fn a_sign_is_read_only_where_it_is_asked_for() {
    // RFC 004 rule 23 (amended, F-17): a `real` quantity may begin with `-`, never `+`; `-0` is
    // zero with no sign; a field that cannot be negative refuses a sign as out of range, `-0` too.
    let q = Quantity::parse_signed("-12V").unwrap();
    assert!(q.negative);
    assert_eq!(q.as_pair(), ("-12".to_string(), "V".to_string()));
    assert_eq!(q.to_string(), "-12V");
    for zero in ["-0V", "-0.0V", "-0e5V"] {
        let q = Quantity::parse_signed(zero).unwrap();
        assert!(!q.negative, "{zero}: -0 is zero, with no sign");
        assert_eq!(q.as_pair().0, "0");
    }
    assert!(!Quantity::parse_signed("5V").unwrap().negative);
    // The exponent's own sign is not the number's.
    let q = Quantity::parse_signed("-1e-3s").unwrap();
    assert!(q.negative && q.number == dec("0.001"));
    // Never `+`, never two signs, no space after it.
    for bad in ["+5V", "--5V", "-+5V", "- 5V", "-", "-V", "-e5V"] {
        assert!(Quantity::parse_signed(bad).is_err(), "{bad}");
    }
    // The unsigned readers refuse a sign as out of range, with the sign in the refusal.
    for token in ["-5ms", "-0ms", "-0.5mH"] {
        let Err(QuantityRefusal::OutOfRange { number, unit }) = Quantity::classify(token) else {
            panic!("{token} was not refused as out of range");
        };
        assert!(number.starts_with('-'), "{token}: {number}");
        assert!(!unit.is_empty());
        assert!(Quantity::parse(token).is_err(), "{token}");
    }
    // An unsigned token is read as before by both.
    assert_eq!(Quantity::parse("5ms"), Quantity::parse_signed("5ms"));
    assert_eq!(
        Quantity::classify("+5ms").err(),
        Some(QuantityRefusal::Malformed)
    );
}
