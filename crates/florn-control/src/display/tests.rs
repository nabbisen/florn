use super::*;

#[test]
fn every_kind_round_trips_in_sixteen_bytes() {
    let records = [
        Record::Instant { tick: u64::MAX },
        Record::Flight {
            connection: 65_535,
            port: 255,
            delay: 1.5e12,
            sent: 7,
        },
        Record::Activity {
            actor: 42,
            count: 65_535,
            faulted: true,
            last: 9,
        },
    ];
    for r in records {
        let b = r.encode();
        assert_eq!(b.len(), RECORD_BYTES);
        assert_eq!(Record::decode(&b), Some(r));
    }
    let bytes = encode_all(&records);
    assert_eq!(decode_all(&bytes).as_deref(), Some(&records[..]));
    assert_eq!(decode_all(&bytes[..17]), None);
    let mut unknown = records[0].encode();
    unknown[0] = 9;
    assert_eq!(Record::decode(&unknown), None);
    assert_eq!(Record::decode(&unknown[..15]), None);
}

#[test]
fn the_reduction_folds_batches() {
    let mut r = Reduced::default();
    r.fold(&[
        Record::Activity {
            actor: 1,
            count: 2,
            faulted: false,
            last: 5,
        },
        Record::Flight {
            connection: 0,
            port: 1,
            delay: 10.0,
            sent: 3,
        },
        Record::Instant { tick: 5 },
    ]);
    r.fold(&[
        Record::Activity {
            actor: 1,
            count: 3,
            faulted: true,
            last: 8,
        },
        Record::Instant { tick: 9 },
    ]);
    assert_eq!(r.instant, Some(9));
    assert_eq!(r.flights.len(), 1);
    assert_eq!(
        r.activity.get(&1),
        Some(&Activity {
            count: 5,
            faulted: true,
            last: 8
        })
    );
    r.clear();
    assert_eq!(r, Reduced::default());
}
