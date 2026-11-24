//! PR-42, the comparison of Tier 2 (RFC 001 section 1.1 and rules 41, 50 and 62; RFC 013 rules 20
//! and 21; RFC 012 finding F-5): a replay of a run of a continuous domain, compared with its
//! record across Hosts within the domain's tolerance, through the kit's test domain. The test
//! domain is continuous when the scenario's `[test]` table gives `atol` and `rtol`: a report on
//! channel 1 of 16 bytes is a numeric `level` (an `f64` in `khop`) and a whole `located` tick.
//!
//! A record that another Host made is made here by doctoring a real one: the chain differs, the
//! environment is the other Host's, and the one report, the one count or the one key that the case
//! is about is changed in the trace and its payloads.
use std::collections::BTreeMap;

use florn_admission::Admission;
use florn_conformance::host;
use florn_conformance::test_domain::{self as td, CLASS_FRAME};
use florn_control::{Answer, Command, Envelope, Message, Unit};
use florn_host::compare::results;
use florn_host::record::{self, Contents, Read_};
use florn_host::run::{Replayed, Side};
use florn_host::world::Intervention;
use florn_host::{Controller, Run};
use florn_kernel::trace::CommittedEffect;
use florn_kernel::{Digest, Record};
use florn_platform::FixedPlatform;

type Host = Run<florn_engine_wasmtime::WasmtimeEngine>;

/// Two probers over one wire; actor 1 is of a continuous class when the table says so.
fn scenario(continuous: bool) -> String {
    let table = if continuous {
        "\n[test]\natol = 0.001\nrtol = 0.000001\ncontinuous-actors = [1]\n"
    } else {
        ""
    };
    td::text(
        "tier-two",
        400,
        7,
        &format!(
            "{}{}{}{table}",
            td::prober(1, 1),
            td::prober(2, 1),
            td::wire(100, &[(1, 0), (2, 0)], "")
        ),
    )
}

/// A report payload of the test domain: the `level` and the whole `located` tick.
fn payload(level: f64, located: u64) -> Vec<u8> {
    let mut b = level.to_le_bytes().to_vec();
    b.extend_from_slice(&located.to_le_bytes());
    b
}

/// The messages that make reports of actor 1 on channel 1: (tick, level, located).
const REPORTS: [(u64, f64, u64); 3] = [(50, 1.5, 10), (120, 2.5, 20), (220, 3.5, 30)];

fn start(continuous: bool) -> Host {
    start_with(continuous, REPORTS)
}

/// The same run with other reports.
fn start_with(continuous: bool, reports: [(u64, f64, u64); 3]) -> Host {
    let mut run = td::try_start(&scenario(continuous), host::options()).expect("starts");
    for (tick, level, located) in reports {
        run.intervene(
            Some(tick),
            &Intervention::Message {
                actor: 1,
                port: 0,
                payload: payload(level, located),
            },
        );
    }
    run
}

/// A finished run as a record, trace and payloads included: what `export` writes and `read` reads.
fn record_of(run: &mut Host) -> Read_ {
    run.run_to_end();
    let contents = Contents {
        run: Some(run.run_toml()),
        trace: run.records().map(|r| r.to_vec()),
        payloads: Some(run.payloads().clone()),
        log: run.log().clone(),
        readme: None,
    };
    record::read(&record::write(&contents).expect("written")).expect("read")
}

fn honest(continuous: bool) -> Read_ {
    record_of(&mut start(continuous))
}

/// Replay `record` over the scenario and say how it compares.
fn replay(continuous: bool, record: &Read_) -> (Host, Replayed) {
    let mut run = Run::replay(
        host::engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        &td::domains(),
        td::manifest(&scenario(continuous)),
        &td::modules(),
        &record.run,
        host::options(),
    )
    .expect("the replay starts");
    // The record's interventions are applied by the replay; its trace and payloads are what the
    // comparison reads of the recorded run.
    run.record_trace(record.trace.clone(), record.payloads.clone());
    run.run_to_end();
    let outcome = run.replayed().expect("a replay has an outcome");
    (run, outcome)
}

/// A parameter of the replay notification `code`, as the value it holds.
fn param(run: &Host, code: &str, name: &str) -> Option<florn_control::Value> {
    run.log()
        .entries()
        .iter()
        .find(|e| e.notification.code == code)?
        .notification
        .params
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.clone())
}

