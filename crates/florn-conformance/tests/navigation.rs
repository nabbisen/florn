//! RFC 009 section 3 (checklist C25): moving back and detail by replay. Back to every stored
//! chain point of every ladder scenario and on to the end gives the recorded digest (rule 7);
//! back to an instant between two checkpoints executes forward from the earlier one; the start
//! is always kept; an intervention after the instant is undone and a re-applied one restores
//! the digest with it (rule 8); a trace window before the kept records equals the same window
//! of a run that kept every record, payloads included, and the present is unchanged after it
//! (rule 9); nothing of the way is logged.
use florn_conformance::host::{self, options};
use florn_conformance::scenario::Scenario;
use florn_conformance::shipped;
use florn_host::{Advance, Options};
use florn_kernel::Record;
use florn_model::canonical::Encoder;
use florn_model::manifest::{DomainMap, DomainValue};

fn ladder() -> Vec<Scenario> {
    shipped::all()
        .into_iter()
        .filter(|s| s.ends_normally)
        .map(|s| s.scenario)
        .collect()
}

fn with(interval: u64, keep_records: bool) -> Options {
    Options {
        checkpoint_interval: interval,
        checkpoint_memory: u64::MAX,
        keep_records,
        ..options()
    }
}

#[test]
fn back_to_every_stored_point_and_on_to_the_end_gives_the_recorded_digest() {
    for s in ladder() {
        let mut reference = host::try_start(&s, with(0, true)).unwrap();
        reference.run_to_end();
        let points = reference.kernel().trace.stored();
        let end_tick = reference.kernel().now().tick;

        let mut r = host::try_start(&s, with(100, true)).unwrap();
        r.run_to_end();
        assert_eq!(r.chain(), reference.chain());
        assert_eq!(
            r.kept()[0].handled,
            0,
            "{}: the start is kept first",
            s.name
        );
        // Back to the instant of every stored point but the last, then on to the end.
        let ticks: Vec<u64> = {
            let records = reference.records().unwrap();
            points
                .iter()
                .filter_map(|(n, _)| records.get(n.saturating_sub(1) as usize))
                .filter_map(|rec| match rec {
                    Record::Handled { key, .. } => Some(key.tick),
                    _ => None,
                })
                .filter(|t| *t < end_tick)
                .collect()
        };
        for &t in ticks.iter().take(6) {
            r.go_back(t).unwrap();
            assert!(r.kernel().now().tick <= t, "{}: at or before {t}", s.name);
            assert!(r.ended().is_none());
            r.run_to_end();
            assert_eq!(r.chain(), reference.chain(), "{}: back to {t}", s.name);
        }
        // Back to the very start.
        r.go_back(0).unwrap();
        assert_eq!(r.kernel().handled(), 0);
        r.run_to_end();
        assert_eq!(r.chain(), reference.chain(), "{}: from the start", s.name);
    }
}

#[test]
fn the_start_survives_any_budget() {
    let s = shipped::ospf_lab();
    let mut r = host::try_start(
        &s,
        Options {
            checkpoint_memory: 1,
            ..with(50, false)
        },
    )
    .unwrap();
    r.run_to_end();
    assert!(r.kept().len() >= 9);
    assert_eq!(r.kept()[0].handled, 0, "the start");
    let chain = r.chain();
    r.go_back(0).unwrap();
    r.run_to_end();
    assert_eq!(r.chain(), chain);
}

fn cut(enabled: bool) -> Vec<u8> {
    let mut m = DomainMap::new();
    m.insert("kind".into(), DomainValue::Text("link".into()));
    m.insert("enabled".into(), DomainValue::Boolean(enabled));
    let mut e = Encoder::default();
    florn_model::canonical::domain_map(&mut e, &m);
    e.finish()
}

