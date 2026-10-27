//! RFC 005 §6 and RFC 007 §4: every message round-trips through the canonical encoding; a wrong
//! version is refused; an invalid parameter is named; the catalogue equals the RFC's tables.
use florn_control::message::{
    ActorInfo, ActorSummary, CapacityInfo, ExportPreview, FaultInfo, ModuleSummary, ParamSummary,
    PortSummary, RunStateInfo, ScenarioSummary, SupportInfo, TelemetryRecord, TraceEntry,
};
use florn_control::*;

fn roundtrip(m: Message) {
    let env = Envelope::new(m);
    let bytes = encode(&env);
    let back = decode(&bytes).unwrap_or_else(|e| panic!("{e:?} for {env:?}"));
    assert_eq!(back, env);
}

#[test]
fn every_message_round_trips() {
    let n = Notification::new(
        "actor.faulted",
        Severity::Error,
        Scope::actor(3),
        Ends::Activation,
    )
    .with("actor", Value::Identity(3))
    .with("fault", Value::Code("fault.budget".into()))
    .with("metered", Value::Count(7))
    .with("policy", Value::Enum("drop".into()))
    .with("q", Value::Quantity("1.5".into(), "ms".into()))
    .with("t", Value::Tick(9))
    .with(
        "i",
        Value::Instant(At {
            tick: 1,
            microstep: 2,
        }),
    )
    .with("x", Value::Text("a\"b".into()))
    .with("b", Value::Bytes(1 << 40))
    .at(At {
        tick: 5,
        microstep: 0,
    });
    // The canonical encoding orders the parameters by name (a map); a decoded notification has
    // them in that order.
    let mut n = n;
    n.params.sort_by(|a, b| a.0.cmp(&b.0));
    let commands = vec![
        Command::LoadScenario {
            bytes: b"florn = 1".to_vec(),
        },
        Command::LoadModule {
            bytes: vec![0, 97, 115, 109],
        },
        Command::Admit { bytes: vec![1] },
        Command::NewRun {
            seed: SeedChoice::Given(u64::MAX),
        },
        Command::NewRun {
            seed: SeedChoice::Choose,
        },
        Command::EndRun,
        Command::Advance {
            unit: Unit::OneEvent,
        },
        Command::Advance {
            unit: Unit::OneInstant,
        },
        Command::Advance {
            unit: Unit::OneTick,
        },
        Command::Advance {
            unit: Unit::ToTick(42),
        },
        Command::Pause,
        Command::SetPacing {
            pacing: Pacing::Fastest,
        },
        Command::SetPacing {
            pacing: Pacing::TicksPerSecond(1_000),
        },
        Command::Intervene {
            target: 2,
            content: vec![1, 2, 3],
            at: None,
        },
        Command::Intervene {
            target: 2,
            content: vec![1],
            at: Some(10),
        },
        Command::Export {
            kind: ExportKind::RunRecord {
                trace: true,
                payloads: false,
            },
            confirmed: false,
        },
        Command::Export {
            kind: ExportKind::DiagnosticLog,
            confirmed: true,
        },
        Command::Export {
            kind: ExportKind::Package,
            confirmed: true,
        },
        Command::Replay {
            record: vec![9; 100],
        },
    ];
    for c in commands {
        roundtrip(Message::Command(c));
    }
    for q in [
        Query::Scenario,
        Query::RunState,
        Query::Actor { id: 1 },
        Query::TraceWindow { from: 5, count: 10 },
        Query::Capacity,
        Query::Support,
    ] {
        roundtrip(Message::Query(q));
    }
    for s in [
        Subscription::Telemetry {
            channels: vec![1, 2],
            keep_one_in: 3,
        },
        Subscription::Events,
        Subscription::Progress,
        Subscription::Display,
    ] {
        roundtrip(Message::Subscribe(s));
    }
    for k in [
        SubscriptionKind::Telemetry,
        SubscriptionKind::Events,
        SubscriptionKind::Progress,
        SubscriptionKind::Display,
    ] {
        roundtrip(Message::Unsubscribe(k));
    }
    roundtrip(Message::Notification(n.clone()));
    let module = ModuleSummary {
        name: "echo".into(),
        source_sha256: "ab".into(),
        contract: "florn:core/guest@0.1.0".into(),
        kind: "echo".into(),
        description: Some("d".into()),
        port_kinds: vec![("p".into(), "core/port".into(), 0, 8)],
        params: vec![ParamSummary {
            name: "period".into(),
            ty: "u64".into(),
            required: false,
            default: Some("10".into()),
            unit: None,
        }],
        channels: vec![(1, "r".into())],
        needs: vec![("memory".into(), 131_072)],
    };
    let actor = ActorSummary {
        id: 1,
        name: "a1".into(),
        module: Some("echo".into()),
        class: "default".into(),
        ports: vec![PortSummary {
            id: 0,
            name: "p0".into(),
            ty: "core/port".into(),
        }],
    };
    let answers = vec![
        Answer::Scenario(None),
        Answer::Scenario(Some(ScenarioSummary {
            name: "s".into(),
            sha256: "00".into(),
            tick: "1ns".into(),
            end: 100,
            tie: "by-seed".into(),
            seed: 3,
            actors: vec![actor.clone()],
            connections: vec![florn_control::message::ConnectionSummary {
                id: 100,
                name: "c".into(),
                ports: vec![(1, 0), (2, 0)],
            }],
            modules: vec![module.clone()],
            missing_modules: vec!["ff".into()],
        })),
        Answer::RunState(RunStateInfo {
            state: RunState::Paused,
            now: Some(At {
                tick: 1,
                microstep: 1,
            }),
            handled: 2,
            records: 3,
            chain: Some("aa".into()),
            seed: Some(1),
            pacing: Pacing::TicksPerSecond(5),
            ended: None,
            reproducible: true,
            run_id: Some("r".into()),
        }),
        Answer::Actor(None),
        Answer::Actor(Some(ActorInfo {
            summary: actor,
            alive: false,
            faults: vec![FaultInfo {
                at: At {
                    tick: 3,
                    microstep: 0,
                },
                code: "fault.trap".into(),
                metered: 4,
                parameter: 0,
            }],
            recent_telemetry: vec![TelemetryRecord {
                at: At::default(),
                actor: 1,
                channel: 1,
                payload: vec![1],
            }],
        })),
        Answer::TraceWindow(vec![
            TraceEntry::Handled {
                at: At {
                    tick: 1,
                    microstep: 0,
                },
                class: 2,
                origin: 1,
                seq: 5,
                target: 2,
                metered: 100,
                draws: 1,
                effects: 2,
            },
            TraceEntry::Fault {
                at: At {
                    tick: 1,
                    microstep: 0,
                },
                class: 1,
                origin: 1,
                seq: 6,
                actor: 1,
                code: "fault.stack".into(),
                parameter: 0,
                metered: 50,
            },
            TraceEntry::Intervention {
                at: At {
                    tick: 2,
                    microstep: 1,
                },
                seq: 1,
                len: 12,
            },
        ]),
        Answer::Capacity(CapacityInfo {
            instances: 10,
            memory_bytes: 1 << 30,
            swap_cost_us_per_kb: Some(1),
        }),
        Answer::Support(SupportInfo {
            control_version: 1,
            florn_version: "0.0.0".into(),
            host_build: "native".into(),
            engine: "wasmtime".into(),
            engine_version: "48.0.6".into(),
            arch: "x86_64".into(),
            features: vec!["core".into()],
        }),
        Answer::Admitted {
            module,
            admitted_sha256: "cd".into(),
        },
        Answer::Preview(ExportPreview {
            kind: FileKind::RunRecord,
            bytes: 10,
            payloads: true,
            modules: vec![("echo".into(), "ab".into())],
        }),
        Answer::Exported {
            kind: FileKind::Package,
            name: "s.florn".into(),
            bytes: vec![80, 75],
        },
        Answer::Done,
    ];
    for a in answers {
        roundtrip(Message::Answer(a));
    }
    roundtrip(Message::Telemetry(TelemetryRecord {
        at: At {
            tick: 1,
            microstep: 0,
        },
        actor: 2,
        channel: 1,
        payload: vec![0; 8],
    }));
    roundtrip(Message::Progress(Progress {
        state: RunState::Running,
        now: At {
            tick: 7,
            microstep: 2,
        },
        handled: 1,
        records: 1,
    }));
    // The notification's own encoding is the same bytes inside and outside an envelope.
    let mut bytes = encode(&Envelope::new(Message::Notification(n.clone())));
    assert_eq!(bytes.drain(..5).collect::<Vec<u8>>(), [1, 0, 0, 0, 4]);
    assert_eq!(bytes, n.encode());
}

