//! The Host behind the control interface (RFC 005 rules 25 to 27; checklist C8): a journey through
//! the commands; each command with one invalid parameter and in the wrong state produces its code
//! and changes nothing; a wrong interface version is refused.
mod support;

use florn_admission::Admission;
use florn_control::*;
use florn_host::Domains;
use florn_host::controller::Controller;
use florn_host::{Intervention, Options};
use florn_platform::FixedPlatform;
use support::{Guest, ScenarioSpec, actor, engine};

type C = Controller<florn_engine_wasmtime::WasmtimeEngine>;

fn controller() -> C {
    Controller::new(
        engine(),
        Admission::default(),
        Box::new(FixedPlatform::default()),
        Domains::core(),
        Options {
            keep_records: true,
            ..Options::default()
        },
    )
}

fn send(c: &mut C, m: Message) -> Vec<Message> {
    c.handle(Envelope::new(m))
}

fn command(c: &mut C, cmd: Command) -> Vec<Message> {
    send(c, Message::Command(cmd))
}

fn codes(replies: &[Message]) -> Vec<String> {
    replies
        .iter()
        .filter_map(|m| match m {
            Message::Notification(n) => Some(n.code.clone()),
            _ => None,
        })
        .collect()
}

fn answer(replies: &[Message]) -> Option<&Answer> {
    replies.iter().find_map(|m| match m {
        Message::Answer(a) => Some(a),
        _ => None,
    })
}

fn run_state(c: &mut C) -> florn_control::message::RunStateInfo {
    match answer(&send(c, Message::Query(Query::RunState))) {
        Some(Answer::RunState(r)) => r.clone(),
        other => panic!("{other:?}"),
    }
}

fn spec() -> ScenarioSpec {
    ScenarioSpec {
        end: 2_000,
        tie: "by-identity",
        actors: vec![actor(1, Guest::Pinger, 1), actor(2, Guest::Echo, 1)],
        connections: vec![vec![(1, 0), (2, 0)]],
        seed: 42,
    }
}