fn codes(run: &Host) -> Vec<String> {
    run.log()
        .entries()
        .iter()
        .map(|e| e.notification.code.clone())
        .filter(|c| c.starts_with("replay."))
        .collect()
}

/// Another Host's record: its environment, and a chain that differs.
fn from_another_host(mut r: Read_) -> Read_ {
    r.run.environment.host_build = "browser".into();
    r.run.environment.engine = "browser-wasm".into();
    let last = r.run.chain.len() - 1;
    r.run.chain[last].1[0] ^= 1;
    r
}

/// Change the payload of the `n`th numeric report (one of 16 bytes: the probers' frames are 4) to `bytes`, keeping the trace consistent
/// with its payloads (the digest of the record is the digest of the new bytes).
fn change_report(r: &mut Read_, n: usize, bytes: Vec<u8>) {
    let trace = r.trace.as_mut().expect("a trace");
    let mut seen = 0;
    for record in trace.iter_mut() {
        if let Record::Handled { effects, .. } = record {
            for e in effects.iter_mut() {
                if let CommittedEffect::Report { digest, len, .. } = e
                    && *len == 16
                {
                    if seen == n {
                        *digest = *blake3::hash(&bytes).as_bytes();
                        *len = bytes.len() as u32;
                        r.payloads.insert(*digest, bytes);
                        return;
                    }
                    seen += 1;
                }
            }
        }
    }
    panic!("no report number {n}");
}

/// The payload of the `n`th numeric report of the trace.
fn nth_report(r: &Read_, n: usize) -> Vec<u8> {
    let trace = r.trace.as_ref().unwrap();
    let digests: Vec<Digest> = trace
        .iter()
        .filter_map(|record| match record {
            Record::Handled { effects, .. } => Some(effects),
            _ => None,
        })
        .flatten()
        .filter_map(|e| match e {
            CommittedEffect::Report {
                digest, len: 16, ..
            } => Some(*digest),
            _ => None,
        })
        .collect();
    r.payloads[&digests[n]].clone()
}

// ---------------------------------------------------------------------------------------------
// The outcomes of RFC 013 section 11, "Comparison".
// ---------------------------------------------------------------------------------------------

#[test]
fn a_run_against_itself_is_matched() {
    let r = honest(true);
    assert!(r.trace.is_some() && !r.payloads.is_empty());
    let (run, outcome) = replay(true, &r);
    assert!(
        matches!(outcome, Replayed::Matched { points } if points >= 1),
        "{outcome:?}"
    );
    assert_eq!(codes(&run), ["replay.matched"]);
}

#[test]
fn a_payload_less_record_is_matched_where_the_chains_agree_and_needs_payloads_across_hosts() {
    // The record without a trace and payloads, as `export` writes it without `--trace --payloads`.
    let mut r = honest(true);
    r.trace = None;
    r.payloads.clear();
    assert_eq!(
        replay(true, &r).1,
        Replayed::Matched {
            points: r.run.chain.len() as u64
        }
    );
    // Another Host's, the chain doctored: the comparison within a tolerance cannot be made.
    let other = from_another_host(r.clone());
    let (run, outcome) = replay(true, &other);
    assert_eq!(outcome, Replayed::NeedsPayloads(Side::Recorded));
    assert_eq!(codes(&run), ["replay.needs-payloads"]);
    assert_eq!(
        host::log_param(&run, "replay.needs-payloads", "side").as_deref(),
        Some("recorded")
    );
    // On one build the same doctored chain is `diverged`: no tolerance is asked for.
    let mut same = r;
    let last = same.run.chain.len() - 1;
    same.run.chain[last].1[0] ^= 1;
    let (run, outcome) = replay(true, &same);
    assert!(
        matches!(
            outcome,
            Replayed::Diverged {
                difference: None,
                ..
            }
        ),
        "{outcome:?}"
    );
    assert_eq!(codes(&run), ["replay.diverged"]);
}

