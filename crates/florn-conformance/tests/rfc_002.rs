//! RFC 002 §10 under the native Host over the kit's guests. The strict-lifting faults for
//! invalid UTF-8 and an enumeration out of range are `florn-abi`'s tests (the core world carries
//! neither from a guest); the compound-activation and two-domain rows wait for a domain world
//! (Phase 2); AC-005 (`tests/ac.rs`) is the manifest row's runner half.
use florn_admission::{Admission, ClassLimits};
use florn_conformance::host::{self, faults, handled, refused, reports, texts};
use florn_conformance::scenario::{Scenario, actor};
use florn_conformance::{guests, shipped};
use florn_host::{Advance, HostError, Run, Seed};
use florn_kernel::FaultKind;
use florn_model::{HostMaxima, Manifest};
use florn_platform::FixedPlatform;

fn chatter(id: u32, label: &str) -> florn_conformance::scenario::Actor {
    actor(id, "chatter", 0).with("label", format!("\"{label}\""))
}

#[test]
fn one_artifact_admits_to_the_recorded_hash() {
    let recorded = shipped::Digests::read(&shipped::directory()).unwrap();
    let a = Admission::default()
        .admit("echo", &shipped::module_bytes("echo"), &host::kit_limits())
        .expect("admitted");
    assert_eq!(a.admitted_hex(), recorded.admitted["echo"]);
}

#[test]
fn a_list_outside_memory_faults_as_contract() {
    let s = Scenario::new("bad-pointer", 20).actor(actor(1, "hostile_bad_pointer", 1));
    let run = host::run(&s);
    assert_eq!(
        faults(&run)
            .iter()
            .map(|f| (f.0, f.1, f.2))
            .collect::<Vec<_>>(),
        vec![(1, FaultKind::Contract, 8)]
    );
}

#[test]
fn a_module_without_a_manifest_or_with_disagreeing_sections_is_refused() {
    let e = Admission::default()
        .admit("bare", &guests::bare(), &ClassLimits::defaults())
        .expect_err("refused");
    assert!(
        e.codes().contains(&"module.manifest-missing"),
        "{:?}",
        e.codes()
    );
    // The echo guest with its manifest's world changed (the section is the last copy of the
    // text in the module): a world this Host does not have.
    let mut bytes = guests::bytes("echo");
    let needle = b"world = \"florn:core/guest@0.1.0\"";
    let at = bytes
        .windows(needle.len())
        .rposition(|w| w == needle)
        .expect("the manifest");
    bytes[at..at + needle.len()].copy_from_slice(b"world = \"florn:core/other@0.1.0\"");
    let e = Admission::default()
        .admit("echo", &bytes, &ClassLimits::defaults())
        .expect_err("refused");
    assert_eq!(e.codes(), ["module.contract-unsupported"]);
    // A scenario naming another contract for the module: the sections and the scenario disagree.
    // (Two sections of one module that disagree are `florn-admission`'s own test.)
    let s = Scenario::new("contract", 10).actor(actor(1, "echo", 0));
    let text = s.text(&s.hashes()).replace(
        "contract = \"florn:core/guest@0.1.0\"",
        "contract = \"florn:other/guest@0.1.0\"",
    );
    let manifest = Manifest::parse(&text, &HostMaxima::native()).expect("the model accepts a name");
    let err = Run::new(
        host::engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        manifest,
        &s.modules(),
        Seed::Given(1),
        host::options(),
    )
    .err()
    .expect("refused");
    match err {
        HostError::Refused(e) => assert_eq!(e.codes(), ["module.contract-mismatch"]),
        other => panic!("{other}"),
    }
}

