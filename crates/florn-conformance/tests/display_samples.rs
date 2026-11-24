//! The `sample` record of the display stream (RFC 014 rules 2 and 3; RFC 012 finding F-6; checklist
//! B-43): the Host writes one record per `f64` field a domain draws as a series, in a report it
//! committed, before the batch's `instant`; the actor's place in id order, the field's index, the
//! value as an `f32`, the tick of the report. The kit's test domain draws a 16-byte report on
//! channel 1 when its `[test]` table gives a tolerance (it is continuous).
use florn_conformance::host;
use florn_conformance::test_domain as td;
use florn_control::display::{Record, Reduced, decode_all, encode_all};
use florn_host::world::Intervention;
use florn_host::{Advance, Options};

fn scenario(continuous: bool) -> String {
    let table = if continuous {
        "\n[test]\natol = 0.001\nrtol = 0.000001\ncontinuous-actors = [7]\n"
    } else {
        ""
    };
    td::text(
        "samples",
        400,
        3,
        &format!(
            "{}{}{}{table}",
            td::prober(1, 1),
            td::prober(7, 1),
            td::wire(100, &[(1, 0), (7, 0)], "")
        ),
    )
}

fn level(value: f64, located: u64) -> Vec<u8> {
    let mut b = value.to_le_bytes().to_vec();
    b.extend_from_slice(&located.to_le_bytes());
    b
}

/// A run of the scenario with the display on; reports of `actor` at the given ticks.
fn drawn(continuous: bool, display: bool, reports: &[(u32, u64, f64)]) -> host::Host {
    let mut run = td::try_start(
        &scenario(continuous),
        Options {
            display,
            ..host::options()
        },
    )
    .expect("starts");
    for (actor, tick, value) in reports {
        run.intervene(
            Some(*tick),
            &Intervention::Message {
                actor: *actor,
                port: 0,
                payload: level(*value, 9),
            },
        );
    }
    run
}

fn samples(batch: &[Record]) -> Vec<Record> {
    batch
        .iter()
        .filter(|r| matches!(r, Record::Sample { .. }))
        .copied()
        .collect()
}

#[test]
fn a_report_the_domain_draws_is_a_sample_before_the_instant() {
    let mut run = drawn(true, true, &[(1, 50, 1.5), (7, 60, 0.1)]);
    run.advance(Advance::ToTick(100));
    let batch = run.take_display();
    // Actor 1 is the first in id order and actor 7 the second; the value is an f32 and the tick
    // is the tick of the report, which is the delivery's (the next instant after the request).
    let got = samples(&batch);
    assert_eq!(got.len(), 2, "{batch:?}");
    let Record::Sample {
        index,
        actor,
        value,
        tick,
    } = got[0]
    else {
        unreachable!()
    };
    assert_eq!((index, actor, value), (0, 0, 1.5f32));
    assert!((50..=52).contains(&tick), "{tick}");
    let Record::Sample {
        actor: second,
        value: second_value,
        ..
    } = got[1]
    else {
        unreachable!()
    };
    assert_eq!(second, 1, "an actor's place in id order, not its identity");
    assert_eq!(second_value, 0.1f64 as f32, "the f64 narrowed once");
    // The samples come before the instant record, which is the batch's last.
    assert!(matches!(batch.last(), Some(Record::Instant { .. })));
    let last_sample = batch
        .iter()
        .rposition(|r| matches!(r, Record::Sample { .. }))
        .unwrap();
    assert!(last_sample < batch.len() - 1);
    // A batch is taken once: the next holds none of them.
    run.advance(Advance::ToTick(120));
    assert!(samples(&run.take_display()).is_empty());
}

#[test]
fn nothing_is_drawn_for_a_discrete_domain_a_display_that_is_off_or_a_report_of_another_shape() {
    // A discrete scenario (no tolerance): the test domain draws nothing.
    let mut run = drawn(false, true, &[(1, 50, 1.5)]);
    run.advance(Advance::ToTick(100));
    assert!(samples(&run.take_display()).is_empty());
    // The display off: nothing is written at all.
    let mut run = drawn(true, false, &[(1, 50, 1.5)]);
    run.advance(Advance::ToTick(100));
    assert!(run.take_display().is_empty());
    // A report of another length is no level.
    let mut run = drawn(true, true, &[]);
    run.intervene(
        Some(50),
        &Intervention::Message {
            actor: 1,
            port: 0,
            payload: vec![1, 2, 3, 4],
        },
    );
    run.advance(Advance::ToTick(100));
    assert!(samples(&run.take_display()).is_empty());
}

#[test]
fn the_display_never_touches_the_trace() {
    // OBS-003: the display path may drop or thin; the trace never changes (RFC 001 rule 39).
    let reports = [(1, 50, 1.5), (7, 60, 0.1), (1, 120, 2.5)];
    let mut on = drawn(true, true, &reports);
    let mut off = drawn(true, false, &reports);
    on.run_to_end();
    off.run_to_end();
    assert_eq!(on.chain_hex(), off.chain_hex());
}

#[test]
fn samples_are_sixteen_bytes_round_trip_and_are_kept_by_the_reduction() {
    let mut run = drawn(true, true, &[(1, 50, 1.5), (1, 120, 2.5), (7, 130, 0.25)]);
    let mut reduced = Reduced::default();
    let mut all = Vec::new();
    while run.ended().is_none() {
        let to = run.reached() + 50;
        run.advance(Advance::ToTick(to));
        let batch = run.take_display();
        // The wire form: sixteen bytes a record, and the same records read back.
        let bytes = encode_all(&batch);
        assert_eq!(bytes.len(), batch.len() * 16);
        assert_eq!(decode_all(&bytes).as_deref(), Some(batch.as_slice()));
        reduced.fold(&batch);
        all.extend(samples(&batch));
    }
    assert_eq!(all.len(), 3);
    assert_eq!(reduced.samples, all, "every sample is kept, in order");
}