#[test]
fn a_last_bit_is_within_tolerance_across_hosts_and_diverged_on_one_build() {
    let mut r = honest(true);
    let level = f64::from_le_bytes(nth_report(&r, 1)[..8].try_into().unwrap());
    change_report(&mut r, 1, payload(f64::from_bits(level.to_bits() + 1), 20));
    // Across Hosts: within the tolerance, one record differs, the field is named.
    let (run, outcome) = replay(true, &from_another_host(r.clone()));
    let Replayed::WithinTolerance(agreement) = &outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(agreement.differing, 1);
    assert_eq!(agreement.compared, r.trace.as_ref().unwrap().len() as u64);
    let worst = agreement.worst.as_ref().expect("a field differs");
    assert_eq!(
        (worst.field.as_str(), worst.largest.1.as_str()),
        ("level", "khop")
    );
    assert_eq!(worst.at, 120, "the tick of the second report");
    assert_eq!(codes(&run), ["replay.within-tolerance"]);
    assert_eq!(
        param(&run, "replay.within-tolerance", "worst"),
        Some(florn_control::Value::Name("level".into()))
    );
    // On one build the same record is diverged by its digest, with the interval.
    let mut same = r;
    let last = same.run.chain.len() - 1;
    same.run.chain[last].1[0] ^= 1;
    let (run, outcome) = replay(true, &same);
    assert!(
        matches!(
            outcome,
            Replayed::Diverged {
                difference: None,
                ..
            }
        ),
        "{outcome:?}"
    );
    assert_eq!(codes(&run), ["replay.diverged"]);
}

#[test]
fn a_difference_in_consumed_work_alone_is_within_tolerance_for_a_continuous_actor_only() {
    // Actor 1 is of a continuous class: the work and the draws of its activations are exempt.
    let mut r = honest(true);
    let trace = r.trace.as_mut().unwrap();
    let work_of = |t: &mut Vec<Record>, actor| {
        for record in t.iter_mut() {
            if let Record::Handled {
                target, metered, ..
            } = record
                && *target == actor
            {
                *metered += 1000;
                return;
            }
        }
        panic!("no activation of {actor}");
    };
    work_of(trace, 1);
    let (_, outcome) = replay(true, &from_another_host(r.clone()));
    let Replayed::WithinTolerance(a) = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!((a.differing, a.consumption_differing), (0, 1));
    // Nothing compared differs: `replay.exempt-only`, not a tolerance comparison naming no field.
    let (run, _) = replay(true, &from_another_host(r.clone()));
    assert_eq!(codes(&run), ["replay.exempt-only"]);
    assert_eq!(
        param(&run, "replay.exempt-only", "compared"),
        Some(florn_control::Value::Count(a.compared))
    );
    // The same on one build is diverged.
    let mut same = r.clone();
    let last = same.run.chain.len() - 1;
    same.run.chain[last].1[0] ^= 1;
    assert!(matches!(replay(true, &same).1, Replayed::Diverged { .. }));
    // Actor 2 is not: its work is compared exactly, and the first record that differs is named.
    let mut r2 = honest(true);
    work_of(r2.trace.as_mut().unwrap(), 2);
    let (run, outcome) = replay(true, &from_another_host(r2));
    assert!(
        matches!(
            outcome,
            Replayed::Diverged {
                difference: Some(_),
                ..
            }
        ),
        "{outcome:?}"
    );
    assert_eq!(codes(&run), ["replay.record-differs"]);
    assert_eq!(
        host::log_param(&run, "replay.record-differs", "what").as_deref(),
        Some("structure")
    );
}

#[test]
fn a_numeric_difference_beyond_the_tolerance_is_diverged_naming_the_record_and_the_field() {
    let mut r = honest(true);
    change_report(&mut r, 2, payload(3.6, 30));
    let (run, outcome) = replay(true, &from_another_host(r));
    let Replayed::Diverged {
        difference:
            Some(florn_host::compare::Difference::Value {
                field,
                recorded,
                replayed,
                ..
            }),
        ..
    } = outcome
    else {
        panic!("{outcome:?}");
    };
    // Both values in the field's own unit: the recorded 3.6 khop and the replayed 3.5 khop.
    assert_eq!(field, "level");
    assert_eq!(recorded, ("3.6".to_string(), "khop".to_string()));
    assert_eq!(replayed, ("3.5".to_string(), "khop".to_string()));
    // In place of `replay.diverged`.
    assert_eq!(codes(&run), ["replay.value-differs"]);
    assert_eq!(
        param(&run, "replay.value-differs", "field"),
        Some(florn_control::Value::Name("level".into()))
    );
}

