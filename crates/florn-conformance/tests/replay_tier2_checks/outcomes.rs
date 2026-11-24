//! The outcomes of the table (RFC 013 section 11, Comparison): matched, needs-payloads, the exempt
//! work, an unexplained chain, the tier, the windowed replay, a Fault's work, `Incomparable`.

use super::values::last_bit;
use super::*;

// ---- B1, B2 ---------------------------------------------------------------------------------

#[test]
fn b1_a_run_against_itself_is_matched_on_one_build_and_across_hosts() {
    let d = base();
    expect!(compare(&d), "matched", ["replay.matched"]);
    let mut a = d.clone();
    a.across();
    expect!(compare(&a), "matched", ["replay.matched"]);
}

#[test]
fn b2_a_payload_less_record_with_equal_chains_is_matched() {
    let d = base();
    for across in [false, true] {
        let mut a = d.clone();
        if across {
            a.across();
        }
        let o = go(&a, None, BTreeMap::new(), host::options());
        expect!(o, "matched", ["replay.matched"]);
    }
}

// ---- B3 -------------------------------------------------------------------------------------

#[test]
fn b3_a_payload_less_record_with_a_doctored_chain_needs_payloads_across_hosts_and_diverges_on_one_build()
 {
    let mut d = base();
    d.flip_chain();
    let o = go(&d, None, BTreeMap::new(), host::options());
    assert_eq!(o.kind(), "diverged", "one build: {:?}", o.codes());
    assert!(
        o.codes().contains(&"replay.diverged".to_string()),
        "{:?}",
        o.codes()
    );
    d.across();
    let o = go(&d, None, BTreeMap::new(), host::options());
    expect!(o, "needs-payloads", ["replay.needs-payloads"]);
    assert_eq!(o.text("replay.needs-payloads", "side"), "recorded");
}

#[test]
fn b3b_a_missing_payload_byte_string_needs_payloads_across_hosts() {
    let mut d = base();
    d.flip_chain();
    let victim = level_records(&d.trace)[1];
    let (v, l) = d.level(victim);
    let digest = *blake3::hash(&payload(v, l)).as_bytes();
    let mut payloads = d.payloads.clone();
    payloads.remove(&digest).expect("held");
    d.across();
    let o = go(&d, Some(d.trace.clone()), payloads.clone(), host::options());
    expect!(o, "needs-payloads", ["replay.needs-payloads"]);
    let mut one = d.clone();
    one.toml.environment = base().toml.environment;
    let o = go(&one, Some(one.trace.clone()), payloads, host::options());
    assert_eq!(o.kind(), "diverged");
}

#[test]
fn b3c_a_trace_without_payloads_needs_payloads_across_hosts() {
    let mut d = base();
    d.flip_chain();
    d.across();
    let o = go(&d, Some(d.trace.clone()), BTreeMap::new(), host::options());
    expect!(o, "needs-payloads", ["replay.needs-payloads"]);
    assert_eq!(o.text("replay.needs-payloads", "side"), "recorded");
}

#[test]
fn b3d_a_replay_always_keeps_its_own_trace_for_a_continuous_domain_whatever_the_options_say() {
    // docs/automation.md: `florn replay` keeps its own trace and payloads for such a scenario whatever `--trace` and `--payloads` say.
    let mut d = base();
    let l = level_records(&d.trace)[1];
    d.set_level(l, last_bit(2.0));
    d.flip_chain();
    d.across();
    let bare = Options {
        keep_records: false,
        keep_payloads: false,
        ..host::options()
    };
    let o = go(&d, Some(d.trace.clone()), d.payloads.clone(), bare);
    expect!(o, "within-tolerance", ["replay.within-tolerance"]);
}

// ---- B5: work and draws -----------------------------------------------------------------------

fn target_of(r: &Record) -> u32 {
    match r {
        Record::Handled { target, .. } => *target,
        Record::Fault { actor, .. } => *actor,
        Record::Intervention { .. } => 0,
    }
}

