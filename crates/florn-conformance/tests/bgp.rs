//! RFC 011, the BGP subset and its learning lab (checklist C29): the session machine's
//! transitions (rule 2) driven by frames, by `on-link` and by the hold timer; the lab's winner
//! by local preference, through AS 3 after the break and back after the repair, each within
//! `hold + 2 × keepalive` of the event (rules 3 to 5); the tie-breakers in their order (rule 4);
//! a looped path dropped and counted; every refusal logged with the feature named while the
//! router keeps running (rule 6); every frame of the lab decoding and a truncated one not
//! (rule 7); the two shipped scenarios' digests are the kit's (rules 9 and 10).
use std::collections::BTreeMap;

use florn_conformance::host::{self, reports, sends, texts};
use florn_conformance::scenario::{Scenario, router};
use florn_conformance::shipped;
use florn_domain_network::NetworkDomain;
use florn_host::domain::Domain;

const HOLD: u64 = 90;
const KEEPALIVE: u64 = 30;
/// The bound of RFC 011 section 6: a decision follows its event within this.
const WITHIN: u64 = HOLD + 2 * KEEPALIVE;

fn text_at(p: &[u8], at: usize) -> (String, usize) {
    let n = u32::from_le_bytes(p[at..at + 4].try_into().unwrap()) as usize;
    (
        String::from_utf8_lossy(&p[at + 4..at + 4 + n]).into_owned(),
        at + 4 + n,
    )
}
fn u64_at(p: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(p[at..at + 8].try_into().unwrap())
}

/// A `peer` report: (tick, port, state, routes in the Adj-RIB-In).
fn peers(run: &host::Host, actor: u32) -> Vec<(u64, u64, String, u64)> {
    reports(run, actor, 1)
        .iter()
        .map(|(i, p)| {
            let port = u64_at(p, 0);
            let (state, at) = text_at(p, 8);
            (i.tick, port, state, u64_at(p, at))
        })
        .collect()
}

/// A `route` report: (tick, prefix, next hop, path length, local-pref, med, winner).
#[allow(clippy::type_complexity)]
fn routes(run: &host::Host, actor: u32) -> Vec<(u64, String, u64, u64, u64, u64, String)> {
    reports(run, actor, 2)
        .iter()
        .map(|(i, p)| {
            let (prefix, at) = text_at(p, 0);
            let (winner, _) = text_at(p, at + 32);
            (
                i.tick,
                prefix,
                u64_at(p, at),
                u64_at(p, at + 8),
                u64_at(p, at + 16),
                u64_at(p, at + 24),
                winner,
            )
        })
        .collect()
}

/// The states one port of a router went through, in order (a report that only changed the
/// Adj-RIB-In's count repeats the state and is folded).
fn states(run: &host::Host, actor: u32, port: u64) -> Vec<(u64, String)> {
    let mut out: Vec<(u64, String)> = Vec::new();
    for (t, p, s, _) in peers(run, actor) {
        if p == port && out.last().is_none_or(|(_, last)| *last != s) {
            out.push((t, s));
        }
    }
    out
}

/// The winner of `prefix` at `actor` as its reports leave it at `tick`: (next hop, local-pref).
fn winner_at(run: &host::Host, actor: u32, prefix: &str, tick: u64) -> Option<(u64, u64)> {
    let mut w = None;
    for (at, p, next, _, lp, _, winner) in routes(run, actor) {
        if at > tick || p != prefix {
            continue;
        }
        w = if winner == "withdrawn" {
            None
        } else {
            Some((next, lp))
        };
    }
    w
}

fn bgp(id: u32, interfaces: u32, as_: u32) -> florn_conformance::scenario::Actor {
    router(id, "bgp", interfaces)
        .with("as", as_.to_string())
        .with("hold", format!("\"{HOLD}ns\""))
        .with("keepalive", format!("\"{KEEPALIVE}ns\""))
}

const PREFIX: &str = "10.4.0.0/16";