#[test]
fn a_different_condition_tick_is_diverged() {
    // The `located` tick is no numeric field: compared exactly, whatever the tolerance.
    let mut r = honest(true);
    change_report(&mut r, 0, payload(1.5, 11));
    let (run, outcome) = replay(true, &from_another_host(r));
    assert!(
        matches!(
            outcome,
            Replayed::Diverged {
                difference: Some(_),
                ..
            }
        ),
        "{outcome:?}"
    );
    assert_eq!(codes(&run), ["replay.record-differs"]);
    assert_eq!(
        host::log_param(&run, "replay.record-differs", "what").as_deref(),
        Some("payload")
    );
}

#[test]
fn a_different_record_count_a_key_and_a_structure_are_diverged() {
    // One record fewer in the recorded trace: the length, at the end of the shorter.
    let mut r = honest(true);
    let total = r.trace.as_ref().unwrap().len();
    r.trace.as_mut().unwrap().pop();
    let (run, outcome) = replay(true, &from_another_host(r));
    assert!(matches!(
        outcome,
        Replayed::Diverged {
            difference: Some(_),
            ..
        }
    ));
    assert_eq!(codes(&run), ["replay.record-differs"]);
    assert_eq!(
        host::log_param(&run, "replay.record-differs", "what").as_deref(),
        Some("length")
    );
    assert_eq!(
        host::log_param(&run, "replay.record-differs", "record").as_deref(),
        Some(total.to_string().as_str())
    );
    // A key: the tick of the first handled record moved.
    let mut r = honest(true);
    if let Some(Record::Handled { key, .. }) = r
        .trace
        .as_mut()
        .unwrap()
        .iter_mut()
        .find(|r| matches!(r, Record::Handled { .. }))
    {
        key.tick += 1;
    }
    let (run, _) = replay(true, &from_another_host(r));
    assert_eq!(
        host::log_param(&run, "replay.record-differs", "what").as_deref(),
        Some("key")
    );
    // A structure: an effect removed.
    let mut r = honest(true);
    for record in r.trace.as_mut().unwrap() {
        if let Record::Handled { effects, .. } = record
            && !effects.is_empty()
        {
            effects.pop();
            break;
        }
    }
    let (run, _) = replay(true, &from_another_host(r));
    assert_eq!(
        host::log_param(&run, "replay.record-differs", "what").as_deref(),
        Some("structure")
    );
}

#[test]
fn a_record_exported_before_the_end_holds_a_prefix_and_is_compared_as_one() {
    // A run stopped at tick 130 with two of the three reports made; its record's chain stores the
    // points it reached. Replayed to the scenario's end, its prefix compares at those points, and
    // its doctored last bit across Hosts is within tolerance and not a difference in length.
    let mut run = start(true);
    run.advance(florn_host::Advance::ToTick(130));
    let mut r = {
        let contents = Contents {
            run: Some(run.run_toml()),
            trace: run.records().map(|r| r.to_vec()),
            payloads: Some(run.payloads().clone()),
            log: run.log().clone(),
            readme: None,
        };
        record::read(&record::write(&contents).unwrap()).unwrap()
    };
    assert!(
        r.run.ended.is_none(),
        "the record was exported before the run ended"
    );
    let whole = honest(true).trace.unwrap().len();
    assert!(r.trace.as_ref().unwrap().len() < whole);
    let level = f64::from_le_bytes(nth_report(&r, 1)[..8].try_into().unwrap());
    change_report(&mut r, 1, payload(f64::from_bits(level.to_bits() + 1), 20));
    let (_, outcome) = replay(true, &from_another_host(r.clone()));
    let Replayed::WithinTolerance(a) = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(a.compared, r.trace.as_ref().unwrap().len() as u64);
}

#[test]
fn only_runs_of_one_version_and_a_continuous_domain_are_compared_within_a_tolerance() {
    let mut r = honest(true);
    let level = f64::from_le_bytes(nth_report(&r, 0)[..8].try_into().unwrap());
    change_report(&mut r, 0, payload(f64::from_bits(level.to_bits() + 1), 10));
    // Another florn version: Tier 3, nothing is promised, a difference is `diverged`.
    let mut other_version = from_another_host(r.clone());
    other_version.run.environment.florn_version = "0.5.0".into();
    assert!(matches!(
        replay(true, &other_version).1,
        Replayed::Diverged {
            difference: None,
            ..
        }
    ));
    // A discrete scenario (no tolerance) is exact across Hosts too.
    let mut discrete = honest(false);
    discrete.run.environment.host_build = "browser".into();
    let last = discrete.run.chain.len() - 1;
    discrete.run.chain[last].1[0] ^= 1;
    let (run, outcome) = replay(false, &discrete);
    assert!(
        matches!(
            outcome,
            Replayed::Diverged {
                difference: None,
                ..
            }
        ),
        "{outcome:?}"
    );
    assert_eq!(codes(&run), ["replay.diverged"]);
    // The same environment and a different chain: Tier 1, `diverged`.
    let mut same = r;
    let last = same.run.chain.len() - 1;
    same.run.chain[last].1[0] ^= 1;
    assert!(matches!(
        replay(true, &same).1,
        Replayed::Diverged {
            difference: None,
            ..
        }
    ));
}