#[test]
fn b5_work_and_draws_of_a_continuous_actor_are_exempt_across_hosts_only() {
    for what in ["work", "draws"] {
        let mut d = base();
        let i = d
            .trace
            .iter()
            .position(|r| matches!(r, Record::Handled { .. }) && target_of(r) == 1)
            .unwrap();
        if what == "work" {
            d.metered(i, 3)
        } else {
            d.draws(i, 1)
        }
        d.flip_chain();
        let one = compare(&d);
        assert_eq!(one.kind(), "diverged", "{what}: {:?}", one.codes());
        d.across();
        let o = compare(&d);
        assert_eq!(o.kind(), "within-tolerance", "{what}: {:?}", o.codes());
        assert_eq!(o.codes(), ["replay.exempt-only"], "{what}");
    }
}

#[test]
fn b5b_work_of_an_actor_that_is_not_continuous_is_a_difference() {
    let mut d = base();
    let i = d
        .trace
        .iter()
        .position(|r| matches!(r, Record::Handled { .. }) && target_of(r) == 2)
        .unwrap();
    d.metered(i, 3);
    d.flip_chain();
    d.across();
    let o = compare(&d);
    assert_eq!(o.kind(), "diverged", "{:?}", o.codes());
    assert_eq!(o.codes(), ["replay.record-differs"]);
    assert_eq!(o.text("replay.record-differs", "what"), "structure");
    // and draws
    let mut d = base();
    d.draws(i, 1);
    d.flip_chain();
    d.across();
    let o = compare(&d);
    assert_eq!(o.text("replay.record-differs", "what"), "structure");
}

#[test]
fn b17_a_numeric_difference_and_a_work_difference_report_the_tolerance_not_the_exemption() {
    let mut d = base();
    let l = level_records(&d.trace)[0];
    d.set_level(l, last_bit(1.0));
    let i = d
        .trace
        .iter()
        .position(|r| matches!(r, Record::Handled { .. }) && target_of(r) == 1)
        .unwrap();
    d.metered(i, 5);
    d.flip_chain();
    d.across();
    let o = compare(&d);
    expect!(o, "within-tolerance", ["replay.within-tolerance"]);
}

// ---- B15: chains differ and no record does ---------------------------------------------------------

#[test]
fn b15_chains_that_differ_while_no_record_does_are_diverged() {
    let mut d = base();
    d.flip_chain();
    for across in [false, true] {
        let mut a = d.clone();
        if across {
            a.across();
        }
        let o = compare(&a);
        expect!(o, "diverged", ["replay.diverged"]);
    }
}

// ---- B18: which tier -------------------------------------------------------------------------------

#[test]
fn b18_the_tier_is_decided_by_the_environment_of_the_two_runs() {
    let mk = |f: &dyn Fn(&mut Doc)| {
        let mut d = base();
        let l = level_records(&d.trace)[1];
        d.set_level(l, last_bit(2.0));
        d.flip_chain();
        f(&mut d);
        compare(&d).kind()
    };
    assert_eq!(mk(&|_| {}), "diverged", "same build");
    assert_eq!(
        mk(&|d| d.toml.environment.host_build = "browser".into()),
        "within-tolerance",
        "host"
    );
    assert_eq!(
        mk(&|d| d.toml.environment.engine = "other-engine".into()),
        "within-tolerance",
        "engine"
    );
    assert_eq!(
        mk(&|d| d.toml.environment.arch = "aarch64".into()),
        "within-tolerance",
        "arch"
    );
    assert_eq!(
        mk(&|d| d.toml.environment.florn_version = "0.0.1".into()),
        "diverged",
        "other version"
    );
    assert_eq!(
        mk(&|d| {
            d.toml.environment.florn_version = "0.0.1".into();
            d.toml.environment.host_build = "browser".into()
        }),
        "diverged",
        "other version and host"
    );
    // an engine version alone: RFC 001 rule 41 names it part of the environment; RFC 013 rule 20 names Host, engine or architecture.
    println!(
        "engine-version only: {}",
        mk(&|d| d.toml.environment.engine_version = "0.0.0".into())
    );
}

#[test]
fn b3e_a_replay_whose_records_are_windowed_lacks_its_trace_and_the_side_is_named() {
    // DATA-008: the browser keeps records since the last checkpoint only; the replay's own trace is then incomplete.
    let mut d = base();
    d.flip_chain();
    d.across();
    let windowed = Options {
        records_since_checkpoint: true,
        checkpoint_interval: 50,
        ..host::options()
    };
    let o = go(
        &d,
        Some(d.trace.clone()),
        d.payloads.clone(),
        windowed.clone(),
    );
    println!(
        "windowed replay, record complete: {:?} {:?}",
        o.run.replayed(),
        o.codes()
    );
    assert_eq!(o.kind(), "needs-payloads");
    assert_eq!(o.text("replay.needs-payloads", "side"), "replayed");
    let o = go(&d, None, BTreeMap::new(), windowed);
    assert_eq!(o.text("replay.needs-payloads", "side"), "both");
}

