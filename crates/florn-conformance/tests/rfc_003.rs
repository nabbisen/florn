//! RFC 003 §10 under the native Host over the kit's guests. The run-free rows (custom sections,
//! validation, classification by globals, the manifest schema) are `florn-admission`'s tests;
//! the same-fault-point row is AC-002, export and restore AC-006 (`tests/ac.rs`); the capacity
//! probe is the runner's test; cost-model version 2 does not exist yet.
use florn_admission::Admission;
use florn_conformance::host::{self, faults, reports};
use florn_conformance::scenario::{Scenario, actor};
use florn_conformance::{bench, guests, shipped};
use florn_host::{Advance, HostError, Run, Seed};
use florn_kernel::FaultKind;
use florn_platform::{Capacity, FixedPlatform};

#[test]
fn every_kit_module_admits_to_its_recorded_hash() {
    let recorded = shipped::Digests::read(&shipped::directory()).unwrap();
    for name in guests::SDK {
        if *name == "hostile_wasi" {
            continue;
        }
        let a = host::admission()
            .admit(name, &shipped::module_bytes(name), &host::kit_limits())
            .unwrap_or_else(|e| panic!("{name}: {:?}", e.codes()));
        assert_eq!(a.admission_version, florn_admission::ADMISSION_VERSION);
        assert_eq!(
            Some(&a.admitted_hex()),
            recorded.admitted.get(*name),
            "{name}: the admitted bytes for admission version {}",
            a.admission_version
        );
    }
}

#[test]
fn a_looping_realloc_faults_as_budget_not_as_a_host_error() {
    let s = Scenario::new("realloc-loop", 20)
        .module("realloc_loop", guests::realloc_loop())
        .actor(
            actor(1, "pinger", 1)
                .with("period", "5")
                .with("payload", "8"),
        )
        .actor(actor(2, "realloc_loop", 1))
        .connect(&[(1, 0), (2, 0)]);
    let run = host::run(&s);
    let f = faults(&run);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0], (2, FaultKind::Budget, 0, 100_000_000));
    assert_eq!(host::ended(&run), florn_host::EndReason::EndReached);
    assert!(run.reproducible(), "a guest fault, not a Host fault");
    assert!(!run.log().codes().contains(&"host.not-reproducible"));
}

#[test]
fn each_quota_faults_one_above_and_passes_one_below() {
    let chatter = || {
        actor(1, "chatter", 1)
            .with("label", "\"q\"")
            .with("draws", "0")
    };
    // effects-per-activation: each timer activation re-arms (one effect) and sends `sends`.
    for (sends, ok) in [(9u64, true), (10, false)] {
        let s = Scenario::new("effects", 10)
            .limit("effects-per-activation", "10")
            .actor(chatter().with("sends", sends.to_string()));
        let f = faults(&host::run(&s));
        if ok {
            assert!(
                f.is_empty(),
                "{sends} sends and the re-arm: 10 effects pass"
            );
        } else {
            assert_eq!(
                (f[0].1, f[0].2),
                (FaultKind::Quota, 4),
                "{sends} sends: quota 4"
            );
        }
    }
    // payload
    for (payload, ok) in [(64u64, true), (65, false)] {
        let s = Scenario::new("payload", 10)
            .limit("payload", "\"64B\"")
            .actor(
                chatter()
                    .with("sends", "1")
                    .with("payload", payload.to_string()),
            );
        let f = faults(&host::run(&s));
        if ok {
            assert!(f.is_empty(), "a payload of {payload} bytes passes");
        } else {
            assert_eq!(
                (f[0].1, f[0].2),
                (FaultKind::Quota, 5),
                "{payload} bytes: quota 5"
            );
        }
    }
    // pending-timers and telemetry-per-activation: the RFC 002 timer and telemetry tests.
}

#[test]
fn an_estimate_above_capacity_is_refused_before_anything_is_instantiated() {
    let mut s = Scenario::new("many", 10);
    for id in 1..=10_000u32 {
        s = s.actor(actor(id, "echo", 0));
    }
    let manifest = s.manifest();
    assert_eq!(manifest.actors.len(), 10_000);
    let mut platform = FixedPlatform {
        capacity: Capacity {
            instances: 1_000,
            memory_bytes: 1 << 40,
            swap_cost_us_per_kb: None,
        },
        ..Default::default()
    };
    let err = Run::new(
        host::engine(),
        Admission::default(),
        &mut platform,
        &host::domains(),
        manifest,
        &s.modules(),
        Seed::Given(1),
        host::options(),
    )
    .err()
    .expect("refused");
    match err {
        HostError::Refused(e) => assert_eq!(e.codes(), ["host.capacity-exceeded"]),
        other => panic!("{other}"),
    }
}