#[test]
fn a_replay_of_a_continuous_scenario_keeps_its_own_trace_and_payloads() {
    // `florn replay` keeps them for such a scenario whatever was asked (RFC 013 rule 20).
    let r = honest(true);
    let mut options = host::options();
    options.keep_records = false;
    options.keep_payloads = false;
    let mut run = Run::replay(
        host::engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        &td::domains(),
        td::manifest(&scenario(true)),
        &td::modules(),
        &r.run,
        options,
    )
    .unwrap();
    run.record_trace(r.trace.clone(), r.payloads.clone());
    run.run_to_end();
    assert!(run.records().is_some_and(|t| !t.is_empty()));
    assert!(!run.payloads().is_empty());
    assert!(matches!(run.replayed(), Some(Replayed::Matched { .. })));
    // A discrete scenario's replay keeps nothing more than it was asked to.
    let d = honest(false);
    let mut options = host::options();
    options.keep_records = false;
    options.keep_payloads = false;
    let mut run = Run::replay(
        host::engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        &td::domains(),
        td::manifest(&scenario(false)),
        &td::modules(),
        &d.run,
        options,
    )
    .unwrap();
    run.run_to_end();
    assert!(run.records().is_none());
    assert!(run.payloads().is_empty());
}

// ---------------------------------------------------------------------------------------------
// The control interface and the results file.
// ---------------------------------------------------------------------------------------------

fn controller() -> Controller<florn_engine_wasmtime::WasmtimeEngine> {
    Controller::new(
        host::engine(),
        Admission::default(),
        Box::new(FixedPlatform::default()),
        td::domains(),
        host::options(),
    )
}

fn replay_through_the_controller(bytes: Vec<u8>) -> Vec<String> {
    let mut c = controller();
    let send = |c: &mut Controller<_>, m: Message| c.handle(Envelope::new(m));
    send(
        &mut c,
        Message::Command(Command::LoadScenario {
            bytes: scenario(true).into_bytes(),
        }),
    );
    send(
        &mut c,
        Message::Command(Command::LoadModule {
            bytes: td::guest_bytes(),
        }),
    );
    let started = send(&mut c, Message::Command(Command::Replay { record: bytes }));
    assert!(
        started
            .iter()
            .any(|m| matches!(m, Message::Answer(Answer::Done))),
        "the replay started: {started:?}"
    );
    let replies = send(
        &mut c,
        Message::Command(Command::Advance {
            unit: Unit::ToTick(u64::MAX),
        }),
    );
    replies
        .into_iter()
        .filter_map(|m| match m {
            Message::Notification(n) if n.code.starts_with("replay.") => Some(n.code),
            _ => None,
        })
        .collect()
}

fn bytes_of(r: &Read_) -> Vec<u8> {
    record::write(&Contents {
        run: Some(r.run.clone()),
        trace: r.trace.clone(),
        payloads: Some(r.payloads.clone()),
        log: Default::default(),
        readme: None,
    })
    .unwrap()
}

#[test]
fn the_control_interface_replays_a_record_with_its_trace_and_payloads() {
    // The trace and the payloads of the record reach the comparison through `Command::Replay`.
    let mut r = honest(true);
    let level = f64::from_le_bytes(nth_report(&r, 1)[..8].try_into().unwrap());
    change_report(&mut r, 1, payload(f64::from_bits(level.to_bits() + 1), 20));
    let other = from_another_host(r.clone());
    assert_eq!(
        replay_through_the_controller(bytes_of(&other)),
        ["replay.within-tolerance"]
    );
    // The same record without its trace and payloads needs them.
    let mut bare = other.clone();
    bare.trace = None;
    bare.payloads.clear();
    assert_eq!(
        replay_through_the_controller(bytes_of(&bare)),
        ["replay.needs-payloads"]
    );
    // And an honest record matches.
    assert_eq!(
        replay_through_the_controller(bytes_of(&honest(true))),
        ["replay.matched"]
    );
}

