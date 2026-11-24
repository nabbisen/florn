//! What is not a numeric value: key, structure, payload and length.

use super::*;

// ---- B9..B14: key, structure, payload, length -----------------------------------------------------

fn diverged_what(d: &mut Doc) -> (String, Vec<String>) {
    d.flip_chain();
    d.across();
    let o = compare(d);
    assert_eq!(o.kind(), "diverged", "{:?}", o.codes());
    (o.text("replay.record-differs", "what"), o.codes())
}

#[test]
fn b9_a_condition_like_exact_field_and_a_key_tick() {
    // `located` is a whole tick, compared exactly: a payload difference.
    let mut d = base();
    let l = level_records(&d.trace)[1];
    let (_, loc) = d.level(l);
    d.set_located(l, loc + 1);
    let (what, codes) = diverged_what(&mut d);
    assert_eq!(
        (what.as_str(), codes),
        ("payload", vec!["replay.record-differs".to_string()])
    );
    // the record's key tick
    let mut d = base();
    if let Record::Handled { key, .. } = &mut d.trace[l] {
        key.tick += 1;
    }
    let (what, _) = diverged_what(&mut d);
    assert_eq!(what, "key");
}

#[test]
fn b10_a_different_count_is_length() {
    let mut d = base();
    d.trace.pop();
    let (what, codes) = diverged_what(&mut d);
    assert_eq!(
        (what.as_str(), codes),
        ("length", vec!["replay.record-differs".to_string()])
    );
    let mut d = base();
    let extra = d.trace[0].clone();
    d.trace.push(extra);
    let (what, _) = diverged_what(&mut d);
    assert_eq!(what, "length");
}

#[test]
fn b10b_a_record_missing_in_the_middle_names_the_first_that_no_longer_agrees() {
    let mut d = base();
    let k = d.trace.len() / 2;
    d.trace.remove(k);
    d.flip_chain();
    d.across();
    let o = compare(&d);
    assert_eq!(o.kind(), "diverged");
    let r: usize = o
        .text("replay.record-differs", "record")
        .parse()
        .unwrap_or(usize::MAX);
    println!(
        "middle removal at {k}: record {r}, what {}",
        o.text("replay.record-differs", "what")
    );
    // Ruling: `record` counts from 1.
    assert_eq!(r, k + 1, "named {r}, removed {k}");
}

#[test]
fn b11_each_key_field_and_the_target_is_a_key_difference() {
    for field in [
        "tick",
        "microstep",
        "class",
        "tie",
        "origin",
        "seq",
        "target",
    ] {
        let mut d = base();
        let i = level_records(&d.trace)[1];
        match &mut d.trace[i] {
            Record::Handled { key, target, .. } => match field {
                "tick" => key.tick += 1,
                "microstep" => key.microstep += 1,
                "class" => key.class = key.class.wrapping_add(1),
                "tie" => key.tie += 1,
                "origin" => key.origin += 1,
                "seq" => key.seq += 1,
                _ => *target += 1,
            },
            _ => unreachable!(),
        }
        let (what, codes) = diverged_what(&mut d);
        assert_eq!(
            (what.as_str(), codes),
            ("key", vec!["replay.record-differs".to_string()]),
            "{field}"
        );
    }
}

#[test]
fn b12_kind_and_effect_structure_are_structure_differences() {
    // the kind of record
    let mut d = base();
    let i = level_records(&d.trace)[1];
    let key = match &d.trace[i] {
        Record::Handled { key, .. } => *key,
        _ => unreachable!(),
    };
    d.trace[i] = Record::Fault {
        key,
        actor: 1,
        kind: florn_kernel::FaultKind::Contract,
        parameter: 1,
        metered: 0,
    };
    assert_eq!(diverged_what(&mut d).0, "structure");
    // effects: the sequence number, the channel, the length
    for what in ["seq", "channel", "len"] {
        let mut d = base();
        let i = level_records(&d.trace)[1];
        if let Record::Handled { effects, .. } = &mut d.trace[i] {
            for e in effects {
                if let CommittedEffect::Report {
                    seq, channel, len, ..
                } = e
                {
                    match what {
                        "seq" => *seq += 1,
                        "channel" => *channel = 9,
                        _ => *len += 1,
                    }
                }
            }
        }
        let (w, codes) = diverged_what(&mut d);
        assert_eq!(
            (w.as_str(), codes),
            ("structure", vec!["replay.record-differs".to_string()]),
            "{what}"
        );
    }
    // an extra effect
    let mut d = base();
    let i = level_records(&d.trace)[1];
    if let Record::Handled { effects, .. } = &mut d.trace[i] {
        effects.push(CommittedEffect::CancelTimer { seq: 99, timer: 0 });
    }
    assert_eq!(diverged_what(&mut d).0, "structure");
}

#[test]
fn b13_a_payload_with_no_numeric_layout_is_compared_by_digest() {
    let mut d = base();
    let i = frame_records(&d.trace)[0];
    let (_, _) = (0, 0);
    let digest = match &d.trace[i] {
        Record::Handled { effects, .. } => effects
            .iter()
            .find_map(|e| match e {
                CommittedEffect::Report {
                    channel: 1, digest, ..
                } => Some(*digest),
                _ => None,
            })
            .unwrap(),
        _ => unreachable!(),
    };
    let mut bytes = d.payloads[&digest].clone();
    bytes[0] ^= 1;
    d.set_report(i, bytes);
    let (what, codes) = diverged_what(&mut d);
    assert_eq!(
        (what.as_str(), codes),
        ("payload", vec!["replay.record-differs".to_string()])
    );
}

#[test]
fn b14_an_intervention_content_is_a_payload_difference() {
    let mut d = base();
    let i = d
        .trace
        .iter()
        .position(|r| matches!(r, Record::Intervention { .. }))
        .expect("an intervention record");
    if let Record::Intervention { content_digest, .. } = &mut d.trace[i] {
        content_digest[0] ^= 1;
    }
    let (what, codes) = diverged_what(&mut d);
    assert_eq!(
        (what.as_str(), codes),
        ("payload", vec!["replay.record-differs".to_string()])
    );
}
