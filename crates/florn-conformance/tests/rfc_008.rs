//! RFC 008 §12, "Interface": a second domain implemented against §2 with no change to the Host.
//! The kit's test domain exercises every operation of rule 1 here; the network domain of PR-16
//! is the first real one. Also RFC 008 rule 2 (`connection.mixed-types`), rule 4 (interventions
//! of a domain kind, recorded and replayed), rule 6 (`module.export-unsupported`), the units of
//! RFC 004 rule 27, and the domain's state in a checkpoint (RFC 003 §6).
use std::collections::BTreeMap;

use florn_admission::Admission;
use florn_admission::world::{FuncSig, WorldSpec};
use florn_conformance::host::{self, handled, log_param, reports, sends, texts};
use florn_conformance::test_domain::{self as td, CLASS_FRAME, CLASS_WIRE_STATE, TestDomain};
use florn_control::{Answer, Command, Envelope, Message, SeedChoice};
use florn_engine::FlatType;
use florn_host::domain::{Domain, DomainWorld};
use florn_host::{Advance, Checkpoint, Controller, Domains, HostError, Run, Seed};
use florn_kernel::trace::Comparison;
use florn_kernel::{Record, Stream, StreamKey};
use florn_model::canonical::Encoder;
use florn_model::manifest::{DomainMap, DomainValue};
use florn_platform::FixedPlatform;

/// Two probers over one wire.
fn pair(end: u64, seed: u64, attrs: &str) -> String {
    td::text(
        "pair",
        end,
        seed,
        &format!(
            "{}{}{}",
            td::prober(1, 1),
            td::prober(2, 1),
            td::wire(100, &[(1, 0), (2, 0)], attrs)
        ),
    )
}

fn domain_codes(e: HostError) -> Vec<String> {
    match e {
        HostError::Domain(ns) => ns.iter().map(|n| n.code.clone()).collect(),
        HostError::Refused(e) => e.codes().iter().map(|c| c.to_string()).collect(),
        other => vec![other.to_string()],
    }
}

