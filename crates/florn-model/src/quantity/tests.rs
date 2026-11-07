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
