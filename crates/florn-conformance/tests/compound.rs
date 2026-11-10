//! PR-38, the kernel's discard and the call-in handle (RFC 012 findings F-2 and F-3), through the
//! kit's test domain: a compound activation of a guest (RFC 002 rules 44 and 45) played by a
//! script (`test_domain::steps`), with the contract rules the checklist names:
//!
//! - RFC 001 rule 24: a fault discards the effects and the stream position and commits the fault;
//! - RFC 002 rule 44: one budget and one set of quotas for the whole; effects recorded across the
//!   calls and committed at the committing close or discarded at the discarding one; a fault in any
//!   call ends the whole as a fault; no other activation of the actor starts before it closes; a
//!   call out of sequence is a contract violation;
//! - RFC 003 rule 12: the budget is set at the opening call and read at the closing call;
//! - RFC 012 rule 20: the Host checks the length of `evaluate`'s result before it lifts the list;
//!   a send or a timer in a device is a contract violation (rule 19);
//! - RFC 012 rule 21: a rejected step is discarded whole: not recorded, the random stream restored,
//!   no fault, the actor alive.
use florn_conformance::host::{self, faults, reports, texts};
use florn_conformance::test_domain::steps::STEP;
use florn_conformance::test_domain::{self as td};
use florn_kernel::{FaultKind, Record};
use florn_model::canonical::Encoder;
use florn_model::manifest::{DomainMap, DomainValue};

/// A prober (actor 1) and a second one (actor 2) on one wire (id 100).
fn scenario() -> String {
    td::text(
        "steps",
        400,
        7,
        &format!(
            "{}{}{}",
            td::prober(1, 1),
            td::prober(2, 1),
            td::wire(100, &[(1, 0), (2, 0)], "")
        ),
    )
}

/// The `step` intervention for `actor`, as the canonical map the Host takes.
fn step(actor: u32, script: &str) -> Vec<u8> {
    let mut m = DomainMap::new();
    m.insert("kind".into(), DomainValue::Text(STEP.into()));
    m.insert("actor".into(), DomainValue::Integer(i64::from(actor)));
    m.insert("script".into(), DomainValue::Text(script.into()));
    let mut e = Encoder::default();
    florn_model::canonical::domain_map(&mut e, &m);
    e.finish()
}

/// A run with the given scripts, each at its own tick, to the end.
fn play(scripts: &[(u64, u32, &str)]) -> host::Host {
    let mut run = td::start(&scenario());
    for (tick, actor, script) in scripts {
        run.intervene_map(Some(*tick), 100, &step(*actor, script))
            .unwrap_or_else(|n| panic!("{}", n.code));
    }
    run.run_to_end();
    run
}

/// What the domain said each script came to, in order.
fn reported(run: &host::Host) -> Vec<String> {
    run.log()
        .entries()
        .iter()
        .filter(|e| e.notification.code == "domain.event")
        .filter(|e| {
            e.notification
                .params
                .iter()
                .any(|(k, v)| k == "event" && format!("{v:?}").contains("compound"))
        })
        .map(|e| {
            let detail = e.notification.params.iter().find(|(k, _)| k == "detail");
            match detail {
                Some((_, florn_control::Value::Text(t))) => t.clone(),
                other => format!("{other:?}"),
            }
        })
        .collect()
}

/// The records of the run that are the guest's activations of `actor` for a `step` event.
fn step_records(run: &host::Host, actor: u32) -> Vec<Record> {
    run.records()
        .unwrap()
        .iter()
        .filter(|r| match r {
            Record::Handled { key, target, .. } => {
                *target == actor && key.class == td::steps::CLASS_STEP
            }
            Record::Fault { key, actor: a, .. } => {
                *a == actor && key.class == td::steps::CLASS_STEP
            }
            _ => false,
        })
        .cloned()
        .collect()
}

#[test]
fn a_committed_compound_activation_is_one_record_with_the_work_of_all_its_calls() {
    let run = play(&[(50, 1, "open:0 eval:3 eval:2 commit")]);
    assert_eq!(
        reported(&run),
        vec!["open=opened eval=evaluated:3 eval=evaluated:2 commit=committed"]
    );
    let records = step_records(&run, 1);
    assert_eq!(records.len(), 1, "one record for four calls: {records:?}");
    let Record::Handled {
        metered, effects, ..
    } = &records[0]
    else {
        panic!("a Handled record");
    };
    assert!(*metered > 0);
    // The effects of the calls, in the order they were made (RFC 001 rule 23): `begin` reported on
    // channel 3, `accept` on channels 2 and 0.
    let channels: Vec<u32> = effects
        .iter()
        .filter_map(|e| match e {
            florn_kernel::trace::CommittedEffect::Report { channel, .. } => Some(*channel),
            _ => None,
        })
        .collect();
    assert_eq!(channels, vec![3, 2, 0]);
    // The text of channel 0 reaches the diagnostic log as for any committed activation.
    assert!(texts(&run).iter().any(|(a, t)| *a == 1 && t == "accepted"));
    // The same key as the event that caused it, and the actor lives.
    assert!(faults(&run).is_empty());
}

