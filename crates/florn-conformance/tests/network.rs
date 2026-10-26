//! RFC 008 §12 over the network domain: delay and serialisation, loss, jitter, link down, node
//! down, a lan, validation in one pass, the blackhole built-in, the decoder of the first rung.
//! The shipped scenarios here also run in the three browsers through the kit's page (AC-001).
use florn_admission::manifest::parse_manifest;
use florn_conformance::host::{self, handled, reports, sends};
use florn_conformance::scenario::{Scenario, builtin, router};
use florn_conformance::shipped;
use florn_control::Severity;
use florn_domain_network::{CLASS_FRAME, CLASS_LINK_STATE, NetworkDomain};
use florn_host::domain::Domain;
use florn_host::{HostError, Run};
use florn_kernel::trace::Comparison;
use florn_kernel::{Record, Stream};
use florn_model::canonical::Encoder;
use florn_model::manifest::{DomainMap, DomainValue};
use florn_platform::FixedPlatform;

fn shipped_named(name: &str) -> Scenario {
    shipped::all()
        .into_iter()
        .find(|s| s.scenario.name == name)
        .unwrap_or_else(|| panic!("shipped scenario {name}"))
        .scenario
}

fn dropped(run: &host::Host) -> Vec<String> {
    run.log()
        .entries()
        .iter()
        .filter(|e| e.notification.code == "domain.event")
        .filter(|e| {
            e.notification
                .params
                .iter()
                .any(|(k, v)| k == "event" && format!("{v:?}").contains("frame-dropped"))
        })
        .map(|e| {
            e.notification
                .params
                .iter()
                .find(|(k, _)| k == "detail")
                .map(|(_, v)| format!("{v:?}"))
                .unwrap_or_default()
        })
        .collect()
}

fn link_reports(run: &host::Host, actor: u32) -> Vec<(u64, u64, bool)> {
    reports(run, actor, 1)
        .iter()
        .map(|(i, p)| {
            (
                i.tick,
                u64::from_le_bytes(p[..8].try_into().unwrap()),
                p[8] != 0,
            )
        })
        .collect()
}

#[test]
fn delay_and_serialisation() {
    // 20 ns of delay and 12 ns of serialisation for 1,500 bytes at 1000 Gbit/s: +32 ticks.
    let run = host::run(&shipped_named("net-delay"));
    // A frame still in flight at `end` is never handled.
    let sent: Vec<_> = sends(&run, 1)
        .into_iter()
        .filter(|(s, _, _)| s.tick + 32 < 1_000)
        .collect();
    let got = handled(&run, 2)
        .into_iter()
        .filter(|k| k.class == CLASS_FRAME)
        .collect::<Vec<_>>();
    assert!(sent.len() >= 15);
    assert_eq!(got.len(), sent.len());
    for ((s, _, _), r) in sent.iter().zip(&got) {
        assert_eq!(r.tick, s.tick + 32);
    }
    // The echo comes back 32 ticks after it was sent, with the payload unchanged.
    let echoed: Vec<_> = sends(&run, 2)
        .into_iter()
        .filter(|(s, _, _)| s.tick + 32 < 1_000)
        .collect();
    let back = handled(&run, 1)
        .into_iter()
        .filter(|k| k.class == CLASS_FRAME)
        .collect::<Vec<_>>();
    assert_eq!(echoed.len(), back.len());
    for ((s, _, _), r) in echoed.iter().zip(&back) {
        assert_eq!(r.tick, s.tick + 32);
    }
    assert!(dropped(&run).is_empty());
}

#[test]
fn loss_is_drawn_from_the_links_stream_and_replays() {
    let s = shipped_named("net-loss");
    let a = host::run(&s);
    let b = host::run(&s);
    assert_eq!(a.chain_hex(), b.chain_hex());
    let sent = sends(&a, 1).len();
    assert_eq!(sent, 10_000);
    let lost = dropped(&a).len();
    assert!((900..=1_100).contains(&lost), "lost {lost} of {sent}");
    let arrived = handled(&a, 2)
        .into_iter()
        .filter(|k| k.class == CLASS_FRAME)
        .count();
    assert_eq!(arrived + lost, sent);
    // The guests' own streams are untouched (RFC 008 rule 3): no draw by any actor.
    for actor in [1, 2] {
        assert_eq!(
            a.kernel().actor(actor).unwrap().stream,
            Stream::for_actor(s.seed, actor)
        );
    }
    let record = a.run_toml();
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
    assert!(matches!(
        replay.comparison(),
        Some(Comparison::Matched { .. })
    ));
}

#[test]
fn jitter_keeps_every_arrival_within_the_window() {
    let run = host::run(&shipped_named("net-jitter"));
    let sent = sends(&run, 1);
    let got = handled(&run, 2)
        .into_iter()
        .filter(|k| k.class == CLASS_FRAME)
        .collect::<Vec<_>>();
    assert!(got.len() >= 100);
    let mut seen = std::collections::BTreeSet::new();
    for ((s, _, _), r) in sent.iter().zip(&got) {
        let extra = r.tick - s.tick - 5;
        assert!(extra <= 7, "arrival {} for a send at {}", r.tick, s.tick);
        seen.insert(extra);
    }
    assert!(seen.len() > 3, "the jitter varies: {seen:?}");
}

