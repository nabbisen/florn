//! The control interface's answers of release 0.4 beside those of 0.3 (RFC 012 finding F-12;
//! RFC 014 rule 2): the trailing optional section, recorded in both forms. Each OLD constant is the
//! encoding that release 0.3's code gave for the same value (made by that code, in a scratch checkout of
//! the commit before the section existed); this release gives the same bytes for it, since a section is
//! written only when it has something to say. Each NEW constant is the encoding with the section, which
//! a reader of this release reads, a reader that does not know the section's version skips by its
//! length, and an old Host's answer lacks. A change here is a change of format and needs a new
//! version number, never a silent edit of the record.
use florn_control::message::{
    ActorInfo, ActorSummary, InspectField, InspectValue, Inspection, PortSummary, ReportEntry,
    ScenarioSummary, SendSummary, TraceEntry,
};
use florn_control::{Answer, At, Envelope, Message, Query, decode, encode};

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

thread_local! {
    static MISSING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Compare with the record, or print it when the record is empty (the first run).
fn frozen(what: &str, found: &str, recorded: &str) {
    if recorded.is_empty() {
        eprintln!("{what}: {found}");
        MISSING.with(|m| m.set(true));
        return;
    }
    assert_eq!(found, recorded, "{what}: the format changed");
}

fn complete() {
    assert!(
        !MISSING.with(|m| m.get()),
        "a format has no recorded value yet"
    );
}

fn actor(id: u32, module: Option<&str>, builtin: Option<&str>) -> ActorSummary {
    ActorSummary {
        id,
        name: format!("a{id}"),
        module: module.map(str::to_string),
        class: "default".into(),
        ports: vec![PortSummary {
            id: 0,
            name: "p0".into(),
            ty: "test/pin".into(),
        }],
        config: vec![("period".into(), "5".into())],
        builtin: builtin.map(str::to_string),
    }
}

fn scenario(actors: Vec<ActorSummary>) -> Answer {
    Answer::Scenario(Some(ScenarioSummary {
        name: "s".into(),
        sha256: "ab".into(),
        tick: "1ns".into(),
        end: 100,
        tie: "by-identity".into(),
        seed: 1,
        actors,
        connections: Vec::new(),
        modules: Vec::new(),
        missing_modules: Vec::new(),
        domains: vec!["test".into()],
        changes: Vec::new(),
        classes: vec![(0, "control".into())],
        limits: Vec::new(),
    }))
}

fn window(reports: Vec<ReportEntry>) -> Answer {
    Answer::TraceWindow(vec![
        TraceEntry::Handled {
            at: At {
                tick: 7,
                microstep: 1,
            },
            class: 7,
            origin: 1,
            seq: 2,
            target: 3,
            metered: 4,
            draws: 0,
            effects: 1,
            sends: vec![SendSummary {
                seq: 0,
                port: 0,
                len: 3,
                digest: "cc".into(),
            }],
            reports,
        },
        TraceEntry::Intervention {
            at: At {
                tick: 9,
                microstep: 0,
            },
            seq: 1,
            len: 4,
        },
    ])
}

fn actor_answer(builtin: Option<&str>) -> Answer {
    Answer::Actor(Some(ActorInfo {
        summary: actor(1, builtin.is_none().then_some("prober"), builtin),
        alive: true,
        faults: Vec::new(),
        recent_telemetry: Vec::new(),
    }))
}

fn encoded(a: &Answer) -> String {
    let env = Envelope::new(Message::Answer(a.clone()));
    let bytes = encode(&env);
    assert_eq!(decode(&bytes).ok().as_ref(), Some(&env), "round trip");
    hex(&bytes)
}

/// The forms of 0.3: no built-in and no report, so no section: the very bytes of 0.3.
const SCENARIO_OLD: &str = "01000000050001010000007302000000616203000000316e7364000000000000000b00000062792d6964656e7469747901000000000000000200000001000000020000006131010600000070726f6265720700000064656661756c74010000000000000002000000703008000000746573742f70696e0100000006000000706572696f64010000003507000000020000006137000700000064656661756c74010000000000000002000000703008000000746573742f70696e0100000006000000706572696f64010000003500000000000000000000000001000000040000007465737400000000010000000007000000636f6e74726f6c00000000";
const WINDOW_OLD: &str = "0100000005030200000001070000000000000001000000070100000002000000000000000300000004000000000000000000000001000000010000000000000000000000000000000300000002000000636303090000000000000000000000010000000000000004000000";
const ACTOR_OLD: &str = "0100000005020101000000020000006131010600000070726f6265720700000064656661756c74010000000000000002000000703008000000746573742f70696e0100000006000000706572696f640100000035010000000000000000";

#[test]
fn an_answer_with_nothing_to_add_is_the_answer_of_0_3_byte_for_byte() {
    for (what, a, recorded) in [
        (
            "scenario",
            scenario(vec![actor(1, Some("prober"), None), actor(7, None, None)]),
            SCENARIO_OLD,
        ),
        ("trace window", window(Vec::new()), WINDOW_OLD),
        ("actor", actor_answer(None), ACTOR_OLD),
    ] {
        assert_eq!(encoded(&a), recorded, "{what}: 0.3's bytes");
    }
}

/// The forms of 0.4 with a section: the built-in's name, the reports of an entry.
const SCENARIO_NEW: &str = "01000000050001010000007302000000616203000000316e7364000000000000000b00000062792d6964656e7469747901000000000000000200000001000000020000006131010600000070726f6265720700000064656661756c74010000000000000002000000703008000000746573742f70696e0100000006000000706572696f64010000003507000000020000006137000700000064656661756c74010000000000000002000000703008000000746573742f70696e0100000006000000706572696f64010000003500000000000000000000000001000000040000007465737400000000010000000007000000636f6e74726f6c00000000011300000002000000000109000000746573742f73696e6b";
const WINDOW_NEW: &str = "01000000050302000000010700000000000000010000000701000000020000000000000003000000040000000000000000000000010000000100000000000000000000000000000003000000020000006363030900000000000000000000000100000000000000040000000142000000020000000200000000000000000000000100000010000000011000000001010101010101010101010101010101010000000000000001000000100000000000000000";
const ACTOR_NEW: &str = "0100000005020101000000020000006131000700000064656661756c74010000000000000002000000703008000000746573742f70696e0100000006000000706572696f640100000035010000000000000000010e0000000109000000746573742f73696e6b";
const INSPECT: &str = "010000000109100000f0";
const INSPECTION: &str = "01000000050d0107000000070000006275696c74696e02000000060000006672616d65730001000000330500000064656c617901000000000000f83f050000007469636b73";
const INSPECTION_NONE: &str = "01000000050d00";

#[test]
fn the_sections_and_the_inspect_messages_encode_as_recorded() {
    let report = |seq, payload| ReportEntry {
        seq,
        channel: 1,
        len: 16,
        payload,
    };
    for (what, bytes, recorded) in [
        (
            "scenario with a built-in",
            encoded(&scenario(vec![
                actor(1, Some("prober"), None),
                actor(7, None, Some("test/sink")),
            ])),
            SCENARIO_NEW,
        ),
        (
            "trace window with reports",
            encoded(&window(vec![report(0, Some(vec![1; 16])), report(1, None)])),
            WINDOW_NEW,
        ),
        (
            "actor that is a built-in",
            encoded(&actor_answer(Some("test/sink"))),
            ACTOR_NEW,
        ),
        (
            "inspect",
            hex(&encode(&Envelope::new(Message::Query(Query::Inspect {
                id: 0xF000_0010,
            })))),
            INSPECT,
        ),
        (
            "inspection",
            encoded(&Answer::Inspection(Some(Inspection {
                id: 7,
                kind: "builtin".into(),
                fields: vec![
                    InspectField {
                        name: "frames".into(),
                        value: InspectValue::Text("3".into()),
                    },
                    InspectField {
                        name: "delay".into(),
                        value: InspectValue::Real {
                            value: 1.5,
                            unit: "ticks".into(),
                        },
                    },
                ],
            }))),
            INSPECTION,
        ),
        (
            "inspection of nothing",
            encoded(&Answer::Inspection(None)),
            INSPECTION_NONE,
        ),
    ] {
        frozen(what, &bytes, recorded);
    }
    complete();
}

/// Both forms in one: the new bytes are the old bytes of the same answer with the section after
/// them, so a reader of the old prefix reads the old answer.
#[test]
fn the_section_is_after_the_answers_own_bytes() {
    let old = scenario(vec![actor(1, Some("prober"), None), actor(7, None, None)]);
    let new = scenario(vec![
        actor(1, Some("prober"), None),
        actor(7, None, Some("test/sink")),
    ]);
    let (a, b) = (encoded(&old), encoded(&new));
    assert_eq!(a, SCENARIO_OLD);
    assert_eq!(b, SCENARIO_NEW);
    assert!(b.starts_with(&a), "the answer's own bytes are unchanged");
    // After them: the section's version (1) and a block of 0x13 bytes, a list of two names, the
    // first absent and the second `test/sink`.
    let section = &b[a.len()..];
    assert!(section.starts_with("0113000000"), "{section}");
    assert!(section.ends_with(&hex(b"test/sink")));
    // The same for a window: the reports are in the section only, so the old form is a prefix of
    // the new one.
    let w = encoded(&window(vec![ReportEntry {
        seq: 0,
        channel: 1,
        len: 16,
        payload: Some(vec![1; 16]),
    }]));
    assert!(w.starts_with(WINDOW_OLD));
    assert!(w[WINDOW_OLD.len()..].starts_with("01"));
}
