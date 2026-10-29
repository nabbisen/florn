//! RFC 001 §14 under the native Host over the kit's guests. Tier 2 across architectures is
//! AC-001's recorded digests compared in CI on x86-64 and AArch64; Order and Fairness are
//! AC-004; Checkpoint is AC-006 (`tests/ac.rs`).
use florn_conformance::host::{self, faults, log_param, reports, sends};
use florn_conformance::scenario::{Scenario, actor};
use florn_conformance::shipped;
use florn_control::{Answer, Command, Envelope, Message, Query, SeedChoice, Subscription, Unit};
use florn_host::{Advance, Controller, EndReason, Intervention, Options, Run, RunToml, Seed};
use florn_kernel::FaultKind;
use florn_kernel::trace::Comparison;
use florn_platform::FixedPlatform;
use florn_platform_native::NativePlatform;

fn shipped_named(name: &str) -> Scenario {
    shipped::all()
        .into_iter()
        .find(|s| s.scenario.name == name)
        .unwrap_or_else(|| panic!("shipped scenario {name}"))
        .scenario
}

#[test]
fn tier_1_one_scenario_twice_under_one_host() {
    let a = host::run(&shipped_named("echo-pair"));
    let b = host::run(&shipped_named("echo-pair"));
    assert_eq!(a.chain(), b.chain());
    assert_eq!(a.kernel().trace.stored(), b.kernel().trace.stored());
}

#[test]
fn zero_delay_ends_the_run_with_the_fault_of_rule_13() {
    let run = host::run(&shipped_named("zero-delay"));
    assert_eq!(host::ended(&run), EndReason::HostFault);
    assert_eq!(
        log_param(&run, "run.limit-exceeded", "what").as_deref(),
        Some("microsteps-per-tick")
    );
    let origins = log_param(&run, "run.limit-exceeded", "origins").unwrap_or_default();
    assert!(
        !origins.is_empty() && origins.split(',').all(|o| o == "1" || o == "2"),
        "the fault names the origins involved: {origins}"
    );
    assert!(faults(&run).is_empty(), "no actor faulted: the run did");
}

#[test]
fn an_activation_that_faults_commits_nothing() {
    // The chatter sends once per timer and traps at tick 10: the send of tick 5 is committed,
    // the send of tick 10 is not, and the trace holds the fault.
    let s = Scenario::new("activation", 30)
        .actor(
            actor(1, "chatter", 1)
                .with("label", "\"a\"")
                .with("draws", "0")
                .with("sends", "1")
                .with("trap-at", "10"),
        )
        .actor(actor(2, "echo", 1))
        .connect(&[(1, 0), (2, 0)]);
    let run = host::run(&s);
    let sent = sends(&run, 1);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0.tick, 5);
    let f = faults(&run);
    assert_eq!(f.len(), 1);
    assert_eq!((f[0].0, f[0].1), (1, FaultKind::Trap));
    let at_ten: Vec<_> = host::handled(&run, 2)
        .into_iter()
        .filter(|k| k.class == florn_host::CLASS_MESSAGE)
        .map(|k| k.tick)
        .collect();
    assert_eq!(at_ten, vec![5], "the echo received the committed send only");
    assert!(!run.alive(1) && run.alive(2));
}

#[test]
fn a_third_actor_changes_no_draw_of_the_first_two() {
    let base = Scenario::new("streams", 100)
        .actor(actor(1, "chatter", 0).with("label", "\"a\""))
        .actor(actor(2, "echo", 0));
    let with_third = base
        .clone()
        .actor(actor(3, "chatter", 0).with("label", "\"c\""));
    let x = host::run(&base);
    let y = host::run(&with_third);
    for id in [1, 2] {
        let a = reports(&x, id, 1);
        let b = reports(&y, id, 1);
        assert!(!a.is_empty());
        assert_eq!(a, b, "actor {id}'s draws");
    }
    assert!(!reports(&y, 3, 1).is_empty());
    assert_ne!(x.chain(), y.chain());
}