#[test]
fn a_wrong_version_and_malformed_bytes_are_refused_with_their_codes() {
    let mut bytes = encode(&Envelope::new(Message::Command(Command::Pause)));
    bytes[0] = 2;
    let e = decode(&bytes).unwrap_err();
    assert_eq!(e, DecodeError::Version(2));
    let n = e.notification();
    assert_eq!(n.code, "control.version");
    assert_eq!(
        n.params,
        vec![
            ("front-end".to_string(), Value::Text("2".into())),
            ("host".to_string(), Value::Text("1".into()))
        ]
    );
    n.check().unwrap();
    for bad in [
        vec![],
        vec![1, 0, 0, 0],
        vec![1, 0, 0, 0, 99],
        vec![1, 0, 0, 0, 0, 200],
        vec![1, 0, 0, 0, 0, 6, 0],
    ] {
        let e = decode(&bad).unwrap_err();
        assert!(matches!(e, DecodeError::Malformed(_)), "{bad:?}: {e:?}");
        assert_eq!(e.notification().code, "control.invalid");
        e.notification().check().unwrap();
    }
}

#[test]
fn structural_validation_names_the_field() {
    let cases: Vec<(Command, &str, &str)> = vec![
        (
            Command::LoadScenario { bytes: vec![] },
            "missing-field",
            "bytes",
        ),
        (
            Command::LoadModule { bytes: vec![] },
            "missing-field",
            "bytes",
        ),
        (Command::Admit { bytes: vec![] }, "missing-field", "bytes"),
        (
            Command::Replay { record: vec![] },
            "missing-field",
            "record",
        ),
        (
            Command::SetPacing {
                pacing: Pacing::TicksPerSecond(0),
            },
            "out-of-range",
            "ticks-per-second",
        ),
        (
            Command::Intervene {
                target: 1,
                content: vec![],
                at: None,
            },
            "missing-field",
            "content",
        ),
    ];
    for (c, reason, field) in cases {
        let n = c.validate().unwrap_err();
        assert_eq!(n.code, "control.invalid");
        assert_eq!(
            n.params[0],
            ("command".to_string(), Value::Name(c.name().into()))
        );
        assert_eq!(
            n.params[1],
            ("reason".to_string(), Value::Enum(reason.into()))
        );
        assert_eq!(
            n.params[2],
            ("field".to_string(), Value::Text(field.into()))
        );
        assert_eq!(n.scope.command.as_deref(), Some(c.name()));
        n.check().unwrap();
    }
    assert!(Query::TraceWindow { from: 0, count: 0 }.validate().is_err());
    assert!(
        Query::TraceWindow {
            from: 0,
            count: Query::MAX_WINDOW + 1
        }
        .validate()
        .is_err()
    );
    assert!(
        Query::TraceWindow {
            from: 0,
            count: Query::MAX_WINDOW
        }
        .validate()
        .is_ok()
    );
    assert!(
        Subscription::Telemetry {
            channels: vec![],
            keep_one_in: 0
        }
        .validate()
        .is_err()
    );
    assert!(
        Command::Intervene {
            target: 1,
            content: vec![1],
            at: None
        }
        .is_intervention()
    );
    assert!(!Command::EndRun.is_intervention());
}

