//! RFC 008 rule 21, the OSPF subset, and the learning lab of NET-010 (checklist C21): the
//! adjacencies form and the routes converge, taking the short way round rather than the
//! costly chord; the lab's break reroutes within the dead interval and the repair brings the
//! routes back; the break as the `link` intervention replays to a match (J3); the packets
//! decode in the inspector's query; a feature outside the subset is refused with a logged
//! reason while the router keeps running.
use std::collections::BTreeMap;

use florn_conformance::host::{self, reports, sends, texts};
use florn_conformance::scenario::{Scenario, router};
use florn_conformance::shipped;
use florn_domain_network::NetworkDomain;
use florn_host::Run;
use florn_host::domain::Domain;
use florn_kernel::trace::Comparison;
use florn_model::canonical::Encoder;
use florn_model::manifest::{DomainMap, DomainValue};
use florn_platform::FixedPlatform;

const HELLO: u64 = 50;
const DEAD: u64 = 200;

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

/// The routing table of `actor` at `tick`, as its reports up to then leave it:
/// prefix -> (next hop, metric).
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

fn lab() -> Scenario {
    shipped::ospf_lab()
}

#[test]
fn the_lab_converges_takes_the_short_way_reroutes_and_recovers() {
    let run = host::run(&lab());
    assert!(host::faults(&run).is_empty());
    // Every adjacency is full within three hellos: r1 has three (r2, r4 and the chord to r3).
    let full_r1: Vec<u64> = neighbours(&run, 1)
        .iter()
        .filter(|(at, _, _, s)| *at <= 3 * HELLO && s == "full")
        .map(|(_, _, n, _)| *n)
        .collect();
    assert_eq!(full_r1, vec![2, 3, 4], "{:?}", neighbours(&run, 1));
    // Converged before 500: every router reaches every other; r1 reaches r3 over r2 at
    // cost 2, not over the chord at 5; r2 reaches r4 at cost 2 either way.
    let t1 = table_at(&run, 1, 500);
    assert_eq!(t1.keys().copied().collect::<Vec<_>>(), vec![2, 3, 4]);
    assert_eq!(t1[&3].1, 2, "{t1:?}");
    assert_ne!(t1[&3].0, 2, "not the chord's interface (port 2)");
    for actor in 1..=4u32 {
        assert_eq!(
            table_at(&run, actor, 500).len(),
            3,
            "router {actor} reaches the others"
        );
    }
    // The break at 1500: within the dead interval and a hello, r2 reaches r3 the long way
    // (cost 3) or r1 over the chord (cost 1 + 5): the long way wins, through r1.
    let t2 = table_at(&run, 2, 1_500 + DEAD + 2 * HELLO);
    assert_eq!(t2[&3], (0, 3), "{t2:?}");
    // r1 reaches r3 over the chord now (5) rather than r4 (2): wait, 2 + 1 = 3 round the ring.
    let t1b = table_at(&run, 1, 1_500 + DEAD + 2 * HELLO);
    assert_eq!(t1b[&3].1, 2, "{t1b:?}");
    assert_eq!(t1b[&3].0, 1, "through r4 on port 1");
    // The repair at 3000: back to the short way within three hellos.
    let t2c = table_at(&run, 2, 3_000 + 3 * HELLO);
    assert_eq!(t2c[&3], (1, 1), "{t2c:?}");
}

#[test]
fn the_break_as_an_intervention_has_the_changes_effect_and_replays() {
    // The lab without its scheduled changes, the same break asked at 1500 as an intervention.
    let s = Scenario::new("ospf-intervened", 4_500)
        .domain("network")
        .actor(
            router(1, "ospf", 3)
                .with("hello", "\"50ns\"")
                .with("dead", "\"200ns\""),
        )
        .actor(
            router(2, "ospf", 2)
                .with("hello", "\"50ns\"")
                .with("dead", "\"200ns\""),
        )
        .actor(
            router(3, "ospf", 3)
                .with("hello", "\"50ns\"")
                .with("dead", "\"200ns\""),
        )
        .actor(
            router(4, "ospf", 2)
                .with("hello", "\"50ns\"")
                .with("dead", "\"200ns\""),
        )
        .link("network/link", &[(1, 0), (2, 0)], &[("delay", "\"2ns\"")])
        .link("network/link", &[(2, 1), (3, 0)], &[("delay", "\"2ns\"")])
        .link("network/link", &[(3, 1), (4, 0)], &[("delay", "\"2ns\"")])
        .link("network/link", &[(4, 1), (1, 1)], &[("delay", "\"2ns\"")])
        .link("network/link", &[(1, 2), (3, 2)], &[("delay", "\"2ns\"")])
        .per_port((1, 2), &[("cost", "5")])
        .per_port((3, 2), &[("cost", "5")]);
    let mut run = host::start(&s);
    run.advance(florn_host::Advance::ToTick(1_500));
    let cut = |enabled: bool| {
        let mut m = DomainMap::new();
        m.insert("kind".into(), DomainValue::Text("link".into()));
        m.insert("enabled".into(), DomainValue::Boolean(enabled));
        let mut e = Encoder::default();
        florn_model::canonical::domain_map(&mut e, &m);
        e.finish()
    };
    run.intervene_map(Some(1_500), 1001, &cut(false)).unwrap();
    run.advance(florn_host::Advance::ToTick(3_000));
    run.intervene_map(Some(3_000), 1001, &cut(true)).unwrap();
    run.run_to_end();
    let t2 = table_at(&run, 2, 1_500 + DEAD + 2 * HELLO);
    assert_eq!(
        t2[&3],
        (0, 3),
        "the same reroute as the lab's change: {t2:?}"
    );
    let t2c = table_at(&run, 2, 3_000 + 3 * HELLO);
    assert_eq!(t2c[&3], (1, 1));
    // J3: the record replays to a match with the interventions applied.
    let record = run.run_toml();
    let mut replay = Run::replay(
        host::engine(),
        florn_admission::Admission::default(),
        &mut FixedPlatform::default(),
        &host::domains(),
        s.manifest(),
        &s.modules(),
        &record,
        host::options(),
    )
    .unwrap();
    replay.run_to_end();
    assert!(
        matches!(replay.comparison(), Some(Comparison::Matched { .. })),
        "{:?}",
        replay.comparison()
    );
    assert_eq!(replay.chain_hex(), run.chain_hex());
}

