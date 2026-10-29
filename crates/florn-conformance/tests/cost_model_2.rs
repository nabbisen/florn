//! RFC 010 (checklist C24): cost-model version 2 counts what version 1 counts. Every shipped
//! scenario reproduces its recorded digest under version 2 (rule 1); the metered work of every
//! activation equals version 1's; a function whose longest check-free path exceeds the budget
//! is refused at admission (rule 3); the hostile guests fault with the same kinds (AC-002); a
//! run recorded under version 1 replays under version 1 on a Host whose default is version 2
//! (rule 8); a checkpoint of one version is refused under the other.

use florn_admission::{Admission, ClassLimits};
use florn_conformance::host::{self, options};
use florn_conformance::shipped;
use florn_host::{Advance, Options, Run};
use florn_kernel::Record;
use florn_platform::FixedPlatform;

fn under(version: u32) -> Options {
    Options {
        cost_model: version,
        checkpoint_interval: 0,
        ..options()
    }
}

/// The metered work per activation, in order.
fn metered(run: &host::Host) -> Vec<u64> {
    run.records()
        .unwrap()
        .iter()
        .filter_map(|r| match r {
            Record::Handled { metered, .. } => Some(*metered),
            Record::Fault { metered, .. } => Some(*metered),
            _ => None,
        })
        .collect()
}

#[test]
fn every_shipped_scenario_reproduces_its_digest_under_version_2() {
    for s in shipped::all() {
        let name = s.scenario.name.clone();
        let mut v1 = host::try_start(&s.scenario, under(1)).unwrap();
        v1.run_to_end();
        let mut v2 = host::try_start(&s.scenario, under(2)).unwrap();
        v2.run_to_end();
        assert_eq!(v2.inputs().cost_model_version, 2);
        let (m1, m2) = (metered(&v1), metered(&v2));
        assert_eq!(m1.len(), m2.len(), "{name}: the activations");
        if let Some(i) = m1.iter().zip(m2.iter()).position(|(a, b)| a != b) {
            panic!(
                "{name}: activation {i} metered {} under version 1 and {} under version 2",
                m1[i], m2[i]
            );
        }
        assert_eq!(v2.chain(), v1.chain(), "{name}: the digest");
    }
}

#[test]
fn the_admitted_hash_under_each_version_is_its_own_and_stable() {
    let raw = florn_conformance::guests::bytes("echo");
    let mut a = Admission::default();
    let one = a.admit_under("echo", &raw, &host::kit_limits(), 1).unwrap();
    let two = a.admit_under("echo", &raw, &host::kit_limits(), 2).unwrap();
    assert_ne!(one.admitted_sha256, two.admitted_sha256);
    assert_eq!(one.source_sha256, two.source_sha256);
    assert_eq!(a.transformed(), 2);
    let again = a.admit_under("echo", &raw, &host::kit_limits(), 2).unwrap();
    assert_eq!(again.admitted_sha256, two.admitted_sha256);
    assert_eq!(a.transformed(), 2, "cached per version");
    assert!(two.longest_path > 0 && two.longest_path == one.longest_path);
}

#[test]
fn a_function_longer_than_the_budget_is_refused_under_version_2_only() {
    let raw = florn_conformance::guests::bytes("bench_compute");
    let mut a = Admission::default();
    let bound = a
        .admit_under("bench_compute", &raw, &host::kit_limits(), 2)
        .unwrap()
        .longest_path;
    let limits = ClassLimits {
        budget: bound - 1,
        ..host::kit_limits()
    };
    let err = a
        .admit_under("bench_compute", &raw, &limits, 2)
        .expect_err("refused");
    assert_eq!(err.codes(), vec!["module.segment-too-long"]);
    assert!(a.admit_under("bench_compute", &raw, &limits, 1).is_ok());
}

#[test]
fn a_version_1_record_replays_under_version_1_on_a_version_2_host() {
    let s = shipped::ospf_lab();
    let mut v1 = host::try_start(&s, under(1)).unwrap();
    v1.run_to_end();
    let record = v1.run_toml();
    let mut replay = Run::replay(
        host::engine(),
        host::admission(),
        &mut FixedPlatform::default(),
        &host::domains(),
        s.manifest(),
        &s.modules(),
        &record,
        under(2),
    )
    .unwrap();
    assert_eq!(replay.inputs().cost_model_version, 1);
    replay.run_to_end();
    assert!(matches!(
        replay.comparison(),
        Some(florn_kernel::trace::Comparison::Matched { .. })
    ));

    // A checkpoint of one version restores only under it.
    let mut v2 = host::try_start(&s, under(2)).unwrap();
    v2.advance(Advance::ToTick(1_000));
    assert!(v2.request_checkpoint());
    let cp = v2.kept()[0].checkpoint().clone();
    let err = v1.restore_in_place(&cp).expect_err("refused");
    assert!(matches!(
        err,
        florn_host::HostError::CheckpointMismatch {
            what: "cost-model",
            ..
        }
    ));
    let mut fresh = Run::from_checkpoint(
        host::engine(),
        host::admission(),
        &mut FixedPlatform::default(),
        &host::domains(),
        s.manifest(),
        &s.modules(),
        &cp,
        under(1),
    )
    .unwrap();
    assert_eq!(
        fresh.inputs().cost_model_version,
        2,
        "the checkpoint's version"
    );
    fresh.run_to_end();
    assert_eq!(fresh.chain(), v1.chain());
}
