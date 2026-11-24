//! The trailing optional section of an answer (RFC 012 finding F-12) and the inspect query: an
//! answer with nothing to add is byte for byte the answer of release 0.3; one with something has a
//! section after it that a reader of this release reads, that a reader of another section version
//! skips by its length, and that an answer of a Host without it simply lacks.
use florn_control::message::{
    ActorInfo, ActorSummary, InspectField, InspectValue, Inspection, ReportEntry, ScenarioSummary,
    TraceEntry,
};
use florn_control::{Answer, At, DecodeError, Envelope, Message, Query, decode, encode};

fn at() -> At {
    At {
        tick: 7,
        microstep: 1,
    }
}

fn actor(id: u32, builtin: Option<&str>) -> ActorSummary {
    ActorSummary {
        id,
        name: format!("a{id}"),
        module: builtin.is_none().then(|| "m".to_string()),
        class: "default".into(),
        ports: Vec::new(),
        config: Vec::new(),
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
        classes: Vec::new(),
        limits: Vec::new(),
    }))
}

fn entry(reports: Vec<ReportEntry>) -> TraceEntry {
    TraceEntry::Handled {
        at: at(),
        class: 7,
        origin: 1,
        seq: 2,
        target: 3,
        metered: 4,
        draws: 0,
        effects: reports.len() as u32,
        sends: Vec::new(),
        reports,
    }
}

fn report(seq: u64, payload: Option<Vec<u8>>) -> ReportEntry {
    ReportEntry {
        seq,
        channel: 1,
        len: 16,
        payload,
    }
}

fn bytes(a: Answer) -> Vec<u8> {
    encode(&Envelope::new(Message::Answer(a)))
}

fn answer(bytes: &[u8]) -> Result<Answer, DecodeError> {
    match decode(bytes)?.message {
        Message::Answer(a) => Ok(a),
        other => panic!("{other:?}"),
    }
}

/// A section after the answer: its version, then its block as a length-prefixed byte string.
fn section(version: u8, block: &[u8]) -> Vec<u8> {
    let mut out = vec![version];
    out.extend_from_slice(&(block.len() as u32).to_le_bytes());
    out.extend_from_slice(block);
    out
}

#[test]
fn an_answer_with_nothing_to_add_has_no_section_and_is_the_answer_of_0_3() {
    // No built-in, no report: not one byte more than the answer's own encoding.
    let plain = scenario(vec![actor(1, None), actor(2, None)]);
    let with = scenario(vec![actor(1, None), actor(2, Some("test/sink"))]);
    let (a, b) = (bytes(plain.clone()), bytes(with.clone()));
    assert!(
        b.len() > a.len(),
        "the section is there only when it has something"
    );
    // The answer's own encoding is the same prefix in both: the section is after it.
    let own = a.len();
    assert_eq!(answer(&a).unwrap(), plain);
    assert_eq!(answer(&b).unwrap(), with);
    // The built-in's name is nowhere in the prefix, which is what an older reader reads: a Host
    // without the section writes `a`, and reading it gives no built-in.
    let Answer::Scenario(Some(s)) = answer(&a).unwrap() else {
        unreachable!()
    };
    assert!(s.actors.iter().all(|x| x.builtin.is_none()));
    assert!(own < b.len());
    // Trace windows too: reports with no payload bytes still make a section (they carry the
    // channel and the length), none at all do not.
    let none = Answer::TraceWindow(vec![entry(Vec::new()), entry(Vec::new())]);
    let some = Answer::TraceWindow(vec![entry(Vec::new()), entry(vec![report(0, None)])]);
    assert!(bytes(some.clone()).len() > bytes(none.clone()).len());
    assert_eq!(answer(&bytes(none.clone())).unwrap(), none);
    assert_eq!(answer(&bytes(some.clone())).unwrap(), some);
}

#[test]
fn a_trace_window_carries_the_payloads_of_its_reports() {
    let w = Answer::TraceWindow(vec![
        entry(vec![report(0, Some(vec![1; 16])), report(1, None)]),
        TraceEntry::Intervention {
            at: at(),
            seq: 9,
            len: 4,
        },
        entry(vec![report(0, Some(vec![2; 16]))]),
    ]);
    assert_eq!(answer(&bytes(w.clone())).unwrap(), w);
}