#[test]
fn the_budget_of_the_opening_call_is_the_one_read_at_the_closing() {
    // RFC 003 rule 12: one budget for all the calls. The work of the record is the sum: more
    // calls, more work; and what the opening call set is not set again by the later ones.
    let work = |script: &str| {
        let run = play(&[(50, 1, script)]);
        match &step_records(&run, 1)[0] {
            Record::Handled { metered, .. } => *metered,
            other => panic!("{other:?}"),
        }
    };
    let one = work("open:0 eval:3 commit");
    let three = work("open:0 eval:3 eval:3 eval:3 commit");
    let only_close = work("open:0 commit");
    assert!(
        only_close < one && one < three,
        "{only_close} {one} {three}"
    );
    // Each evaluate costs the same, so the difference is the same each time.
    let two = work("open:0 eval:3 eval:3 commit");
    assert_eq!(two - one, three - two, "{one} {two} {three}");
    // The work is more than the closing call alone: the budget was not reset at the close.
    assert!(three > 2 * only_close);
}

#[test]
fn a_discarded_compound_activation_leaves_the_trace_of_a_run_without_it() {
    // RFC 012 rule 21: not recorded. The run with a rejected step in the middle has the records
    // of the run without it, apart from the intervention that started it and the event it caused.
    let with = play(&[
        (50, 1, "open:0 eval:3 commit"),
        (100, 1, "open:0 eval:3 eval:3 discard"),
        (150, 1, "open:0 eval:3 commit"),
    ]);
    assert_eq!(
        reported(&with)[1],
        "open=opened eval=evaluated:3 eval=evaluated:3 discard=discarded"
    );
    let records = step_records(&with, 1);
    assert_eq!(
        records.len(),
        2,
        "the two commits and nothing for the discard"
    );
    assert!(faults(&with).is_empty(), "no fault");
    // The kernel's own check of the digest is in its tests; here, the guest's reports of the
    // discarded activation are not in the run.
    let on_channel_2 = reports(&with, 1, 2);
    assert_eq!(on_channel_2.len(), 2, "accept reported twice, reject never");
}

#[test]
fn a_discarded_activation_restores_the_random_stream() {
    // `begin` draws a value and `accept` reports it. A step that was drawn for and rejected
    // leaves the stream where it was: the next step, accepted, reports the value the rejected
    // one drew; and so does the first step of a run with no rejection at all.
    let first = |run: &host::Host| reports(run, 1, 2)[0].1.clone();
    let plain = play(&[(50, 1, "open:0 commit")]);
    let rejected = play(&[(50, 1, "open:0 eval:3 discard"), (100, 1, "open:0 commit")]);
    assert_eq!(first(&plain), first(&rejected));
    // And it is a draw, not a constant: another actor's stream differs.
    let other = play(&[(50, 2, "open:0 commit")]);
    assert_ne!(first(&plain), reports(&other, 2, 2)[0].1);
}

#[test]
fn a_discard_leaves_the_actor_alive_and_able_to_start_again() {
    let run = play(&[
        (50, 1, "open:0 discard"),
        (100, 1, "open:0 eval:2 discard"),
        (150, 1, "open:0 commit"),
    ]);
    assert_eq!(
        reported(&run),
        vec![
            "open=opened discard=discarded",
            "open=opened eval=evaluated:2 discard=discarded",
            "open=opened commit=committed"
        ]
    );
    assert!(faults(&run).is_empty());
}

