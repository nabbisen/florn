//! RFC 009 section 4 (checklist C26): branching. A branch at `T` has the parent's chain at `T`
//! and names it (rules 10 and 12); the parent's record is complete and replays to a match; the
//! branch's record replays to a match without the parent (rule 12); the parent's interventions
//! at or before `T` are the branch's inputs; two branches of one run with different
//! interventions differ after `T` only; a branch begins at `T` (moving back stops there), is
//! refused within an instant, and takes nothing of a replay's remaining interventions.
use florn_conformance::host::{self, options};
use florn_conformance::scenario::Scenario;
use florn_conformance::shipped;
use florn_host::{Advance, EndReason, HostError, Options, Run, RunToml};
use florn_kernel::trace::Comparison;
use florn_model::canonical::Encoder;
use florn_model::manifest::{DomainMap, DomainValue};
use florn_platform::FixedPlatform;

fn with(interval: u64, keep_records: bool) -> Options {
    Options {
        checkpoint_interval: interval,
        checkpoint_memory: u64::MAX,
        keep_records,
        ..options()
    }
}

fn cut(enabled: bool) -> Vec<u8> {
    let mut m = DomainMap::new();
    m.insert("kind".into(), DomainValue::Text("link".into()));
    m.insert("enabled".into(), DomainValue::Boolean(enabled));
    let mut e = Encoder::default();
    florn_model::canonical::domain_map(&mut e, &m);
    e.finish()
}

/// A replay of a record on a fresh Host, compared at its end.
fn replay_of(s: &Scenario, record: &RunToml) -> Comparison {
    let mut r = Run::replay(
        host::engine(),
        host::admission(),
        &mut FixedPlatform::default(),
        &host::domains(),
        s.manifest(),
        &s.modules(),
        record,
        with(0, false),
    )
    .unwrap();
    r.run_to_end();
    r.comparison().unwrap()
}

fn matched(c: Comparison) -> bool {
    matches!(c, Comparison::Matched { .. })
}

#[test]
fn a_branch_has_the_parents_chain_at_t_names_it_and_both_records_replay_on_their_own() {
    let s = shipped::ospf_lab();
    let mut plain = host::try_start(&s, with(0, true)).unwrap();
    plain.run_to_end();

    let mut r = host::try_start(&s, with(100, true)).unwrap();
    r.advance(Advance::ToTick(1_200));
    let t = r.kernel().now();
    let chain_t = r.chain();
    let records_t = r.kernel().trace.records();
    let parent_id = r.run_id().to_string();
    assert!(r.kept().len() > 1, "checkpoints before T");
    let parent = r.branch(&mut FixedPlatform::default()).unwrap();

    // The parent's record: ended on the branch, its chain's last point at T, its trace kept.
    let parent_toml = parent.run.clone().unwrap();
    assert_eq!(parent_toml.run_id, parent_id);
    assert_eq!(parent_toml.ended, Some(EndReason::User));
    assert_eq!(parent_toml.chain.last(), Some(&(records_t, chain_t)));
    assert!(parent_toml.parent.is_none());
    assert_eq!(
        parent.trace.as_ref().map(|t| t.len() as u64),
        Some(records_t)
    );
    assert!(parent.payloads.is_none());
    assert!(
        parent
            .log
            .entries()
            .iter()
            .any(|e| e.notification.code == "run.ended")
    );
    let file = florn_host::record::write(&parent).unwrap();
    assert_eq!(florn_host::record::read(&file).unwrap().run, parent_toml);

    // The branch: the parent's chain at T, named; begun at T; nothing of the way lost.
    let p = r.parent().unwrap().clone();
    assert_eq!((p.chain, p.records, p.instant), (chain_t, records_t, t));
    assert_ne!(r.run_id(), parent_id);
    assert!(r.ended().is_none());
    assert_eq!((r.chain(), r.kernel().now()), (chain_t, t));
    assert_eq!(r.kept().len(), 1, "the branch begins at T");
    assert_eq!(r.kept()[0].records, records_t);
    let log = r.log().entries();
    assert_eq!(log[0].notification.code, "host.capacity");
    assert_eq!(log[1].notification.code, "run.branched");
    assert_eq!(
        log[1].notification.params[1].1,
        florn_control::Value::Text(parent_id.clone())
    );
    assert_eq!(log.len(), 2, "nothing of the parent's log");

    // Without interventions of its own the branch runs to the plain digest.
    r.run_to_end();
    assert_eq!(r.chain(), plain.chain());
    let branch_toml = r.run_toml();
    assert_eq!(branch_toml.parent.as_ref(), Some(&p));
    assert!(
        branch_toml.chain.contains(&(records_t, chain_t)),
        "the point T is stored in the branch's record"
    );
    let text = branch_toml.to_toml();
    assert!(text.contains("[parent]"));
    assert_eq!(RunToml::parse(&text).as_ref(), Some(&branch_toml));

    // Each record replays to a match on its own (rule 12).
    assert!(matched(replay_of(&s, &parent_toml)));
    assert!(matched(replay_of(&s, &branch_toml)));
}