#[test]
fn a_checkpoint_of_another_version_is_refused_naming_the_field() {
    let s = shipped::all()
        .into_iter()
        .find(|s| s.scenario.name == "heap")
        .unwrap()
        .scenario;
    let mut run = host::start(&s);
    run.advance(Advance::ToTick(100));
    let checkpoint = run.checkpoint().expect("checkpoint");
    for (what, bump) in [("admission-version", 0u8), ("cost-model", 1)] {
        let mut other = checkpoint.clone();
        if bump == 0 {
            other.identity.admission_version += 1;
        } else {
            // A version no Host publishes (RFC 010 rule 8: version 2 is one this Host has).
            other.identity.cost_model_version += 99;
        }
        let err = Run::from_checkpoint(
            host::engine(),
            Admission::default(),
            &mut FixedPlatform::default(),
            &host::domains(),
            s.manifest(),
            &s.modules(),
            &other,
            host::options(),
        )
        .err()
        .expect("refused");
        match err {
            HostError::CheckpointMismatch { what: found, .. } => assert_eq!(found, what),
            other => panic!("{other}"),
        }
    }
}

#[test]
fn the_cost_benchmark_produces_its_figures() {
    let figures = bench::cost(20);
    // Both guests under both cost-model versions (RFC 010 rule 9), with the same units.
    assert_eq!(figures.len(), 4);
    assert_eq!(
        figures[0].units_per_activation,
        figures[1].units_per_activation
    );
    assert_eq!(
        figures[2].units_per_activation,
        figures[3].units_per_activation
    );
    for f in &figures {
        assert!(
            f.unmetered_micros.is_finite() && f.unmetered_micros > 0.0,
            "{f:?}"
        );
        assert!(
            f.metered_micros.is_finite() && f.metered_micros > 0.0,
            "{f:?}"
        );
        eprintln!(
            "{} (cost model {}): unmetered {:.1} µs, metered {:.1} µs, {:.2}×",
            f.guest,
            f.cost_model,
            f.unmetered_micros,
            f.metered_micros,
            f.ratio()
        );
    }
    assert_eq!(
        (figures[0].guest.as_str(), figures[0].cost_model),
        ("bench_protocol", 1)
    );
    assert_eq!(
        (figures[1].guest.as_str(), figures[1].cost_model),
        ("bench_protocol", 2)
    );
    assert_eq!(
        (figures[2].guest.as_str(), figures[2].cost_model),
        ("bench_compute", 1)
    );
    assert_eq!(
        (figures[3].guest.as_str(), figures[3].cost_model),
        ("bench_compute", 2)
    );
}

#[test]
fn the_nan_probe_reports_five_patterns() {
    let probes = bench::nan_probe();
    let names: Vec<&str> = probes.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["0/0", "inf-inf", "sqrt(-1)", "nan+1", "-nan*1"]);
    for (name, bits) in &probes {
        assert!(f64::from_bits(*bits).is_nan(), "{name}");
        eprintln!("{name}: {bits:#018x} on {}", std::env::consts::ARCH);
    }
    // The probe's trace is the record of the bits: the same under every engine on one architecture.
    let s = Scenario::new("probe", 10).actor(actor(1, "probe", 0));
    assert_eq!(host::run(&s).chain(), host::run(&s).chain());
    assert_eq!(reports(&host::run(&s), 1, 1).len(), 5);
}

#[test]
fn the_hash_and_capacity_benchmarks_produce_their_figures() {
    let h = bench::hash(50);
    assert_eq!(h.len(), 3);
    for f in &h {
        assert!(f.blake3_micros > 0.0 && f.sha256_micros > 0.0, "{f:?}");
        eprintln!(
            "{} bytes: blake3 {:.2} µs, sha256 {:.2} µs",
            f.bytes, f.blake3_micros, f.sha256_micros
        );
    }
    let c = bench::capacity_native(200);
    assert_eq!(c.instances, 200, "{}", c.stopped_by);
    assert!(c.micros_per_instance > 0.0);
}
