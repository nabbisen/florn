//! What the application reads (RFC 012 findings F-6 and F-12; RFC 014 rules 2 and 3; checklist B-43):
//! the `inspect` query for a built-in actor, a connection and a domain's world; a guest is the
//! `actor` query's; the built-in's name in the scenario's summary; the reports of a `trace-window`
//! with their payload bytes, those the Host kept and those of a window it re-executes; and opening
//! an inspector changes no digest.
use florn_admission::Admission;
use florn_conformance::host;
use florn_conformance::test_domain as td;
use florn_control::message::{InspectValue, TraceEntry};
use florn_control::{Answer, Command, Envelope, Message, Query, Unit};
use florn_host::{Controller, Options};
use florn_platform::FixedPlatform;

type C = Controller<florn_engine_wasmtime::WasmtimeEngine>;

/// Two probers and a sink over a wire; the test domain is continuous so that its levels are series.
fn scenario(extra_test_keys: &str) -> String {
    td::text(
        "inspect",
        600,
        5,
        &format!(
            "{}{}{}{}\n[test]\natol = 0.001\nrtol = 0.000001{extra_test_keys}\n",
            td::prober(1, 1),
            td::prober(2, 1),
            td::sink(3),
            td::wire(100, &[(1, 0), (2, 0)], "delay = \"2ns\""),
        ),
    )
}

fn controller(options: Options) -> C {
    Controller::new(
        host::engine(),
        Admission::default(),
        Box::new(FixedPlatform::default()),
        td::domains(),
        options,
    )
}

fn send(c: &mut C, m: Message) -> Vec<Message> {
    c.handle(Envelope::new(m))
}

fn answer(replies: Vec<Message>) -> Answer {
    replies
        .into_iter()
        .find_map(|m| match m {
            Message::Answer(a) => Some(a),
            _ => None,
        })
        .expect("an answer")
}

fn started(options: Options) -> C {
    started_with("", options)
}

/// The scenario with `extra` keys in its `[test]` table.
fn started_with(extra: &str, options: Options) -> C {
    let mut c = controller(options);
    send(
        &mut c,
        Message::Command(Command::LoadScenario {
            bytes: scenario(extra).into_bytes(),
        }),
    );
    send(
        &mut c,
        Message::Command(Command::LoadModule {
            bytes: td::guest_bytes(),
        }),
    );
    let started = send(
        &mut c,
        Message::Command(Command::NewRun {
            seed: florn_control::SeedChoice::Given(5),
            residency: florn_control::Residency::Resident,
        }),
    );
    assert!(
        started.iter().any(|m| matches!(m, Message::Answer(_))),
        "{started:?}"
    );
    c
}

fn run_to_end(c: &mut C) {
    send(
        c,
        Message::Command(Command::Advance {
            unit: Unit::ToTick(u64::MAX),
        }),
    );
}

fn inspect(c: &mut C, id: u32) -> Option<florn_control::message::Inspection> {
    match answer(send(c, Message::Query(Query::Inspect { id }))) {
        Answer::Inspection(i) => i,
        other => panic!("{other:?}"),
    }
}

fn field(i: &florn_control::message::Inspection, name: &str) -> InspectValue {
    i.fields
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no field {name}: {i:?}"))
        .value
        .clone()
}

fn world(c: &mut C) -> u32 {
    let Answer::Scenario(Some(s)) = answer(send(c, Message::Query(Query::Scenario))) else {
        panic!("a scenario");
    };
    // The one declared domain's world: the n-th has identity 0xF000_0010 + n (RFC 004 rule 8).
    assert_eq!(s.domains, ["test"]);
    0xF000_0010
}

#[test]
fn a_domain_says_what_it_knows_of_a_builtin_a_connection_and_its_world() {
    let mut c = started(host::options());
    let world = world(&mut c);
    // A connection: its fields in the domain's order, a real with its unit.
    let wire = inspect(&mut c, 100).expect("the wire");
    assert_eq!(wire.kind, "connection");
    let names: Vec<_> = wire.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["enabled", "pins", "delay"]);
    assert_eq!(field(&wire, "enabled"), InspectValue::Text("true".into()));
    assert_eq!(
        field(&wire, "delay"),
        InspectValue::Real {
            value: 2.0,
            unit: "ticks".into()
        }
    );
    // A built-in actor, and what it counted by the last accepted state.
    let sink = inspect(&mut c, 3).expect("the sink");
    assert_eq!(sink.kind, "builtin");
    assert_eq!(field(&sink, "frames"), InspectValue::Text("0".into()));
    // The sink is on no wire here: the last accepted state still says none, after the run too.
    run_to_end(&mut c);
    let sink = inspect(&mut c, 3).expect("the sink");
    assert_eq!(field(&sink, "frames"), InspectValue::Text("0".into()));
    // The domain's world.
    let w = inspect(&mut c, world).expect("the world");
    assert_eq!(w.kind, "world");
    assert_eq!(field(&w, "wires"), InspectValue::Text("1".into()));
    // A guest is the `actor` query's, an identity the scenario lacks and one no domain inspects
    // are nothing.
    assert_eq!(inspect(&mut c, 1), None);
    assert_eq!(inspect(&mut c, 999), None);
    assert_eq!(inspect(&mut c, 0xF000_0099), None);
    // A domain that would answer for a guest is not asked: the guest is the `actor` query's.
    let mut greedy = started_with("\ninspect-guests = true", host::options());
    assert_eq!(inspect(&mut greedy, 1), None);
    assert!(inspect(&mut greedy, 3).is_some());
    // With no run there is nothing to read.
    let mut idle = controller(host::options());
    assert_eq!(inspect(&mut idle, 3), None);
}

