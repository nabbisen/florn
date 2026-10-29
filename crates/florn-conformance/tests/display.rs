//! The display stream (RFC 005 rule 14; checklist C18's native half): the Host writes
//! sixteen-byte records when a front end subscribes and nothing otherwise; a batch holds the
//! flights of its deliveries, each actor's activity and the instant reached; the stream is the
//! same from one run to the next and never touches the trace; the reduction above the
//! transport folds batches; the transport's notice and its drops reach the run's log.
use florn_admission::Admission;
use florn_conformance::{host, shipped};
use florn_control::display::{DisplayPath, Record, Reduced, Transport, decode_all, encode_all};
use florn_control::{
    Answer, Command, Envelope, Message, SeedChoice, Subscription, SubscriptionKind, Unit,
};
use florn_host::{Advance, Controller, Options};
use florn_kernel::Record as TraceRecord;
use florn_platform::FixedPlatform;

/// The `net-delay` pair: a 1,500-byte frame every 50 ticks over a 20 ns link at 1000 Gbit/s,
/// echoed; a flight of 32 ticks each way.
fn pair() -> florn_conformance::scenario::Scenario {
    shipped::net_pair("display-pair", 1_000, 50, 1500, true).link(
        "network/link",
        &[(1, 0), (2, 0)],
        &[("delay", "\"20ns\""), ("bandwidth", "\"1000Gbit/s\"")],
    )
}

/// The pair with the display on from the start, as the `display` subscription has it.
fn drawn() -> host::Host {
    host::try_start(
        &pair(),
        Options {
            display: true,
            ..host::options()
        },
    )
    .expect("starts")
}

/// Advance in slices of `slice` ticks, taking the display batch after each.
fn batches(run: &mut host::Host, slice: u64) -> Vec<Vec<Record>> {
    let mut out = Vec::new();
    while run.ended().is_none() {
        let to = run.reached() + slice;
        run.advance(Advance::ToTick(to));
        out.push(run.take_display());
    }
    out
}

fn activations(run: &host::Host) -> u64 {
    run.records()
        .expect("kept")
        .iter()
        .filter(|r| matches!(r, TraceRecord::Handled { .. } | TraceRecord::Fault { .. }))
        .count() as u64
}

#[test]
fn nothing_is_written_until_a_front_end_subscribes() {
    let mut run = host::start(&pair());
    run.run_to_end();
    assert!(run.take_display().is_empty());
    let mut run = host::start(&pair());
    run.set_display(true);
    run.set_display(false);
    run.run_to_end();
    assert!(run.take_display().is_empty());
}

#[test]
fn a_batch_holds_the_flights_the_activity_and_the_instant_reached() {
    let mut run = drawn();
    let batches = batches(&mut run, 100);
    // Ten slices to the end, then one that finds the end reached (the echo in flight at 1000
    // is never handled: the first event at or after `end` is not).
    assert_eq!(batches.len(), 10);
    let mut instants = Vec::new();
    for (i, b) in batches.iter().enumerate() {
        // Flights first, then the activity by actor, then the instant: one per batch, last.
        let kinds: Vec<u8> = b.iter().map(|r| r.encode()[0]).collect();
        let body = &kinds[..kinds.len() - 1];
        assert!(
            body.windows(2).all(|w| w[0] <= w[1]),
            "batch {i}: {kinds:?}"
        );
        match b.last() {
            Some(Record::Instant { tick }) => instants.push(*tick),
            other => panic!("batch {i} ends with {other:?}"),
        }
        assert_eq!(
            b.iter()
                .filter(|r| matches!(r, Record::Instant { .. }))
                .count(),
            1
        );
        // The encoding round-trips, sixteen bytes a record.
        let bytes = encode_all(b);
        assert_eq!(bytes.len(), 16 * b.len());
        assert_eq!(decode_all(&bytes).as_deref(), Some(b.as_slice()));
    }
    assert_eq!(instants, (1..=10).map(|i| i * 100).collect::<Vec<_>>());
    let all: Vec<Record> = batches.concat();
    // Nineteen frames sent (the first after one period) and nineteen echoed, each a flight of
    // 32 ticks on connection 0 from the sender's port: the sends from port 0, the echoes
    // from port 1.
    let flights: Vec<(u16, u8, f32, u64)> = all
        .iter()
        .filter_map(|r| match *r {
            Record::Flight {
                connection,
                port,
                delay,
                sent,
            } => Some((connection, port, delay, sent)),
            _ => None,
        })
        .collect();
    assert_eq!(flights.len(), 38);
    assert!(flights.iter().all(|f| f.0 == 0 && f.2 == 32.0));
    assert_eq!(flights.iter().filter(|f| f.1 == 0).count(), 19);
    assert_eq!(flights.iter().filter(|f| f.1 == 1).count(), 19);
    assert!(
        flights
            .iter()
            .filter(|f| f.1 == 0)
            .all(|f| f.3 > 0 && f.3.is_multiple_of(50))
    );
    assert!(
        flights
            .iter()
            .filter(|f| f.1 == 1)
            .all(|f| (f.3 - 32).is_multiple_of(50))
    );
    // The activity sums to the trace's activations (the initialisations among them), none
    // faulted.
    let mut counts = std::collections::BTreeMap::new();
    for r in &all {
        if let Record::Activity {
            actor,
            count,
            faulted,
            ..
        } = r
        {
            assert!(!faulted);
            *counts.entry(*actor).or_insert(0u64) += u64::from(*count);
        }
    }
    assert_eq!(counts.values().sum::<u64>(), activations(&run));
    assert_eq!(counts.len(), 2);
    // The reduction above the transport folds every batch into one update.
    let mut reduced = Reduced::default();
    for b in &batches {
        reduced.fold(b);
    }
    assert_eq!(reduced.instant, Some(1_000));
    assert_eq!(reduced.flights.len(), 38);
    assert_eq!(
        reduced
            .activity
            .values()
            .map(|a| u64::from(a.count))
            .sum::<u64>(),
        activations(&run)
    );
}

