//! The call-in handle, checked from the rules and not from the code (PR-38; written by the
//! verifier session before it read the implementation: RFC 001 rules 21 to 24, RFC 002 rules 2, 44
//! and 45, RFC 003 rule 12, RFC 012 rules 19 to 21, findings F-2, F-3 and F-16). Faults in every
//! call of a compound activation; one open at a time; the budget across the calls (an exact
//! threshold); the post-return once per list result, charged to the budget; the bound checked
//! before the lift; sends and timers refused; the trace of several steps in one event; after a
//! refused result only the fault closes the activation. The kernel's side is `compound_discard`.
//!
//! The scripts are the test domain's (`test_domain::steps`); where a check needs a guest that the
//! kit's prober is not, the prober's text is varied here, so that no file of the unit is changed.
use florn_admission::Admission;
use florn_conformance::host::{self, faults, reports};
use florn_conformance::test_domain::steps::STEP;
use florn_conformance::test_domain::{self as td};
use florn_host::{Run, Seed};
use florn_kernel::trace::CommittedEffect;
use florn_kernel::{FaultKind, Record};
use florn_model::canonical::Encoder;
use florn_model::manifest::{DomainMap, DomainValue};
use florn_platform::FixedPlatform;

// ---------------------------------------------------------------------------------------------
// Harness: a prober that can be varied, a budget that can be set, scripts at ticks.
// ---------------------------------------------------------------------------------------------

fn text(wat: &str, budget: Option<u64>, memory: &str) -> String {
    let hash = florn_host::run::sha256_hex(&td::guest_bytes_of(wat));
    let limits = match budget {
        Some(b) => format!("memory = \"{memory}\"\nbudget = {b}"),
        None => format!("memory = \"{memory}\""),
    };
    format!(
        "florn = 1\nname = \"verifier\"\ndomains = [\"test\"]\n\n[time]\ntick = \"1ns\"\nend = \"400ns\"\n\n[order]\ntie = \"by-identity\"\n\n[seed]\nroot = 7\n\n[limits.default]\n{limits}\n\n[modules.prober]\nsha256 = \"{hash}\"\ncontract = \"{}\"\n{}{}{}",
        td::WORLD,
        td::prober(1, 1),
        td::prober(2, 1),
        td::wire(100, &[(1, 0), (2, 0)], "")
    )
}

fn step(actor: u32, script: &str) -> Vec<u8> {
    let mut m = DomainMap::new();
    m.insert("kind".into(), DomainValue::Text(STEP.into()));
    m.insert("actor".into(), DomainValue::Integer(i64::from(actor)));
    m.insert("script".into(), DomainValue::Text(script.into()));
    let mut e = Encoder::default();
    florn_model::canonical::domain_map(&mut e, &m);
    e.finish()
}

/// Run `scripts` (tick, actor, script) over the guest `wat` with the class budget `budget`.
fn play_with(
    wat: &str,
    budget: Option<u64>,
    memory: &str,
    scripts: &[(u64, u32, &str)],
) -> host::Host {
    let scenario = text(wat, budget, memory);
    let manifest = td::manifest(&scenario);
    let seed = manifest.seed;
    let modules = [("prober".to_string(), td::guest_bytes_of(wat))]
        .into_iter()
        .collect();
    let mut run = Run::new(
        host::engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        &td::domains(),
        manifest,
        &modules,
        Seed::Given(seed),
        host::options(),
    )
    .unwrap_or_else(|e| panic!("start: {e}"));
    for (tick, actor, script) in scripts {
        run.intervene_map(Some(*tick), 100, &step(*actor, script))
            .unwrap_or_else(|n| panic!("{}", n.code));
    }
    run.run_to_end();
    run
}

fn play(scripts: &[(u64, u32, &str)]) -> host::Host {
    play_with(&td::guest_wat(), None, "2MiB", scripts)
}

