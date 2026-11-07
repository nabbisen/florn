use super::*;

#[test]
fn serialisation_rounds_up_over_the_tick() {
    // 1,500 bytes at 1 Mbit/s take 12 ms: 12,000,000 ns ticks.
    assert_eq!(
        serialisation_ticks(1500, 1_000_000, TickLength::Ns),
        12_000_000
    );
    // At 1000 Gbit/s: 12 ns.
    assert_eq!(
        serialisation_ticks(1500, 1_000_000_000_000, TickLength::Ns),
        12
    );
    assert_eq!(serialisation_ticks(1, 1_000_000_000_000, TickLength::Ns), 1);
    assert_eq!(serialisation_ticks(1500, 0, TickLength::Ns), 0);
}

#[test]
fn attributes_parse_by_rule_9() {
    let t = TickLength::Ns;
    assert_eq!(
        attr("delay", &DomainValue::Text("20ns".into()), t),
        Ok(Attr::Delay(20))
    );
    assert_eq!(
        attr("bandwidth", &DomainValue::Text("1Mbit/s".into()), t),
        Ok(Attr::Bandwidth(1_000_000))
    );
    assert_eq!(
        attr("bandwidth", &DomainValue::Text("2.5Gbit/s".into()), t),
        Ok(Attr::Bandwidth(2_500_000_000))
    );
    assert_eq!(
        attr("loss", &DomainValue::Decimal("0.5".into()), t),
        Ok(Attr::Loss(1 << 63))
    );
    assert_eq!(attr("loss", &DomainValue::Integer(0), t), Ok(Attr::Loss(0)));
    assert!(attr("loss", &DomainValue::Decimal("1.0".into()), t).is_err());
    assert!(attr("loss", &DomainValue::Integer(1), t).is_err());
    assert_eq!(
        attr("mtu", &DomainValue::Integer(9000), t),
        Ok(Attr::Mtu(9000))
    );
    assert_eq!(
        attr("enabled", &DomainValue::Boolean(false), t),
        Ok(Attr::Enabled(false))
    );
    assert_eq!(
        attr("colour", &DomainValue::Integer(1), t),
        Err("an attribute of rule 9")
    );
    assert!(attr("delay", &DomainValue::Text("1.5ns".into()), t).is_err());
}

#[test]
fn below_matches_the_sdk() {
    let mut s = Stream::new(StreamKey::derive(1, 2, "jitter"));
    for _ in 0..1000 {
        assert!(below(&mut s, 5) < 5);
    }
    assert_eq!(below(&mut s, 1), 0);
}