#[test]
fn the_lab_establishes_chooses_by_local_preference_reroutes_after_the_break_and_recovers() {
    let run = host::run(&shipped::bgp_lab());
    assert!(host::faults(&run).is_empty());
    // Rule 2 on r1's port to r2: the four states in order, idle at the break, up again after.
    let s: Vec<String> = states(&run, 1, 0).into_iter().map(|(_, s)| s).collect();
    assert_eq!(
        s,
        [
            "open-sent",
            "open-confirm",
            "established",
            "idle",
            "open-sent",
            "open-confirm",
            "established"
        ],
        "{:?}",
        states(&run, 1, 0)
    );
    let t = |state: &str, from: u64| {
        states(&run, 1, 0)
            .into_iter()
            .find(|(at, s)| *at >= from && s == state)
            .map(|(at, _)| at)
            .unwrap()
    };
    assert!(
        t("established", 0) <= 3 * KEEPALIVE,
        "established within three keepalives"
    );
    assert_eq!(t("idle", 1), 1_000, "idle the instant the link goes down");
    assert!(
        t("established", 2_000) <= 2_000 + WITHIN,
        "up again after the repair"
    );
    // Rules 3 to 5: r1's winner is through AS 2 (port 0) by local preference 200 although both
    // paths have one AS between; through AS 3 (port 1) after the break; back after the repair.
    assert_eq!(winner_at(&run, 1, PREFIX, 999), Some((0, 200)));
    assert_eq!(winner_at(&run, 1, PREFIX, 1_000 + WITHIN), Some((1, 100)));
    assert_eq!(winner_at(&run, 1, PREFIX, 2_000 + WITHIN), Some((0, 200)));
    let r1: Vec<_> = routes(&run, 1);
    assert!(
        r1.iter().all(|(_, _, _, len, _, _, _)| *len == 2),
        "one AS between r1 and AS 4: {r1:?}"
    );
    // r4 originates the prefix and learns nothing it originated.
    let r4 = routes(&run, 4);
    assert_eq!(r4.len(), 1, "{r4:?}");
    assert_eq!(r4[0].6, "originated");
    assert!(
        peers(&run, 4).iter().all(|(_, _, _, n)| *n == 0),
        "{:?}",
        peers(&run, 4)
    );
    // r2 and r3 take the one-AS path from r4, with the MED r4 set towards each.
    assert_eq!(routes(&run, 2)[0].5, 10);
    assert_eq!(routes(&run, 3)[0].5, 20);
    // Every text a router logged is the hold timer's or none: nothing refused in the lab.
    assert!(texts(&run).is_empty(), "{:?}", texts(&run));
}

#[test]
fn the_hold_timer_expires_hold_ticks_after_the_last_valid_frame_on_both_sides() {
    // r2 sends a keepalive every 200 ticks, beyond r1's hold of 90: r1's hold timer expires
    // at the first timer tick after 90 ticks of silence, then r2's after r1 falls silent.
    let s = Scenario::new("bgp-hold", 900)
        .domain("network")
        .actor(bgp(1, 1, 1).with("networks", "\"10.1.0.0/16\""))
        .actor(
            router(2, "bgp", 1)
                .with("as", "2")
                .with("hold", format!("\"{HOLD}ns\""))
                .with("keepalive", "\"200ns\""),
        )
        .link("network/link", &[(1, 0), (2, 0)], &[("delay", "\"2ns\"")]);
    let run = host::run(&s);
    let r1 = states(&run, 1, 0);
    let established = r1.iter().find(|(_, s)| s == "established").unwrap().0;
    let idle = r1.iter().find(|(_, s)| s == "idle").unwrap().0;
    // The last frame from r2 is its update right after establishing.
    let last_frame = established + 2;
    assert!(
        idle >= last_frame + HOLD && idle < last_frame + HOLD + KEEPALIVE,
        "idle at {idle}, last frame at {last_frame}"
    );
    assert!(
        texts(&run)
            .iter()
            .any(|(a, t)| *a == 1 && t.contains("hold timer") && t.contains("90 ticks")),
        "{:?}",
        texts(&run)
    );
    // r2 learned the prefix and withdrew it when its own hold timer expired.
    let r2 = routes(&run, 2);
    assert_eq!(r2.len(), 2, "{r2:?}");
    assert_eq!((r2[0].6.as_str(), r2[1].6.as_str()), ("yes", "withdrawn"));
    assert!(states(&run, 2, 0).iter().any(|(_, s)| s == "idle"));
}