#[test]
fn the_stream_is_the_same_from_run_to_run_and_leaves_the_trace_alone() {
    let mut a = host::start(&pair());
    a.set_display(true);
    let first = encode_all(&batches(&mut a, 64).concat());
    let mut b = host::start(&pair());
    b.set_display(true);
    let second = encode_all(&batches(&mut b, 64).concat());
    assert_eq!(first, second);
    assert!(!first.is_empty());
    // The same run without the display: the same trace (the stream is beside it, not in it).
    let silent = host::run(&pair());
    assert_eq!(silent.chain_hex(), a.chain_hex());
    assert_eq!(silent.chain_hex(), b.chain_hex());
    // A faulting actor is marked in its activity.
    let hostile = florn_conformance::scenario::Scenario::new("display-fault", 20)
        .actor(florn_conformance::scenario::actor(1, "hostile_loop", 0));
    let mut run = host::start(&hostile);
    run.set_display(true);
    run.run_to_end();
    let faulted = run.take_display().into_iter().any(|r| {
        matches!(
            r,
            Record::Activity {
                actor: 1,
                faulted: true,
                ..
            }
        )
    });
    assert!(faulted);
}

#[test]
fn through_the_control_interface_with_the_transport_s_notice_and_its_drops() {
    let mut c: Controller<florn_engine_wasmtime::WasmtimeEngine> = Controller::new(
        host::engine(),
        Admission::default(),
        Box::new(FixedPlatform::default()),
        host::domains(),
        host::options(),
    );
    c.set_transport(Transport::Messages);
    let send = |c: &mut Controller<_>, m: Message| c.handle(Envelope::new(m));
    let codes = |out: &[Message]| -> Vec<String> {
        out.iter()
            .filter_map(|m| match m {
                Message::Notification(n) => Some(n.code.clone()),
                _ => None,
            })
            .collect()
    };
    let s = pair();
    let text = s.text(&s.hashes());
    assert!(matches!(
        send(
            &mut c,
            Message::Command(Command::LoadScenario {
                bytes: text.into_bytes()
            })
        )
        .last(),
        Some(Message::Answer(Answer::Done))
    ));
    for bytes in s.modules().into_values() {
        send(&mut c, Message::Command(Command::LoadModule { bytes }));
    }
    assert_eq!(
        send(&mut c, Message::Subscribe(Subscription::Display)),
        vec![Message::Answer(Answer::Done)]
    );
    send(&mut c, Message::Subscribe(Subscription::Events));
    // The run's log opens with the transport in use (rule 14: the user sees one line).
    let out = send(
        &mut c,
        Message::Command(Command::NewRun {
            seed: SeedChoice::Given(1),
            residency: florn_control::Residency::Resident,
        }),
    );
    let transport = out.iter().find_map(|m| match m {
        Message::Notification(n) if n.code == "telemetry.transport" => Some(n.clone()),
        _ => None,
    });
    let transport = transport.expect("telemetry.transport");
    assert_eq!(
        transport
            .params
            .iter()
            .find(|p| p.0 == "transport")
            .map(|p| p.1.clone()),
        Some(florn_control::Value::Enum("messages".into()))
    );
    // The batch of `new-run`: every actor's initialisation (RFC 002 rule 36), at 0.
    let init = c.take_display();
    assert_eq!(init.last(), Some(&Record::Instant { tick: 0 }));
    assert_eq!(
        init.iter()
            .filter(|r| matches!(r, Record::Activity { .. }))
            .count(),
        2
    );
    assert!(!init.iter().any(|r| matches!(r, Record::Flight { .. })));
    // Each advance is a batch; the drops the transport counted are in the next replies.
    let mut all = Vec::new();
    for i in 1..=4 {
        let out = send(
            &mut c,
            Message::Command(Command::Advance {
                unit: Unit::ToTick(i * 250),
            }),
        );
        if i == 2 {
            assert!(!codes(&out).contains(&"telemetry.dropped".to_string()));
            c.display_dropped(0, DisplayPath::Messages);
            assert!(c.drain().is_empty(), "nothing dropped, nothing logged");
            c.display_dropped(7, DisplayPath::Messages);
            // Logged at once and delivered on demand, without waiting for an advance.
            let drained = c.drain();
            let dropped = drained
                .iter()
                .find_map(|m| match m {
                    Message::Notification(n) if n.code == "telemetry.dropped" => Some(n.clone()),
                    _ => None,
                })
                .expect("telemetry.dropped drained after the drop");
            let get = |name: &str| {
                dropped
                    .params
                    .iter()
                    .find(|p| p.0 == name)
                    .map(|p| p.1.clone())
            };
            assert_eq!(get("records"), Some(florn_control::Value::Count(7)));
            assert_eq!(
                get("path"),
                Some(florn_control::Value::Enum("messages".into()))
            );
        }
        if i == 3 {
            assert!(
                !codes(&out).contains(&"telemetry.dropped".to_string()),
                "drained once"
            );
        }
        let batch = c.take_display();
        assert_eq!(batch.last(), Some(&Record::Instant { tick: i * 250 }));
        all.extend(batch);
    }
    assert_eq!(
        all.iter()
            .filter(|r| matches!(r, Record::Flight { .. }))
            .count(),
        38
    );
    // Unsubscribed: nothing more is written, and a new run inherits the choice.
    send(&mut c, Message::Unsubscribe(SubscriptionKind::Display));
    let out = send(
        &mut c,
        Message::Command(Command::NewRun {
            seed: SeedChoice::Given(1),
            residency: florn_control::Residency::Resident,
        }),
    );
    assert!(codes(&out).contains(&"telemetry.transport".to_string()));
    send(
        &mut c,
        Message::Command(Command::Advance {
            unit: Unit::ToTick(1_000),
        }),
    );
    assert!(c.take_display().is_empty());
}