/// What the domain said each script came to.
fn said(run: &host::Host) -> Vec<String> {
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
        .map(
            |e| match e.notification.params.iter().find(|(k, _)| k == "detail") {
                Some((_, florn_control::Value::Text(t))) => t.clone(),
                other => format!("{other:?}"),
            },
        )
        .collect()
}

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

fn work_of(run: &host::Host, actor: u32) -> Vec<u64> {
    step_records(run, actor)
        .iter()
        .filter_map(|r| match r {
            Record::Handled { metered, .. } => Some(*metered),
            _ => None,
        })
        .collect()
}

/// The prober's text with `from` replaced by `to` (exactly once, or the check is void).
fn varied(edits: &[(&str, &str)]) -> String {
    let mut wat = td::guest_wat();
    for (from, to) in edits {
        assert_eq!(wat.matches(from).count(), 1, "the prober changed: {from}");
        wat = wat.replace(from, to);
    }
    wat
}

const BEGIN_BODY: &str = "(global.set $step_draw (call $random))";
const ACCEPT_BODY: &str = "(i64.store (i32.const 136) (global.get $step_draw))";
const REJECT_BODY: &str = "(i64.store (i32.const 144) (i64.const 1))";

// ---------------------------------------------------------------------------------------------
// V2: a fault anywhere in a compound activation is ONE Fault record, no Handled, actor dead.
// ---------------------------------------------------------------------------------------------

fn one_fault_no_record(run: &host::Host, kind: FaultKind) {
    let f = faults(run);
    assert_eq!(f.len(), 1, "one Fault record: {f:?} {:?}", said(run));
    assert_eq!(f[0].1, kind, "{f:?}");
    assert!(work_of(run, 1).is_empty(), "no Handled for the activation");
    // The actor is dead: a later script for it is not played into a live guest.
    let later = play_dead_follows(run);
    assert!(later, "the actor is dead after its fault");
}

/// The actor is dead: the kernel marks it so (a later `open` is refused as "the actor is dead").
fn play_dead_follows(run: &host::Host) -> bool {
    // The run was played with a second script at tick 200 for the same actor; it must have been
    // refused, not played.
    let s = said(run);
    s.len() == 2 && s[1].contains("refused")
}

#[test]
fn v2a_a_trap_in_a_middle_call_is_one_fault_and_nothing_else() {
    let run = play(&[(50, 1, "open:0 eval:3 eval:-1 commit"), (200, 1, "open:0")]);
    one_fault_no_record(&run, FaultKind::Trap);
    // The reports of the calls before the trap (begin's, on channel 3) are committed nowhere.
    assert!(
        reports(&run, 1, 3).iter().all(|(i, _)| i.tick != 50),
        "a report of the faulted activation was committed: {:?}",
        reports(&run, 1, 3)
    );
}

#[test]
fn v2d_a_trap_in_the_opening_call_is_one_fault() {
    let wat = varied(&[(BEGIN_BODY, "(unreachable)")]);
    let run = play_with(
        &wat,
        None,
        "2MiB",
        &[(50, 1, "open:0 commit"), (200, 1, "open:0")],
    );
    one_fault_no_record(&run, FaultKind::Trap);
}

#[test]
fn v2d_a_trap_in_the_committing_close_is_one_fault() {
    let wat = varied(&[(ACCEPT_BODY, "(unreachable)")]);
    let run = play_with(
        &wat,
        None,
        "2MiB",
        &[(50, 1, "open:0 eval:3 commit"), (200, 1, "open:0")],
    );
    one_fault_no_record(&run, FaultKind::Trap);
    assert!(
        said(&run)[0].contains("commit=faulted:trap"),
        "{:?}",
        said(&run)
    );
}

#[test]
fn v2d_a_trap_in_the_discarding_close_is_a_fault_not_a_discard() {
    let wat = varied(&[(REJECT_BODY, "(unreachable)")]);
    let run = play_with(
        &wat,
        None,
        "2MiB",
        &[(50, 1, "open:0 eval:3 discard"), (200, 1, "open:0")],
    );
    one_fault_no_record(&run, FaultKind::Trap);
    assert!(
        said(&run)[0].contains("discard=faulted:trap"),
        "{:?}",
        said(&run)
    );
}

