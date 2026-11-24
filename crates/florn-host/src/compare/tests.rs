// The tests unbox the comparison's difference to read it; the size of the `Err` is no concern.
#![allow(clippy::result_large_err)]

use super::*;

/// A reading with a tolerance and a set of continuous actors, nothing numeric of its own: the
/// rows are built by hand.
struct Fake {
    tolerance: Option<Tolerance>,
    continuous: Vec<Identity>,
}

impl Reading for Fake {
    fn tolerance(&self) -> Option<Tolerance> {
        self.tolerance
    }
    fn continuous_actor(&self, actor: Identity) -> bool {
        self.continuous.contains(&actor)
    }
    fn numeric_fields(&self, _: Identity, _: Site, _: &[u8]) -> Option<Numeric> {
        None
    }
}

/// `compare`, with the difference unboxed for the assertions below.
fn compare(
    recorded: &[Row],
    replayed: &[Row],
    tolerance: Tolerance,
    reading: &dyn Reading,
) -> Result<Agreement, Difference> {
    super::compare(recorded, replayed, tolerance, reading).map_err(|d| *d)
}

const TOLERANCE: Tolerance = Tolerance {
    atol: 1e-9,
    rtol: 1e-6,
};

fn fake(continuous: &[Identity]) -> Fake {
    Fake {
        tolerance: Some(TOLERANCE),
        continuous: continuous.to_vec(),
    }
}

fn key(tick: u64, seq: u64) -> EventKey {
    EventKey {
        tick,
        microstep: 0,
        class: 7,
        tie: 3,
        origin: 1,
        seq,
    }
}

fn number(name: &str, unit: &str, factor: f64, value: f64) -> Number {
    Number {
        name: name.into(),
        unit: unit.into(),
        factor: factor.to_bits(),
        value: value.to_bits(),
    }
}

fn report(seq: u64, fields: Vec<Number>) -> Effect {
    Effect::Report {
        seq,
        channel: 1,
        len: 8 * fields.len() as u32,
        payload: Payload::Numbers {
            fields,
            rest: *blake3::hash(&[]).as_bytes(),
        },
    }
}

fn handled(tick: u64, target: Identity, work: u64, effects: Vec<Effect>) -> Row {
    Row::Handled {
        key: key(tick, 1),
        target,
        work,
        draws: 0,
        effects,
    }
}

fn level(value: f64) -> Vec<Effect> {
    vec![report(0, vec![number("level", "V", 1.0, value)])]
}

fn env(version: &str, host: &str, arch: &str) -> Environment {
    Environment {
        florn_version: version.into(),
        host_build: host.into(),
        engine: "wasmtime".into(),
        engine_version: "1".into(),
        arch: arch.into(),
    }
}

#[test]
fn the_environments_decide_the_tier() {
    // RFC 001 section 1.1 and rule 41.
    let native = env("0.4.0", "native", "x86_64");
    assert_eq!(tier(&native, &native.clone()), Tier::One);
    assert_eq!(tier(&native, &env("0.4.0", "browser", "wasm32")), Tier::Two);
    assert_eq!(tier(&native, &env("0.4.0", "native", "aarch64")), Tier::Two);
    let mut other_engine = native.clone();
    other_engine.engine = "wasmi".into();
    assert_eq!(tier(&native, &other_engine), Tier::Two);
    let mut other_engine_version = native.clone();
    other_engine_version.engine_version = "2".into();
    assert_eq!(tier(&native, &other_engine_version), Tier::Two);
    // A different florn version is Tier 3, whatever else differs.
    assert_eq!(
        tier(&native, &env("0.5.0", "native", "x86_64")),
        Tier::Three
    );
    assert_eq!(
        tier(&native, &env("0.5.0", "browser", "wasm32")),
        Tier::Three
    );
}

