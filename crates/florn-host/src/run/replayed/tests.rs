use super::*;

#[test]
fn the_side_that_lacks_its_trace_or_a_payload_is_named() {
    // RFC 013 rule 20: `replay.needs-payloads` says which of the two runs it is.
    assert_eq!(lacking(false, false), None);
    assert_eq!(lacking(true, false), Some(Side::Recorded));
    assert_eq!(lacking(false, true), Some(Side::Replayed));
    assert_eq!(lacking(true, true), Some(Side::Both));
}

#[test]
fn the_notifications_of_every_outcome_are_the_catalogues() {
    use crate::compare::{Agreement, RecordDifference, Worst};
    // One notification each, with the declared parameters, and none of them is `replay.diverged`
    // when a record is named: it is said in place of it.
    let diverged = |difference| Replayed::Diverged {
        from: 1,
        to: 2,
        difference,
    };
    let cases = [
        (Replayed::Matched { points: 3 }, "replay.matched"),
        (
            Replayed::WithinTolerance(Agreement {
                compared: 10,
                differing: 2,
                worst: Some(Worst {
                    field: "v".into(),
                    largest: ("0.5".into(), "V".into()),
                    at: 9,
                }),
                consumption_differing: 0,
            }),
            "replay.within-tolerance",
        ),
        (
            Replayed::WithinTolerance(Agreement {
                compared: 10,
                differing: 0,
                worst: None,
                consumption_differing: 1,
            }),
            "replay.exempt-only",
        ),
        (diverged(None), "replay.diverged"),
        (
            diverged(Some(Difference::Value {
                record: 4,
                field: "v".into(),
                recorded: ("1".into(), "V".into()),
                replayed: ("2".into(), "V".into()),
            })),
            "replay.value-differs",
        ),
        (
            diverged(Some(Difference::Record {
                record: 4,
                what: RecordDifference::Length,
            })),
            "replay.record-differs",
        ),
        (Replayed::NeedsPayloads(Side::Both), "replay.needs-payloads"),
    ];
    for (outcome, code) in cases {
        let ns = outcome.notifications();
        assert_eq!(ns.len(), 1, "{outcome:?}");
        assert_eq!(ns[0].code, code);
        ns[0].check().unwrap_or_else(|e| panic!("{code}: {e:?}"));
    }
    // Only exempt values differ: no field is named, and the one parameter is the count compared.
    let n = &Replayed::WithinTolerance(Agreement {
        compared: 4,
        differing: 0,
        worst: None,
        consumption_differing: 2,
    })
    .notifications()[0];
    assert_eq!(n.code, "replay.exempt-only");
    assert_eq!(n.params, [("compared".to_string(), Value::Count(4))]);
    // A numeric difference is the other code, with its field.
    let n = &Replayed::WithinTolerance(Agreement {
        compared: 4,
        differing: 1,
        worst: Some(Worst {
            field: "v".into(),
            largest: ("1".into(), "V".into()),
            at: 3,
        }),
        consumption_differing: 0,
    })
    .notifications()[0];
    assert_eq!(n.code, "replay.within-tolerance");
}
