//! RFC 009 section 5 (checklist C27): actor swapping. Every shipped scenario through a pool of
//! the full capacity, and every one up to 20,000 records (the ladder entirely) through a pool of
//! one and of two, gives the digest and the records of the resident run (rule 13), each run
//! saying it swaps with the resident count (rule 14); a scenario beyond the
//! capacity is refused by default with the need and the capacity and runs when swapping is asked
//! for; one actor beyond the memory, or nothing resident at all, is refused even then; a
//! checkpoint of a swapping run is byte for byte the resident run's and restores in either kind
//! of run, a restore into a swapping run leaving every actor non-resident (rule 15);
//! interventions and moving back under swapping give the resident digests.
use florn_conformance::host::{self, options};
use florn_conformance::shipped;
use florn_host::{Advance, HostError, Options, Residency, Run, Seed};
use florn_model::canonical::Encoder;
use florn_model::manifest::{DomainMap, DomainValue};
use florn_platform::{Capacity, FixedPlatform};

fn swap(limit: Option<u64>) -> Options {
    Options {
        residency: Residency::Swap,
        resident_limit: limit,
        checkpoint_interval: 0,
        ..options()
    }
}

fn resident() -> Options {
    Options {
        checkpoint_interval: 0,
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

#[test]
fn every_shipped_scenario_through_a_pool_of_one_two_and_all_gives_the_resident_digest() {
    let full = FixedPlatform::default().capacity.instances - Options::RESIDENT_RESERVE;
    for s in shipped::all() {
        let name = s.scenario.name.clone();
        let mut plain = host::try_start(&s.scenario, resident()).unwrap();
        assert_eq!(plain.pool(), None);
        assert!(host::log_param(&plain, "host.swapping", "resident").is_none());
        plain.run_to_end();
        let actors = plain
            .scenario()
            .actors
            .iter()
            .filter(|a| matches!(a.kind, florn_model::ActorKind::Module(_)))
            .count() as u64;
        // A swap costs a store and a memory copy: the four race scenarios and bench-protocol,
        // with their tens of thousands of activations, take the full pool only.
        let pools: &[Option<u64>] = if plain.records().is_some_and(|r| r.len() > 20_000) {
            &[None]
        } else {
            &[Some(1), Some(2), None]
        };
        for &limit in pools {
            let mut r = host::try_start(&s.scenario, swap(limit)).unwrap();
            let pool = limit.unwrap_or(full);
            assert_eq!(r.pool(), Some(pool), "{name}");
            assert_eq!(
                host::log_param(&r, "host.swapping", "resident"),
                Some(pool.min(actors).to_string()),
                "{name}: said with the resident count"
            );
            assert_eq!(
                host::log_param(&r, "host.swapping", "actors"),
                Some(actors.to_string())
            );
            assert!(r.resident() <= pool);
            r.run_to_end();
            assert_eq!(r.chain(), plain.chain(), "{name}: pool {limit:?}");
            assert_eq!(r.records(), plain.records(), "{name}: pool {limit:?}");
            assert_eq!(host::ended(&r), host::ended(&plain));
            assert!(r.resident() <= pool, "{name}: the pool held");
            if limit == Some(1) && actors > 1 {
                assert!(r.swaps() > 0, "{name}: it swapped");
            }
        }
    }
}

fn with_capacity(instances: u64, memory_bytes: u64) -> FixedPlatform {
    FixedPlatform {
        capacity: Capacity {
            instances,
            memory_bytes,
            swap_cost_us_per_kb: None,
        },
        ..FixedPlatform::default()
    }
}

fn refusal(err: HostError) -> Vec<(String, String)> {
    match err {
        HostError::Refused(e) => {
            e.0.iter()
                .flat_map(|x| {
                    x.params()
                        .into_iter()
                        .map(move |(k, v)| (format!("{}:{k}", x.code()), format!("{v:?}")))
                })
                .collect()
        }
        other => panic!("{other}"),
    }
}

#[test]
fn beyond_the_capacity_is_refused_by_default_and_runs_when_swapping_is_asked_for() {
    let s = shipped::ospf_lab();
    let plain = host::run(&s);
    let start = |platform: &mut FixedPlatform, options: Options| {
        Run::new(
            host::engine(),
            host::admission(),
            platform,
            &host::domains(),
            s.manifest(),
            &s.modules(),
            Seed::Given(s.seed),
            options,
        )
    };
    let actors = s.manifest().actors.len() as u64;
    assert!(actors > 3);
    // Resident by default: refused with the need and the capacity.
    let params = refusal(
        start(&mut with_capacity(3, 1 << 40), resident())
            .err()
            .expect("refused"),
    );
    assert!(
        params.contains(&(
            "host.capacity-exceeded:what".into(),
            "Enum(\"instances\")".into()
        )),
        "{params:?}"
    );
    assert!(
        params.contains(&("host.capacity-exceeded:capacity".into(), "Count(3)".into())),
        "{params:?}"
    );
    // Asked for: a pool of the capacity less the reserve, the recorded digest.
    let mut r = start(&mut with_capacity(3, 1 << 40), swap(None)).unwrap();
    assert_eq!(r.pool(), Some(3 - Options::RESIDENT_RESERVE));
    r.run_to_end();
    assert_eq!(r.chain(), plain.chain());
    // One actor's limit beyond the memory: refused even then, naming the memory.
    let params = refusal(
        start(&mut with_capacity(3, 1024), swap(None))
            .err()
            .expect("refused"),
    );
    assert!(
        params.contains(&(
            "host.capacity-exceeded:what".into(),
            "Enum(\"memory\")".into()
        )),
        "{params:?}"
    );
    // Nothing can be resident: refused.
    let params = refusal(
        start(
            &mut with_capacity(Options::RESIDENT_RESERVE, 1 << 40),
            swap(None),
        )
        .err()
        .expect("refused"),
    );
    assert!(
        params.contains(&(
            "host.capacity-exceeded:what".into(),
            "Enum(\"instances\")".into()
        )),
        "{params:?}"
    );
    assert!(
        params.contains(&("host.capacity-exceeded:capacity".into(), "Count(0)".into())),
        "{params:?}"
    );
}

#[test]
fn a_checkpoint_of_a_swapping_run_is_the_resident_runs_and_restores_either_way() {
    let s = shipped::ospf_lab();
    let mut a = host::try_start(&s, resident()).unwrap();
    a.advance(Advance::ToTick(2_000));
    let mut b = host::try_start(&s, swap(Some(1))).unwrap();
    b.advance(Advance::ToTick(2_000));
    assert_eq!(b.resident(), 1);
    let ca = a.checkpoint().unwrap();
    let cb = b.checkpoint().unwrap();
    assert_eq!(
        ca.encode(),
        cb.encode(),
        "one checkpoint whatever the residency"
    );
    assert_eq!(b.resident(), 1, "nothing restored to be exported again");
    let from = |checkpoint: &florn_host::Checkpoint, options: Options| {
        Run::from_checkpoint(
            host::engine(),
            host::admission(),
            &mut FixedPlatform::default(),
            &host::domains(),
            s.manifest(),
            &s.modules(),
            checkpoint,
            options,
        )
        .unwrap()
    };
    // The swapping run's checkpoint in a resident run; the resident run's in a swapping one.
    let mut fresh = from(&cb, resident());
    assert_eq!(fresh.resident(), s.manifest().actors.len() as u64);
    fresh.run_to_end();
    let mut swapped = from(&ca, swap(Some(2)));
    assert!(swapped.resident() <= 1, "non-resident but the one measured");
    swapped.run_to_end();
    a.run_to_end();
    assert_eq!(fresh.chain(), a.chain());
    assert_eq!(swapped.chain(), a.chain());
    // In place: every actor non-resident until its activation brings it in.
    b.advance(Advance::ToTick(3_000));
    b.restore_in_place(&cb).unwrap();
    assert_eq!(b.resident(), 0);
    b.run_to_end();
    assert_eq!(b.chain(), a.chain());
    assert_eq!(b.resident(), 1);
}

#[test]
fn interventions_moving_back_and_detail_under_swapping_give_the_resident_digests() {
    let s = shipped::dv_triangle(100);
    let mut plain = host::try_start(&s, resident()).unwrap();
    plain.run_to_end();
    let mut cut_run = host::try_start(&s, resident()).unwrap();
    cut_run.advance(Advance::ToTick(1_500));
    cut_run
        .intervene_map(Some(1_500), 1001, &cut(false))
        .unwrap();
    cut_run.run_to_end();

    let mut r = host::try_start(
        &s,
        Options {
            checkpoint_interval: 100,
            ..swap(Some(1))
        },
    )
    .unwrap();
    r.advance(Advance::ToTick(1_500));
    r.intervene_map(Some(1_500), 1001, &cut(false)).unwrap();
    r.advance(Advance::ToTick(2_200));
    let window = r.trace_window(0, 40);
    assert_eq!(window, plain.records().unwrap()[..40].to_vec());
    r.go_back(1_000).unwrap();
    assert!(r.resident() <= 1);
    r.run_to_end();
    assert_eq!(r.chain(), plain.chain(), "undone");
    r.go_back(1_500).unwrap();
    r.intervene_map(Some(1_500), 1001, &cut(false)).unwrap();
    r.run_to_end();
    assert_eq!(r.chain(), cut_run.chain(), "re-applied");
    assert!(r.swaps() > 0);
}
