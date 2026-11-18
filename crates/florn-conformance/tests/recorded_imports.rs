//! PR-40, finding F-11 (RFC 002 rule 47; RFC 012 rule 11): an import of a domain's world that
//! records an effect. The kit's second domain declares `set-level(port, value)` as one: the
//! guest names a port and a value, the Host records a `send` of the value's eight canonical
//! bytes on that port, and the domain is not asked.
use florn_admission::Admission;
use florn_conformance::host;
use florn_conformance::test_domain::second::{self, ASKED};
use florn_conformance::test_domain::{self as td};
use florn_host::{Options, Run, Seed};
use florn_kernel::FaultKind;
use florn_platform::FixedPlatform;
use std::sync::atomic::Ordering;

/// A run of the pinger module over `wat` for 20 ticks, one actor with one pin.
fn run(wat: &str) -> host::Host {
    let bytes = td::guest_bytes_in(wat, "pinger", second::WORLD);
    let hash = florn_host::run::sha256_hex(&bytes);
    let text = format!(
        "florn = 1\nname = \"recorded\"\ndomains = [\"test\", \"second\"]\n\n[time]\ntick = \"1ns\"\nend = \"20ns\"\n\n[order]\ntie = \"by-identity\"\n\n[seed]\nroot = 5\n\n[limits.default]\nmemory = \"2MiB\"\n\n[modules.prober]\nsha256 = \"{hash}\"\ncontract = \"{}\"\n{}",
        second::WORLD,
        td::prober(1, 1)
    );
    let manifest = td::manifest(&text);
    let seed = manifest.seed;
    let modules = [("prober".to_string(), bytes)].into_iter().collect();
    let domains = second::domains();
    let mut admission = Admission::default();
    for w in domains.worlds() {
        admission.add_world(w);
    }
    let mut run = Run::new(
        host::engine(),
        admission,
        &mut FixedPlatform::default(),
        &domains,
        manifest,
        &modules,
        Seed::Given(seed),
        Options {
            keep_records: true,
            keep_payloads: true,
            ..Options::default()
        },
    )
    .unwrap_or_else(|e| panic!("start: {e}"));
    run.run_to_end();
    run
}

#[test]
fn a_recorded_import_produces_a_send_of_the_eight_canonical_bytes() {
    let r = run(&second::pinger_wat());
    assert!(host::faults(&r).is_empty(), "{:?}", host::faults(&r));
    let sends = host::sends(&r, 1);
    assert_eq!(sends.len(), 1, "{sends:?}");
    let (at, port, payload) = &sends[0];
    assert_eq!((at.tick, *port), (1, 0));
    assert_eq!(payload, &1.5f64.to_le_bytes().to_vec(), "little-endian f64");
    assert_eq!(payload.len(), 8);
}

#[test]
fn the_domain_is_not_asked() {
    // A void import the domain declines to answer succeeds quietly, so that a fault would not
    // show it: the domain counts the calls it is asked to answer.
    let r = run(&second::pinger_wat());
    assert!(host::faults(&r).is_empty());
    assert_eq!(host::sends(&r, 1).len(), 1);
    assert_eq!(
        ASKED.load(Ordering::SeqCst),
        0,
        "the import was recorded, not asked"
    );
}

#[test]
fn a_value_that_is_not_finite_is_the_contract_violation_of_its_rule() {
    for value in ["nan", "inf", "-inf"] {
        let r = run(&second::pinger_setting("0", value));
        let f = host::faults(&r);
        assert_eq!(f.len(), 1, "{value}: {f:?}");
        assert_eq!((f[0].1, f[0].2), (FaultKind::Contract, 12018), "{value}");
        assert!(
            host::sends(&r, 1).is_empty(),
            "{value}: nothing was committed"
        );
    }
}

#[test]
fn a_port_the_actor_does_not_have_is_refused_as_a_send_to_it_would_be() {
    let r = run(&second::pinger_setting("9", "1.5"));
    let f = host::faults(&r);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].1, FaultKind::Contract);
    assert!(host::sends(&r, 1).is_empty());
}

#[test]
fn a_negative_zero_and_a_subnormal_keep_their_bits() {
    for (text, value) in [("-0.0", -0.0f64), ("5e-324", f64::from_bits(1))] {
        let r = run(&second::pinger_setting("0", text));
        let sends = host::sends(&r, 1);
        assert_eq!(sends.len(), 1, "{text}");
        assert_eq!(sends[0].2, value.to_le_bytes().to_vec(), "{text}");
    }
}