#[test]
fn parameters_are_checked_and_reach_the_guest_in_order_with_defaults() {
    let missing = Scenario::new("missing", 10).actor(actor(1, "chatter", 0));
    assert_eq!(refused(&missing), ["config.missing-parameter"]);
    let unknown = Scenario::new("unknown", 10).actor(chatter(1, "a").with("colour", "1"));
    assert_eq!(refused(&unknown), ["config.unknown-parameter"]);
    let not_whole =
        Scenario::new("not-whole", 10).actor(actor(1, "timers", 0).with("delay", "\"1.5ns\""));
    assert_eq!(refused(&not_whole), ["quantity.not-whole-ticks"]);
    // Given and defaulted values arrive in declaration order: label, period, draws, trap-at.
    let s = Scenario::new("config", 10).actor(chatter(1, "lab").with("draws", "3"));
    let run = host::run(&s);
    let r = reports(&run, 1, 2);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].0.tick, 0);
    let mut expect = vec![3, 0, 0, 0, b'l', b'a', b'b'];
    expect.extend_from_slice(&5u64.to_le_bytes());
    expect.extend_from_slice(&3u64.to_le_bytes());
    expect.extend_from_slice(&0u64.to_le_bytes());
    assert_eq!(r[0].1, expect);
}

#[test]
fn port_kinds_are_checked_against_the_declaration() {
    let s = Scenario::new("nine-ports", 10).actor(actor(1, "echo", 9));
    assert_eq!(refused(&s), ["module.port-count"]);
    let s = Scenario::new("other-type", 10).actor(actor(1, "echo", 1));
    let text = s.text(&s.hashes()).replace("core/port", "core/other");
    let manifest = Manifest::parse(&text, &HostMaxima::native()).expect("the model accepts a type");
    let err = Run::new(
        host::engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        manifest,
        &s.modules(),
        Seed::Given(1),
        host::options(),
    )
    .err()
    .expect("refused");
    match err {
        HostError::Refused(e) => assert_eq!(e.codes(), ["module.port-type-unknown"]),
        other => panic!("{other}"),
    }
}

#[test]
fn init_sets_a_timer_that_fires_at_its_delay_and_runs_in_identity_order() {
    let s = Scenario::new("init-delay", 30).actor(actor(1, "timers", 0));
    let run = host::run(&s);
    let r = reports(&run, 1, 1);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].0.tick, 10);
    assert_eq!(
        u64::from_le_bytes(r[0].1[..8].try_into().unwrap()),
        1,
        "timer 1"
    );
    assert_eq!(
        u64::from_le_bytes(r[0].1[8..16].try_into().unwrap()),
        10,
        "at tick 10"
    );
    // Declared 3, 1, 2: initialised 1, 2, 3.
    let s = Scenario::new("init-order", 5)
        .actor(chatter(3, "c"))
        .actor(actor(1, "echo", 0))
        .actor(chatter(2, "b"));
    let run = host::run(&s);
    let inits: Vec<u32> = run
        .records()
        .unwrap()
        .iter()
        .filter_map(|r| match r {
            florn_kernel::Record::Handled { key, target, .. }
                if key.tick == 0 && key.class == florn_kernel::CLASS_CONTROL =>
            {
                Some(*target)
            }
            _ => None,
        })
        .collect();
    assert_eq!(inits, vec![1, 2, 3]);
}

#[test]
fn timers_cancel_replace_and_count_against_the_quota() {
    let timers = |mode: &str| actor(1, "timers", 0).with("mode", format!("\"{mode}\""));
    let run = host::run(&Scenario::new("set-cancel", 40).actor(timers("set-cancel")));
    let r = reports(&run, 1, 1);
    assert_eq!(r.len(), 1, "the cancelled timer never fires");
    assert_eq!(r[0].0.tick, 20);
    assert_eq!(u64::from_le_bytes(r[0].1[..8].try_into().unwrap()), 2);
    let run = host::run(&Scenario::new("set-twice", 40).actor(timers("set-twice")));
    let r = reports(&run, 1, 1);
    assert_eq!(r.len(), 1, "set twice with one identity fires once");
    assert_eq!(r[0].0.tick, 15, "at the delay of the second set");
    for (count, ok) in [(16u64, true), (17, false)] {
        let s = Scenario::new("many", 60)
            .limit("pending-timers", "16")
            .actor(timers("many").with("count", count.to_string()));
        let run = host::run(&s);
        if ok {
            assert!(faults(&run).is_empty(), "{count} pending timers pass");
            assert_eq!(reports(&run, 1, 1).len(), 16);
        } else {
            let f = faults(&run);
            assert_eq!(f.len(), 1);
            assert_eq!(
                (f[0].1, f[0].2),
                (FaultKind::Quota, 6),
                "the 17th timer: quota 6"
            );
            assert!(
                reports(&run, 1, 1).is_empty(),
                "nothing of the activation committed"
            );
        }
    }
}