#[test]
fn an_actor_answer_and_a_scenario_carry_the_builtins_name() {
    let info = Answer::Actor(Some(ActorInfo {
        summary: actor(5, Some("test/sink")),
        alive: true,
        faults: Vec::new(),
        recent_telemetry: Vec::new(),
    }));
    assert_eq!(answer(&bytes(info.clone())).unwrap(), info);
    let s = scenario(vec![actor(1, Some("circuit/resistor")), actor(2, None)]);
    assert_eq!(answer(&bytes(s.clone())).unwrap(), s);
}

#[test]
fn a_section_of_another_version_is_skipped_by_its_length() {
    // A newer Host's section: this reader reads the answer and ignores it, as it ignores the
    // fields of a block of its own version that it does not know.
    for plain in [
        scenario(vec![actor(1, None)]),
        Answer::TraceWindow(vec![entry(Vec::new())]),
        Answer::Done,
    ] {
        let mut b = bytes(plain.clone());
        b.extend(section(2, &[1, 2, 3, 4, 5]));
        assert_eq!(answer(&b).unwrap(), plain);
        // And one of version 0.
        let mut b = bytes(plain.clone());
        b.extend(section(0, &[]));
        assert_eq!(answer(&b).unwrap(), plain);
    }
    // A block of this version for an answer that has none to read is skipped too.
    let mut b = bytes(Answer::Done);
    b.extend(section(1, &[9, 9]));
    assert_eq!(answer(&b).unwrap(), Answer::Done);
}

#[test]
fn a_section_that_does_not_fit_or_is_cut_is_malformed() {
    // The built-in list as long as the actors, or the answer is refused.
    let mut b = bytes(scenario(vec![actor(1, None), actor(2, None)]));
    let mut block = Vec::new();
    block.extend_from_slice(&1u32.to_le_bytes()); // one name for two actors
    block.push(0);
    b.extend(section(1, &block));
    assert!(matches!(answer(&b), Err(DecodeError::Malformed(_))));
    // A length that runs past the end.
    let mut b = bytes(Answer::Done);
    b.push(1);
    b.extend_from_slice(&1000u32.to_le_bytes());
    assert!(matches!(answer(&b), Err(DecodeError::Malformed(_))));
    // A version byte alone.
    let mut b = bytes(Answer::Done);
    b.push(1);
    assert!(matches!(answer(&b), Err(DecodeError::Malformed(_))));
    // Bytes after the section.
    let mut b = bytes(scenario(vec![actor(1, Some("x/y"))]));
    b.push(0);
    assert!(matches!(answer(&b), Err(DecodeError::Malformed(_))));
}

#[test]
fn the_inspect_query_and_its_answer_round_trip() {
    let q = Envelope::new(Message::Query(Query::Inspect { id: 0xF000_0010 }));
    assert_eq!(decode(&encode(&q)).unwrap(), q);
    assert_eq!(Query::Inspect { id: 1 }.name(), "inspect");
    assert!(Query::Inspect { id: 1 }.validate().is_ok());
    let a = Answer::Inspection(Some(Inspection {
        id: 12,
        kind: "builtin".into(),
        fields: vec![
            InspectField {
                name: "state".into(),
                value: InspectValue::Text("conducting".into()),
            },
            InspectField {
                name: "current".into(),
                value: InspectValue::Real {
                    value: -1.5e-3,
                    unit: "A".into(),
                },
            },
            InspectField {
                name: "ratio".into(),
                value: InspectValue::Real {
                    value: 0.0,
                    unit: String::new(),
                },
            },
        ],
    }));
    assert_eq!(answer(&bytes(a.clone())).unwrap(), a);
    assert_eq!(
        answer(&bytes(Answer::Inspection(None))).unwrap(),
        Answer::Inspection(None)
    );
    // A real is its bits: `-0` is not `+0`, and a NaN keeps its payload.
    let Answer::Inspection(Some(i)) = answer(&bytes(Answer::Inspection(Some(Inspection {
        id: 1,
        kind: "world".into(),
        fields: vec![InspectField {
            name: "z".into(),
            value: InspectValue::Real {
                value: -0.0,
                unit: String::new(),
            },
        }],
    }))))
    .unwrap() else {
        unreachable!()
    };
    let InspectValue::Real { value, .. } = &i.fields[0].value else {
        unreachable!()
    };
    assert_eq!(value.to_bits(), (-0.0f64).to_bits());
}
