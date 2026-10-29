//! RFC 009 section 2 (checklist C23): checkpoints are observation (the digest of a run with
//! checkpoints, exports and restores equals the digest without); a checkpoint of every ladder
//! scenario, taken at half its handled events, restores through its file into a fresh Host and
//! in place, to the recorded digest; the kept set follows the interval and the budget; a file
//! over a maximum is refused; the interventions after a restored instant are undone and a
//! replay re-applies them.
use florn_conformance::host::{self, options};
use florn_conformance::scenario::Scenario;
use florn_conformance::shipped;
use florn_host::{Advance, Checkpoint, Options, Run};
use florn_model::canonical::Encoder;
use florn_model::manifest::{DomainMap, DomainValue};

fn ladder() -> Vec<Scenario> {
    shipped::all()
        .into_iter()
        .filter(|s| s.ends_normally)
        .map(|s| s.scenario)
        .collect()
}

fn with_checkpoints(interval: u64, memory: u64) -> Options {
    Options {
        checkpoint_interval: interval,
        checkpoint_memory: memory,
        ..options()
    }
}

fn no_checkpoints() -> Options {
    with_checkpoints(0, u64::MAX)
}

fn handled_of(s: &Scenario) -> u64 {
    let mut r = host::try_start(s, no_checkpoints()).unwrap();
    r.run_to_end();
    r.kernel().handled()
}

#[test]
fn a_checkpoint_of_every_ladder_scenario_restores_to_the_recorded_digest() {
    for s in ladder() {
        let mut reference = host::try_start(&s, no_checkpoints()).unwrap();
        reference.run_to_end();
        let half = reference.kernel().handled() / 2;

        // Taken at the first boundary after half the events, through its file.
        let mut a = host::try_start(&s, no_checkpoints()).unwrap();
        while a.kernel().handled() < half {
            a.advance(Advance::OneEvent);
        }
        assert!(
            a.request_checkpoint() || {
                a.advance(Advance::OneInstant);
                a.kept().len() == 2
            }
        );
        let kept = a.kept().last().unwrap();
        let bytes = kept.checkpoint().encode();
        let decoded = Checkpoint::decode_file(&bytes, u64::MAX).unwrap();
        assert_eq!(
            &decoded,
            kept.checkpoint(),
            "{}: encodes and decodes",
            s.name
        );
        assert_eq!(kept.handled, decoded.kernel.handled);

        // In a fresh Host (a new process would read the same bytes).
        let mut b = Run::from_checkpoint(
            host::engine(),
            host::admission(),
            &mut florn_platform::FixedPlatform::default(),
            &host::domains(),
            s.manifest(),
            &s.modules(),
            &decoded,
            no_checkpoints(),
        )
        .unwrap();
        b.run_to_end();
        assert_eq!(b.chain(), reference.chain(), "{}: a fresh Host", s.name);

        // In place, after the run went on to its end: the trace after the instant is discarded
        // and the continuation gives the same digest (RFC 001 rule 52, RFC 009 rule 7).
        a.run_to_end();
        assert_eq!(a.chain(), reference.chain());
        a.restore_kept(1).unwrap();
        assert_eq!(a.kernel().trace.records(), decoded.kernel.trace.records);
        assert_eq!(a.kept().len(), 2, "the kept set ends at the restored one");
        a.run_to_end();
        assert_eq!(a.chain(), reference.chain(), "{}: in place", s.name);
        assert_eq!(
            a.kernel().trace.stored(),
            reference.kernel().trace.stored(),
            "{}: the stored points after a restore",
            s.name
        );
    }
}