/// The catalogue equals the tables of RFC 007 §4.1 to §4.4 and the enumerations of §4.5.
#[test]
fn the_catalogue_is_the_rfc() {
    let rfc = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../rfcs/done/007-error-model-and-internationalisation.md"
    ))
    .unwrap();
    let start = rfc.find("### 4.1").unwrap();
    let end = rfc.find("### 4.5").unwrap();
    let mut rows: Vec<&str> = rfc[start..end]
        .lines()
        .filter(|l| l.starts_with("| `"))
        .collect();
    // Section 4.6, the network domain's codes, follows the enumerations.
    let start = rfc.find("### 4.6").unwrap();
    let end = rfc.find("## 5.").unwrap();
    rows.extend(rfc[start..end].lines().filter(|l| l.starts_with("| `")));
    assert_eq!(rows.len(), CATALOGUE.len(), "one entry per row");
    for (row, entry) in rows.iter().zip(CATALOGUE) {
        let cells: Vec<&str> = row.trim_matches('|').split('|').map(str::trim).collect();
        let code = cells[0].trim_matches('`');
        assert_eq!(entry.name, code);
        assert_eq!(Code::parse(code), Some(entry.code));
        assert_eq!(entry.code.as_str(), code);
        let params: Vec<(String, String)> = cells[1]
            .split('`')
            .filter(|p| p.contains(':'))
            .map(|p| {
                let (n, t) = p.split_once(": ").unwrap();
                (n.to_string(), t.to_string())
            })
            .collect();
        assert_eq!(params.len(), entry.params.len(), "{code}");
        for ((n, t), (en, et)) in params.iter().zip(entry.params) {
            assert_eq!(n, en, "{code}");
            let expected = match et {
                ParamType::Identity => "identity".to_string(),
                ParamType::Name => "name".into(),
                ParamType::Count => "count".into(),
                ParamType::Bytes => "bytes".into(),
                ParamType::Quantity => "quantity".into(),
                ParamType::Tick => "tick".into(),
                ParamType::Instant => "instant".into(),
                ParamType::Code => "code".into(),
                ParamType::Text => "text".into(),
                ParamType::Enum(set) => format!("enum({set})"),
            };
            assert_eq!(*t, expected, "{code}.{n}");
            if let ParamType::Enum(set) = et {
                assert!(
                    florn_control::catalogue::enumeration(set).is_some(),
                    "{code}: enumeration {set} exists"
                );
            }
        }
        assert_eq!(entry.severity.as_str(), cells[2], "{code}");
        let scope = match entry.scope {
            ScopeKind::File => "file",
            ScopeKind::Field => "field",
            ScopeKind::Run => "run",
            ScopeKind::Actor => "actor",
            ScopeKind::Command => "command",
            ScopeKind::None => "—",
        };
        assert_eq!(scope, cells[3], "{code}");
        assert_eq!(entry.ends.as_str(), cells[4], "{code}");
    }
    // Every enumeration of §4.5 is listed, and every fault code has its parameters.
    let sets_line = rfc
        .lines()
        .find(|l| l.starts_with("Each `enum(set)` above"))
        .unwrap();
    for (name, _) in florn_control::catalogue::ENUMERATIONS {
        assert!(sets_line.contains(&format!("`{name}`")), "{name}");
    }
    assert_eq!(FaultCode::parse("fault.quota").unwrap().params().len(), 2);
    assert_eq!(FaultCode::Memory.as_str(), "fault.memory");
    // A notification with a wrong parameter type or an enumeration value outside its set fails the check.
    let bad = Code::RunEnded.notification(Scope::run(), vec![Value::Enum("whenever".into())]);
    assert!(matches!(
        bad.check(),
        Err(florn_control::catalogue::CatalogueError::EnumValue {
            set: "end-reason",
            ..
        })
    ));
    let bad = Code::RunEnded.notification(Scope::run(), vec![Value::Text("end-reached".into())]);
    assert!(matches!(
        bad.check(),
        Err(florn_control::catalogue::CatalogueError::Parameter { .. })
    ));
    let good = Code::RunEnded.notification(Scope::run(), vec![Value::Enum("end-reached".into())]);
    assert_eq!(good.check().unwrap().name, "run.ended");
    assert!(
        Notification::new("no.such", Severity::Info, Scope::run(), Ends::Nothing)
            .check()
            .is_err()
    );
}
