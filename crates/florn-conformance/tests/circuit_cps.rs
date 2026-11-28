//! PR-50 (checklist B-50): two domains in one actor. RFC 012 rules 9, 22 and 26; RFC 002 rule 48.
//! `networked-pid` is the controller of `motor-loop` with its set point arriving as a frame on a
//! network interface: the network domain delivers the frame to `on-message`, the circuit domain
//! delivers the samples to `on-sample`, and a `set-output` committed from `on-message` is an input
//! change at a tick that is not a sample.
use florn_admission::Admission;
use florn_conformance::circuit_scenarios::{cps_link, cps_loop, cps_loop_text, motor_loop};
use florn_conformance::host;
use florn_conformance::scenario::Scenario;
use florn_domain_circuit::CHANNEL_PROBE;
use florn_host::{Advance, Domains, EndReason, Intervention, Run};
use florn_kernel::trace::Record;

const MS: u64 = 1_000_000;
/// The controller's identity in `cps-loop` (`pid`, kind module).
const PID: u32 = 5;
/// The motor's, whose `speed` the voltmeter-like sensor samples every millisecond.
const MOTOR: u32 = 2;

fn f64s(bytes: &[u8]) -> Vec<f64> {
    bytes
        .chunks(8)
        .map(|c| f64::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

fn speeds(run: &host::Host) -> Vec<(u64, f64)> {
    host::reports(run, MOTOR, CHANNEL_PROBE)
        .into_iter()
        .map(|(at, b)| (at.tick, f64s(&b)[0]))
        .collect()
}

fn frame(set_point: f64) -> Intervention {
    Intervention::Message {
        actor: PID,
        port: 1,
        payload: set_point.to_le_bytes().to_vec(),
    }
}

/// The run of `cps-loop` with a frame at `at` ticks (or none), to the end.
fn with_frame(at: Option<(u64, Intervention)>) -> host::Host {
    let mut run = host::start(&cps_loop());
    if let Some((tick, i)) = at {
        run.intervene(Some(tick), &i);
    }
    run.run_to_end();
    run
}

#[test]
fn the_loop_with_its_controller_on_a_network_settles_as_the_loop_it_derives_from() {
    let run = host::run(&cps_loop());
    assert_eq!(host::ended(&run), EndReason::EndReached);
    assert!(host::faults(&run).is_empty());
    // One actor of two worlds' interfaces: it samples (circuit) and has a network port. Its plant
    // is `motor-loop`'s, and its set point the parameter that `motor-loop` read from a signal:
    // the speeds are the same to the last bit.
    let (a, b) = (speeds(&run), speeds(&host::run(&motor_loop())));
    assert_eq!(a.len(), 1500);
    assert_eq!(a, b);
    let settled = a.iter().find(|s| s.0 == 800 * MS).unwrap().1;
    assert!((settled - 100.0).abs() < 2.0, "{settled}");
}

#[test]
fn a_frame_between_two_samples_changes_the_plant_from_the_tick_it_was_committed_and_not_before() {
    // RFC 012 rule 9 and 22: a frame handled at 600.5 ms is an input outside a sample. The
    // controller answers it in the same activation, so the plant sees the new command from
    // 600.5 ms: the speed sampled at 600 ms is the limit from the left, the one at 601 ms is not.
    let (without, with) = (
        speeds(&with_frame(None)),
        speeds(&with_frame(Some((600 * MS + 500_000, frame(40.0))))),
    );
    let at = |s: &[(u64, f64)], ms: u64| s.iter().find(|x| x.0 == ms * MS).unwrap().1;
    for ms in [100, 400, 599, 600] {
        assert_eq!(
            at(&without, ms).to_bits(),
            at(&with, ms).to_bits(),
            "the sample at {ms} ms is the limit from the left"
        );
    }
    assert_ne!(
        at(&without, 601).to_bits(),
        at(&with, 601).to_bits(),
        "the plant saw the new command for half a millisecond before the sample at 601 ms"
    );
    // The loop then follows the new set point: it falls from 100 rad/s, undershoots, takes the load
    // step of one second and is at 40 rad/s at the end.
    assert!(
        at(&with, 800) < 40.0,
        "it has fallen past the new set point"
    );
    let late = at(&with, 1499);
    assert!((late - 40.0).abs() < 1.0, "{late}");
}

#[test]
fn the_frame_answers_at_once_and_the_next_sample_carries_on_from_its_set_point() {
    // The controller's own reports tell the order: the frame's report at 600.5 ms with the set
    // point it took, then the next `control` report at 601 ms with an error from that set point.
    let run = with_frame(Some((600 * MS + 500_000, frame(40.0))));
    let frames = host::reports(&run, PID, 3);
    assert_eq!(frames.len(), 1, "one frame, one report");
    assert_eq!(frames[0].0.tick, 600 * MS + 500_000);
    let f = f64s(&frames[0].1[..8]);
    assert_eq!(f[0], 40.0);
    assert_eq!(frames[0].1[8], 1, "accepted");
    let controls = host::reports(&run, PID, 1);
    let next = controls
        .iter()
        .find(|c| c.0.tick > 600 * MS + 500_000)
        .unwrap();
    assert_eq!(
        next.0.tick,
        601 * MS + 1,
        "the next sample, one tick after the motor's"
    );
    let c = f64s(&next.1);
    // error = set point − measurement, from the new set point.
    assert!((c[2] - (40.0 - c[0])).abs() < 1e-9, "{c:?}");
}

#[test]
fn a_frame_that_is_not_a_set_point_is_ignored_and_says_so() {
    for payload in [
        vec![1u8, 2, 3],
        f64::NAN.to_le_bytes().to_vec(),
        f64::INFINITY.to_le_bytes().to_vec(),
    ] {
        let bad = Intervention::Message {
            actor: PID,
            port: 1,
            payload,
        };
        let run = with_frame(Some((600 * MS + 500_000, bad)));
        let frames = host::reports(&run, PID, 3);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].1[8], 0, "not accepted");
        // The set point stays 100: the plant is the plant of no frame at all.
        assert_eq!(speeds(&run), speeds(&with_frame(None)));
    }
}

