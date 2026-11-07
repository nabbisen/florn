use super::*;

#[test]
fn a_replay_stores_the_records_points_and_compares_there() {
    let key = EventKey {
        tick: 1,
        microstep: 0,
        class: 0,
        tie: 0,
        origin: 0,
        seq: 0,
    };
    let r = Record::Intervention {
        key,
        content_digest: [1; 32],
        content_len: 1,
    };
    // A record exported after 7 records with an interval of 5: points at 5 and 7.
    let mut recorded = Trace::new(5, false);
    for _ in 0..7 {
        recorded.push(r.clone());
    }
    let reference = recorded.stored();
    assert_eq!(
        reference.iter().map(|p| p.0).collect::<Vec<_>>(),
        vec![5, 7]
    );
    // The replay runs on to 12 records; it stored the chain at 7 as asked.
    let mut replay = Trace::new(5, false);
    replay.watch(reference.iter().map(|p| p.0).collect());
    for _ in 0..12 {
        replay.push(r.clone());
    }
    assert_eq!(
        compare_reference(&replay.stored(), &reference),
        Comparison::Matched { points: 2 }
    );
    assert!(matches!(
        compare(&replay.stored(), &reference),
        Comparison::Incomparable
    ));
    // A replay that differs at 7 diverges after 5; one that stops short is incomparable.
    let mut other = Trace::new(5, false);
    other.watch(vec![5, 7]);
    for i in 0..7 {
        other.push(if i == 6 {
            Record::Intervention {
                key,
                content_digest: [2; 32],
                content_len: 1,
            }
        } else {
            r.clone()
        });
    }
    assert_eq!(
        compare_reference(&other.stored(), &reference),
        Comparison::Diverged { from: 5, to: 7 }
    );
    let mut short = Trace::new(5, false);
    short.watch(vec![5, 7]);
    for _ in 0..6 {
        short.push(r.clone());
    }
    assert_eq!(
        compare_reference(&short.stored(), &reference),
        Comparison::Incomparable
    );
}

fn rec(seq: u64) -> Record {
    Record::Handled {
        key: EventKey {
            tick: seq,
            microstep: 0,
            class: 1,
            tie: 0,
            origin: 1,
            seq,
        },
        target: 1,
        metered: 7,
        draws: 0,
        effects: vec![],
    }
}

#[test]
fn chain_interval_and_comparison() {
    let mut a = Trace::new(3, false);
    let mut b = Trace::new(3, false);
    for i in 0..7 {
        a.push(rec(i));
        b.push(rec(i));
    }
    assert_eq!(
        a.stored().iter().map(|s| s.0).collect::<Vec<_>>(),
        vec![3, 6, 7]
    );
    assert_eq!(a.compare(&b.stored()), Comparison::Matched { points: 3 });
    let mut c = Trace::new(3, false);
    for i in 0..7 {
        c.push(rec(if i == 4 { 99 } else { i }));
    }
    assert_eq!(
        a.compare(&c.stored()),
        Comparison::Diverged { from: 3, to: 6 }
    );
    let mut d = Trace::new(2, false);
    d.push(rec(0));
    assert_eq!(a.compare(&d.stored()), Comparison::Incomparable);
}

#[test]
fn payload_hashed_once_and_encoding_is_fixed() {
    let e = CommittedEffect::from_effect(
        1,
        &Effect::Send {
            port: 0,
            payload: vec![1, 2, 3],
        },
    );
    match &e {
        CommittedEffect::Send { len, digest, .. } => {
            assert_eq!(*len, 3);
            assert_eq!(digest, blake3::hash(&[1, 2, 3]).as_bytes());
        }
        _ => panic!(),
    }
    let r = Record::Handled {
        key: EventKey {
            tick: 1,
            microstep: 2,
            class: 3,
            tie: 4,
            origin: 5,
            seq: 6,
        },
        target: 7,
        metered: 8,
        draws: 9,
        effects: vec![e],
    };
    let bytes = encode(&r);
    assert_eq!(bytes.len(), 1 + 33 + 4 + 8 + 4 + 4 + (1 + 8 + 4 + 4 + 32));
    assert_eq!(&bytes[..9], &[1, 1, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn per_actor_chain_from_kept_records() {
    let mut t = Trace::new(100, true);
    t.push(rec(0));
    t.push(Record::Handled {
        key: EventKey {
            tick: 1,
            microstep: 0,
            class: 1,
            tie: 0,
            origin: 2,
            seq: 0,
        },
        target: 2,
        metered: 0,
        draws: 0,
        effects: vec![],
    });
    t.push(rec(2));
    let one = t.actor_chain(1).unwrap();
    let two = t.actor_chain(2).unwrap();
    assert_ne!(one, two);
    assert_ne!(one, [0; 32]);
    assert_eq!(Trace::new(100, false).actor_chain(1), None);
}