#[test]
fn checkpoints_are_observation() {
    let s = shipped::ospf_lab();
    let mut reference = host::try_start(&s, no_checkpoints()).unwrap();
    reference.run_to_end();
    let total = reference.kernel().handled();

    // One every 100 events, exported three times, restored three times on the way.
    let mut r = host::try_start(&s, with_checkpoints(100, u64::MAX)).unwrap();
    let mut exports = 0;
    let mut restores = 0;
    let mut step = 0u64;
    while r.ended().is_none() {
        r.advance(Advance::OneInstant);
        step += 1;
        if step.is_multiple_of(7) && !r.kept().is_empty() {
            let bytes = r.kept()[r.kept().len() - 1].checkpoint().encode();
            assert!(Checkpoint::decode(&bytes).is_some());
            exports += 1;
        }
        if step.is_multiple_of(11) && r.kept().len() > 1 && restores < 3 {
            let index = r.kept().len() - 2;
            r.restore_kept(index).unwrap();
            restores += 1;
        }
    }
    assert!(
        exports >= 3 && restores == 3,
        "{exports} exports, {restores} restores"
    );
    assert!(r.kept().len() > 8, "{} kept", r.kept().len());
    assert_eq!(r.kernel().handled(), total);
    assert_eq!(r.chain(), reference.chain());
    assert_eq!(r.kernel().trace.stored(), reference.kernel().trace.stored());
}

#[test]
fn the_kept_set_follows_the_interval_and_the_budget() {
    let s = shipped::ospf_lab();
    let total = handled_of(&s);

    // Without a budget: one checkpoint per interval of handled events, at boundaries.
    let mut r = host::try_start(&s, with_checkpoints(50, u64::MAX)).unwrap();
    r.run_to_end();
    let n = r.kept().len() as u64;
    assert!(
        n >= total / 50 / 2 && n <= total / 50 + 1,
        "{n} kept for {total} events"
    );
    let mut last = 0;
    for k in r.kept() {
        assert!(
            k.handled >= last + 50 || last == 0,
            "{} after {last}",
            k.handled
        );
        last = k.handled;
    }

    // Under a budget of a few checkpoints: the newest eight survive; the rest is thinned,
    // dense at the end and sparse at the start.
    let one = r.kept()[1].bytes;
    let mut t = host::try_start(&s, with_checkpoints(50, one * 12)).unwrap();
    t.run_to_end();
    let kept = t.kept();
    assert!(
        kept.len() >= 8 && (kept.len() as u64) < n,
        "{} kept under the budget",
        kept.len()
    );
    let newest: Vec<u64> = kept.iter().rev().take(8).map(|k| k.handled).collect();
    let all: Vec<u64> = r.kept().iter().rev().take(8).map(|k| k.handled).collect();
    assert_eq!(
        newest, all,
        "the newest eight are the same as without a budget"
    );
    if kept.len() > 9 {
        let first_gap = kept[1].handled - kept[0].handled;
        let last_gap = kept[kept.len() - 1].handled - kept[kept.len() - 2].handled;
        assert!(
            first_gap >= last_gap,
            "sparse at the start ({first_gap}) and dense at the end ({last_gap})"
        );
    }
    let budget_notices = t
        .log()
        .entries()
        .iter()
        .filter(|e| e.notification.code == "checkpoint.budget")
        .count();
    assert!(budget_notices > 0, "the thinning is said");
    assert_eq!(t.chain(), r.chain());
}

#[test]
fn a_file_over_a_maximum_is_refused_and_a_corrupt_one_is_unreadable() {
    let s = shipped::dv_triangle(100);
    let mut r = host::try_start(&s, no_checkpoints()).unwrap();
    r.advance(Advance::ToTick(500));
    assert!(r.request_checkpoint());
    let bytes = r.kept()[1].checkpoint().encode();
    assert_eq!(
        Checkpoint::decode_file(&bytes, bytes.len() as u64 - 1).err(),
        Some("file.too-large")
    );
    assert!(Checkpoint::decode_file(&bytes, bytes.len() as u64).is_ok());
    let mut truncated = bytes.clone();
    truncated.truncate(bytes.len() / 2);
    assert_eq!(
        Checkpoint::decode_file(&truncated, u64::MAX).err(),
        Some("file.unreadable")
    );
    // A count that claims more than the file holds is refused by the decoder before anything
    // is allocated: the version, the scenario hash, then a module list of four billion.
    let mut lying = vec![florn_host::snapshot::CHECKPOINT_VERSION];
    lying.extend_from_slice(&[0u8; 32]);
    lying.extend_from_slice(&u32::MAX.to_le_bytes());
    let before = std::time::Instant::now();
    assert_eq!(
        Checkpoint::decode_file(&lying, u64::MAX).err(),
        Some("file.unreadable")
    );
    assert!(
        before.elapsed().as_millis() < 100,
        "refused in bounded time"
    );

    // Another scenario's checkpoint is refused with the field named, before anything restores.
    let other = shipped::dv_triangle(200);
    let mut o = host::try_start(&other, no_checkpoints()).unwrap();
    let err = o
        .restore_in_place(r.kept()[1].checkpoint())
        .expect_err("refused");
    assert!(matches!(
        err,
        florn_host::HostError::CheckpointMismatch {
            what: "scenario-hash",
            ..
        }
    ));
}