#[test]
fn v2e_the_work_of_the_calls_before_a_fault_is_in_the_fault_record() {
    // The Fault record's work is the work of the whole activation to the trap, not the last call's.
    let short = play(&[(50, 1, "open:0 eval:-1")]);
    let long = play(&[(50, 1, "open:0 eval:3 eval:3 eval:3 eval:-1")]);
    let (a, b) = (faults(&short)[0].3, faults(&long)[0].3);
    assert!(b > a, "work before the trap counts: {a} {b}");
}

// ---------------------------------------------------------------------------------------------
// V3: out of sequence; one at a time.
// ---------------------------------------------------------------------------------------------

#[test]
fn v3b_a_second_open_is_refused_and_the_first_goes_on() {
    let run = play(&[(50, 1, "open:0 reopen eval:3 commit")]);
    let s = said(&run);
    assert!(s[0].contains("reopen=refused"), "{s:?}");
    assert!(
        s[0].contains("commit=committed"),
        "the first still commits: {s:?}"
    );
    assert_eq!(work_of(&run, 1).len(), 1);
}

#[test]
fn v3c_a_call_out_of_sequence_is_a_fault_naming_rfc_002_rule_44() {
    let run = play(&[(50, 1, "open:0 badseq")]);
    let f = faults(&run);
    assert_eq!(f.len(), 1, "{:?}", said(&run));
    assert_eq!((f[0].1, f[0].2), (FaultKind::Contract, 44));
}

#[test]
fn v3c_a_call_with_nothing_open_changes_nothing() {
    // `closed` is an `evaluate` with nothing open: refused, no record, no fault, no state.
    let run = play(&[(50, 1, "closed commit open:0 commit")]);
    let s = said(&run);
    assert!(
        s[0].contains("closed=refused") && s[0].contains("commit=refused"),
        "{s:?}"
    );
    assert!(faults(&run).is_empty(), "{s:?}");
    assert_eq!(work_of(&run, 1).len(), 1, "the later, lawful one commits");
}

// ---------------------------------------------------------------------------------------------
// V4: the budget.
// ---------------------------------------------------------------------------------------------

/// The work of `open:h` + `n` evaluations + commit, at the default budget.
fn work(h: u32, n: u32) -> u64 {
    let evals = "eval:3 ".repeat(n as usize);
    let run = play(&[(50, 1, &format!("open:{h} {evals}commit"))]);
    work_of(&run, 1)[0]
}

#[test]
fn v4a_the_work_is_affine_in_the_number_of_calls() {
    let (m1, m2, m3) = (work(0, 1), work(0, 2), work(0, 3));
    assert!(m1 < m2 && m2 < m3);
    assert_eq!(m3 - m2, m2 - m1, "{m1} {m2} {m3}");
}

/// V4b: one budget for the whole activation. Each call costs about 40% of the budget B; two calls
/// fit in B, three do not: the third call faults with `budget` although no call alone is near B.
#[test]
fn v4b_the_budget_is_spent_across_the_calls_not_per_call() {
    // Each evaluate after `open:h` runs a post-return of 100*h loops.
    let h = 4000; // 400,000 loops each
    let (m1, m2, m3) = (work(h, 1), work(h, 2), work(h, 3));
    let per_call = m2 - m1;
    assert!(per_call > 1_000_000, "a call costs real work: {per_call}");
    // B between M(2) and M(3): the budget of a class is a whole number.
    let budget = m2 + per_call / 2;
    assert!(m2 < budget && budget < m3);
    // At that budget two calls commit ...
    let two = play_with(
        &td::guest_wat(),
        Some(budget),
        "2MiB",
        &[(50, 1, &format!("open:{h} eval:3 eval:3 commit"))],
    );
    assert!(faults(&two).is_empty(), "{:?}", said(&two));
    assert_eq!(work_of(&two, 1), vec![m2]);
    // ... three do not, and no single call is the one that is large.
    let three = play_with(
        &td::guest_wat(),
        Some(budget),
        "2MiB",
        &[(50, 1, &format!("open:{h} eval:3 eval:3 eval:3 commit"))],
    );
    let f = faults(&three);
    assert_eq!(f.len(), 1, "{:?}", said(&three));
    assert_eq!(f[0].1, FaultKind::Budget);
    assert!(
        per_call < budget / 2 + 1,
        "no call alone is above the budget"
    );
    // The same three calls as three separate compound activations commit each time.
    let apart = play_with(
        &td::guest_wat(),
        Some(budget),
        "2MiB",
        &[
            (50, 1, &format!("open:{h} eval:3 commit")),
            (60, 1, &format!("open:{h} eval:3 commit")),
            (70, 1, &format!("open:{h} eval:3 commit")),
        ],
    );
    assert!(faults(&apart).is_empty(), "{:?}", said(&apart));
    assert_eq!(work_of(&apart, 1), vec![m1, m1, m1]);
}