/// r1 at the apex of two paths to AS 4's prefix, with the extra parameters given to r1 and to
/// the two routers between; the winner's port at r1 once everything settled.
fn apex(
    r1_extra: &[(&'static str, &str)],
    r2_extra: &[(&'static str, &str)],
    r3_extra: &[(&'static str, &str)],
    via_r3_longer: bool,
) -> u64 {
    let mut r1 = bgp(1, 2, 1);
    for (k, v) in r1_extra {
        r1 = r1.with(k, v.to_string());
    }
    let mut r2 = bgp(2, 2, 2);
    for (k, v) in r2_extra {
        r2 = r2.with(k, v.to_string());
    }
    let mut r3 = bgp(3, 2, 3);
    for (k, v) in r3_extra {
        r3 = r3.with(k, v.to_string());
    }
    let mut s = Scenario::new("bgp-tie", 600)
        .domain("network")
        .actor(r1)
        .actor(r2)
        .actor(r3)
        .actor(bgp(4, 2, 4).with("networks", format!("\"{PREFIX}\"")))
        .link("network/link", &[(1, 0), (2, 0)], &[("delay", "\"2ns\"")])
        .link("network/link", &[(1, 1), (3, 0)], &[("delay", "\"2ns\"")])
        .link("network/link", &[(2, 1), (4, 0)], &[("delay", "\"2ns\"")]);
    if via_r3_longer {
        // One more autonomous system between r3 and r4.
        s = s
            .actor(bgp(5, 2, 5))
            .link("network/link", &[(3, 1), (5, 0)], &[("delay", "\"2ns\"")])
            .link("network/link", &[(5, 1), (4, 1)], &[("delay", "\"2ns\"")]);
    } else {
        s = s.link("network/link", &[(3, 1), (4, 1)], &[("delay", "\"2ns\"")]);
    }
    let run = host::run(&s);
    assert!(host::faults(&run).is_empty());
    winner_at(&run, 1, PREFIX, 600).expect("a winner").0
}

#[test]
fn the_decision_breaks_ties_by_path_length_then_med_then_router_id() {
    // Equal local preference: the shorter AS path (through r2) wins over the longer (r3, r5).
    assert_eq!(apex(&[], &[], &[], true), 0, "AS-path length");
    // Equal lengths: the lower MED wins; r4 sets it per port, here lower towards r3.
    let med_r4 = |text: &str| -> u64 {
        let s = Scenario::new("bgp-med", 600)
            .domain("network")
            .actor(bgp(1, 2, 1))
            .actor(bgp(2, 2, 2))
            .actor(bgp(3, 2, 3))
            .actor(
                bgp(4, 2, 4)
                    .with("networks", format!("\"{PREFIX}\""))
                    .with("med", format!("\"{text}\"")),
            )
            .link("network/link", &[(1, 0), (2, 0)], &[("delay", "\"2ns\"")])
            .link("network/link", &[(1, 1), (3, 0)], &[("delay", "\"2ns\"")])
            .link("network/link", &[(2, 1), (4, 0)], &[("delay", "\"2ns\"")])
            .link("network/link", &[(3, 1), (4, 1)], &[("delay", "\"2ns\"")]);
        let run = host::run(&s);
        winner_at(&run, 1, PREFIX, 600).expect("a winner").0
    };
    assert_eq!(med_r4("0=10,1=20"), 0, "MED 10 towards r2");
    assert_eq!(med_r4("0=20,1=10"), 1, "MED 10 towards r3");
    // Equal MEDs: the lower router id of the advertising peer; r3 given the lower id.
    assert_eq!(
        apex(&[], &[("router-id", "9")], &[("router-id", "7")], false),
        1,
        "router id"
    );
    assert_eq!(
        apex(&[], &[("router-id", "7")], &[("router-id", "9")], false),
        0,
        "router id"
    );
    // Local preference comes first: the longer path wins when r1 prefers that port.
    assert_eq!(
        apex(&[("local-pref", "\"0=100,1=200\"")], &[], &[], true),
        1,
        "local preference"
    );
}

#[test]
fn a_looped_path_is_dropped_and_counted_and_refusals_are_logged_while_the_router_runs() {
    // r1 and r3 are both AS 1 but do not peer; r3's prefix reaches r1 through AS 2 with AS 1
    // in its path: dropped as a loop, counted, and r1 has no route to it.
    let s = Scenario::new("bgp-loop", 400)
        .domain("network")
        .actor(bgp(1, 1, 1))
        .actor(bgp(2, 2, 2))
        .actor(bgp(3, 1, 1).with("networks", "\"10.3.0.0/16\""))
        .link("network/link", &[(1, 0), (2, 0)], &[("delay", "\"2ns\"")])
        .link("network/link", &[(2, 1), (3, 0)], &[("delay", "\"2ns\"")]);
    let run = host::run(&s);
    assert!(host::faults(&run).is_empty());
    let loops: Vec<String> = texts(&run)
        .into_iter()
        .filter(|(a, t)| *a == 1 && t.contains("dropped as loops"))
        .map(|(_, t)| t)
        .collect();
    assert!(
        !loops.is_empty() && loops[0].contains("my own AS 1"),
        "{loops:?}"
    );
    assert!(routes(&run, 1).is_empty(), "{:?}", routes(&run, 1));
    assert!(
        states(&run, 1, 0).iter().any(|(_, s)| s == "established"),
        "the session stands"
    );
    assert!(host::handled(&run, 1).len() > 5, "r1 keeps running");

    // Internal BGP (rule 1): both refuse with the reason, a notification each way, no route.
    let run = host::run(&shipped::bgp_outside());
    assert!(host::faults(&run).is_empty());
    for actor in [1, 2] {
        let said: Vec<String> = texts(&run)
            .into_iter()
            .filter(|(a, _)| *a == actor)
            .map(|(_, t)| t)
            .collect();
        assert!(
            said.iter()
                .any(|t| t.contains("my own AS 1") && t.contains("outside the subset")),
            "{said:?}"
        );
        assert!(
            said.iter().any(|t| t.contains("notification 2")),
            "{said:?}"
        );
        let s = states(&run, actor, 0);
        assert_eq!(s.last().map(|(_, s)| s.as_str()), Some("idle"), "{s:?}");
        assert!(!s.iter().any(|(_, s)| s == "established"));
    }
    assert!(routes(&run, 2).is_empty(), "no route forms");
    assert!(host::handled(&run, 2).len() > 5, "the timers keep going");

    // Another hold time (rule 2): refused with a notification, said, both idle.
    let s = Scenario::new("bgp-hold-mismatch", 400)
        .domain("network")
        .actor(bgp(1, 1, 1))
        .actor(
            router(2, "bgp", 1)
                .with("as", "2")
                .with("hold", "\"60ns\"")
                .with("keepalive", "\"20ns\""),
        )
        .link("network/link", &[(1, 0), (2, 0)], &[("delay", "\"2ns\"")]);
    let run = host::run(&s);
    assert!(
        texts(&run)
            .iter()
            .any(|(_, t)| t.contains("does not negotiate the hold time")),
        "{:?}",
        texts(&run)
    );
    assert!(!states(&run, 1, 0).iter().any(|(_, s)| s == "established"));
    assert!(!states(&run, 2, 0).iter().any(|(_, s)| s == "established"));

    // A lan (rule 1): the second router on an interface is refused once per port.
    let s = Scenario::new("bgp-lan", 400)
        .domain("network")
        .actor(bgp(1, 1, 1))
        .actor(bgp(2, 1, 2))
        .actor(bgp(3, 1, 3))
        .link(
            "network/lan",
            &[(1, 0), (2, 0), (3, 0)],
            &[("delay", "\"2ns\"")],
        );
    let run = host::run(&s);
    assert!(host::faults(&run).is_empty());
    let refusals: Vec<(u32, String)> = texts(&run)
        .into_iter()
        .filter(|(_, t)| t.contains("a second router"))
        .collect();
    assert!(!refusals.is_empty(), "{:?}", texts(&run));
    for (actor, _) in &refusals {
        assert_eq!(
            refusals.iter().filter(|(a, _)| a == actor).count(),
            1,
            "once per port"
        );
    }
}

#[test]
fn every_frame_of_the_lab_decodes_and_a_truncated_one_does_not() {
    let run = host::run(&shipped::bgp_lab());
    let domain = NetworkDomain::new();
    let mut kinds: BTreeMap<String, u32> = BTreeMap::new();
    for actor in 1..=4u32 {
        for (_, _, payload) in sends(&run, actor) {
            let fields = domain
                .decode("bgp", &payload)
                .unwrap_or_else(|| panic!("a frame of r{actor} decodes: {payload:?}"));
            assert_eq!(fields[1], ("sender-as".to_string(), actor.to_string()));
            assert_eq!(fields[2], ("router-id".to_string(), actor.to_string()));
            *kinds.entry(fields[0].1.clone()).or_insert(0) += 1;
            match fields[0].1.as_str() {
                "open" => assert_eq!(fields[3], ("hold".to_string(), HOLD.to_string())),
                "update" => {
                    let advertised = &fields.iter().find(|(k, _)| k == "advertised").unwrap().1;
                    let withdrawn = &fields.iter().find(|(k, _)| k == "withdrawn").unwrap().1;
                    assert!(
                        advertised.contains(PREFIX) || withdrawn.contains(PREFIX),
                        "{fields:?}"
                    );
                    if advertised.contains(PREFIX) {
                        assert!(advertised.contains('[') && advertised.contains(']'));
                    }
                }
                "keepalive" => assert_eq!(fields.len(), 3),
                other => panic!("{other} in the lab"),
            }
            // A truncated frame is not this layout.
            if payload.len() > 2 {
                assert!(
                    domain
                        .decode("bgp", &payload[..payload.len() - 1])
                        .is_none()
                );
            }
        }
    }
    assert!(
        kinds["open"] >= 10 && kinds["update"] >= 6 && kinds["keepalive"] > 50,
        "{kinds:?}"
    );
    assert!(domain.decode("bgp", &[1, 9, 0, 0]).is_none());
    assert!(domain.decode("bgp", b"not bgp").is_none());
    // A notification decodes with its code named.
    let run = host::run(&shipped::bgp_outside());
    let notification = sends(&run, 1)
        .into_iter()
        .map(|(_, _, p)| p)
        .find(|p| p.get(1) == Some(&4))
        .expect("a notification");
    let fields = domain.decode("bgp", &notification).unwrap();
    assert_eq!(fields[0].1, "notification");
    assert!(
        fields.iter().any(|(k, v)| k == "code" && v == "own-as"),
        "{fields:?}"
    );
}