#[test]
fn a_link_goes_down_and_up() {
    let run = host::run(&shipped_named("net-link-down"));
    for actor in [1, 2] {
        let states: Vec<(u64, bool)> = link_reports(&run, actor)
            .into_iter()
            .map(|(t, _, up)| (t, up))
            .collect();
        assert_eq!(states, vec![(300, false), (600, true)], "actor {actor}");
        let state_events = handled(&run, actor)
            .into_iter()
            .filter(|k| k.class == CLASS_LINK_STATE)
            .count();
        assert_eq!(state_events, 2);
    }
    // `link-info` on each `on-link`: cost 7 for a1 (per-port), 1 for a2; mtu 1500; bandwidth 0.
    let info = |actor: u32| -> Vec<(u64, u64, u64)> {
        reports(&run, actor, 2)
            .iter()
            .map(|(_, p)| {
                let f = |i: usize| u64::from_le_bytes(p[i..i + 8].try_into().unwrap());
                (f(8), f(16), f(24))
            })
            .collect()
    };
    assert_eq!(info(1), vec![(7, 1500, 0), (7, 1500, 0)]);
    assert_eq!(info(2), vec![(1, 1500, 0), (1, 1500, 0)]);
    // Frames sent while the link is down are dropped with `down`; none arrives between the
    // in-flight ones (sent before 300, delay 3) and the first after 600.
    let down: Vec<_> = dropped(&run);
    assert!(!down.is_empty() && down.iter().all(|r| r.contains("down")));
    for k in handled(&run, 2) {
        if k.class == CLASS_FRAME {
            assert!(k.tick < 303 || k.tick >= 603, "a frame at {}", k.tick);
        }
    }
    // On-link reaches a guest before frames that were in flight arrive at the next microstep
    // (rule 11): the state event at 300 precedes any frame handled at 300.
    let at_300: Vec<u8> = handled(&run, 2)
        .into_iter()
        .filter(|k| k.tick == 300)
        .map(|k| k.class)
        .collect();
    assert_eq!(at_300.first(), Some(&CLASS_LINK_STATE));
}

#[test]
fn a_node_goes_down_by_intervention() {
    let s = net_pair_for_test();
    let mut run = host::start(&s);
    let node = |up: bool| -> Vec<u8> {
        let mut m = DomainMap::new();
        m.insert("kind".into(), DomainValue::Text("node".into()));
        m.insert("up".into(), DomainValue::Boolean(up));
        let mut e = Encoder::default();
        florn_model::canonical::domain_map(&mut e, &m);
        e.finish()
    };
    run.intervene_map(Some(200), 1, &node(false)).unwrap();
    run.intervene_map(Some(400), 1, &node(true)).unwrap();
    // A node that is not an interface's actor, or a malformed payload: refused at request.
    assert!(run.intervene_map(None, 99, &node(false)).is_err());
    let mut bad = DomainMap::new();
    bad.insert("kind".into(), DomainValue::Text("node".into()));
    let mut e = Encoder::default();
    florn_model::canonical::domain_map(&mut e, &bad);
    assert!(run.intervene_map(None, 1, &e.finish()).is_err());
    run.run_to_end();
    // The node's timers keep firing: it sends through the whole run, and its sends while down
    // are dropped with `down`.
    let sent = sends(&run, 1);
    assert!(sent.iter().any(|(i, _, _)| (200..400).contains(&i.tick)));
    let down = dropped(&run);
    assert!(!down.is_empty() && down.iter().all(|r| r.contains("down")));
    // Both ends see on-link(false) at 200 and on-link(true) at 400 (rule 13).
    for actor in [1, 2] {
        let states: Vec<(u64, bool)> = link_reports(&run, actor)
            .into_iter()
            .map(|(t, _, up)| (t, up))
            .collect();
        assert_eq!(states, vec![(200, false), (400, true)], "actor {actor}");
    }
    // Frames to the node while it is down are dropped at delivery: nothing handled at a2's
    // echoes... a2 echoes nothing here; a1 handles no frame between 200 and 400.
    for k in handled(&run, 1) {
        if k.class == CLASS_FRAME {
            assert!(!(200..400).contains(&k.tick));
        }
    }
    assert_eq!(run.inputs().interventions.len(), 2);
}

/// Two routers over a 3 ns link, `a1` sending every 10 ticks, `a2` echoing.
fn net_pair_for_test() -> Scenario {
    Scenario::new("node-down", 600)
        .domain("network")
        .actor(
            router(1, "net_echo", 1)
                .with("period", "10")
                .with("echo", "false"),
        )
        .actor(router(2, "net_echo", 1))
        .link("network/link", &[(1, 0), (2, 0)], &[("delay", "\"3ns\"")])
}