#[test]
fn the_packets_decode_in_the_inspectors_query() {
    let run = host::run(&lab());
    let domain = NetworkDomain::new();
    let mut kinds = BTreeMap::new();
    for (_, _, payload) in sends(&run, 1) {
        let fields = domain
            .decode("ospf", &payload)
            .unwrap_or_else(|| panic!("a packet of r1 decodes: {payload:?}"));
        assert_eq!(fields[1], ("sender".to_string(), "1".to_string()));
        assert_eq!(fields[2].0, "area");
        *kinds.entry(fields[0].1.clone()).or_insert(0u32) += 1;
        match fields[0].1.as_str() {
            "hello" => assert!(fields.iter().any(|(k, _)| k == "heard")),
            "update" => {
                let lsas = fields
                    .iter()
                    .find(|(k, _)| k == "lsas")
                    .map(|(_, v)| v.clone())
                    .unwrap();
                assert!(lsas.contains('#') && lsas.contains('['), "{lsas}");
            }
            "acknowledgement" => assert!(fields.iter().any(|(k, _)| k == "acks")),
            other => panic!("{other}"),
        }
    }
    assert!(
        kinds["hello"] > 10 && kinds["update"] > 0 && kinds["acknowledgement"] > 0,
        "{kinds:?}"
    );
    assert!(domain.decode("ospf", &[1, 9, 0, 0]).is_none());
    assert!(domain.decode("ospf", b"not ospf").is_none());
}

#[test]
fn a_feature_outside_the_subset_is_refused_with_a_reason_and_the_router_keeps_running() {
    // Another area: the router says so once and stays idle, still answering its timers.
    let run = host::run(&shipped::ospf_outside());
    assert!(host::faults(&run).is_empty());
    let said: Vec<String> = texts(&run)
        .into_iter()
        .filter(|(a, _)| *a == 2)
        .map(|(_, t)| t)
        .collect();
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(
        said[0].contains("area 1") && said[0].contains("outside the subset"),
        "{said:?}"
    );
    assert!(neighbours(&run, 1).is_empty(), "r1 never gets an adjacency");
    assert!(host::handled(&run, 2).len() > 5, "r2's timers keep going");
    // A lan: a second router on one interface would need an election, outside the subset.
    let s = Scenario::new("ospf-lan", 400)
        .domain("network")
        .actor(
            router(1, "ospf", 1)
                .with("hello", "\"50ns\"")
                .with("dead", "\"200ns\""),
        )
        .actor(
            router(2, "ospf", 1)
                .with("hello", "\"50ns\"")
                .with("dead", "\"200ns\""),
        )
        .actor(
            router(3, "ospf", 1)
                .with("hello", "\"50ns\"")
                .with("dead", "\"200ns\""),
        )
        .link(
            "network/lan",
            &[(1, 0), (2, 0), (3, 0)],
            &[("delay", "\"2ns\"")],
        );
    let run = host::run(&s);
    assert!(host::faults(&run).is_empty());
    let refusals: Vec<(u32, String)> = texts(&run)
        .into_iter()
        .filter(|(_, t)| t.contains("multi-access"))
        .collect();
    assert!(!refusals.is_empty(), "{:?}", texts(&run));
    for (actor, _) in &refusals {
        // One adjacency per interface still forms; the other router is ignored, once.
        assert_eq!(refusals.iter().filter(|(a, _)| a == actor).count(), 1);
    }
    // Differing intervals: no adjacency, said.
    let s = Scenario::new("ospf-mismatch", 400)
        .domain("network")
        .actor(
            router(1, "ospf", 1)
                .with("hello", "\"50ns\"")
                .with("dead", "\"200ns\""),
        )
        .actor(
            router(2, "ospf", 1)
                .with("hello", "\"25ns\"")
                .with("dead", "\"100ns\""),
        )
        .link("network/link", &[(1, 0), (2, 0)], &[("delay", "\"2ns\"")]);
    let run = host::run(&s);
    assert!(neighbours(&run, 1).is_empty() && neighbours(&run, 2).is_empty());
    assert!(
        texts(&run).iter().any(|(_, t)| t.contains("no adjacency")),
        "{:?}",
        texts(&run)
    );
}