#[test]
fn the_tolerance_is_atol_plus_rtol_times_the_larger_magnitude() {
    // RFC 013 rule 20, item 2: `|a − b| ≤ atol + rtol·max(|a|, |b|)`, in SI units.
    let t = Tolerance {
        atol: 1e-3,
        rtol: 1e-2,
    };
    assert!(agree(100.0, 101.0, t), "1 ≤ 0.001 + 0.01·101 = 1.011");
    assert!(!agree(100.0, 101.02, t), "1.02 > 0.001 + 0.01·101.02");
    assert!(agree(0.0, 1e-3, t), "the absolute part alone");
    assert!(!agree(0.0, 1.02e-3, t), "1.02e-3 > 1e-3 + 1e-2·1.02e-3");
    assert!(agree(-5.0, -5.0, t));
    // Exactly on the edge: the bound is inclusive.
    let edge = Tolerance {
        atol: 0.5,
        rtol: 0.0,
    };
    assert!(agree(1.0, 1.5, edge));
    assert!(!agree(1.0, 1.5000000000000002, edge));
}

#[test]
fn every_nan_is_one_value_and_two_infinities_of_one_sign_are_equal() {
    let t = TOLERANCE;
    assert!(agree(f64::NAN, f64::NAN, t));
    assert!(agree(f64::NAN, -f64::NAN, t));
    assert!(!agree(f64::NAN, 1.0, t));
    assert!(!agree(1.0, f64::NAN, t));
    assert!(agree(f64::INFINITY, f64::INFINITY, t));
    assert!(agree(f64::NEG_INFINITY, f64::NEG_INFINITY, t));
    assert!(!agree(f64::INFINITY, f64::NEG_INFINITY, t));
    assert!(
        !agree(f64::INFINITY, f64::MAX, t),
        "never within a tolerance of a finite value"
    );
}

#[test]
fn the_field_is_compared_in_si_units() {
    // A field held in a unit of factor 1000 (`kohm`): 1 kohm against 1.0000001 kohm is 1e-7 of its
    // value; against an absolute tolerance in ohms of 1e-9 and a relative one of 1e-6 it agrees,
    // and the same two numbers in a unit of factor 1 are 1e-7 apart in a value of 1, which agrees
    // too, but 0.001 kohm of difference is 1 ohm, which does not.
    let a = vec![handled(
        1,
        2,
        10,
        vec![report(0, vec![number("r", "kohm", 1000.0, 1.0)])],
    )];
    let near = vec![handled(
        1,
        2,
        10,
        vec![report(0, vec![number("r", "kohm", 1000.0, 1.0000001)])],
    )];
    let far = vec![handled(
        1,
        2,
        10,
        vec![report(0, vec![number("r", "kohm", 1000.0, 1.001)])],
    )];
    assert!(compare(&a, &near, TOLERANCE, &fake(&[])).is_ok());
    let Err(Difference::Value {
        record,
        field,
        recorded,
        replayed,
    }) = compare(&a, &far, TOLERANCE, &fake(&[]))
    else {
        panic!("1.001 kohm is not 1 kohm within 1e-6");
    };
    assert_eq!((record, field.as_str()), (1, "r"), "the first record");
    // Both values are shown in the field's own unit, not in SI.
    assert_eq!(recorded, ("1".to_string(), "kohm".to_string()));
    assert_eq!(replayed, ("1.001".to_string(), "kohm".to_string()));
    // The absolute tolerance is in SI units: 1e-10 kohm is 1e-7 ohm, beyond an `atol` of 1e-9,
    // though 1e-10 itself is within it.
    let zero = vec![handled(
        1,
        2,
        10,
        vec![report(0, vec![number("r", "kohm", 1000.0, 0.0)])],
    )];
    let tiny = vec![handled(
        1,
        2,
        10,
        vec![report(0, vec![number("r", "kohm", 1000.0, 1e-10)])],
    )];
    assert!(matches!(
        compare(&zero, &tiny, TOLERANCE, &fake(&[])),
        Err(Difference::Value { .. })
    ));
    let tiny_in_ohms = vec![handled(
        1,
        2,
        10,
        vec![report(0, vec![number("r", "ohm", 1.0, 1e-10)])],
    )];
    let zero_in_ohms = vec![handled(
        1,
        2,
        10,
        vec![report(0, vec![number("r", "ohm", 1.0, 0.0)])],
    )];
    assert!(compare(&zero_in_ohms, &tiny_in_ohms, TOLERANCE, &fake(&[])).is_ok());
}

