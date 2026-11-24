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
        Record::Sample {
            index: 255,
            actor: 65_535,
            value: -1.5e-7,
            tick: u64::MAX,
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

/// The layout of a `sample` (RFC 014 rule 3) is fixed: the kind, the field's index, the actor's
/// index in id order, the value as an f32, the tick of the report, all little-endian.
#[test]
fn a_sample_has_the_layout_of_rfc_014_rule_3() {
    let bytes = Record::Sample {
        index: 2,
        actor: 0x0102,
        value: 1.0,
        tick: 0x1112_1314_1516_1718,
    }
    .encode();
    assert_eq!(
        bytes,
        [
            3, 2, 0x02, 0x01, 0, 0, 0x80, 0x3f, 0x18, 0x17, 0x16, 0x15, 0x14, 0x13, 0x12, 0x11
        ]
    );
}

/// RFC 014 rule 3, the reader rules: a record of a kind the reader does not know is skipped by its
/// fixed size, and the records around it are read; a length that is no multiple of a record's is
/// still no stream.
#[test]
fn a_reader_skips_a_kind_it_does_not_know_by_its_fixed_size() {
    let a = Record::Instant { tick: 1 };
    let b = Record::Sample {
        index: 0,
        actor: 1,
        value: 2.0,
        tick: 3,
    };
    let mut unknown = a.encode();
    unknown[0] = 200;
    unknown[1] = 77;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&a.encode());
    bytes.extend_from_slice(&unknown);
    bytes.extend_from_slice(&b.encode());
    assert_eq!(decode_all(&bytes), Some(vec![a, b]));
    // Several unknown kinds in a row, and nothing but unknown kinds.
    let mut only = unknown.to_vec();
    only.extend_from_slice(&unknown);
    assert_eq!(decode_all(&only), Some(Vec::new()));
    assert_eq!(decode_all(&bytes[..bytes.len() - 1]), None);
    assert_eq!(decode_all(&[]), Some(Vec::new()));
}

/// RFC 014 rule 3: a reader of a newer stream's version refuses it before reading a record; an
/// older or equal one is read. The version is 2 in this release.
#[test]
fn a_newer_stream_is_refused_before_a_record_is_read() {
    assert_eq!(VERSION, 2);
    assert_eq!(check_version(1), Ok(()));
    assert_eq!(check_version(VERSION), Ok(()));
    assert_eq!(check_version(VERSION + 1), Err(NewerStream { found: 3 }));
    assert_eq!(
        check_version(u32::MAX),
        Err(NewerStream { found: u32::MAX })
    );
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
        Record::Sample {
            index: 0,
            actor: 1,
            value: 0.5,
            tick: 7,
        },
        Record::Sample {
            index: 0,
            actor: 1,
            value: 0.25,
            tick: 8,
        },
        Record::Instant { tick: 9 },
    ]);
    // Every sample is kept, in order (RFC 014 rule 3).
    assert_eq!(r.samples.len(), 2);
    assert!(matches!(r.samples[1], Record::Sample { tick: 8, .. }));
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
