//! RFC 008 rule 20, the ladder's second rung: the triangle converges, breaks and reconverges
//! within the declared bound; the link break as a scheduled change and as the `link`
//! intervention have one effect and the intervention replays (J3, AC-009); routes beyond
//! `infinity` are never installed; a neighbour unheard for three periods is withdrawn; the
//! `distance-vector` frames decode in the inspector's query.
use std::collections::BTreeMap;

use florn_conformance::host::{self, reports, sends};
use florn_conformance::scenario::{Scenario, router};
use florn_conformance::shipped;
use florn_domain_network::NetworkDomain;
use florn_host::Run;
use florn_host::domain::Domain;
use florn_kernel::trace::Comparison;
use florn_model::canonical::Encoder;
use florn_model::manifest::{DomainMap, DomainValue};
use florn_platform::FixedPlatform;

const PERIOD: u64 = 100;
/// Reconvergence after a change: three periods (the expiry bound of rule 20).
const BOUND: u64 = 3 * PERIOD;

fn shipped_named(name: &str) -> Scenario {
    shipped::all()
        .into_iter()
        .find(|s| s.scenario.name == name)
        .unwrap_or_else(|| panic!("shipped scenario {name}"))
        .scenario
}

/// A `route` report: (tick, prefix, next-hop, metric, action).
fn routes(run: &host::Host, actor: u32) -> Vec<(u64, u64, u64, u64, String)> {
    reports(run, actor, 2)
        .iter()
        .map(|(i, p)| {
            let n = u32::from_le_bytes(p[..4].try_into().unwrap()) as usize;
            let prefix: u64 = std::str::from_utf8(&p[4..4 + n]).unwrap().parse().unwrap();
            let at = 4 + n;
            let next = u64::from_le_bytes(p[at..at + 8].try_into().unwrap());
            let metric = u64::from_le_bytes(p[at + 8..at + 16].try_into().unwrap());
            let m = u32::from_le_bytes(p[at + 16..at + 20].try_into().unwrap()) as usize;
            let action = std::str::from_utf8(&p[at + 20..at + 20 + m])
                .unwrap()
                .to_string();
            (i.tick, prefix, next, metric, action)
        })
        .collect()
}

/// A `neighbour` report: (tick, port, neighbour, state).
fn neighbours(run: &host::Host, actor: u32) -> Vec<(u64, u64, u64, String)> {
    reports(run, actor, 1)
        .iter()
        .map(|(i, p)| {
            let port = u64::from_le_bytes(p[..8].try_into().unwrap());
            let n = u64::from_le_bytes(p[8..16].try_into().unwrap());
            let m = u32::from_le_bytes(p[16..20].try_into().unwrap()) as usize;
            let state = std::str::from_utf8(&p[20..20 + m]).unwrap().to_string();
            (i.tick, port, n, state)
        })
        .collect()
}

/// The routing table of an actor as it stood at `tick`, replayed from its reports:
/// prefix to (next-hop, metric).
fn table_at(run: &host::Host, actor: u32, tick: u64) -> BTreeMap<u64, (u64, u64)> {
    let mut t = BTreeMap::new();
    for (at, prefix, next, metric, action) in routes(run, actor) {
        if at > tick {
            break;
        }
        if action == "withdraw" {
            t.remove(&prefix);
        } else {
            t.insert(prefix, (next, metric));
        }
    }
    t
}

#[test]
fn the_triangle_converges_breaks_and_reconverges_within_the_bound() {
    let run = host::run(&shipped_named("dv-triangle"));
    // Converged before the break: every router knows the two others one hop away.
    assert_eq!(
        table_at(&run, 1, 999),
        [(1, (0, 0)), (2, (0, 1)), (3, (1, 1))]
            .into_iter()
            .collect()
    );
    assert_eq!(
        table_at(&run, 2, 999),
        [(1, (0, 1)), (2, (0, 0)), (3, (1, 1))]
            .into_iter()
            .collect()
    );
    assert_eq!(
        table_at(&run, 3, 999),
        [(1, (1, 1)), (2, (0, 1)), (3, (0, 0))]
            .into_iter()
            .collect()
    );
    // Converged within two periods of the start.
    assert_eq!(table_at(&run, 1, 2 * PERIOD), table_at(&run, 1, 999));
    // After the break at 1000: r1 reaches r2 through r3, two hops, within the bound.
    assert_eq!(
        table_at(&run, 1, 1_000 + BOUND),
        [(1, (0, 0)), (2, (1, 2)), (3, (1, 1))]
            .into_iter()
            .collect()
    );
    assert_eq!(
        table_at(&run, 2, 1_000 + BOUND),
        [(1, (1, 2)), (2, (0, 0)), (3, (1, 1))]
            .into_iter()
            .collect()
    );
    // The table is stable between reconvergence and the repair.
    assert_eq!(table_at(&run, 1, 1_999), table_at(&run, 1, 1_000 + BOUND));
    // After the repair at 2000: one hop again, within the bound.
    assert_eq!(table_at(&run, 1, 2_000 + BOUND), table_at(&run, 1, 999));
    assert_eq!(table_at(&run, 2, 2_000 + BOUND), table_at(&run, 2, 999));
    // The neighbour on r1's port 0 went down at the break and up at the repair.
    let n1: Vec<(u64, u64, u64, String)> = neighbours(&run, 1)
        .into_iter()
        .filter(|(_, port, _, _)| *port == 0)
        .collect();
    let states: Vec<&str> = n1.iter().map(|(_, _, _, s)| s.as_str()).collect();
    assert_eq!(states, vec!["up", "down", "up"]);
    assert_eq!(n1[1].0, 1_000);
    assert!(n1[2].0 >= 2_000 && n1[2].0 <= 2_000 + BOUND);
    assert!(n1.iter().all(|(_, _, n, _)| *n == 2));
}