#[test]
fn a_lan_reaches_every_other_interface_with_its_own_draw() {
    let run = host::run(&shipped_named("net-lan"));
    let sent = sends(&run, 1).len();
    assert!(sent >= 40);
    let got2 = handled(&run, 2)
        .into_iter()
        .filter(|k| k.class == CLASS_FRAME)
        .count();
    let got3 = handled(&run, 3)
        .into_iter()
        .filter(|k| k.class == CLASS_FRAME)
        .count();
    let lost = dropped(&run).len();
    assert_eq!(got2 + got3 + lost, 2 * sent);
    assert!(got2 != got3 || lost == 0, "each receiver draws for itself");
    assert!(got2 > sent / 2 && got3 > sent / 2);
}

#[test]
fn every_code_of_rule_18_in_one_pass() {
    let s = Scenario::new("codes", 100)
        .domain("network")
        .actor(router(1, "net_echo", 1))
        .actor(router(2, "net_echo", 1))
        .actor(router(3, "net_echo", 1))
        .link(
            "network/link",
            &[(1, 0), (2, 0), (3, 0)],
            &[("colour", "1"), ("delay", "3"), ("loss", "1.5")],
        )
        .per_port((1, 0), &[("weight", "2")])
        .per_port((2, 0), &[("mtu", "0")]);
    let e = host::try_start(&s, host::options()).err().expect("refused");
    let mut codes = match e {
        HostError::Domain(ns) => ns.iter().map(|n| n.code.clone()).collect::<Vec<_>>(),
        other => panic!("{other}"),
    };
    codes.sort();
    assert_eq!(
        codes,
        vec![
            "network.attr-type",
            "network.attr-type",
            "network.attr-type",
            "network.attr-unknown",
            "network.attr-unknown",
            "network.link-ports",
        ]
    );
    // A port of another type on a network connection: `network.port-type`, beside the
    // unknown-type refusal of the Host.
    let text = s.text(&s.hashes()).replace(
        "type = \"network/interface\" }]\n\n[[actors]]\nid = 3",
        "type = \"network/eth\" }]\n\n[[actors]]\nid = 3",
    );
    let manifest = florn_model::Manifest::parse(&text, &florn_model::HostMaxima::native()).unwrap();
    let refusals = florn_host::domain::validate(
        &[Box::new(NetworkDomain::new()) as Box<dyn Domain>],
        &manifest,
    );
    assert!(refusals.iter().any(|n| n.code == "network.port-type"));
    assert!(refusals.iter().any(|n| n.code == "reference.unresolved"));
    // The module side: an unknown protocol and a channel layout the view does not read.
    let d = NetworkDomain::new();
    let m = parse_manifest(
        "florn-module = 1\nworld = \"florn:network/router@0.1.0\"\nkind = \"x\"\n[[telemetry]]\nchannel = 1\nname = \"route\"\nfields = [{ name = \"prefix\", type = \"text\" }]\n[network]\nprotocol = \"nobody\"\n",
    )
    .unwrap();
    let ns = d.validate_module("x", &m);
    let codes: Vec<&str> = ns.iter().map(|n| n.code.as_str()).collect();
    assert_eq!(
        codes,
        vec!["network.protocol-unknown", "network.telemetry-layout"]
    );
    assert!(ns.iter().all(|n| n.severity == Severity::Warning));
    let m = parse_manifest(
        "florn-module = 1\nworld = \"florn:network/router@0.1.0\"\nkind = \"x\"\n[[telemetry]]\nchannel = 1\nname = \"route\"\nfields = [{ name = \"prefix\", type = \"text\" }, { name = \"next-hop\", type = \"u64\" }, { name = \"metric\", type = \"u64\" }, { name = \"action\", type = \"text\" }]\n[network]\nprotocol = \"net-echo\"\n",
    )
    .unwrap();
    assert!(d.validate_module("x", &m).is_empty());
}

#[test]
fn the_blackhole_drops_and_reports_with_metered_zero() {
    let run = host::run(&shipped_named("net-blackhole"));
    let counts: Vec<u64> = reports(&run, 2, 1)
        .iter()
        .map(|(_, p)| u64::from_le_bytes(p[..8].try_into().unwrap()))
        .collect();
    assert_eq!(counts.len(), sends(&run, 1).len());
    assert_eq!(counts, (1..=counts.len() as u64).collect::<Vec<_>>());
    let metered: Vec<u64> = run
        .records()
        .unwrap()
        .iter()
        .filter_map(|r| match r {
            Record::Handled {
                target, metered, ..
            } if *target == 2 => Some(*metered),
            _ => None,
        })
        .collect();
    assert!(metered.len() > 1 && metered.iter().all(|m| *m == 0));
    assert!(sends(&run, 2).is_empty(), "nothing comes back");
    let _ = builtin(9, "network/blackhole", "network/interface", 1);
}

#[test]
fn the_decoder_of_the_first_rung() {
    let d = NetworkDomain::new();
    let mut frame = 7u64.to_le_bytes().to_vec();
    frame.extend_from_slice(&[0; 56]);
    assert_eq!(
        d.decode("net-echo", &frame),
        Some(vec![
            ("ordinal".to_string(), "7".to_string()),
            ("length".to_string(), "64".to_string())
        ])
    );
    assert_eq!(d.decode("net-echo", &[1, 2]), None);
    assert_eq!(d.decode("nobody", &frame), None);
}