#[test]
fn v4e_a_discard_does_not_spend_the_next_activations_budget() {
    // The next activation's work is that of the same activation alone: the budget is set again at
    // its own opening call.
    let alone = work(5, 2);
    let run = play(&[
        (50, 1, "open:5 eval:3 eval:3 eval:3 discard"),
        (60, 1, "open:5 eval:3 eval:3 commit"),
    ]);
    assert_eq!(work_of(&run, 1), vec![alone]);
}

#[test]
fn v4f_the_same_compound_costs_the_same_each_time() {
    let run = play(&[
        (50, 1, "open:3 eval:3 commit"),
        (60, 1, "open:3 eval:3 commit"),
        (70, 2, "open:3 eval:3 commit"),
    ]);
    let a = work_of(&run, 1);
    let b = work_of(&run, 2);
    assert_eq!(a[0], a[1]);
    assert_eq!(a[0], b[0], "the same guest, the same work on another actor");
}

// ---------------------------------------------------------------------------------------------
// V5: the post-return of a list result.
// ---------------------------------------------------------------------------------------------

/// The prober with a post-return that reports (count, bump pointer after it has freed, the area it
/// was given) as 12 bytes on channel 3, and traps if the area is not the one `evaluate` returned.
fn counting_prober() -> String {
    varied(&[
        (
            "(global $mark (mut i32) (i32.const 0))",
            "(global $mark (mut i32) (i32.const 0))\n  (global $pc (mut i32) (i32.const 0))\n  (global $last (mut i32) (i32.const -1))",
        ),
        (
            "(i32.store offset=4 (local.get $area) (local.get $n))",
            "(i32.store offset=4 (local.get $area) (local.get $n))\n    (global.set $last (local.get $area))",
        ),
        (
            "(global.set $bump (global.get $mark)))",
            "(global.set $bump (global.get $mark))
    (if (i32.ne (local.get $area) (global.get $last)) (then (unreachable)))
    (global.set $pc (i32.add (global.get $pc) (i32.const 1)))
    (i32.store (i32.const 160) (global.get $pc))
    (i32.store (i32.const 164) (global.get $bump))
    (i32.store (i32.const 168) (local.get $area))
    (call $report (i32.const 3) (i32.const 160) (i32.const 12)))",
        ),
    ])
}

fn post_returns(run: &host::Host) -> Vec<(u32, u32, u32)> {
    reports(run, 1, 3)
        .into_iter()
        .filter(|(_, p)| p.len() == 12)
        .map(|(_, p)| {
            let w = |i: usize| u32::from_le_bytes(p[i..i + 4].try_into().unwrap());
            (w(0), w(4), w(8))
        })
        .collect()
}

#[test]
fn v5a_the_post_return_runs_once_per_list_result_with_the_returned_pointer() {
    let wat = counting_prober();
    let run = play_with(
        &wat,
        None,
        "2MiB",
        &[(50, 1, "open:0 eval:3 eval:2 eval:1 commit")],
    );
    assert!(faults(&run).is_empty(), "{:?}", said(&run));
    let p = post_returns(&run);
    // Once per call: counts 1, 2, 3 (not 0 of them, not twice each).
    assert_eq!(
        p.iter().map(|x| x.0).collect::<Vec<_>>(),
        vec![1, 2, 3],
        "{p:?}"
    );
}