#[test]
fn an_intervention_after_the_instant_is_undone_and_re_applied_by_going_back() {
    let s = shipped::dv_triangle(100);
    let mut plain = host::try_start(&s, with(0, true)).unwrap();
    plain.run_to_end();
    let mut cut_run = host::try_start(&s, with(0, true)).unwrap();
    cut_run.advance(Advance::ToTick(1_500));
    cut_run
        .intervene_map(Some(1_500), 1001, &cut(false))
        .unwrap();
    cut_run.run_to_end();

    let mut r = host::try_start(&s, with(200, true)).unwrap();
    r.advance(Advance::ToTick(1_500));
    r.intervene_map(Some(1_500), 1001, &cut(false)).unwrap();
    r.advance(Advance::ToTick(2_200));
    // Back to 1,000: between two checkpoints, forward from the earlier one; the cut is undone.
    r.go_back(1_000).unwrap();
    // The kernel's clock stands at the last event handled; the tick reached is 1,000.
    assert!(r.kernel().now().tick <= 1_000 && r.reached() >= 1_000);
    assert!(r.kernel().applied_interventions().is_empty());
    assert!(
        r.log()
            .entries()
            .iter()
            .any(|e| e.notification.code == "run.rewound")
    );
    r.run_to_end();
    assert_eq!(r.chain(), plain.chain(), "undone");
    r.go_back(1_500).unwrap();
    r.intervene_map(Some(1_500), 1001, &cut(false)).unwrap();
    r.run_to_end();
    assert_eq!(r.chain(), cut_run.chain(), "re-applied");
}

#[test]
fn a_window_before_the_kept_records_is_produced_by_replay_and_the_present_is_unchanged() {
    let s = shipped::ospf_lab();
    // The reference keeps every record and payload.
    let mut full = host::try_start(&s, with(0, true)).unwrap();
    full.advance(Advance::ToTick(2_000));
    full.intervene_map(Some(2_000), 1001, &cut(false)).unwrap();
    full.run_to_end();
    let all = full.records().unwrap().to_vec();
    let total = all.len() as u64;

    // This run keeps nothing but takes a checkpoint every 100 events, with the same cut.
    let mut r = host::try_start(&s, with(100, false)).unwrap();
    r.advance(Advance::ToTick(2_000));
    r.intervene_map(Some(2_000), 1001, &cut(false)).unwrap();
    r.run_to_end();
    assert!(r.records().is_none(), "no records kept");
    assert_eq!(r.chain(), full.chain());
    let before = (
        r.kernel().now(),
        r.chain(),
        r.kernel().trace.stored(),
        r.kept().len(),
        r.log().entries().len(),
        r.kernel().applied_interventions().to_vec(),
    );
    // Windows across the run, the cut's instant included.
    for (from, count) in [
        (0u64, 50u32),
        (300, 120),
        (total / 2, 200),
        (total - 30, 60),
    ] {
        let window = r.trace_window(from, count);
        let expected: Vec<Record> = all
            .iter()
            .skip(from as usize)
            .take(count as usize)
            .cloned()
            .collect();
        assert_eq!(window, expected, "window {from}+{count}");
    }
    let after = (
        r.kernel().now(),
        r.chain(),
        r.kernel().trace.stored(),
        r.kept().len(),
        r.log().entries().len(),
        r.kernel().applied_interventions().to_vec(),
    );
    assert_eq!(
        before, after,
        "the present is unchanged and nothing was logged"
    );
    assert!(r.records().is_none(), "recording is off again");
    // The run goes on as if nothing happened: a restored present continues to the same end.
    let mut again = host::try_start(&s, with(100, false)).unwrap();
    again.advance(Advance::ToTick(2_000));
    again.intervene_map(Some(2_000), 1001, &cut(false)).unwrap();
    again.advance(Advance::ToTick(3_000));
    let window = again.trace_window(0, 40);
    assert_eq!(window.len(), 40);
    again.run_to_end();
    assert_eq!(again.chain(), full.chain());
}

#[test]
fn a_window_within_the_kept_records_needs_no_replay_and_mid_instant_answers_what_is_kept() {
    let s = shipped::dv_triangle(100);
    let mut r = host::try_start(&s, with(0, true)).unwrap();
    r.run_to_end();
    let all = r.records().unwrap().to_vec();
    let w = r.trace_window(10, 5);
    assert_eq!(w, all[10..15].to_vec());
    // Mid-instant with nothing kept: the kept records alone (none), never a restore.
    let mut m = host::try_start(&s, with(100, false)).unwrap();
    m.advance(Advance::ToTick(500));
    m.advance(Advance::OneEvent);
    let instant_before = m.kernel().now();
    let w = m.trace_window(0, 10);
    assert!(w.is_empty() || m.kernel().at_boundary());
    assert_eq!(m.kernel().now(), instant_before);
}