/// A paced slice advances by its ticks from the tick reached, not from the last event (the
/// kernel's clock stays there): a scenario whose events are sparse still progresses slice by
/// slice, and each batch's instant is the slice's end.
#[test]
fn a_paced_slice_advances_from_the_tick_reached() {
    use florn_control::Pacing;
    let mut c: Controller<florn_engine_wasmtime::WasmtimeEngine> = Controller::new(
        host::engine(),
        Admission::default(),
        Box::new(FixedPlatform::default()),
        host::domains(),
        host::options(),
    );
    let send = |c: &mut Controller<_>, m: Message| c.handle(Envelope::new(m));
    let s = shipped::dv_triangle(100);
    send(
        &mut c,
        Message::Command(Command::LoadScenario {
            bytes: s.text(&s.hashes()).into_bytes(),
        }),
    );
    for bytes in s.modules().into_values() {
        send(&mut c, Message::Command(Command::LoadModule { bytes }));
    }
    send(&mut c, Message::Subscribe(Subscription::Display));
    send(
        &mut c,
        Message::Command(Command::NewRun {
            seed: SeedChoice::Given(1),
            residency: florn_control::Residency::Resident,
        }),
    );
    send(
        &mut c,
        Message::Command(Command::SetPacing {
            pacing: Pacing::TicksPerSecond(1_000),
        }),
    );
    send(
        &mut c,
        Message::Command(Command::Advance {
            unit: Unit::ToTick(3_000),
        }),
    );
    c.take_display();
    let mut instants = Vec::new();
    for _ in 0..10 {
        c.pump(30);
        if let Some(Record::Instant { tick }) = c.take_display().last() {
            instants.push(*tick);
        }
    }
    assert_eq!(instants, (1..=10).map(|i| i * 30).collect::<Vec<_>>());
}