#[test]
fn the_scenarios_summary_names_a_builtin_and_the_actor_answer_does_too() {
    let mut c = started(host::options());
    let Answer::Scenario(Some(s)) = answer(send(&mut c, Message::Query(Query::Scenario))) else {
        panic!("a scenario");
    };
    let named: Vec<_> = s
        .actors
        .iter()
        .map(|a| (a.id, a.builtin.as_deref(), a.module.as_deref()))
        .collect();
    assert_eq!(
        named,
        [
            (1, None, Some("prober")),
            (2, None, Some("prober")),
            (3, Some("test/sink"), None)
        ]
    );
    let Answer::Actor(Some(info)) = answer(send(&mut c, Message::Query(Query::Actor { id: 3 })))
    else {
        panic!("an actor");
    };
    assert_eq!(info.summary.builtin.as_deref(), Some("test/sink"));
    // And through the bytes: the section survives the encoding.
    let bytes = c.handle_bytes(&florn_control::encode(&Envelope::new(Message::Query(
        Query::Scenario,
    ))));
    let Message::Answer(Answer::Scenario(Some(decoded))) =
        florn_control::decode(&bytes[0]).unwrap().message
    else {
        panic!("a scenario");
    };
    assert_eq!(decoded.actors[2].builtin.as_deref(), Some("test/sink"));
}

#[test]
fn opening_every_inspector_at_every_step_changes_no_digest() {
    // RFC 001 rule 39: observing a run leaves it as it was. A run stepped to its end with every
    // inspector and the trace window asked for after each step has the digest of one that is not.
    let ids = [1, 2, 3, 100, 0xF000_0010, 999];
    let mut watched = started(host::options());
    let mut left_alone = started(host::options());
    for tick in (50..=600).step_by(50) {
        send(
            &mut watched,
            Message::Command(Command::Advance {
                unit: Unit::ToTick(tick),
            }),
        );
        for id in ids {
            inspect(&mut watched, id);
            send(&mut watched, Message::Query(Query::Actor { id }));
        }
        send(
            &mut watched,
            Message::Query(Query::TraceWindow {
                from: 0,
                count: 100,
            }),
        );
    }
    run_to_end(&mut watched);
    run_to_end(&mut left_alone);
    let chain = |c: &mut C| match answer(send(c, Message::Query(Query::RunState))) {
        Answer::RunState(r) => r.chain,
        other => panic!("{other:?}"),
    };
    assert!(chain(&mut watched).is_some());
    assert_eq!(chain(&mut watched), chain(&mut left_alone));
}

/// A level report of 16 bytes: an `f64` and a whole tick, as the test domain's numeric report is.
fn level(value: f64) -> Vec<u8> {
    let mut b = value.to_le_bytes().to_vec();
    b.extend_from_slice(&9u64.to_le_bytes());
    b
}

fn with_reports(mut c: C) -> C {
    // Reports of actor 1 at ticks 50 and 120: interventions the run applies at their instants.
    for (tick, value) in [(50u64, 1.5f64), (120, 2.5)] {
        let content = florn_host::world::Intervention::Message {
            actor: 1,
            port: 0,
            payload: level(value),
        }
        .encode();
        let out = send(
            &mut c,
            Message::Command(Command::Intervene {
                target: 1,
                content,
                at: Some(tick),
            }),
        );
        assert!(
            out.iter()
                .any(|m| matches!(m, Message::Answer(Answer::Done))),
            "{out:?}"
        );
    }
    c
}

fn window(c: &mut C) -> Vec<TraceEntry> {
    match answer(send(
        c,
        Message::Query(Query::TraceWindow {
            from: 0,
            count: 10_000,
        }),
    )) {
        Answer::TraceWindow(w) => w,
        other => panic!("{other:?}"),
    }
}

fn payloads_of(w: &[TraceEntry]) -> Vec<Vec<u8>> {
    w.iter()
        .filter_map(|t| match t {
            TraceEntry::Handled { reports, .. } => Some(reports),
            _ => None,
        })
        .flatten()
        .filter(|r| r.len == 16)
        .map(|r| r.payload.clone().expect("a payload"))
        .collect()
}

#[test]
fn a_trace_window_carries_the_reports_payload_bytes_kept_or_re_executed() {
    // RFC 014 rule 2: the entries of a `trace-window` gain the reports' payloads. A Host that keeps
    // records and payloads has them; one that keeps neither re-executes the window with recording
    // on, and the bytes are the same.
    let mut keeping = with_reports(started(host::options()));
    run_to_end(&mut keeping);
    let kept = window(&mut keeping);
    let bytes = payloads_of(&kept);
    assert_eq!(bytes, [level(1.5), level(2.5)]);
    let mut thin = with_reports(started(Options {
        keep_records: false,
        keep_payloads: false,
        ..host::options()
    }));
    run_to_end(&mut thin);
    let rebuilt = window(&mut thin);
    assert_eq!(
        payloads_of(&rebuilt),
        bytes,
        "the re-executed window's bytes"
    );
    // The entries agree otherwise, and the thin Host kept no payload for the next asker.
    assert_eq!(rebuilt.len(), kept.len());
    let again = window(&mut thin);
    assert_eq!(payloads_of(&again), bytes);
    assert!(thin.run().unwrap().payloads().is_empty());
}

#[test]
fn a_run_that_has_not_started_answers_a_window_with_nothing() {
    // No run, so no checkpoint to start from: the window is empty, and it is still an answer.
    let mut c = controller(Options {
        keep_records: false,
        keep_payloads: false,
        ..host::options()
    });
    assert!(window(&mut c).is_empty());
}