#[test]
fn the_parents_interventions_at_or_before_t_are_the_branchs_inputs() {
    let s = shipped::dv_triangle(100);
    let mut reference = host::try_start(&s, with(0, true)).unwrap();
    reference.advance(Advance::ToTick(500));
    reference
        .intervene_map(Some(500), 1001, &cut(false))
        .unwrap();
    reference.run_to_end();

    let mut r = host::try_start(&s, with(0, true)).unwrap();
    r.advance(Advance::ToTick(500));
    r.intervene_map(Some(500), 1001, &cut(false)).unwrap();
    r.advance(Advance::ToTick(1_000));
    let parent = r
        .branch(&mut FixedPlatform::default())
        .unwrap()
        .run
        .unwrap();
    assert_eq!(parent.inputs.interventions.len(), 1);
    r.run_to_end();
    assert_eq!(r.chain(), reference.chain());
    let toml = r.run_toml();
    assert_eq!(toml.inputs.interventions, parent.inputs.interventions);
    assert!(matched(replay_of(&s, &toml)));
}

#[test]
fn two_branches_with_different_interventions_differ_after_t_only() {
    let s = shipped::dv_triangle(100);
    let branch_with = |at: u64| {
        let mut r = host::try_start(&s, with(0, true)).unwrap();
        r.advance(Advance::ToTick(1_000));
        let parent = r
            .branch(&mut FixedPlatform::default())
            .unwrap()
            .run
            .unwrap();
        r.intervene_map(Some(at), 1001, &cut(false)).unwrap();
        r.run_to_end();
        let records = r.records().unwrap().to_vec();
        (parent, r.run_toml(), records)
    };
    let (pa, a, ra) = branch_with(1_500);
    let (pb, b, rb) = branch_with(2_500);
    assert_eq!(pa.chain, pb.chain, "one parent");
    assert_eq!(a.parent, b.parent);
    let t = a.parent.as_ref().unwrap().records as usize;
    assert_eq!(ra[..t], rb[..t], "equal up to T");
    let first = ra.iter().zip(&rb).position(|(x, y)| x != y).unwrap();
    assert!(
        first >= t,
        "different after T only: first at {first}, T {t}"
    );
    for ((na, da), (nb, db)) in a.chain.iter().zip(&b.chain) {
        if *na as usize <= t {
            assert_eq!((na, da), (nb, db));
        }
    }
    assert_ne!(a.chain.last(), b.chain.last());
    assert!(matched(replay_of(&s, &a)));
    assert!(matched(replay_of(&s, &b)));
}

#[test]
fn a_branch_begins_at_t_is_refused_within_an_instant_and_takes_nothing_of_a_replay() {
    let s = shipped::dv_triangle(100);
    let mut r = host::try_start(&s, with(100, true)).unwrap();
    r.advance(Advance::ToTick(1_000));
    for _ in 0..50 {
        if !r.kernel().at_boundary() {
            break;
        }
        r.advance(Advance::OneEvent);
    }
    assert!(!r.kernel().at_boundary(), "within an instant");
    let id = r.run_id().to_string();
    assert_eq!(
        r.branch(&mut FixedPlatform::default()).unwrap_err(),
        HostError::MidInstant
    );
    assert_eq!(r.run_id(), id, "unchanged");
    r.advance(Advance::ToTick(1_100));
    r.branch(&mut FixedPlatform::default()).unwrap();
    let t = r.kernel().now().tick;
    assert!(matches!(r.go_back(500), Err(HostError::Malformed(_))));
    assert_eq!(r.kernel().now().tick, t);
    r.advance(Advance::ToTick(1_800));
    r.go_back(t).unwrap();
    assert_eq!(r.kernel().now().tick, t);

    // A replay's remaining interventions are the parent's inputs after T: not the branch's.
    let mut plain = host::try_start(&s, with(0, true)).unwrap();
    plain.run_to_end();
    let mut cut_run = host::try_start(&s, with(0, true)).unwrap();
    cut_run.advance(Advance::ToTick(1_500));
    cut_run
        .intervene_map(Some(1_500), 1001, &cut(false))
        .unwrap();
    cut_run.run_to_end();
    let record = cut_run.run_toml();
    let mut replay = Run::replay(
        host::engine(),
        host::admission(),
        &mut FixedPlatform::default(),
        &host::domains(),
        s.manifest(),
        &s.modules(),
        &record,
        with(0, false),
    )
    .unwrap();
    replay.advance(Advance::ToTick(1_000));
    replay.branch(&mut FixedPlatform::default()).unwrap();
    replay.run_to_end();
    assert!(replay.comparison().is_none(), "no longer a replay");
    assert_eq!(replay.chain(), plain.chain(), "the cut never applied");
    assert!(replay.run_toml().inputs.interventions.is_empty());
}