#[test]
fn a_run_with_the_frame_replays_to_the_same_digest_and_the_frame_is_an_input() {
    let s = cps_loop();
    let mut run = host::start(&s);
    run.intervene(Some(600 * MS + 500_000), &frame(40.0));
    run.run_to_end();
    assert_eq!(run.kernel().applied_interventions().len(), 1);
    let record = run.run_toml();
    let mut replay = Run::replay(
        host::engine(),
        host::admission(),
        &mut florn_platform::FixedPlatform::default(),
        &host::domains(),
        s.manifest(),
        &s.modules(),
        &record,
        host::options(),
    )
    .expect("the replay starts");
    replay.run_to_end();
    assert!(matches!(
        replay.comparison(),
        Some(florn_kernel::trace::Comparison::Matched { .. })
    ));
    assert_eq!(replay.chain_hex(), run.chain_hex());
    assert_ne!(
        host::run(&s).chain_hex(),
        run.chain_hex(),
        "the frame is an input"
    );
}

#[test]
fn a_run_restored_across_the_frame_continues_to_the_same_digest() {
    use florn_host::{Checkpoint, Options};
    let s = cps_loop();
    let whole = with_frame(Some((600 * MS + 500_000, frame(40.0)))).chain_hex();
    let options = || Options {
        checkpoint_interval: 0,
        checkpoint_memory: u64::MAX,
        ..host::options()
    };
    for at in [300 * MS, 600 * MS + 500_000, 900 * MS] {
        let mut a = host::try_start(&s, options()).expect("starts");
        a.intervene(Some(600 * MS + 500_000), &frame(40.0));
        a.advance(Advance::ToTick(at));
        assert!(
            a.request_checkpoint() || {
                a.advance(Advance::OneInstant);
                !a.kept().is_empty()
            }
        );
        let c = Checkpoint::decode_file(&a.kept().last().unwrap().checkpoint().encode(), u64::MAX)
            .unwrap();
        let mut b = Run::from_checkpoint(
            host::engine(),
            host::admission(),
            &mut florn_platform::FixedPlatform::default(),
            &host::domains(),
            s.manifest(),
            &s.modules(),
            &c,
            options(),
        )
        .expect("restores");
        b.run_to_end();
        // The frame is in the record of a checkpoint taken after it, and a pending input of the
        // run before it; either way the continuation is the run's.
        assert_eq!(b.chain_hex(), whole, "restored at {at}");
    }
}