#[test]
fn a_run_agrees_with_itself_and_nothing_differs() {
    let rows = vec![handled(1, 2, 10, level(1.5)), handled(2, 2, 12, level(2.5))];
    let a = compare(&rows, &rows, TOLERANCE, &fake(&[])).unwrap();
    assert_eq!(
        a,
        Agreement {
            compared: 2,
            differing: 0,
            worst: None,
            consumption_differing: 0
        }
    );
}

#[test]
fn a_last_bit_is_within_tolerance_and_names_the_worst_field() {
    let one_ulp = f64::from_bits(1.5f64.to_bits() + 1);
    let a = vec![
        handled(1, 2, 10, level(1.5)),
        handled(
            2,
            2,
            10,
            vec![report(
                0,
                vec![number("x", "V", 1.0, 2.0), number("y", "A", 1.0, 3.0)],
            )],
        ),
    ];
    let b = vec![
        handled(1, 2, 10, level(one_ulp)),
        handled(
            2,
            2,
            10,
            vec![report(
                0,
                vec![
                    number("x", "V", 1.0, 2.0),
                    // 1000 ulps: a larger share of the tolerance than the first record's 1 ulp.
                    number("y", "A", 1.0, f64::from_bits(3.0f64.to_bits() + 1000)),
                ],
            )],
        ),
    ];
    let a = compare(&a, &b, TOLERANCE, &fake(&[])).expect("within the tolerance");
    assert_eq!((a.compared, a.differing), (2, 2));
    let worst = a.worst.expect("a field differs");
    assert_eq!(worst.field, "y", "the largest share of its own tolerance");
    assert_eq!(worst.at, 2);
    assert_eq!(worst.largest.1, "A");
    let ulps = (f64::from_bits(3.0f64.to_bits() + 1000) - 3.0).abs();
    assert_eq!(worst.largest.0, number_text(ulps));
}

#[test]
fn a_difference_beyond_the_tolerance_names_the_first_record_and_field() {
    let a = vec![handled(1, 2, 10, level(1.0)), handled(2, 2, 10, level(1.0))];
    let b = vec![handled(1, 2, 10, level(1.0)), handled(2, 2, 10, level(1.1))];
    assert_eq!(
        compare(&a, &b, TOLERANCE, &fake(&[])),
        Err(Difference::Value {
            record: 2,
            field: "level".into(),
            recorded: ("1".into(), "V".into()),
            replayed: ("1.1".into(), "V".into()),
        })
    );
}

#[test]
fn the_other_differences_are_named_by_what_differs() {
    use RecordDifference as D;
    let base = handled(1, 2, 10, level(1.0));
    let record = |what| Err(Difference::Record { record: 1, what });
    let run = |b: Row| compare(std::slice::from_ref(&base), &[b], TOLERANCE, &fake(&[]));
    // The key, the target.
    let mut k = key(1, 1);
    k.tick = 2;
    assert_eq!(
        run(Row::Handled {
            key: k,
            target: 2,
            work: 10,
            draws: 0,
            effects: level(1.0)
        }),
        record(D::Key)
    );
    assert_eq!(run(handled(1, 3, 10, level(1.0))), record(D::Key));
    // The kind of record, the effects.
    assert_eq!(
        run(Row::Fault {
            key: key(1, 1),
            actor: 2,
            kind: 1,
            parameter: 0,
            work: 10
        }),
        record(D::Structure)
    );
    assert_eq!(run(handled(1, 2, 10, vec![])), record(D::Structure));
    let mut other_seq = level(1.0);
    if let Effect::Report { seq, .. } = &mut other_seq[0] {
        *seq = 9;
    }
    assert_eq!(run(handled(1, 2, 10, other_seq)), record(D::Structure));
    // A payload that is a digest, or the rest of one that is a number.
    let digest = |d: u8| {
        vec![Effect::Send {
            seq: 0,
            port: 0,
            len: 4,
            payload: Payload::Digest([d; 32]),
        }]
    };
    assert_eq!(
        compare(
            &[handled(1, 2, 10, digest(1))],
            &[handled(1, 2, 10, digest(2))],
            TOLERANCE,
            &fake(&[])
        ),
        record(D::Payload)
    );
    let mut other_rest = level(1.0);
    if let Effect::Report {
        payload: Payload::Numbers { rest, .. },
        ..
    } = &mut other_rest[0]
    {
        *rest = [7; 32];
    }
    assert_eq!(run(handled(1, 2, 10, other_rest)), record(D::Payload));
    // An intervention's content.
    let iv = |d: u8| Row::Intervention {
        key: key(1, 1),
        digest: [d; 32],
        len: 4,
    };
    assert_eq!(
        compare(&[iv(1)], &[iv(2)], TOLERANCE, &fake(&[])),
        record(D::Payload)
    );
}