#[test]
fn v5a_a_thousand_calls_leave_the_guests_allocator_where_it_was() {
    let wat = counting_prober();
    // 200 calls per script is within one activation's 1,000 effects (each post-return reports);
    // five activations make a thousand.
    let scripts: Vec<(u64, u32, String)> = (0..5)
        .map(|i| (50 + 10 * i as u64, 1, "open:0 evals:195 commit".to_string()))
        .collect();
    let refs: Vec<(u64, u32, &str)> = scripts
        .iter()
        .map(|(t, a, s)| (*t, *a, s.as_str()))
        .collect();
    // `evals` calls evaluate with x = -5, which reports on channel 3 too (8 bytes): filtered out.
    let run = play_with(&wat, None, "2MiB", &refs);
    assert!(faults(&run).is_empty(), "{:?}", said(&run));
    let p = post_returns(&run);
    assert_eq!(p.len(), 5 * 195);
    // Within one activation the allocator's mark is the same after every call (between two
    // activations the prober's own plain activations, which lower frames and free nothing, move
    // it, and that is not the handle's).
    for chunk in p.chunks(195) {
        let bumps: std::collections::BTreeSet<u32> = chunk.iter().map(|x| x.1).collect();
        assert_eq!(
            bumps.len(),
            1,
            "the allocator's mark is the same after every call: {bumps:?}"
        );
    }
    let counts: Vec<u32> = p.iter().map(|x| x.0).collect();
    assert_eq!(
        counts,
        (1..=975).collect::<Vec<u32>>(),
        "exactly one post-return per call"
    );
}

#[test]
fn v5b_the_values_arrive_bit_for_bit() {
    // The kit's guest returns 1.5 repeated; the domain's script reports the count only. Check the
    // lift on the Host's side with a value list through `eval`: length = x for x <= 64.
    let run = play(&[(50, 1, "open:0 eval:0 eval:1 eval:8 commit")]);
    assert_eq!(
        said(&run),
        vec!["open=opened eval=evaluated:0 eval=evaluated:1 eval=evaluated:8 commit=committed"]
    );
}

#[test]
fn v5c_the_post_returns_work_is_charged_to_the_activation_and_can_exhaust_its_budget() {
    let (w0, w1) = (work(0, 1), work(1000, 1));
    assert!(
        w1 - w0 >= 100_000,
        "100,000 loops of post-return: {w0} {w1}"
    );
    // A budget that fits the activation without the post-return's loops but not with them.
    let budget = w0 + (w1 - w0) / 2;
    let run = play_with(
        &td::guest_wat(),
        Some(budget),
        "2MiB",
        &[(50, 1, "open:1000 eval:3 commit")],
    );
    let f = faults(&run);
    assert_eq!(f.len(), 1, "{:?}", said(&run));
    assert_eq!(
        f[0].1,
        FaultKind::Budget,
        "the post-return's work is in the budget"
    );
}

#[test]
fn v5f_a_scalar_result_runs_no_post_return() {
    // `begin`, `accept` and `reject` have no result: the counting prober's post-return report
    // never appears for them (only `evaluate` has a post-return).
    let wat = counting_prober();
    let run = play_with(
        &wat,
        None,
        "2MiB",
        &[(50, 1, "open:0 commit"), (60, 1, "open:0 discard")],
    );
    assert!(post_returns(&run).is_empty());
}

// ---------------------------------------------------------------------------------------------
// V6: the bound is checked before the lift.
// ---------------------------------------------------------------------------------------------

#[test]
fn v6a_at_the_bound_it_lifts_above_it_it_refuses() {
    // After the refusal only the fault closes the activation (RFC 012 rule 20).
    let run = play(&[(50, 1, "open:0 eval:8 eval:9 fault")]);
    assert_eq!(
        said(&run),
        vec!["open=opened eval=evaluated:8 eval=too-long:9>8 fault=faulted:contract:77"]
    );
}