// --- validation with both domains and with one ------------------------------------------------------

/// The codes with which a Host that has `domains` admits or refuses the module of `cps-loop`.
fn admit(domains: &Domains) -> Vec<String> {
    let bytes = cps_loop().modules().remove("networked_pid").unwrap();
    let mut adm = Admission::default();
    for w in domains.worlds() {
        adm.add_world(w);
    }
    match adm.admit("networked_pid", &bytes, &host::kit_limits()) {
        Ok(_) => Vec::new(),
        Err(e) => e.codes().iter().map(|c| c.to_string()).collect(),
    }
}

#[test]
fn the_module_is_admitted_on_a_host_that_holds_both_domains_and_refused_on_one() {
    assert_eq!(admit(&host::domains()), Vec::<String>::new());
    // The circuit domain alone, or the network domain alone, with the composition registered as a
    // shell without the other would: the world is not one this Host offers.
    let circuit_only = florn_conformance::circuit::domains().with_composed(
        florn_domain_circuit::CPS_WORLD,
        &[
            (florn_domain_network::NAME, florn_domain_network::WORLD),
            (
                florn_domain_circuit::NAME,
                florn_domain_circuit::controllers::WORLD,
            ),
        ],
    );
    let network_only = Domains::core()
        .with(florn_domain_network::NetworkDomain::new)
        .with_composed(
            florn_domain_circuit::CPS_WORLD,
            &[
                (florn_domain_network::NAME, florn_domain_network::WORLD),
                (
                    florn_domain_circuit::NAME,
                    florn_domain_circuit::controllers::WORLD,
                ),
            ],
        );
    for (held, which) in [
        (circuit_only, "circuit only"),
        (network_only, "network only"),
    ] {
        let codes = admit(&held);
        assert!(
            codes.contains(&"module.contract-unsupported".to_string()),
            "{which}: {codes:?}"
        );
    }
}

#[test]
fn a_scenario_that_uses_the_world_declares_both_domains() {
    // `cps-loop` with the network domain left out of `domains`: the module's world uses it.
    let text = cps_loop_text().replace(
        "domains = [\"core\", \"network\", \"circuit\"]",
        "domains = [\"core\", \"circuit\"]",
    );
    let s = Scenario::raw("cps-loop-one", 1_500_000_000, text, &["networked_pid"]);
    let refused = host::refused(&s);
    assert!(
        !refused.is_empty(),
        "a scenario with one of the two domains starts"
    );
    assert!(
        refused.iter().any(|c| c.contains("domain.undeclared")),
        "{refused:?}"
    );
    // And with both it starts (the other tests run it).
    assert!(host::refused(&cps_loop()).is_empty());
}

#[test]
fn each_domain_validates_the_ports_of_its_own_types() {
    // The network interface on a signal connection is the circuit domain's refusal of a port that
    // is not a signal port; a circuit pin on a link is the network domain's of one that is not an
    // interface. (The ports are the module's own: the circuit domain judges `in` and `out`, the
    // network domain `eth`.)
    let text = cps_loop_text().replace(
        "ports = [\"m1.speed\", \"pid.measurement\"]",
        "ports = [\"m1.speed\", \"pid.eth\"]",
    );
    let s = Scenario::raw(
        "cps-loop-wrong-port",
        1_500_000_000,
        text,
        &["networked_pid"],
    );
    let refused = host::refused(&s);
    assert!(
        !refused.is_empty(),
        "an interface on a signal is refused: {refused:?}"
    );
}