fn dropped_events(run: &host::Host) -> Vec<String> {
    run.log()
        .entries()
        .iter()
        .filter(|e| e.notification.code == "domain.event")
        .filter(|e| {
            e.notification
                .params
                .iter()
                .any(|(k, v)| k == "event" && format!("{v:?}").contains("wire-dropped"))
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

#[test]
fn name_types_and_validation_in_one_pass() {
    // A scenario of the domain starts under a Host that has it.
    let run = td::run(&pair(100, 1, "delay = \"2ns\""));
    assert_eq!(run.domains().len(), 1);
    assert_eq!(run.domains()[0].name(), "test");
    // The same scenario under the core alone: `domain.unknown`.
    let manifest = td::manifest(&pair(100, 1, ""));
    let e = Run::new(
        host::engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        &Domains::core(),
        manifest,
        &td::modules(),
        Seed::Given(1),
        host::options(),
    )
    .err()
    .expect("refused");
    assert_eq!(domain_codes(e), vec!["domain.unknown"]);
    // Every refusal of the domain in one pass: two unknown keys, a wrong type, a sink on a
    // core port, and a connection type the domain does not own.
    let body = format!(
        "{}{}{}\n[[actors]]\nid = 3\nname = \"a3\"\nkind = \"builtin\"\nbuiltin = \"test/sink\"\nports = [{{ id = 0, name = \"p0\", type = \"core/port\" }}]\n\n[[connections]]\nid = 101\nname = \"w101\"\ntype = \"test/rope\"\nports = [\"a1.p0\", \"a3.p0\"]\n\n[test]\njunk = 1\n",
        td::prober(1, 1),
        td::prober(2, 1),
        td::wire(100, &[(1, 0), (2, 0)], "bogus = 1\nenabled = 3")
    );
    let text = td::text("bad", 100, 1, &body);
    // The core refuses the mixed connection first (RFC 008 rule 2); `core` itself is a domain
    // the scenario did not declare (RFC 004 rule 5).
    let parsed = florn_model::Manifest::parse(&text, &florn_model::HostMaxima::native());
    assert_eq!(
        parsed.unwrap_err().codes(),
        vec!["domain.undeclared", "connection.mixed-types"]
    );
    let body = body.replace("type = \"core/port\"", "type = \"test/pin\"");
    let e = td::try_start(&td::text("bad", 100, 1, &body), host::options())
        .err()
        .expect("refused");
    let mut codes = domain_codes(e);
    codes.sort();
    assert_eq!(
        codes,
        vec![
            "manifest.type",
            "manifest.unknown-key",
            "manifest.unknown-key",
            "reference.unresolved",
        ]
    );
}

#[test]
fn units_registered_by_the_domain_reach_the_config() {
    let with = |value: &str| {
        let body = format!(
            "{}[actors.config]\nweight = \"{value}\"\n{}{}",
            td::prober(1, 1),
            td::prober(2, 1),
            td::wire(100, &[(1, 0), (2, 0)], "")
        );
        td::try_start(&td::text("units", 50, 1, &body), host::options())
    };
    assert!(with("2khop").is_ok(), "a registered unit");
    let e = with("2ohm").err().expect("an unknown unit is refused");
    assert_eq!(domain_codes(e), vec!["quantity.unknown-unit"]);
}

#[test]
fn classes_are_named_and_ordered() {
    let run = td::start(&pair(100, 1, ""));
    let names: Vec<(u8, &str)> = run.class_names().iter().map(|(c, n)| (*c, *n)).collect();
    assert_eq!(
        names,
        vec![
            (0, "control"),
            (1, "timer"),
            (2, "message"),
            (3, "wire-state"),
            (4, "frame"),
            (5, "wire-dropped")
        ]
    );
}

#[test]
fn a_world_function_outside_the_shape_table_is_refused_at_admission() {
    // The test world as the domain declares it is admitted.
    let mut adm = Admission::default();
    adm.add_world(TestDomain::new().world().unwrap().spec());
    assert!(
        adm.admit("prober", &td::guest_bytes(), &host::kit_limits())
            .is_ok()
    );
    // The same world with one more import, of a shape no Host calls: refused (RFC 008 rule 6).
    let mut world: WorldSpec = TestDomain::new().world().unwrap().spec();
    world.imports[1].functions.push(FuncSig {
        name: "bad".into(),
        params: vec![FlatType::I64, FlatType::I64],
        results: vec![],
    });
    let mut adm = Admission::default();
    adm.add_world(world);
    let Err(e) = adm.admit("prober", &td::guest_bytes(), &host::kit_limits()) else {
        panic!("admitted a world function outside the table");
    };
    assert_eq!(e.codes(), vec!["module.export-unsupported"]);
    let _ = DomainWorld {
        name: String::new(),
        imports_ids: vec![],
        exports_ids: vec![],
        imports: vec![],
        exports: vec![],
    };
}

#[test]
fn delivery_follows_the_wire_delay_and_the_loss_stream() {
    // Delay: a frame sent at tick t arrives at t + 7, as a frame event (class 4).
    let run = td::run(&pair(60, 3, "delay = \"7ns\""));
    // A frame still in flight at `end` is never handled (RFC 004 rule 14).
    let sent: Vec<_> = sends(&run, 1)
        .into_iter()
        .filter(|(s, _, _)| s.tick + 7 < 60)
        .collect();
    let got = reports(&run, 2, 1);
    assert!(sent.len() >= 10);
    assert_eq!(sent.len(), got.len());
    for ((s, _, payload), (r, report)) in sent.iter().zip(&got) {
        assert_eq!(r.tick, s.tick + 7);
        assert_eq!(report, payload, "the payload is unchanged");
    }
    let frames: Vec<_> = handled(&run, 2)
        .into_iter()
        .filter(|k| k.class == CLASS_FRAME)
        .collect();
    assert_eq!(frames.len(), got.len());
    // Loss: a quarter of the frames, drawn from the wire's stream (RFC 001 rules 28 to 31):
    // the same under two runs, and the draws are the wire's, not a guest's.
    let a = td::run(&pair(4_000, 5, "loss = 0.25"));
    let b = td::run(&pair(4_000, 5, "loss = 0.25"));
    assert_eq!(a.chain_hex(), b.chain_hex());
    let lost = dropped_events(&a).len();
    let total = sends(&a, 1).len() + sends(&a, 2).len();
    assert!(total >= 1_000, "{total}");
    let ratio = lost as f64 / total as f64;
    assert!((0.18..0.32).contains(&ratio), "lost {lost} of {total}");
    assert!(dropped_events(&a).iter().all(|r| r.contains("loss")));
    // A guest's own draws were untouched: the stream of actor 1 never advanced.
    let state = a.kernel().actor(1).unwrap();
    assert_eq!(state.stream, Stream::for_actor(5, 1));
    let _ = StreamKey::derive(5, 100, "loss");
    // Replay from the record matches.
    let record = a.run_toml();
    let mut replay = Run::replay(
        host::engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        &td::domains(),
        td::manifest(&pair(4_000, 5, "loss = 0.25")),
        &td::modules(),
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
fn a_change_and_an_intervention_have_the_same_effect() {
    // Scheduled changes: the wire goes down at 50 and up at 100; `on-wire` reaches both ends,
    // frames sent in between are dropped with `down`, `probe` answers the per-port cost.
    let body = format!(
        "{}{}{}[connections.attrs.per-port]\n\"a1.p0\" = {{ cost = 7 }}\n\n[[changes]]\nat = \"50ns\"\ntarget = \"w100\"\n[changes.payload]\nenabled = false\n\n[[changes]]\nat = \"100ns\"\ntarget = \"w100\"\n[changes.payload]\nenabled = true\n",
        td::prober(1, 1),
        td::prober(2, 1),
        td::wire(100, &[(1, 0), (2, 0)], "")
    );
    let run = td::run(&td::text("toggle", 150, 2, &body));
    for actor in [1, 2] {
        let wire: Vec<(u64, u32)> = reports(&run, actor, 2)
            .iter()
            .map(|(i, p)| (i.tick, u32::from_le_bytes(p[..4].try_into().unwrap())))
            .collect();
        assert_eq!(wire, vec![(50, 0), (100, 1)], "actor {actor}");
        let state: Vec<u8> = handled(&run, actor)
            .into_iter()
            .filter(|k| k.class == CLASS_WIRE_STATE)
            .map(|k| k.class)
            .collect();
        assert_eq!(state.len(), 2);
    }
    let cost = |actor| -> Vec<u32> {
        reports(&run, actor, 3)
            .iter()
            .map(|(_, p)| u32::from_le_bytes(p[..4].try_into().unwrap()))
            .collect()
    };
    assert_eq!(cost(1), vec![7, 7]);
    assert_eq!(cost(2), vec![1, 1]);
    let down: Vec<_> = dropped_events(&run);
    assert!(!down.is_empty() && down.iter().all(|r| r.contains("down")));
    for (i, _) in reports(&run, 2, 1) {
        assert!(!(50..100).contains(&i.tick), "no frame arrives while down");
    }
    // What the changes did, as the actors saw it: the wire state and the frames that arrived.
    let seen = |run: &host::Host| -> Vec<(u64, Vec<u8>)> {
        let mut v: Vec<(u64, Vec<u8>)> = Vec::new();
        for actor in [1, 2] {
            for channel in [1, 2, 3] {
                v.extend(
                    reports(run, actor, channel)
                        .into_iter()
                        .map(|(i, p)| (i.tick, p)),
                );
            }
        }
        v
    };
    let changed = seen(&run);

    // The same as interventions of the domain's kind `cut` (RFC 008 rule 4): recorded as
    // inputs, applied at the boundary, with the same effect on the actors (the trace differs
    // by the interventions' own records, RFC 001 rule 44), and replayed to the same digest.
    let plain = td::text(
        "toggle",
        150,
        2,
        &body[..body.find("\n[[changes]]").unwrap()],
    );
    let mut run = td::start(&plain);
    let cut = |enabled: bool| -> Vec<u8> {
        let mut m = DomainMap::new();
        m.insert("kind".into(), DomainValue::Text("cut".into()));
        m.insert("enabled".into(), DomainValue::Boolean(enabled));
        let mut e = Encoder::default();
        florn_model::canonical::domain_map(&mut e, &m);
        e.finish()
    };
    run.intervene_map(Some(50), 100, &cut(false)).unwrap();
    run.intervene_map(Some(100), 100, &cut(true)).unwrap();
    // A malformed one is refused at request time, nothing queued.
    let mut bad = DomainMap::new();
    bad.insert("kind".into(), DomainValue::Text("cut".into()));
    bad.insert("enabled".into(), DomainValue::Integer(3));
    let mut e = Encoder::default();
    florn_model::canonical::domain_map(&mut e, &bad);
    let n = run.intervene_map(None, 100, &e.finish()).unwrap_err();
    assert_eq!(n.code, "control.invalid");
    assert!(
        run.intervene_map(None, 999, &cut(false)).is_err(),
        "not a wire"
    );
    run.run_to_end();
    assert_eq!(run.inputs().interventions.len(), 2);
    assert_eq!(
        seen(&run),
        changed,
        "a change and an intervention: one effect"
    );
    let chain = run.chain_hex();
    let applied: Vec<String> = run
        .log()
        .entries()
        .iter()
        .filter(|e| e.notification.code == "intervention.applied")
        .filter_map(|e| {
            e.notification
                .params
                .iter()
                .find(|(k, _)| k == "kind")
                .map(|(_, v)| format!("{v:?}"))
        })
        .collect();
    assert_eq!(applied.len(), 2);
    assert!(applied.iter().all(|k| k.contains("cut")));
    let record = run.run_toml();
    let mut replay = Run::replay(
        host::engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        &td::domains(),
        td::manifest(&plain),
        &td::modules(),
        &record,
        host::options(),
    )
    .unwrap();
    replay.run_to_end();
    assert!(matches!(
        replay.comparison(),
        Some(Comparison::Matched { .. })
    ));
    assert_eq!(replay.chain_hex(), chain);
}

#[test]
fn interventions_of_a_domain_kind_through_the_control_interface() {
    let mut c: Controller<florn_engine_wasmtime::WasmtimeEngine> = Controller::new(
        host::engine(),
        Admission::default(),
        Box::new(FixedPlatform::default()),
        td::domains(),
        host::options(),
    );
    let send = |c: &mut Controller<_>, cmd: Command| c.handle(Envelope::new(Message::Command(cmd)));
    let done = |out: &[Message]| {
        out.iter()
            .any(|m| matches!(m, Message::Answer(Answer::Done)))
    };
    let codes = |out: &[Message]| -> Vec<String> {
        out.iter()
            .filter_map(|m| match m {
                Message::Notification(n) => Some(n.code.clone()),
                _ => None,
            })
            .collect()
    };
    let text = pair(200, 9, "");
    let out = send(
        &mut c,
        Command::LoadScenario {
            bytes: text.into_bytes(),
        },
    );
    assert!(done(&out), "{out:?}");
    let out = send(
        &mut c,
        Command::LoadModule {
            bytes: td::guest_bytes(),
        },
    );
    assert!(done(&out), "{out:?}");
    let out = send(
        &mut c,
        Command::NewRun {
            seed: SeedChoice::Given(9),
            residency: florn_control::Residency::Resident,
        },
    );
    assert!(done(&out), "{out:?}");
    let cut = |kind: &str| -> Vec<u8> {
        let mut m = DomainMap::new();
        m.insert("kind".into(), DomainValue::Text(kind.into()));
        m.insert("enabled".into(), DomainValue::Boolean(false));
        let mut e = Encoder::default();
        florn_model::canonical::domain_map(&mut e, &m);
        e.finish()
    };
    let out = send(
        &mut c,
        Command::Intervene {
            target: 100,
            content: cut("cut"),
            at: Some(20),
        },
    );
    assert!(done(&out), "{out:?}");
    // An unknown kind, and a target that is not the wire: `control.invalid`.
    let out = send(
        &mut c,
        Command::Intervene {
            target: 100,
            content: cut("saw"),
            at: None,
        },
    );
    assert_eq!(codes(&out), vec!["control.invalid"]);
    let out = send(
        &mut c,
        Command::Intervene {
            target: 1,
            content: cut("cut"),
            at: None,
        },
    );
    assert_eq!(codes(&out), vec!["control.invalid"]);
    // A load under a Host without the domain: refused at load with `domain.unknown`.
    let mut core: Controller<florn_engine_wasmtime::WasmtimeEngine> = Controller::new(
        host::engine(),
        Admission::default(),
        Box::new(FixedPlatform::default()),
        Domains::core(),
        host::options(),
    );
    let out = core.handle(Envelope::new(Message::Command(Command::LoadScenario {
        bytes: pair(200, 9, "").into_bytes(),
    })));
    assert_eq!(codes(&out), vec!["domain.unknown"]);
}

#[test]
fn a_builtin_runs_as_an_actor_with_metered_zero() {
    let body = format!(
        "{}{}{}",
        td::prober(1, 1),
        td::sink(2),
        td::wire(100, &[(1, 0), (2, 0)], "delay = \"1ns\"")
    );
    let run = td::run(&td::text("sink", 60, 4, &body));
    let counts: Vec<u64> = reports(&run, 2, 1)
        .iter()
        .map(|(_, p)| u64::from_le_bytes(p[..8].try_into().unwrap()))
        .collect();
    assert_eq!(counts.len(), sends(&run, 1).len());
    assert_eq!(counts, (1..=counts.len() as u64).collect::<Vec<_>>());
    assert!(texts(&run).contains(&(2, "sink ready".to_string())));
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
    assert!(log_param(&run, "actor.faulted", "actor").is_none());
}

#[test]
fn decoders_and_telemetry_layouts_are_the_domains() {
    let run = td::start(&pair(10, 1, ""));
    let d = &run.domains()[0];
    assert_eq!(
        d.decode("probe-proto", &7u32.to_le_bytes()),
        Some(vec![("value".to_string(), "7".to_string())])
    );
    assert_eq!(d.decode("other", &[1, 2, 3, 4]), None);
    assert_eq!(d.telemetry(), vec![("count", vec![("n", "u64")])]);
}

#[test]
fn a_checkpoint_carries_the_domain_state() {
    let text = pair(3_000, 11, "loss = 0.5\ndelay = \"3ns\"");
    // The uninterrupted run.
    let whole = td::run(&text);
    // The same run checkpointed at tick 1000 and continued from the checkpoint.
    let mut first = td::start(&text);
    first.advance(Advance::ToTick(1_000));
    let checkpoint = first.checkpoint().unwrap();
    assert!(
        checkpoint.domains.contains_key("test"),
        "the domain's state is in the checkpoint"
    );
    let bytes = checkpoint.encode();
    let again = Checkpoint::decode(&bytes).unwrap();
    assert_eq!(again.domains, checkpoint.domains);
    let mut second = Run::from_checkpoint(
        host::engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        &td::domains(),
        td::manifest(&text),
        &td::modules(),
        &again,
        host::options(),
    )
    .unwrap();
    second.run_to_end();
    assert_eq!(second.chain_hex(), whole.chain_hex());
    // Without the domain's state the stream would start over: a different digest.
    let mut stripped = checkpoint.clone();
    stripped.domains = BTreeMap::new();
    let mut third = Run::from_checkpoint(
        host::engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        &td::domains(),
        td::manifest(&text),
        &td::modules(),
        &stripped,
        host::options(),
    )
    .unwrap();
    third.run_to_end();
    assert_ne!(third.chain_hex(), whole.chain_hex());
}