/// The `link` intervention of RFC 008 rule 15, encoded as the control interface carries it.
fn cut(enabled: bool) -> Vec<u8> {
    let mut m = DomainMap::new();
    m.insert("kind".into(), DomainValue::Text("link".into()));
    m.insert("enabled".into(), DomainValue::Boolean(enabled));
    let mut e = Encoder::default();
    florn_model::canonical::domain_map(&mut e, &m);
    e.finish()
}

#[test]
fn interventions_after_a_restored_instant_are_undone_and_a_replay_reapplies_them() {
    let s = shipped::dv_triangle(100);
    // The reference without an intervention and the reference with one at 1,500.
    let mut plain = host::try_start(&s, no_checkpoints()).unwrap();
    plain.run_to_end();
    let mut cut_run = host::try_start(&s, no_checkpoints()).unwrap();
    cut_run.advance(Advance::ToTick(1_500));
    cut_run
        .intervene_map(Some(1_500), 1001, &cut(false))
        .unwrap();
    cut_run.run_to_end();
    assert_ne!(cut_run.chain(), plain.chain());

    // A checkpoint at 1,000, the intervention at 1,500, then back to 1,000: the intervention is
    // undone and the run ends as the plain one; the Host says so.
    let mut r = host::try_start(&s, no_checkpoints()).unwrap();
    r.advance(Advance::ToTick(1_000));
    assert!(r.request_checkpoint());
    r.advance(Advance::ToTick(1_500));
    r.intervene_map(Some(1_500), 1001, &cut(false)).unwrap();
    r.advance(Advance::ToTick(2_000));
    assert_eq!(r.kernel().applied_interventions().len(), 1);
    r.restore_kept(1).unwrap();
    assert!(r.kernel().applied_interventions().is_empty());
    let rewound = r
        .log()
        .entries()
        .iter()
        .find(|e| e.notification.code == "run.rewound")
        .expect("run.rewound");
    assert_eq!(
        rewound.notification.params[1].1,
        florn_control::Value::Count(1)
    );
    r.run_to_end();
    assert_eq!(r.chain(), plain.chain(), "undone");

    // Re-applied at its instant after another restore: the digest with it.
    r.restore_kept(1).unwrap();
    r.advance(Advance::ToTick(1_500));
    r.intervene_map(Some(1_500), 1001, &cut(false)).unwrap();
    r.run_to_end();
    assert_eq!(r.chain(), cut_run.chain(), "re-applied");

    // A replay of the cut run, moved back to 1,000 on the way, re-applies the recorded
    // intervention at its instant and matches.
    let record = cut_run.run_toml();
    let mut replay = Run::replay(
        host::engine(),
        host::admission(),
        &mut florn_platform::FixedPlatform::default(),
        &host::domains(),
        s.manifest(),
        &s.modules(),
        &record,
        no_checkpoints(),
    )
    .unwrap();
    replay.advance(Advance::ToTick(1_000));
    assert!(replay.request_checkpoint());
    replay.advance(Advance::ToTick(2_000));
    replay.restore_kept(1).unwrap();
    replay.run_to_end();
    assert_eq!(replay.chain(), cut_run.chain(), "a replay re-applies");
    assert!(matches!(
        replay.comparison(),
        Some(florn_kernel::trace::Comparison::Matched { .. })
    ));
}