#[test]
fn a_journey_through_the_interface() {
    let mut c = controller();
    let s = spec();
    assert_eq!(run_state(&mut c).state, RunState::NoScenario);
    // Support and capacity at start-up.
    match answer(&send(&mut c, Message::Query(Query::Support))) {
        Some(Answer::Support(i)) => {
            assert_eq!(i.control_version, CONTROL_VERSION);
            assert_eq!(i.engine, "wasmtime");
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        answer(&send(&mut c, Message::Query(Query::Capacity))),
        Some(Answer::Capacity(_))
    ));
    // Load the manifest; the modules are missing until supplied.
    let r = command(
        &mut c,
        Command::LoadScenario {
            bytes: s.text().into_bytes(),
        },
    );
    assert_eq!(answer(&r), Some(&Answer::Done));
    assert_eq!(run_state(&mut c).state, RunState::Ready);
    match answer(&send(&mut c, Message::Query(Query::Scenario))) {
        Some(Answer::Scenario(Some(sc))) => {
            assert_eq!(sc.actors.len(), 2);
            assert_eq!(sc.missing_modules.len(), 2);
            assert_eq!(sc.modules[0].kind, "", "unknown until the module is held");
        }
        other => panic!("{other:?}"),
    }
    let r = command(
        &mut c,
        Command::NewRun {
            seed: SeedChoice::Given(1),
            residency: florn_control::Residency::Resident,
        },
    );
    assert_eq!(codes(&r), ["control.invalid"]);
    for (_, bytes) in s.modules() {
        assert_eq!(
            answer(&command(&mut c, Command::LoadModule { bytes })),
            Some(&Answer::Done)
        );
    }
    match answer(&send(&mut c, Message::Query(Query::Scenario))) {
        Some(Answer::Scenario(Some(sc))) => {
            assert!(sc.missing_modules.is_empty());
            assert_eq!(
                sc.modules
                    .iter()
                    .map(|m| m.kind.as_str())
                    .collect::<Vec<_>>(),
                ["echo", "pinger"]
            );
            assert_eq!(sc.modules[0].params[0].name, "period");
        }
        other => panic!("{other:?}"),
    }
    // Admit one module on its own.
    match answer(&command(
        &mut c,
        Command::Admit {
            bytes: Guest::Echo.bytes(),
        },
    )) {
        Some(Answer::Admitted {
            module,
            admitted_sha256,
        }) => {
            assert_eq!(module.kind, "echo");
            assert_eq!(admitted_sha256.len(), 64);
        }
        other => panic!("{other:?}"),
    }
    assert!(
        codes(&command(
            &mut c,
            Command::Admit {
                bytes: wat::parse_str("(module)").unwrap()
            }
        ))
        .iter()
        .all(|c| c.starts_with("module."))
    );
    // A new run: paused at tick 0 with init done.
    let r = command(
        &mut c,
        Command::NewRun {
            seed: SeedChoice::Given(7),
            residency: florn_control::Residency::Resident,
        },
    );
    assert_eq!(answer(&r), Some(&Answer::Done));
    assert!(codes(&r).starts_with(&["host.capacity".to_string(), "admission.result".to_string()]));
    let st = run_state(&mut c);
    assert_eq!(st.state, RunState::Paused);
    assert_eq!(st.seed, Some(7));
    assert_eq!(st.handled, 0);
    assert!(st.records >= 2, "init records");
    // Subscriptions, then steps.
    assert_eq!(
        answer(&send(&mut c, Message::Subscribe(Subscription::Progress))),
        Some(&Answer::Done)
    );
    assert_eq!(
        answer(&send(
            &mut c,
            Message::Subscribe(Subscription::Telemetry {
                channels: vec![1],
                keep_one_in: 1
            })
        )),
        Some(&Answer::Done)
    );
    assert_eq!(
        answer(&send(&mut c, Message::Subscribe(Subscription::Events))),
        Some(&Answer::Done)
    );
    let r = command(
        &mut c,
        Command::Advance {
            unit: Unit::OneEvent,
        },
    );
    assert!(
        r.iter()
            .any(|m| matches!(m, Message::Progress(p) if p.handled == 1))
    );
    let r = command(
        &mut c,
        Command::Advance {
            unit: Unit::ToTick(500),
        },
    );
    assert!(
        r.iter()
            .any(|m| matches!(m, Message::Telemetry(t) if t.channel == 1))
    );
    assert!(run_state(&mut c).now.unwrap().tick >= 490);
    // An intervention, then the trace window and the actor.
    let content = Intervention::Message {
        actor: 2,
        port: 0,
        payload: vec![1, 2],
    }
    .encode();
    assert_eq!(
        answer(&command(
            &mut c,
            Command::Intervene {
                target: 2,
                content: content.clone(),
                at: Some(600)
            }
        )),
        Some(&Answer::Done)
    );
    let r = command(
        &mut c,
        Command::Advance {
            unit: Unit::ToTick(700),
        },
    );
    assert!(codes(&r).contains(&"intervention.applied".to_string()));
    match answer(&send(
        &mut c,
        Message::Query(Query::TraceWindow { from: 0, count: 10 }),
    )) {
        Some(Answer::TraceWindow(w)) => assert_eq!(w.len(), 10),
        other => panic!("{other:?}"),
    }
    match answer(&send(&mut c, Message::Query(Query::Actor { id: 2 }))) {
        Some(Answer::Actor(Some(a))) => {
            assert!(a.alive);
            assert_eq!(a.summary.module.as_deref(), Some("echo"));
        }
        other => panic!("{other:?}"),
    }
    // Paced advance: running, pumped in slices, paused at the target.
    assert_eq!(
        answer(&command(
            &mut c,
            Command::SetPacing {
                pacing: Pacing::TicksPerSecond(100)
            }
        )),
        Some(&Answer::Done)
    );
    assert_eq!(
        answer(&command(
            &mut c,
            Command::Advance {
                unit: Unit::ToTick(1_000)
            }
        )),
        Some(&Answer::Done)
    );
    assert_eq!(run_state(&mut c).state, RunState::Running);
    let mut pumps = 0;
    while c.state() == RunState::Running {
        let _ = c.pump(50);
        pumps += 1;
        assert!(pumps < 100);
    }
    assert!(pumps >= 5, "{pumps} slices");
    assert_eq!(run_state(&mut c).state, RunState::Paused);
    let now = run_state(&mut c).now.unwrap().tick;
    assert!(
        (980..1_000).contains(&now),
        "every event before the target is handled: {now}"
    );
    // Export: a preview, then the file; the record replays to a match.
    let r = command(
        &mut c,
        Command::Export {
            kind: ExportKind::RunRecord {
                trace: true,
                payloads: true,
            },
            confirmed: false,
        },
    );
    assert_eq!(codes(&r), ["export.preview"]);
    let preview = match answer(&r) {
        Some(Answer::Preview(p)) => p.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(preview.modules.len(), 2);
    assert!(preview.payloads);
    // As fast as possible: the advance to the end runs in one go.
    assert_eq!(
        answer(&command(
            &mut c,
            Command::SetPacing {
                pacing: Pacing::Fastest
            }
        )),
        Some(&Answer::Done)
    );
    assert_eq!(
        answer(&command(
            &mut c,
            Command::Advance {
                unit: Unit::ToTick(u64::MAX)
            }
        )),
        Some(&Answer::Done)
    );
    assert_eq!(run_state(&mut c).state, RunState::Ended);
    let record = match answer(&command(
        &mut c,
        Command::Export {
            kind: ExportKind::RunRecord {
                trace: false,
                payloads: false,
            },
            confirmed: true,
        },
    )) {
        Some(Answer::Exported {
            kind: FileKind::RunRecord,
            name,
            bytes,
        }) => {
            assert!(name.ends_with(".florn-run"));
            bytes.clone()
        }
        other => panic!("{other:?}"),
    };
    let chain = run_state(&mut c).chain.unwrap();
    assert!(matches!(
        answer(&command(
            &mut c,
            Command::Export {
                kind: ExportKind::DiagnosticLog,
                confirmed: true
            }
        )),
        Some(Answer::Exported {
            kind: FileKind::DiagnosticLog,
            ..
        })
    ));
    let package = match answer(&command(
        &mut c,
        Command::Export {
            kind: ExportKind::Package,
            confirmed: true,
        },
    )) {
        Some(Answer::Exported {
            kind: FileKind::Package,
            bytes,
            ..
        }) => bytes.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        answer(&command(&mut c, Command::Replay { record })),
        Some(&Answer::Done)
    );
    assert_eq!(run_state(&mut c).state, RunState::Paused);
    let r = command(
        &mut c,
        Command::Advance {
            unit: Unit::ToTick(u64::MAX),
        },
    );
    assert!(
        codes(&r).contains(&"replay.matched".to_string()),
        "{:?}",
        codes(&r)
    );
    assert_eq!(run_state(&mut c).chain.unwrap(), chain);
    // The package loads as a scenario with its modules and runs to the same chain.
    let mut d = controller();
    assert_eq!(
        answer(&command(&mut d, Command::LoadScenario { bytes: package })),
        Some(&Answer::Done)
    );
    assert_eq!(
        answer(&command(
            &mut d,
            Command::NewRun {
                seed: SeedChoice::Given(7),
                residency: florn_control::Residency::Resident,
            }
        )),
        Some(&Answer::Done)
    );
    command(
        &mut d,
        Command::Advance {
            unit: Unit::ToTick(500),
        },
    );
    command(
        &mut d,
        Command::Intervene {
            target: 2,
            content,
            at: Some(600),
        },
    );
    command(
        &mut d,
        Command::Advance {
            unit: Unit::ToTick(u64::MAX),
        },
    );
    assert_eq!(run_state(&mut d).chain.unwrap(), chain);
}

#[test]
fn c8_an_invalid_parameter_or_state_produces_its_code_and_changes_nothing() {
    let mut c = controller();
    let s = spec();
    // Before a scenario: every command that needs one is `control.not-now` with the state.
    for cmd in [
        Command::NewRun {
            seed: SeedChoice::Choose,
            residency: florn_control::Residency::Resident,
        },
        Command::EndRun,
        Command::Advance {
            unit: Unit::OneTick,
        },
        Command::Pause,
        Command::Intervene {
            target: 1,
            content: vec![1],
            at: None,
        },
        Command::Export {
            kind: ExportKind::RunRecord {
                trace: false,
                payloads: false,
            },
            confirmed: false,
        },
        Command::Replay { record: vec![1] },
        Command::Branch,
        Command::Export {
            kind: ExportKind::ParentRecord,
            confirmed: true,
        },
    ] {
        let r = command(&mut c, cmd.clone());
        assert_eq!(codes(&r), ["control.not-now"], "{cmd:?}");
        match &r[0] {
            Message::Notification(n) => {
                assert_eq!(
                    n.params[0],
                    ("command".into(), Value::Name(cmd.name().into()))
                );
                assert_eq!(
                    n.params[1],
                    ("state".into(), Value::Enum("no-scenario".into()))
                );
                n.check().unwrap();
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(run_state(&mut c).state, RunState::NoScenario);
    }
    // An invalid manifest: `invalid`, with the model's codes; nothing else changes.
    let r = command(
        &mut c,
        Command::LoadScenario {
            bytes: b"florn = 1\nname = 3".to_vec(),
        },
    );
    assert!(
        codes(&r).iter().all(|c| c.starts_with("manifest.")),
        "{:?}",
        codes(&r)
    );
    assert_eq!(run_state(&mut c).state, RunState::Invalid);
    command(
        &mut c,
        Command::LoadScenario {
            bytes: s.text().into_bytes(),
        },
    );
    for (_, bytes) in s.modules() {
        command(&mut c, Command::LoadModule { bytes });
    }
    command(
        &mut c,
        Command::NewRun {
            seed: SeedChoice::Given(1),
            residency: florn_control::Residency::Resident,
        },
    );
    command(
        &mut c,
        Command::Advance {
            unit: Unit::ToTick(100),
        },
    );
    let before = run_state(&mut c);
    // One invalid parameter per command: the field is named; the state is unchanged.
    let good_content = Intervention::Message {
        actor: 2,
        port: 0,
        payload: vec![1],
    }
    .encode();
    let cases: Vec<(Command, &str)> = vec![
        (Command::LoadScenario { bytes: vec![] }, "bytes"),
        (Command::LoadModule { bytes: vec![] }, "bytes"),
        (Command::Admit { bytes: vec![] }, "bytes"),
        (
            Command::SetPacing {
                pacing: Pacing::TicksPerSecond(0),
            },
            "ticks-per-second",
        ),
        (
            Command::Intervene {
                target: 2,
                content: vec![],
                at: None,
            },
            "content",
        ),
        (
            Command::Intervene {
                target: 2,
                content: vec![9, 9, 9],
                at: None,
            },
            "content",
        ),
        (
            Command::Intervene {
                target: 1,
                content: good_content.clone(),
                at: None,
            },
            "target",
        ),
        (
            Command::Intervene {
                target: 2,
                content: good_content.clone(),
                at: Some(50),
            },
            "at",
        ),
        (
            Command::Intervene {
                target: 2,
                content: good_content.clone(),
                at: Some(2_000),
            },
            "at",
        ),
        (Command::Replay { record: vec![] }, "record"),
    ];
    for (cmd, field) in cases {
        let r = command(&mut c, cmd.clone());
        assert_eq!(codes(&r), ["control.invalid"], "{cmd:?}");
        match &r[0] {
            Message::Notification(n) => {
                assert_eq!(
                    n.params[0],
                    ("command".into(), Value::Name(cmd.name().into()))
                );
                assert_eq!(
                    n.params[2],
                    ("field".into(), Value::Text(field.into())),
                    "{cmd:?}"
                );
                n.check().unwrap();
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(run_state(&mut c), before, "{cmd:?} changed the state");
    }
    // Wrong-state commands while paused.
    for cmd in [
        Command::LoadScenario {
            bytes: s.text().into_bytes(),
        },
        Command::NewRun {
            seed: SeedChoice::Choose,
            residency: florn_control::Residency::Resident,
        },
        Command::Pause,
        Command::Replay { record: vec![1] },
    ] {
        assert_eq!(
            codes(&command(&mut c, cmd.clone())),
            ["control.not-now"],
            "{cmd:?}"
        );
        assert_eq!(run_state(&mut c), before);
    }
    // A record that is not one, and a replay of another scenario's record.
    command(&mut c, Command::EndRun);
    assert_eq!(
        codes(&command(
            &mut c,
            Command::Replay {
                record: vec![80, 75, 3, 4, 0]
            }
        )),
        ["file.unreadable"]
    );
    // Queries with an invalid parameter; a message a Host does not take; a wrong version.
    assert_eq!(
        codes(&send(
            &mut c,
            Message::Query(Query::TraceWindow { from: 0, count: 0 })
        )),
        ["control.invalid"]
    );
    assert_eq!(
        codes(&send(
            &mut c,
            Message::Subscribe(Subscription::Telemetry {
                channels: vec![],
                keep_one_in: 0
            })
        )),
        ["control.invalid"]
    );
    assert_eq!(
        codes(&send(&mut c, Message::Answer(Answer::Done))),
        ["control.invalid"]
    );
    let r = c.handle(Envelope {
        version: 7,
        message: Message::Command(Command::Pause),
    });
    assert_eq!(codes(&r), ["control.version"]);
    let mut bytes = encode(&Envelope::new(Message::Query(Query::RunState)));
    bytes[0] = 9;
    let replies = c.handle_bytes(&bytes);
    assert_eq!(replies.len(), 1);
    match decode(&replies[0]).unwrap().message {
        Message::Notification(n) => assert_eq!(n.code, "control.version"),
        other => panic!("{other:?}"),
    }
    // The transport-facing form answers a query.
    let replies = c.handle_bytes(&encode(&Envelope::new(Message::Query(Query::RunState))));
    assert!(matches!(
        decode(&replies[0]).unwrap().message,
        Message::Answer(Answer::RunState(_))
    ));
    // Every notification the Host produced in this test passes the catalogue.
    for e in c.run().unwrap().log().entries() {
        e.notification
            .check()
            .unwrap_or_else(|err| panic!("{:?}: {err:?}", e.notification));
    }
}

/// RFC 009 rules 10 to 12 over the interface: `branch` ends the run and starts the branch in one
/// command, says both, keeps the parent's record for `export { kind: parent-record }`, and the
/// branch moves back no further than its instant.
#[test]
fn a_branch_through_the_interface() {
    let mut c = controller();
    let s = spec();
    command(
        &mut c,
        Command::LoadScenario {
            bytes: s.text().into_bytes(),
        },
    );
    for m in s.modules().values() {
        command(&mut c, Command::LoadModule { bytes: m.clone() });
    }
    // Nothing to branch from yet.
    assert_eq!(
        codes(&command(&mut c, Command::Branch)),
        ["control.not-now"]
    );
    command(
        &mut c,
        Command::NewRun {
            seed: SeedChoice::Given(1),
            residency: florn_control::Residency::Resident,
        },
    );
    command(
        &mut c,
        Command::Advance {
            unit: Unit::ToTick(1_000),
        },
    );
    let before = run_state(&mut c);
    let parent_id = before.run_id.clone().unwrap();
    assert!(matches!(
        answer(&send(&mut c, Message::Query(Query::Support))),
        Some(Answer::Support(i)) if i.features.iter().any(|f| f == "branch")
    ));
    let r = command(&mut c, Command::Branch);
    assert_eq!(answer(&r), Some(&Answer::Done));
    let seen = codes(&r);
    let pos = |code: &str| {
        seen.iter()
            .position(|c| c == code)
            .unwrap_or_else(|| panic!("{code}: {seen:?}"))
    };
    assert!(pos("run.ended") < pos("host.capacity") && pos("host.capacity") < pos("run.branched"));
    let branched = r
        .iter()
        .find_map(|m| match m {
            Message::Notification(n) if n.code == "run.branched" => Some(n.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        branched.params[1],
        ("parent".into(), Value::Text(parent_id.clone()))
    );
    branched.check().unwrap();
    let after = run_state(&mut c);
    assert_eq!(after.state, RunState::Paused);
    assert_eq!(after.ended, None);
    assert_ne!(after.run_id, before.run_id);
    assert_eq!(
        (&after.chain, after.records, after.now),
        (&before.chain, before.records, before.now)
    );
    // The parent's record: previewed, then written; a run record whose run ended on the branch.
    let r = command(
        &mut c,
        Command::Export {
            kind: ExportKind::ParentRecord,
            confirmed: false,
        },
    );
    assert_eq!(codes(&r), ["export.preview"]);
    assert!(
        matches!(answer(&r), Some(Answer::Preview(p)) if p.kind == FileKind::RunRecord && !p.payloads)
    );
    let (name, bytes) = match answer(&command(
        &mut c,
        Command::Export {
            kind: ExportKind::ParentRecord,
            confirmed: true,
        },
    )) {
        Some(Answer::Exported {
            kind: FileKind::RunRecord,
            name,
            bytes,
        }) => (name.clone(), bytes.clone()),
        other => panic!("{other:?}"),
    };
    assert_eq!(name, format!("{parent_id}.florn-run"));
    let parent = florn_host::record::read(&bytes).unwrap();
    assert_eq!(parent.run.run_id, parent_id);
    assert_eq!(parent.run.ended, Some(florn_host::EndReason::User));
    assert_eq!(parent.run.records, after.records);
    assert!(parent.run.parent.is_none());
    assert!(parent.trace.is_some(), "the trace it kept");
    // The branch begins at its instant: one checkpoint kept, and nothing earlier to move to.
    assert!(matches!(
        answer(&send(&mut c, Message::Query(Query::Checkpoints))),
        Some(Answer::Checkpoints(list)) if list.len() == 1 && list[0].records == after.records
    ));
    let r = command(
        &mut c,
        Command::Advance {
            unit: Unit::ToTick(500),
        },
    );
    assert_eq!(codes(&r), ["control.invalid"]);
    assert_eq!(run_state(&mut c).now, after.now);
    // On to the end: the branch's record names the parent and replays on its own.
    command(
        &mut c,
        Command::Advance {
            unit: Unit::ToTick(u64::MAX),
        },
    );
    assert_eq!(run_state(&mut c).state, RunState::Ended);
    let record = match answer(&command(
        &mut c,
        Command::Export {
            kind: ExportKind::RunRecord {
                trace: false,
                payloads: false,
            },
            confirmed: true,
        },
    )) {
        Some(Answer::Exported { bytes, .. }) => bytes.clone(),
        other => panic!("{other:?}"),
    };
    let branch = florn_host::record::read(&record).unwrap();
    let p = branch.run.parent.clone().unwrap();
    assert_eq!(p.records, after.records);
    assert_eq!(Some(florn_model::canonical::hex(&p.chain)), after.chain);
    assert!(branch.run.chain.contains(&(p.records, p.chain)));
    // A branch of the ended branch is possible: a paused run at the end. After the next run
    // starts, the parent's record is gone.
    let r = command(&mut c, Command::Branch);
    assert_eq!(answer(&r), Some(&Answer::Done), "{:?}", codes(&r));
    assert_eq!(run_state(&mut c).state, RunState::Paused);
    command(&mut c, Command::EndRun);
    let r = command(&mut c, Command::Replay { record });
    assert_eq!(answer(&r), Some(&Answer::Done));
    command(
        &mut c,
        Command::Advance {
            unit: Unit::ToTick(u64::MAX),
        },
    );
    assert!(
        codes(&command(
            &mut c,
            Command::Export {
                kind: ExportKind::ParentRecord,
                confirmed: true,
            },
        )) == ["control.not-now"]
    );
}

/// RFC 009 rule 14 over the interface: `new-run { residency: swap }` says `host.swapping` with
/// the resident count, under the Host's cap on the pool; the digest is the resident run's.
#[test]
fn a_run_asked_to_swap_says_so_and_gives_the_resident_digest() {
    let mut c = controller();
    let s = spec();
    command(
        &mut c,
        Command::LoadScenario {
            bytes: s.text().into_bytes(),
        },
    );
    for m in s.modules().values() {
        command(&mut c, Command::LoadModule { bytes: m.clone() });
    }
    assert!(matches!(
        answer(&send(&mut c, Message::Query(Query::Support))),
        Some(Answer::Support(i)) if i.features.iter().any(|f| f == "swap")
    ));
    let r = command(
        &mut c,
        Command::NewRun {
            seed: SeedChoice::Given(1),
            residency: florn_control::Residency::Resident,
        },
    );
    assert!(!codes(&r).iter().any(|c| c == "host.swapping"));
    command(
        &mut c,
        Command::Advance {
            unit: Unit::ToTick(u64::MAX),
        },
    );
    let plain = run_state(&mut c);
    c.set_resident_limit(Some(1));
    let r = command(
        &mut c,
        Command::NewRun {
            seed: SeedChoice::Given(1),
            residency: florn_control::Residency::Swap,
        },
    );
    let said = r
        .iter()
        .find_map(|m| match m {
            Message::Notification(n) if n.code == "host.swapping" => Some(n.clone()),
            _ => None,
        })
        .expect("host.swapping");
    said.check().unwrap();
    assert_eq!(said.params[0], ("resident".into(), Value::Count(1)));
    assert_eq!(said.params[1], ("actors".into(), Value::Count(2)));
    command(
        &mut c,
        Command::Advance {
            unit: Unit::ToTick(u64::MAX),
        },
    );
    let swapped = run_state(&mut c);
    assert_eq!(
        (swapped.chain, swapped.records),
        (plain.chain, plain.records)
    );
}

/// NFR-004 (RFC 009 rule 21): a paced slice that overruns its share of the second says what
/// limited it, from the Host's own figures, once a second; a swapping run whose swaps keep up
/// with its activations names swapping.
#[test]
fn a_slow_slice_names_its_limiting_factor() {
    let platform = FixedPlatform {
        // Every reading of the clock adds 20 ms: each slice takes longer than a sixtieth.
        slice_us: 20_000,
        ..FixedPlatform::default()
    };
    let mut c = Controller::new(
        engine(),
        Admission::default(),
        Box::new(platform),
        Domains::core(),
        Options::default(),
    );
    let s = spec();
    command(
        &mut c,
        Command::LoadScenario {
            bytes: s.text().into_bytes(),
        },
    );
    for m in s.modules().values() {
        command(&mut c, Command::LoadModule { bytes: m.clone() });
    }
    let limited = |c: &mut C, residency: florn_control::Residency| {
        command(
            c,
            Command::NewRun {
                seed: SeedChoice::Given(1),
                residency,
            },
        );
        command(
            c,
            Command::SetPacing {
                pacing: Pacing::TicksPerSecond(600),
            },
        );
        command(
            c,
            Command::Advance {
                unit: Unit::ToTick(2_000),
            },
        );
        let mut said = Vec::new();
        for _ in 0..200 {
            for m in c.pump(10) {
                if let Message::Notification(n) = m
                    && n.code == "host.limited-by"
                {
                    n.check().unwrap();
                    said.push(n.params[0].1.clone());
                }
            }
            if c.state() != RunState::Running {
                break;
            }
        }
        command(c, Command::EndRun);
        said
    };
    let resident = limited(&mut c, florn_control::Residency::Resident);
    assert!(!resident.is_empty(), "said");
    assert!(
        resident.iter().all(|v| *v == Value::Enum("actors".into())),
        "{resident:?}"
    );
    // Two hundred slices, the clock read three times per slice at 20 ms each: once a second,
    // not once per slice.
    assert!(resident.len() < 30, "{}", resident.len());
    c.set_resident_limit(Some(1));
    let swapping = limited(&mut c, florn_control::Residency::Swap);
    assert!(
        swapping
            .iter()
            .any(|v| *v == Value::Enum("swapping".into())),
        "{swapping:?}"
    );
}