#[test]
fn a_fault_restores_the_stream_to_the_start_of_the_activation() {
    let trapping = Scenario::new("draws", 30).actor(chatter(1, "a").with("trap-at", "15"));
    let plain = Scenario::new("draws", 30).actor(chatter(1, "a"));
    let a = host::run(&trapping);
    let mut b = host::start(&plain);
    b.advance(Advance::ToTick(11));
    let before: Vec<_> = reports(&b, 1, 1);
    assert_eq!(before.len(), 4, "two draws at tick 5 and two at tick 10");
    assert_eq!(
        reports(&a, 1, 1),
        before,
        "the draws before the fault are the same"
    );
    assert_eq!(faults(&a).len(), 1);
    let drawn = |run: &host::Host| run.kernel().actor(1).expect("actor").stream.drawn();
    assert_eq!(
        drawn(&a),
        4,
        "the two draws of the faulted activation are rolled back"
    );
    assert_eq!(drawn(&b), 4);
    assert_eq!(
        a.kernel().actor(1).unwrap().stream,
        b.kernel().actor(1).unwrap().stream,
        "the stream is where it was before the activation"
    );
    b.run_to_end();
    assert_eq!(drawn(&b), 10, "five timers of two draws before the end");
}

#[test]
fn telemetry_channels_and_quota_are_enforced_and_text_reaches_the_log() {
    let run =
        host::run(&Scenario::new("undeclared", 10).actor(chatter(1, "a").with("channel", "9")));
    let f = faults(&run);
    assert_eq!(f.len(), 1);
    assert_eq!(
        (f[0].1, f[0].2),
        (FaultKind::Contract, 32),
        "RFC 002 rule 32"
    );
    for (bytes, ok) in [(60u64, true), (61, false)] {
        let s = Scenario::new("telemetry", 10)
            .limit("telemetry-per-activation", "\"64B\"")
            .actor(
                chatter(1, "a")
                    .with("draws", "0")
                    .with("telemetry", bytes.to_string()),
            );
        let run = host::run(&s);
        if ok {
            assert!(faults(&run).is_empty(), "{bytes} + 4 bytes of length pass");
            assert_eq!(reports(&run, 1, 3).len(), 1);
        } else {
            let f = faults(&run);
            assert_eq!(
                (f[0].1, f[0].2),
                (FaultKind::Quota, 7),
                "one above: quota 7"
            );
        }
    }
    let run =
        host::run(&Scenario::new("log", 10).actor(chatter(1, "a").with("log", "\"hello from a\"")));
    assert_eq!(texts(&run), vec![(1, "hello from a".to_string())]);
    assert!(run.log().codes().contains(&"actor.text"));
}

#[test]
fn no_ambient_access() {
    let wasi = Scenario::new("wasi", 10).actor(actor(1, "hostile_wasi", 0));
    assert!(
        refused(&wasi).contains(&"module.import-refused".to_string()),
        "{:?}",
        refused(&wasi)
    );
    let run = host::run(&Scenario::new("clock", 10).actor(actor(1, "clock", 0)));
    let f = faults(&run);
    assert_eq!(f.len(), 1);
    assert_eq!(
        f[0].1,
        FaultKind::Trap,
        "the clock read traps inside the guest"
    );
    assert!(reports(&run, 1, 1).is_empty());
    let _ = handled(&run, 1);
}