#[test]
fn a_trap_in_a_call_records_one_fault_and_kills_the_actor() {
    let run = play(&[(50, 1, "open:0 eval:3 eval:-1 commit")]);
    assert_eq!(
        reported(&run),
        vec!["open=opened eval=evaluated:3 eval=faulted:trap:0"],
        "the script stops at the fault"
    );
    let f = faults(&run);
    assert_eq!(f.len(), 1, "one Fault record");
    assert_eq!((f[0].0, f[0].1), (1, FaultKind::Trap));
    // Nothing of the activation is committed: no Handled record for it, and no effects.
    assert!(
        step_records(&run, 1)
            .iter()
            .all(|r| matches!(r, Record::Fault { .. }))
    );
    assert!(reports(&run, 1, 3).is_empty() && reports(&run, 1, 2).is_empty());
    // The actor is dead: a second script finds it so.
    let again = play(&[(50, 1, "open:0 eval:-1"), (100, 1, "open:0 commit")]);
    assert_eq!(
        reported(&again)[1],
        "open=refused:the actor is dead commit=refused:no compound activation is open",
        "a dead actor opens nothing"
    );
    assert_eq!(faults(&again).len(), 1);
}

#[test]
fn a_budget_exhausted_in_a_later_call_is_a_budget_fault_of_the_whole() {
    let run = play(&[(50, 1, "open:0 eval:3 eval:-2 commit")]);
    let f = faults(&run);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].1, FaultKind::Budget);
    // The work it used is the whole budget of the one activation, not of the last call.
    assert!(f[0].3 > 1_000_000, "metered {}", f[0].3);
}

#[test]
fn a_send_or_a_timer_in_a_compound_activation_is_a_contract_violation() {
    // RFC 012 rule 19: a device has no queue presence; the handle commits with a world that
    // delivers nothing, so neither is ever recorded.
    for (x, what) in [(-3, "send"), (-4, "set-timer")] {
        let run = play(&[(50, 1, &format!("open:0 eval:{x} commit"))]);
        let f = faults(&run);
        assert_eq!(f.len(), 1, "{what}");
        assert_eq!(f[0].1, FaultKind::Contract, "{what}");
        assert_eq!(f[0].2, 19, "the parameter names RFC 012 rule 19");
        // Nothing of the activation was recorded, a send included: its only record is the Fault.
        let records = step_records(&run, 1);
        assert!(
            matches!(records.as_slice(), [Record::Fault { .. }]),
            "{what}: {records:?}"
        );
    }
}

#[test]
fn a_call_out_of_sequence_is_a_contract_violation() {
    // RFC 002 rule 44. A call that is not one the declaration permits at this point faults the
    // activation: here `evaluate` offered as the committing close.
    let run = play(&[(50, 1, "open:0 eval:3 badseq")]);
    let f = faults(&run);
    assert_eq!(f.len(), 1);
    assert_eq!((f[0].1, f[0].2), (FaultKind::Contract, 44));
    // Nothing open: refused, and nothing changes (there is no activation to fault).
    let closed = play(&[(50, 1, "closed")]);
    assert_eq!(
        reported(&closed),
        vec!["closed=refused:no compound activation is open"]
    );
    assert!(faults(&closed).is_empty());
    // A second opening while one is open: refused, and the first stays open.
    let twice = play(&[(50, 1, "open:0 reopen eval:2 commit")]);
    assert_eq!(
        reported(&twice),
        vec![
            "open=opened reopen=refused:a compound activation is already open \
             eval=evaluated:2 commit=committed"
        ]
    );
    assert!(faults(&twice).is_empty());
}

#[test]
fn the_domain_may_fault_the_activation_it_judges_broken() {
    let run = play(&[(50, 1, "open:0 eval:3 fault eval:3")]);
    assert_eq!(
        reported(&run),
        vec!["open=opened eval=evaluated:3 fault=faulted:contract:77"]
    );
    let f = faults(&run);
    assert_eq!((f.len(), f[0].1, f[0].2), (1, FaultKind::Contract, 77));
}

#[test]
fn a_list_longer_than_the_bound_is_refused_before_it_is_lifted() {
    // RFC 012 rule 20. The bound is 8. A list of 9 that the guest holds is refused as too long;
    // one of a billion elements that it does not hold is refused the same way, not as a failure
    // to lift it: the length is read first. The activation stays open, and the domain closes it.
    let run = play(&[
        (50, 1, "open:0 eval:8 eval:9 discard"),
        (100, 1, "open:0 eval:1000000000 discard"),
    ]);
    assert_eq!(
        reported(&run),
        vec![
            "open=opened eval=evaluated:8 eval=too-long:9>8 discard=discarded",
            "open=opened eval=too-long:1000000000>8 discard=discarded"
        ]
    );
    assert!(faults(&run).is_empty(), "no contract fault from a lift");
}