#[test]
fn the_replay_notifications_are_in_the_catalogue() {
    // Every notification of a replay passes the check of RFC 007's catalogue: the parameters are
    // the declared ones, of the declared types, and the enumerations' values are in their sets.
    let mut r = honest(true);
    let level = f64::from_le_bytes(nth_report(&r, 1)[..8].try_into().unwrap());
    change_report(&mut r, 1, payload(f64::from_bits(level.to_bits() + 1), 20));
    let mut bare = honest(true);
    bare.trace = None;
    let mut length = honest(true);
    length.trace.as_mut().unwrap().pop();
    let mut beyond = honest(true);
    change_report(&mut beyond, 2, payload(9.0, 30));
    let mut same = honest(true);
    let last = same.run.chain.len() - 1;
    same.run.chain[last].1[0] ^= 1;
    let mut work_only = honest(true);
    for record in work_only.trace.as_mut().unwrap() {
        if let Record::Handled {
            target: 1, metered, ..
        } = record
        {
            *metered += 1000;
            break;
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for record in [
        honest(true),
        from_another_host(r),
        from_another_host(bare),
        from_another_host(length),
        from_another_host(beyond),
        from_another_host(work_only),
        same,
    ] {
        let (run, _) = replay(true, &record);
        for entry in run.log().entries() {
            if entry.notification.code.starts_with("replay.") {
                entry.notification.check().expect("in the catalogue");
                seen.insert(entry.notification.code.clone());
            }
        }
    }
    let expected: std::collections::BTreeSet<String> = [
        "replay.matched",
        "replay.within-tolerance",
        "replay.exempt-only",
        "replay.diverged",
        "replay.value-differs",
        "replay.record-differs",
        "replay.needs-payloads",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(seen, expected);
}

#[test]
fn the_results_of_a_run_are_the_text_the_comparison_reads() {
    // Rows from a record, as text, and back: what the browser scripts and the native comparison
    // both read (RFC 013 rule 21).
    let r = honest(true);
    let run = replay(true, &r).0;
    let rows = run.rows().expect("the replay kept its trace");
    let text = results::write(&rows);
    assert!(text.starts_with("florn-results 1\n"));
    assert_eq!(text.lines().count(), rows.len() + 1);
    assert_eq!(results::parse(&text).unwrap(), rows);
    // The numeric fields of the reports are there as bits with their name and unit.
    assert!(text.contains("level|khop|"), "{text}");
    // Another run of the same record gives the same text.
    let again = replay(true, &r).0.rows().unwrap();
    assert_eq!(results::write(&again), text);
    // Every kind of effect the run has is in the rows.
    assert!(rows.iter().any(
        |r| matches!(r, florn_host::compare::Row::Handled { effects, .. } if !effects.is_empty())
    ));
    let _ = (CLASS_FRAME, BTreeMap::<u8, u8>::new());
}

#[test]
fn the_results_command_writes_the_file_from_a_record_with_payloads() {
    // `florn-conformance results`: the same text as the rows of the run, from the bytes of the
    // record, the scenario and the modules (RFC 013 rule 21).
    let r = honest(true);
    let record_bytes = bytes_of(&r);
    let text = florn_conformance::results::from_record(
        td::domains(),
        &record_bytes,
        scenario(true).as_bytes(),
        &[td::guest_bytes()],
    )
    .expect("the results");
    let rows = replay(true, &r).0.rows().unwrap();
    assert_eq!(text, results::write(&rows));
    assert!(text.starts_with("florn-results 1\n"));
    assert!(text.contains("level|khop|"));
    // A record without its trace, or without payloads, is refused with what to do.
    let mut bare = r.clone();
    bare.trace = None;
    let e = florn_conformance::results::from_record(
        td::domains(),
        &bytes_of(&bare),
        scenario(true).as_bytes(),
        &[td::guest_bytes()],
    )
    .unwrap_err();
    assert!(e.contains("no trace"), "{e}");
    let mut no_payloads = r;
    no_payloads.payloads.clear();
    let e = florn_conformance::results::from_record(
        td::domains(),
        &bytes_of(&no_payloads),
        scenario(true).as_bytes(),
        &[td::guest_bytes()],
    )
    .unwrap_err();
    assert!(e.contains("no payloads"), "{e}");
    // A scenario that is not the record's is refused by the replay's own check.
    let e = florn_conformance::results::from_record(
        td::domains(),
        &record_bytes,
        scenario(false).as_bytes(),
        &[td::guest_bytes()],
    )
    .unwrap_err();
    assert!(e.contains("not started") || e.contains("mismatch"), "{e}");
}

#[test]
fn chains_that_differ_while_no_record_does_are_diverged_not_agreement() {
    // RFC 013 rule 20: when the chains differ and no record differs at all, exempt values
    // included, nothing explains the difference; the comparison must not call it agreement.
    // Across Hosts, the trace and payloads of the record intact, one chain bit doctored:
    let r = from_another_host(honest(true));
    let (run, outcome) = replay(true, &r);
    assert!(
        matches!(
            outcome,
            Replayed::Diverged {
                difference: None,
                ..
            }
        ),
        "{outcome:?}"
    );
    assert_eq!(codes(&run), ["replay.diverged"]);
    // The same record with one exempt difference (the work of the continuous actor) is
    // `replay.exempt-only`: something does explain the chains.
    let mut exempt = r.clone();
    for record in exempt.trace.as_mut().unwrap() {
        if let Record::Handled {
            target: 1, metered, ..
        } = record
        {
            *metered += 1;
            break;
        }
    }
    let (run, outcome) = replay(true, &exempt);
    assert!(
        matches!(outcome, Replayed::WithinTolerance(_)),
        "{outcome:?}"
    );
    assert_eq!(codes(&run), ["replay.exempt-only"]);
}

#[test]
fn stored_points_at_other_positions_are_diverged_under_every_tier() {
    // RFC 013 rule 20: the kernel's `Incomparable` is unchanged and still `replay.diverged`: the rows
    // are not compared, so a record whose first stored point is at another position is diverged
    // even when its trace agrees within the tolerance across Hosts.
    let mut r = honest(true);
    let level = f64::from_le_bytes(nth_report(&r, 1)[..8].try_into().unwrap());
    change_report(&mut r, 1, payload(f64::from_bits(level.to_bits() + 1), 20));
    let mut r = from_another_host(r);
    r.run.chain[0].0 += 1;
    let (run, outcome) = replay(true, &r);
    assert_eq!(
        outcome,
        Replayed::Diverged {
            from: 0,
            to: 0,
            difference: None
        }
    );
    assert_eq!(codes(&run), ["replay.diverged"]);
}

#[test]
fn a_pair_of_signed_zeros_is_within_tolerance_and_names_the_field() {
    // RFC 013 rule 19 leaves the sign of a zero to the target: a trace that differs only there is
    // within tolerance and names the field, not an unexplained difference. The run reports `+0` at
    // its second report; the record, as another Host's, holds `-0`.
    let zeros = [(50, 1.5, 10), (120, 0.0, 20), (220, 3.5, 30)];
    let mut r = record_of(&mut start_with(true, zeros));
    change_report(&mut r, 1, payload(-0.0, 20));
    let (run, outcome) = replay(true, &from_another_host(r));
    let Replayed::WithinTolerance(a) = &outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(a.differing, 1);
    assert_eq!(codes(&run), ["replay.within-tolerance"]);
    assert_eq!(
        param(&run, "replay.within-tolerance", "worst"),
        Some(florn_control::Value::Name("level".into()))
    );
    assert_eq!(
        param(&run, "replay.within-tolerance", "largest"),
        Some(florn_control::Value::Quantity("0".into(), "khop".into()))
    );
}

#[test]
fn a_record_is_counted_from_one() {
    // `record` counts from 1, as the stored points count records.
    let mut r = honest(true);
    change_report(&mut r, 0, payload(9.0, 10));
    let (run, _) = replay(true, &from_another_host(r.clone()));
    let first_numeric = r
        .trace
        .as_ref()
        .unwrap()
        .iter()
        .position(|record| {
            matches!(record, Record::Handled { effects, .. }
                if effects.iter().any(|e| matches!(e, CommittedEffect::Report { len: 16, .. })))
        })
        .expect("a numeric report")
        + 1;
    assert_eq!(
        param(&run, "replay.value-differs", "record"),
        Some(florn_control::Value::Count(first_numeric as u64))
    );
}