#[test]
fn the_frame_is_delivered_by_the_network_domain_and_the_samples_by_the_circuit_domain() {
    let run = with_frame(Some((600 * MS + 500_000, frame(40.0))));
    let records = run.records().unwrap();
    let mut classes = std::collections::BTreeSet::new();
    for r in records {
        if let Record::Handled { key, target, .. } = r
            && *target == PID
        {
            classes.insert((key.class, key.tick == 600 * MS + 500_000));
        }
    }
    // The sample events are the circuit's class 7; the intervention's activation is a core event
    // (class 0) at the tick of the frame.
    assert!(classes.iter().any(|(c, _)| *c == 7), "{classes:?}");
    assert!(classes.contains(&(0, true)), "{classes:?}");
}

#[test]
fn the_answer_is_the_proportional_and_integral_terms_the_controller_has_on_the_measurement_held_now()
 {
    // The output the frame commits at once: `kp·(set point − measurement) + integral`, limited to
    // `[0, 1]` (the loop's gains: kp = 0.004), with the measurement of the last sample and the
    // integral that sample left; the integral and the derivative do not advance.
    let run = with_frame(Some((600 * MS + 500_000, frame(40.0))));
    let controls = host::reports(&run, PID, 1);
    let terms = host::reports(&run, PID, 2);
    let before = |v: &[(florn_kernel::Instant, Vec<u8>)]| {
        v.iter()
            .rfind(|c| c.0.tick < 600 * MS + 500_000)
            .map(|c| f64s(&c.1))
            .unwrap()
    };
    let (measurement, integral) = (before(&controls)[0], before(&terms)[1]);
    let expected = (0.004 * (40.0 - measurement) + integral).clamp(0.0, 1.0);
    let sent: Vec<f64> = host::sends(&run, PID)
        .into_iter()
        .filter(|s| s.0.tick == 600 * MS + 500_000)
        .map(|s| f64::from_le_bytes(s.2[..8].try_into().unwrap()))
        .collect();
    assert_eq!(sent.len(), 1, "one output committed by the frame");
    assert_eq!(
        sent[0].to_bits(),
        expected.to_bits(),
        "{sent:?} against {expected}"
    );
    assert!(integral != 0.0, "the integral is in the answer");
}

#[test]
fn a_frame_from_a_peer_over_a_link_reaches_the_controller_through_the_network_domain() {
    // The network domain's own path, as `cps-link` ships it: a peer on a link sends the set point
    // once, 40 rad/s at 600.5 ms, and `on-message` of the composed actor reads it.
    let run = host::run(&cps_link());
    assert_eq!(host::ended(&run), EndReason::EndReached);
    assert!(host::faults(&run).is_empty(), "{:?}", host::faults(&run));
    let frames = host::reports(&run, PID, 3);
    assert_eq!(frames.len(), 1, "one frame, one report");
    assert!(
        frames[0].0.tick >= 600 * MS + 500_000 && frames[0].0.tick < 601 * MS,
        "the frame arrives after it is sent and before the next sample: {}",
        frames[0].0.tick
    );
    assert_eq!(f64s(&frames[0].1[..8])[0], 40.0);
    assert_eq!(frames[0].1[8], 1, "accepted");
    // The loop follows it, as the intervention's does: the samples up to the frame are the loop's.
    let (sp, plain) = (speeds(&run), speeds(&host::run(&cps_loop())));
    let at = |s: &[(u64, f64)], ms: u64| s.iter().find(|x| x.0 == ms * MS).unwrap().1;
    assert_eq!(at(&sp, 600).to_bits(), at(&plain, 600).to_bits());
    assert_ne!(at(&sp, 601).to_bits(), at(&plain, 601).to_bits());
    assert!((at(&sp, 1499) - 40.0).abs() < 1.0, "{}", at(&sp, 1499));
}
