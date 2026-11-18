//! The kernel's discard, checked from the rules and not from the code (PR-38; written by the
//! verifier session before it read the implementation: RFC 001 rules 21 to 24, RFC 012 rule 21 and
//! finding F-2). A discarded activation leaves the kernel exactly as if it had never been begun,
//! apart from the event it consumed: the whole snapshot, the records, the stream, the sequence
//! numbers; the actor lives; nothing is open; two activations open at once still assert. The
//! handle's side is `compound_handle` in the kit.
use florn_kernel::*;

fn config() -> Config {
    Config {
        root_seed: 99,
        end: 1_000_000,
        chain_interval: 4,
        keep_records: true,
        ..Config::default()
    }
}

fn sink() -> Sink {
    Sink::new(Quotas::default(), vec![0], vec![1])
}

/// Three events: tick 5 and 10 for actor 1, tick 15 for actor 2.
fn kernel() -> Kernel {
    let mut k = Kernel::new(config());
    k.add_actor(1);
    k.add_actor(2);
    k.schedule_change(5, 1, vec![]);
    k.schedule_change(10, 1, vec![]);
    k.schedule_change(15, 2, vec![]);
    k
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum First {
    /// Begin the first event's activation and discard it having done nothing.
    DiscardEmpty,
    /// Begin it, draw five values, record a send, a timer and a report, and discard.
    DiscardBusy,
    /// Begin it and commit it empty (the control: a Handled record must appear).
    CommitEmpty,
}

struct Outcome {
    snapshot: KernelSnapshot,
    second_draws: Vec<u64>,
    second_effect_seqs: Vec<u64>,
    records: Vec<Record>,
}

fn drive(first: First) -> Outcome {
    let mut k = kernel();
    let mut world = NoWorld;
    let mut s = sink();

    let e1 = k.next_event().expect("event 1");
    assert_eq!((e1.key.tick, e1.target), (5, 1));
    let act = k.begin(1, e1);
    s.reset();
    match first {
        First::DiscardEmpty => k.discard(act),
        First::DiscardBusy => {
            for _ in 0..5 {
                k.draw(1);
            }
            s.send(0, vec![1, 2, 3]).unwrap();
            s.set_timer(0, 3).unwrap();
            s.report(1, vec![7; 8]).unwrap();
            k.discard(act);
            // The caller's part of a discard (the kernel's doc): drop the effects.
            s.reset();
        }
        First::CommitEmpty => k.commit(act, &mut s, 0, &mut world).unwrap(),
    }

    // Event 2, for the same actor: it draws three values and reports (two effects).
    let e2 = k.next_event().expect("event 2");
    assert_eq!((e2.key.tick, e2.target), (10, 1));
    let act = k.begin(1, e2);
    s.reset();
    let second_draws: Vec<u64> = (0..3).map(|_| k.draw(1)).collect();
    s.report(1, vec![1; 8]).unwrap();
    s.report(1, vec![2; 8]).unwrap();
    k.commit(act, &mut s, 1, &mut world).unwrap();

    let e3 = k.next_event().expect("event 3");
    assert_eq!((e3.key.tick, e3.target), (15, 2));
    let act = k.begin(2, e3);
    s.reset();
    k.commit(act, &mut s, 0, &mut world).unwrap();

    assert!(k.next_event().is_none(), "nothing left: nothing leaked");
    let records = k.trace.kept_records().expect("kept").to_vec();
    let second_effect_seqs = records
        .iter()
        .filter_map(|r| match r {
            Record::Handled {
                key,
                target: 1,
                effects,
                ..
            } if key.tick == 10 => Some(effects.iter().map(|e| e.seq()).collect::<Vec<_>>()),
            _ => None,
        })
        .flatten()
        .collect();
    Outcome {
        snapshot: k.snapshot(),
        second_draws,
        second_effect_seqs,
        records,
    }
}

/// V1a: whatever the discarded activation did, the kernel ends in the state of a run in which it
/// did nothing: the queue, the actors (sequence numbers, stream positions, timers), the trace
/// chain and the counts.
#[test]
fn v1a_a_busy_discard_leaves_the_state_of_an_empty_one() {
    let empty = drive(First::DiscardEmpty);
    let busy = drive(First::DiscardBusy);
    assert_eq!(busy.snapshot, empty.snapshot);
    assert_eq!(busy.records, empty.records);
}

/// The control that gives V1a its teeth: committing the first event instead is visibly different
/// (a Handled record), so the equality above can fail.
#[test]
fn v1a_control_a_commit_is_not_a_discard() {
    let empty = drive(First::DiscardEmpty);
    let commit = drive(First::CommitEmpty);
    assert_ne!(commit.snapshot, empty.snapshot);
    assert_eq!(commit.records.len(), empty.records.len() + 1);
}

/// V1b: the stream. The draws of the next activation are those of an actor that never drew.
#[test]
fn v1b_the_random_stream_is_restored() {
    let empty = drive(First::DiscardEmpty);
    let busy = drive(First::DiscardBusy);
    assert_eq!(busy.second_draws, empty.second_draws);
    // And they are the stream's own first three values for the actor, not a coincidence of zeros.
    let mut fresh = Kernel::new(config());
    fresh.add_actor(1);
    let expect: Vec<u64> = (0..3).map(|_| fresh.draw(1)).collect();
    assert_eq!(busy.second_draws, expect);
    assert!(expect.iter().any(|v| *v != 0));
}

/// V1c: nothing recorded for the discarded event: no Handled and no Fault with its key.
#[test]
fn v1c_nothing_is_recorded() {
    let busy = drive(First::DiscardBusy);
    let for_first = busy.records.iter().filter(|r| match r {
        Record::Handled { key, .. } | Record::Fault { key, .. } => key.tick == 5,
        _ => false,
    });
    assert_eq!(for_first.count(), 0, "{:?}", busy.records);
}

/// V1d, V1e: the actor is alive; the activation is cleared (begin and snapshot do not panic) —
/// both are exercised by `drive` itself; here the explicit assertions.
#[test]
fn v1d_e_the_actor_lives_and_nothing_is_open() {
    let mut k = kernel();
    let e1 = k.next_event().unwrap();
    let act = k.begin(1, e1);
    k.discard(act);
    assert!(k.actor(1).unwrap().alive);
    let _ = k.snapshot(); // asserts !in_activation
    assert!(k.next_event().is_some(), "the next event is handed out");
}

/// V1f: a discarded activation takes no sequence number: the next committed one starts at 1.
#[test]
fn v1f_a_discard_takes_no_sequence_number() {
    let busy = drive(First::DiscardBusy);
    assert_eq!(busy.second_effect_seqs, vec![1, 2]);
}

/// V3a: two activations open at once still assert, and so do the other calls that need none open.
#[test]
#[should_panic(expected = "an activation is in progress")]
fn v3a_begin_twice_asserts() {
    let mut k = kernel();
    let e1 = k.next_event().unwrap();
    let _a = k.begin(1, e1.clone());
    let _b = k.begin(1, e1);
}

#[test]
#[should_panic(expected = "next_event during an activation")]
fn v3a_next_event_while_open_asserts() {
    let mut k = kernel();
    let e1 = k.next_event().unwrap();
    let _a = k.begin(1, e1);
    let _ = k.next_event();
}

#[test]
#[should_panic(expected = "snapshot during an activation")]
fn v3a_snapshot_while_open_asserts() {
    let mut k = kernel();
    let e1 = k.next_event().unwrap();
    let _a = k.begin(1, e1);
    let _ = k.snapshot();
}

/// V1: a discard does not clear the *next* open flag by accident: after a discard a `begin` that
/// follows a `begin` still asserts (the flag is cleared once, by the discard, and set again).
#[test]
#[should_panic(expected = "an activation is in progress")]
fn v3a_after_a_discard_a_second_begin_still_asserts() {
    let mut k = kernel();
    let e1 = k.next_event().unwrap();
    let a = k.begin(1, e1);
    k.discard(a);
    let e2 = k.next_event().unwrap();
    let _b = k.begin(1, e2.clone());
    let _c = k.begin(1, e2);
}