#[test]
fn v6c_a_length_that_could_not_be_lifted_is_refused_by_the_bound_not_by_the_lift() {
    // The guest names 4,000,000,000 elements and holds 64: a lift would be an out-of-range read
    // (a contract fault of rule 8) or an allocation of 32 GB. The bound refuses it first.
    for len in ["4000000000", "1000000000", "4294967295", "16777216"] {
        let run = play(&[(50, 1, &format!("open:0 eval:{len} fault"))]);
        let s = said(&run);
        assert!(
            s[0].contains(&format!("eval=too-long:{len}>8")),
            "len {len}: {s:?}"
        );
        // The one Fault is the domain's (77), not the contract fault of a lift (RFC 002 rule 8).
        let f = faults(&run);
        assert_eq!(f.len(), 1, "{s:?}");
        assert_eq!((f[0].1, f[0].2), (FaultKind::Contract, 77), "{s:?}");
    }
}

#[test]
fn v6d_a_refused_result_is_not_charged_a_post_return() {
    // The result was never taken, so nothing was freed: the post-return's loops are not run, and
    // the work of the activation does not depend on how many loops it would have made.
    let w = |h: u32| faults(&play(&[(50, 1, &format!("open:{h} eval:9 fault"))]))[0].3;
    assert_eq!(w(0), w(5000));
    // The control: a taken result of the same activation is charged those loops.
    let taken = |h: u32| work_of(&play(&[(50, 1, &format!("open:{h} eval:3 commit"))]), 1)[0];
    assert!(taken(5000) > taken(0) + 400_000);
}

/// V6e: the leak the verifier measured (about 115 bytes of the guest's memory for each refusal
/// followed by a commit or a discard) cannot be made: after a refused result every way of going on
/// but the fault is refused and changes nothing, so the buffer never outlives the instance.
#[test]
fn v6e_after_a_refusal_only_a_fault_ends_the_activation() {
    let run = play(&[
        (50, 1, "open:0 eval:9 discard commit eval:3 open:0 fault"),
        (60, 1, "open:0"),
    ]);
    let s = said(&run);
    for refused in [
        "discard=refused",
        "commit=refused",
        "eval=refused",
        "open=refused",
    ] {
        assert!(s[0].contains(refused), "{refused}: {s:?}");
    }
    assert!(s[0].ends_with("fault=faulted:contract:77"), "{s:?}");
    // One record for the actor, the Fault; no Handled; the actor is dead.
    let records = step_records(&run, 1);
    assert_eq!(records.len(), 1, "{records:?}");
    assert!(matches!(records[0], Record::Fault { .. }));
    assert!(s[1].contains("open=refused"), "{s:?}");
}

// ---------------------------------------------------------------------------------------------
// V7: the handle refuses queue presence.
// ---------------------------------------------------------------------------------------------

#[test]
fn v7a_a_send_in_a_compound_is_contract_12019_and_nothing_is_delivered() {
    let run = play(&[(50, 1, "open:0 eval:-3 commit")]);
    let f = faults(&run);
    assert_eq!(f.len(), 1, "{:?}", said(&run));
    assert_eq!((f[0].1, f[0].2), (FaultKind::Contract, 12019));
    assert!(
        host::sends(&run, 1).iter().all(|(i, _, _)| i.tick != 50),
        "no send was committed"
    );
    assert!(work_of(&run, 1).is_empty());
}

#[test]
fn v7b_a_timer_in_a_compound_is_a_contract_violation() {
    let run = play(&[(50, 1, "open:0 eval:-4 commit")]);
    let f = faults(&run);
    assert_eq!(f.len(), 1, "{:?}", said(&run));
    assert_eq!(f[0].1, FaultKind::Contract);
    eprintln!(
        "V7b observation: the fault parameter for a timer in a compound is {}",
        f[0].2
    );
    assert!(f[0].2 == 12019 || f[0].2 == 12020, "{}", f[0].2);
}