// ---- B5c: a Fault record's metered work (RFC 013 rule 20: "a Fault's kind, parameter or metered work ... is `structure`") --------

fn step_bytes(actor: u32, script: &str) -> Vec<u8> {
    let mut m = DomainMap::new();
    m.insert("kind".into(), DomainValue::Text("step".into()));
    m.insert("actor".into(), DomainValue::Integer(i64::from(actor)));
    m.insert("script".into(), DomainValue::Text(script.into()));
    let mut e = Encoder::default();
    florn_model::canonical::domain_map(&mut e, &m);
    e.finish()
}

/// A genuine run in which actor `actor` faults once (the domain judges the contract broken).
fn faulting(actor: u32) -> Doc {
    let text = scenario("1e-9", "1e-6");
    let mut run = td::start(&text);
    run.intervene_map(Some(150), 100, &step(actor, "open:2 fault"))
        .unwrap_or_else(|n| panic!("{}", n.code));
    run.run_to_end();
    Doc {
        text,
        toml: run.run_toml(),
        trace: run.records().unwrap().to_vec(),
        payloads: run.payloads().clone(),
    }
}
fn step(actor: u32, script: &str) -> Vec<u8> {
    step_bytes(actor, script)
}

#[test]
fn b5c_a_faults_kind_and_parameter_are_structure_and_its_work_is_exempt_for_a_continuous_actor() {
    // Ruling V-1 (RFC 013 rule 20, corrected 2026-11-25): a Fault's metered work is the work of an
    // activation and is exempt for an actor of a continuous class, as a Handled record's is; the
    // kind and the parameter are `structure` for every actor, and the work of a discrete actor too.
    for (actor, class) in [(1u32, "continuous"), (2u32, "discrete")] {
        for field in ["metered", "parameter", "kind"] {
            let mut d = faulting(actor);
            let i = d
                .trace
                .iter()
                .position(|r| matches!(r, Record::Fault { .. }))
                .expect("a Fault record");
            if let Record::Fault {
                metered,
                parameter,
                kind,
                ..
            } = &mut d.trace[i]
            {
                match field {
                    "metered" => *metered += 1,
                    "parameter" => *parameter += 1,
                    _ => *kind = florn_kernel::FaultKind::Trap,
                }
            }
            d.flip_chain();
            d.across();
            let o = compare(&d);
            println!(
                "fault of a {class} actor, {field} changed: {} {:?} what={}",
                o.kind(),
                o.codes(),
                o.text("replay.record-differs", "what")
            );
            if class == "continuous" && field == "metered" {
                // Ruling: exempt, and the only difference: `replay.exempt-only`, the word `within-tolerance`.
                assert_eq!(
                    o.kind(),
                    "within-tolerance",
                    "{class} {field}: {:?}",
                    o.codes()
                );
                assert_eq!(o.codes(), ["replay.exempt-only"]);
            } else {
                assert_eq!(o.kind(), "diverged", "{class} {field}: {:?}", o.codes());
                assert_eq!(
                    o.text("replay.record-differs", "what"),
                    "structure",
                    "{class} {field}"
                );
            }
        }
    }
}

// ---- B23: the kernel's Incomparable stays `replay.diverged` (RFC 013 rule 20) --------------------------------------------

#[test]
fn b23_stored_points_at_other_positions_are_diverged_even_when_the_traces_agree_within_tolerance() {
    let mut d = base();
    let l = level_records(&d.trace)[1];
    d.set_level(l, last_bit(2.0)); // would be within tolerance across Hosts
    d.toml.chain[0].0 += 1; // the first stored point is at another position
    d.across();
    let o = compare(&d);
    println!(
        "incomparable + within-tolerance rows, across Hosts: {} {:?}",
        o.kind(),
        o.codes()
    );
    expect!(o, "diverged", ["replay.diverged"]);
}