#[test]
fn a_different_number_of_records_is_the_length_at_the_end_of_the_shorter() {
    let r = |n: u64| handled(n, 2, 10, level(1.0));
    assert_eq!(
        compare(&[r(1), r(2), r(3)], &[r(1), r(2)], TOLERANCE, &fake(&[])),
        Err(Difference::Record {
            record: 3,
            what: RecordDifference::Length
        })
    );
    assert_eq!(
        compare(&[r(1)], &[r(1), r(2)], TOLERANCE, &fake(&[])),
        Err(Difference::Record {
            record: 2,
            what: RecordDifference::Length
        })
    );
    // A difference before the end is the first difference, not the length.
    let mut wrong = handled(1, 2, 10, level(5.0));
    if let Row::Handled { target, .. } = &mut wrong {
        *target = 9;
    }
    assert!(matches!(
        compare(&[r(1), r(2)], &[wrong], TOLERANCE, &fake(&[])),
        Err(Difference::Record {
            record: 1,
            what: RecordDifference::Key
        })
    ));
}

#[test]
fn work_and_draws_are_exempt_for_a_continuous_actor_only() {
    // RFC 013 rule 20, item 4: a branch of the guest's code that a value within tolerance flips
    // changes them; the comparison counts them and does not fail on them for a continuous actor.
    let a = vec![handled(1, 2, 10, level(1.0))];
    let more_work = vec![handled(1, 2, 99, level(1.0))];
    let agreement = compare(&a, &more_work, TOLERANCE, &fake(&[2])).expect("exempt");
    assert_eq!(agreement.consumption_differing, 1);
    assert_eq!(agreement.differing, 0, "no numeric field differs");
    // The same for a discrete actor is a difference.
    assert_eq!(
        compare(&a, &more_work, TOLERANCE, &fake(&[7])),
        Err(Difference::Record {
            record: 1,
            what: RecordDifference::Structure
        })
    );
    // And draws.
    let mut drew = more_work.clone();
    if let Row::Handled { work, draws, .. } = &mut drew[0] {
        *work = 10;
        *draws = 1;
    }
    assert!(compare(&a, &drew, TOLERANCE, &fake(&[2])).is_ok());
    assert!(compare(&a, &drew, TOLERANCE, &fake(&[])).is_err());
    // A fault's metered work is the same.
    let fault = |work| Row::Fault {
        key: key(1, 1),
        actor: 2,
        kind: 2,
        parameter: 0,
        work,
    };
    assert!(compare(&[fault(1)], &[fault(2)], TOLERANCE, &fake(&[2])).is_ok());
    assert!(compare(&[fault(1)], &[fault(2)], TOLERANCE, &fake(&[])).is_err());
    // Its kind is not exempt.
    let mut other_kind = fault(1);
    if let Row::Fault { kind, .. } = &mut other_kind {
        *kind = 3;
    }
    assert!(compare(&[fault(1)], &[other_kind], TOLERANCE, &fake(&[2])).is_err());
}