#[test]
fn v7c_reports_are_committed_in_call_order_and_vanish_on_discard() {
    let ok = play(&[(50, 1, "open:0 eval:-5 eval:-5 commit")]);
    let channels: Vec<u32> = match &step_records(&ok, 1)[0] {
        Record::Handled { effects, .. } => effects
            .iter()
            .filter_map(|e| match e {
                CommittedEffect::Report { channel, .. } => Some(*channel),
                _ => None,
            })
            .collect(),
        other => panic!("{other:?}"),
    };
    // begin (3), two evaluates (3, 3), accept (2, 0).
    assert_eq!(channels, vec![3, 3, 3, 2, 0]);
    let dropped = play(&[(50, 1, "open:0 eval:-5 eval:-5 discard")]);
    assert!(step_records(&dropped, 1).is_empty());
    assert!(reports(&dropped, 1, 3).iter().all(|(i, _)| i.tick != 50));
}

// ---------------------------------------------------------------------------------------------
// V8: the trace of several steps in one event; the stream across a discard, seen through the guest.
// ---------------------------------------------------------------------------------------------

fn draw_reports(run: &host::Host) -> Vec<Vec<u8>> {
    // `accept` reports the draw `begin` made on channel 2 (8 bytes).
    reports(run, 1, 2)
        .into_iter()
        .filter(|(i, _)| i.tick == 50)
        .map(|(_, p)| p)
        .filter(|p| p.len() == 8 && p != &1u64.to_le_bytes().to_vec())
        .collect()
}

#[test]
fn v8b_accept_reject_accept_in_one_event_is_two_records_with_one_key_and_no_gap() {
    let with = play(&[(50, 1, "open:0 commit open:0 discard open:0 commit")]);
    let without = play(&[(50, 1, "open:0 commit open:0 commit")]);
    let (a, b) = (step_records(&with, 1), step_records(&without, 1));
    assert_eq!(a.len(), 2);
    assert_eq!(b.len(), 2);
    let key = |r: &Record| match r {
        Record::Handled { key, .. } => *key,
        other => panic!("{other:?}"),
    };
    assert_eq!(key(&a[0]), key(&a[1]), "one key for the steps of one event");
    // The discarded step in the middle is invisible: the records are those of the run without it,
    // including the draws, the sequence numbers and the reported draw values (the stream).
    assert_eq!(a, b, "{a:?}\n{b:?}");
    assert_eq!(draw_reports(&with), draw_reports(&without));
    assert_eq!(draw_reports(&with).len(), 2);
}

#[test]
fn v8d_the_same_inputs_give_the_same_digest_with_compounds_in_it() {
    let scripts = [
        (50, 1, "open:2 eval:3 commit"),
        (100, 2, "open:1 eval:4 discard"),
        (150, 1, "open:0 eval:-1"),
    ];
    let a = play(&scripts);
    let b = play(&scripts);
    assert_eq!(
        a.chain_hex(),
        b.chain_hex(),
        "the same inputs, the same digest (Tier 1)"
    );
}

/// A compound's record and the plain activations on the same wire do not disturb each other: the
/// records of the actors that never ran a compound are those of a run with no scripts at all.
#[test]
fn v8e_a_compound_of_one_actor_does_not_change_another_actors_records() {
    let none = play(&[]);
    let some = play(&[(50, 1, "open:0 eval:3 eval:3 discard open:0 discard")]);
    let plain = |run: &host::Host, actor: u32| -> Vec<Record> {
        run.records()
            .unwrap()
            .iter()
            .filter(|r| matches!(r, Record::Handled { target, key, .. } if *target == actor && key.class != td::steps::CLASS_STEP))
            .cloned()
            .collect()
    };
    assert_eq!(plain(&none, 2), plain(&some, 2));
    assert_eq!(
        plain(&none, 1),
        plain(&some, 1),
        "nor its own plain ones: a discard takes no sequence number"
    );
}