#[test]
fn a_run_with_an_intervention_replays_to_the_same_digest() {
    let s = shipped_named("echo-pair");
    let mut run = host::start(&s);
    run.advance(Advance::ToTick(500));
    run.intervene(
        None,
        &Intervention::Message {
            actor: 2,
            port: 0,
            payload: vec![7, 7, 7],
        },
    );
    run.run_to_end();
    assert_eq!(run.kernel().applied_interventions().len(), 1);
    let text = run.run_toml().to_toml();
    let record = RunToml::parse(&text).expect("run.toml parses");
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
    .expect("the replay starts");
    replay.run_to_end();
    assert_eq!(replay.chain(), run.chain());
    assert!(matches!(
        replay.comparison(),
        Some(Comparison::Matched { .. })
    ));
    assert_ne!(
        host::run(&s).chain(),
        run.chain(),
        "the intervention is an input"
    );
}

fn answer(replies: &[Message]) -> Option<&Answer> {
    replies.iter().find_map(|m| match m {
        Message::Answer(a) => Some(a),
        _ => None,
    })
}

#[test]
fn observation_changes_nothing() {
    let s = shipped_named("echo-pair");
    // No inspector: nothing kept.
    let none = host::try_start(&s, Options::default()).expect("starts");
    let mut none = none;
    none.run_to_end();
    // Every inspector of the Host: records, payloads, telemetry.
    let every = host::run(&s);
    assert_eq!(none.chain(), every.chain());
    // Every subscription and query of the control interface, exercised while the run advances.
    let mut c = Controller::new(
        host::engine(),
        florn_host::run::default_admission(),
        Box::new(NativePlatform::new()),
        host::domains(),
        host::options(),
    );
    let send = |c: &mut Controller<_>, m: Message| c.handle(Envelope::new(m));
    send(
        &mut c,
        Message::Command(Command::LoadScenario {
            bytes: s.text(&s.hashes()).into_bytes(),
        }),
    );
    for (_, bytes) in s.modules() {
        send(&mut c, Message::Command(Command::LoadModule { bytes }));
    }
    send(
        &mut c,
        Message::Command(Command::NewRun {
            seed: SeedChoice::Given(s.seed),
            residency: florn_control::Residency::Resident,
        }),
    );
    send(&mut c, Message::Subscribe(Subscription::Events));
    send(&mut c, Message::Subscribe(Subscription::Progress));
    send(
        &mut c,
        Message::Subscribe(Subscription::Telemetry {
            channels: vec![],
            keep_one_in: 1,
        }),
    );
    let mut seen_telemetry = 0;
    for step in 1..=10u64 {
        let replies = send(
            &mut c,
            Message::Command(Command::Advance {
                unit: Unit::ToTick(step * 1_000),
            }),
        );
        seen_telemetry += replies
            .iter()
            .filter(|m| matches!(m, Message::Telemetry(_)))
            .count();
        send(&mut c, Message::Query(Query::Actor { id: 2 }));
        send(
            &mut c,
            Message::Query(Query::TraceWindow { from: 0, count: 16 }),
        );
    }
    send(
        &mut c,
        Message::Command(Command::Advance {
            unit: Unit::ToTick(u64::MAX),
        }),
    );
    assert!(seen_telemetry > 0, "the inspectors were open");
    let replies = send(&mut c, Message::Query(Query::RunState));
    let Some(Answer::RunState(state)) = answer(&replies) else {
        panic!("no run state");
    };
    assert_eq!(state.chain.as_deref(), Some(every.chain_hex().as_str()));
}

#[test]
fn memory_within_the_limit_is_never_refused() {
    // The grow guest asks page by page and reports where the limit refused: exactly the limit.
    // (The guest's initial memory is above 1 MiB: a stack of 1 MiB and its data.)
    for (limit, pages) in [("\"2MiB\"", 32u64), ("\"3MiB\"", 48)] {
        let s = Scenario::new("grow", 20)
            .limit("memory", limit)
            .actor(actor(1, "hostile_grow", 0).with("trap", "false"));
        let run = host::run(&s);
        assert!(faults(&run).is_empty(), "no fault: the refusal is -1");
        let r = reports(&run, 1, 1);
        assert_eq!(r.len(), 1);
        assert_eq!(
            u64::from_le_bytes(r[0].1[..8].try_into().unwrap()),
            pages,
            "refused at the limit of {limit}"
        );
    }
    // The heap guest grows within its limit for 10,000 activations and is never refused.
    let run = host::run(&shipped_named("heap"));
    assert!(faults(&run).is_empty());
    assert_eq!(reports(&run, 1, 1).len(), 10_000);
    let _ = Seed::Given(0);
}