#[test]
fn a_trace_without_records_or_a_payload_cannot_be_read() {
    let none: BTreeMap<Digest, Vec<u8>> = BTreeMap::new();
    assert_eq!(rows(None, &none, &fake(&[])), Err(Missing::Trace));
    let record = Record::Handled {
        key: key(1, 1),
        target: 2,
        metered: 5,
        draws: 0,
        effects: vec![CommittedEffect::Report {
            seq: 0,
            channel: 1,
            len: 3,
            digest: *blake3::hash(b"abc").as_bytes(),
        }],
    };
    assert_eq!(
        rows(Some(std::slice::from_ref(&record)), &none, &fake(&[])),
        Err(Missing::Payloads)
    );
    let with: BTreeMap<Digest, Vec<u8>> = [(*blake3::hash(b"abc").as_bytes(), b"abc".to_vec())]
        .into_iter()
        .collect();
    let rows = rows(Some(&[record]), &with, &fake(&[])).unwrap();
    // The reading has no numeric field of it: it stays a digest, compared exactly.
    assert!(matches!(
        &rows[0],
        Row::Handled { effects, .. }
            if matches!(&effects[0], Effect::Report { payload: Payload::Digest(_), .. })
    ));
}

#[test]
fn numbers_are_written_as_the_shortest_text_with_an_exponent_when_far_from_one() {
    assert_eq!(number_text(0.0), "0");
    assert_eq!(number_text(1.5), "1.5");
    assert_eq!(number_text(-1.5), "-1.5");
    assert_eq!(number_text(1000.0), "1000");
    assert_eq!(number_text(1e-10), "1e-10");
    assert_eq!(number_text(2.5e20), "2.5e20");
    assert_eq!(number_text(0.001), "0.001");
    for v in [
        1e-10,
        3.3e-12,
        0.1 + 0.2,
        123456.789,
        1.7976931348623157e308,
    ] {
        assert_eq!(number_text(v).parse::<f64>().unwrap(), v, "{v}");
    }
}

#[test]
fn a_field_differs_when_its_bits_differ_and_agrees_by_the_bound() {
    // RFC 013 rule 20 (amended): `-0` against `+0`, and two NaNs of other bits, differ and agree:
    // the pair is within tolerance and the field is named, with no distance between them.
    let nan_a = f64::from_bits(0x7ff8_0000_0000_0001);
    let nan_b = f64::from_bits(0x7ff8_0000_0000_0000);
    for (a, b) in [(-0.0, 0.0), (0.0, -0.0), (nan_a, nan_b), (f64::NAN, nan_a)] {
        let one = |v: f64| vec![handled(1, 2, 10, level(v))];
        let agreement = compare(&one(a), &one(b), TOLERANCE, &fake(&[])).expect("they agree");
        assert_eq!(
            agreement.differing, 1,
            "{a:?} against {b:?} differ in their bits"
        );
        let worst = agreement.worst.expect("the field is named");
        assert_eq!((worst.field.as_str(), worst.at), ("level", 1));
        assert_eq!(
            worst.largest,
            ("0".to_string(), "V".to_string()),
            "no distance"
        );
    }
    // The same bits are no difference at all, whatever the value (NaN included).
    for v in [0.0, -0.0, f64::NAN, f64::INFINITY] {
        let one = vec![handled(1, 2, 10, level(v))];
        assert_eq!(
            compare(&one, &one, TOLERANCE, &fake(&[]))
                .unwrap()
                .differing,
            0,
            "{v:?}"
        );
    }
    // A pair beyond the bound is still a value difference, signs of zero or not.
    let near = vec![handled(1, 2, 10, level(0.0))];
    let far = vec![handled(1, 2, 10, level(1.0))];
    assert!(matches!(
        compare(&near, &far, TOLERANCE, &fake(&[])),
        Err(Difference::Value { .. })
    ));
}

#[test]
fn the_worst_field_is_the_largest_share_among_pairs_that_differ_in_bits_only() {
    // A pair of signed zeros has share zero: a later pair with a real, tiny difference is worse.
    let a = vec![handled(1, 2, 10, level(0.0)), handled(2, 2, 10, level(1.0))];
    let b = vec![
        handled(1, 2, 10, level(-0.0)),
        handled(2, 2, 10, level(f64::from_bits(1.0f64.to_bits() + 1))),
    ];
    let agreement = compare(&a, &b, TOLERANCE, &fake(&[])).unwrap();
    assert_eq!(agreement.differing, 2);
    assert_eq!(agreement.worst.unwrap().at, 2);
}