#[test]
fn the_post_return_runs_inside_the_activation_and_its_work_is_charged_to_it() {
    // `begin(h)` sets the post-return of `evaluate` to loop `h * 100` times. The work of the
    // record grows with h by the post-return's loops, which only the post-return makes.
    let work = |h: u32| {
        let run = play(&[(50, 1, &format!("open:{h} eval:3 commit"))]);
        match &step_records(&run, 1)[0] {
            Record::Handled { metered, .. } => *metered,
            other => panic!("{other:?}"),
        }
    };
    let (w0, w10, w20) = (work(0), work(10), work(20));
    assert!(w10 > w0 && w20 > w10, "{w0} {w10} {w20}");
    assert_eq!(w20 - w10, w10 - w0, "work in proportion to the loops");
    // A call whose result is not taken (refused as too long) does not run the post-return.
    let refused = |h: u32| {
        let run = play(&[(50, 1, &format!("open:{h} eval:9 commit"))]);
        match &step_records(&run, 1)[0] {
            Record::Handled { metered, .. } => *metered,
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(refused(0), refused(20), "no post-return, no loops");
}

#[test]
fn a_compound_activation_is_replayed_to_the_same_digest() {
    let scripts = [
        (50, 1, "open:2 eval:3 commit"),
        (100, 2, "open:1 eval:4 discard"),
        (150, 1, "open:0 eval:-1"),
    ];
    let a = play(&scripts);
    let b = play(&scripts);
    assert_eq!(a.chain_hex(), b.chain_hex());
    assert_eq!(a.records().unwrap().len(), b.records().unwrap().len());
}

#[test]
fn an_actor_swapped_out_is_brought_in_for_its_compound_activation() {
    // RFC 009 rule 13: a pool of one for two actors. Each script's opening call finds its actor
    // swapped out and the handle brings it in, as a plain activation does; the records are the
    // resident run's (swapping is invisible to the trace).
    let scripts = [
        (50, 1, "open:1 eval:3 commit"),
        (100, 2, "open:1 eval:3 commit"),
        (150, 1, "open:0 eval:2 discard"),
        (200, 2, "open:0 commit"),
    ];
    let run_with = |options: florn_host::Options| {
        let mut run = td::try_start(&scenario(), options).unwrap_or_else(|e| panic!("{e}"));
        for (tick, actor, script) in scripts {
            run.intervene_map(Some(tick), 100, &step(actor, script))
                .unwrap();
        }
        run.run_to_end();
        run
    };
    let resident = run_with(host::options());
    let swapping = run_with(florn_host::Options {
        residency: florn_host::Residency::Swap,
        resident_limit: Some(1),
        ..host::options()
    });
    assert!(
        swapping.swaps() > 1,
        "actors were swapped in: {}",
        swapping.swaps()
    );
    assert_eq!(resident.swaps(), 0);
    assert_eq!(reported(&swapping), reported(&resident));
    assert_eq!(swapping.chain_hex(), resident.chain_hex());
    assert!(faults(&swapping).is_empty());
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "left open")]
fn a_domain_that_leaves_an_activation_open_is_a_defect_that_a_debug_build_stops_on() {
    let _ = play(&[(50, 1, "open:0 eval:3")]);
}

#[test]
#[cfg(not(debug_assertions))]
fn a_domain_that_leaves_an_activation_open_has_it_discarded() {
    // The handle never outlives the call: the activation is discarded and the next one starts.
    let run = play(&[(50, 1, "open:0 eval:3"), (100, 1, "open:0 commit")]);
    assert!(faults(&run).is_empty());
    assert_eq!(step_records(&run, 1).len(), 1);
}

#[test]
fn one_set_of_quotas_covers_all_the_calls() {
    // RFC 002 rule 44: "one budget and one set of quotas apply to the whole". Each `evaluate`
    // here records one report; the effects-per-activation quota is 1,000 (RFC 003 rule 19). A
    // thousand calls fit it with the two reports of `begin` and the close to spare, none of them
    // near the quota by itself; five hundred and sixty more than fit do not.
    let fits = play(&[(50, 1, "open:0 evals:990 commit")]);
    assert!(faults(&fits).is_empty(), "{:?}", reported(&fits));
    assert_eq!(step_records(&fits, 1).len(), 1);
    let over = play(&[(50, 1, "open:0 evals:1100 commit")]);
    let f = faults(&over);
    assert_eq!(f.len(), 1, "{:?}", reported(&over));
    assert_eq!(
        (f[0].1, f[0].2),
        (FaultKind::Quota, 4),
        "effects-per-activation"
    );
    assert!(reported(&over)[0].contains("evals=faulted:quota:4"));
}