#[test]
fn j3_the_break_as_an_intervention_has_the_changes_effect_and_replays() {
    let changed = host::run(&shipped_named("dv-triangle"));
    let s = shipped::dv_triangle(PERIOD);
    let plain = Scenario {
        changes: Vec::new(),
        ..s
    };
    let link = |enabled: bool| -> Vec<u8> {
        let mut m = DomainMap::new();
        m.insert("kind".into(), DomainValue::Text("link".into()));
        m.insert("enabled".into(), DomainValue::Boolean(enabled));
        let mut e = Encoder::default();
        florn_model::canonical::domain_map(&mut e, &m);
        e.finish()
    };
    let mut run = host::start(&plain);
    run.intervene_map(Some(1_000), 1_000, &link(false)).unwrap();
    run.intervene_map(Some(2_000), 1_000, &link(true)).unwrap();
    run.run_to_end();
    for actor in [1, 2, 3] {
        assert_eq!(
            routes(&run, actor),
            routes(&changed, actor),
            "actor {actor}"
        );
        assert_eq!(neighbours(&run, actor), neighbours(&changed, actor));
    }
    assert_eq!(run.inputs().interventions.len(), 2);
    let record = run.run_toml();
    let mut replay = Run::replay(
        host::engine(),
        florn_admission::Admission::default(),
        &mut FixedPlatform::default(),
        &host::domains(),
        plain.manifest(),
        &plain.modules(),
        &record,
        host::options(),
    )
    .unwrap();
    replay.run_to_end();
    assert!(matches!(
        replay.comparison(),
        Some(Comparison::Matched { .. })
    ));
    assert_eq!(replay.chain_hex(), run.chain_hex());
}

#[test]
fn routes_beyond_infinity_are_never_installed() {
    let run = host::run(&shipped_named("dv-chain"));
    let t1 = table_at(&run, 1, 1_499);
    assert_eq!(
        t1,
        [(1, (0, 0)), (2, (0, 1)), (3, (0, 2))]
            .into_iter()
            .collect(),
        "r4 is three hops away, which is infinity"
    );
    let t4 = table_at(&run, 4, 1_499);
    assert_eq!(
        t4,
        [(2, (0, 2)), (3, (0, 1)), (4, (0, 0))]
            .into_iter()
            .collect()
    );
    assert!(routes(&run, 1).iter().all(|(_, _, _, m, _)| *m < 3));
}

#[test]
fn a_silent_neighbour_is_withdrawn_after_three_periods() {
    // A lan: a node going down tells no neighbour (RFC 008 implementation note), so r1 learns
    // of r2's silence by expiry alone.
    let s = Scenario::new("dv-lan", 2_000)
        .domain("network")
        .actor(router(1, "distance_vector", 1).with("period", "\"100ns\""))
        .actor(router(2, "distance_vector", 1).with("period", "\"100ns\""))
        .actor(router(3, "distance_vector", 1).with("period", "\"100ns\""))
        .link(
            "network/lan",
            &[(1, 0), (2, 0), (3, 0)],
            &[("delay", "\"1ns\"")],
        );
    let mut run = host::start(&s);
    let mut m = DomainMap::new();
    m.insert("kind".into(), DomainValue::Text("node".into()));
    m.insert("up".into(), DomainValue::Boolean(false));
    let mut e = Encoder::default();
    florn_model::canonical::domain_map(&mut e, &m);
    run.intervene_map(Some(1_000), 2, &e.finish()).unwrap();
    run.run_to_end();
    assert_eq!(table_at(&run, 1, 999).len(), 3);
    let withdrawn: Vec<u64> = routes(&run, 1)
        .into_iter()
        .filter(|(_, prefix, _, _, action)| *prefix == 2 && action == "withdraw")
        .map(|(t, ..)| t)
        .collect();
    assert_eq!(withdrawn.len(), 1);
    assert!(
        (1_000 + 3 * PERIOD..=1_000 + 4 * PERIOD).contains(&withdrawn[0]),
        "withdrawn at {}",
        withdrawn[0]
    );
    let down: Vec<u64> = neighbours(&run, 1)
        .into_iter()
        .filter(|(_, _, n, s)| *n == 2 && s == "down")
        .map(|(t, ..)| t)
        .collect();
    assert_eq!(down, vec![withdrawn[0]]);
    assert_eq!(table_at(&run, 1, 1_999).len(), 2);
    assert_eq!(table_at(&run, 3, 1_999).len(), 2);
}

#[test]
fn the_frames_decode_in_the_inspectors_query() {
    let run = host::run(&shipped_named("dv-triangle"));
    let d = NetworkDomain::new();
    let frames = sends(&run, 1);
    let (_, _, first) = &frames[0];
    assert_eq!(
        d.decode("distance-vector", first),
        Some(vec![
            ("sender".to_string(), "1".to_string()),
            ("count".to_string(), "1".to_string()),
            ("routes".to_string(), "1:0".to_string())
        ])
    );
    // Once converged, an advertisement on port 1 (to r3) carries r1 and r2, not r3 (split horizon).
    let later = frames
        .iter()
        .find(|(i, port, _)| i.tick > 2 * PERIOD && i.tick < 1_000 && *port == 1)
        .expect("a converged advertisement");
    let fields = d.decode("distance-vector", &later.2).unwrap();
    assert_eq!(fields[2], ("routes".to_string(), "1:0 2:1".to_string()));
    assert_eq!(d.decode("distance-vector", &[2, 0]), None);
}
