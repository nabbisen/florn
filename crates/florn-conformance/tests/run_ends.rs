//! PR-40, findings F-7 and F-14 (RFC 012 rules 17 and 30; RFC 001 rule 26): a domain can end a run
//! (as an action, and as a flag the Host reads after a commit for what a delivery found), the end
//! reasons `non-convergence` and `actor-fault`, and the faulted-actor policy a domain declares in
//! the scenario, the more severe of the domains' for an actor of a world of several. The kit's
//! test domain does what its `[test]` table says.
use florn_admission::Admission;
use florn_conformance::host;
use florn_conformance::test_domain::second::{self, COMPOSED};
use florn_conformance::test_domain::steps::STEP;
use florn_conformance::test_domain::{self as td, END};
use florn_host::{EndReason, FaultPolicy, Options, Run, Seed};
use florn_kernel::Record;
use florn_model::canonical::Encoder;
use florn_model::manifest::{DomainMap, DomainValue};
use florn_platform::FixedPlatform;

fn text(table: &str) -> String {
    td::text(
        "ends",
        100,
        13,
        &format!(
            "{}{}{}\n[test]\n{table}\n",
            td::prober(1, 1),
            td::prober(2, 1),
            td::wire(100, &[(1, 0), (2, 0)], "")
        ),
    )
}

fn map(pairs: &[(&str, DomainValue)]) -> Vec<u8> {
    let mut m = DomainMap::new();
    for (k, v) in pairs {
        m.insert((*k).into(), v.clone());
    }
    let mut e = Encoder::default();
    florn_model::canonical::domain_map(&mut e, &m);
    e.finish()
}

fn text_value(s: &str) -> DomainValue {
    DomainValue::Text(s.into())
}

/// The last tick any record carries.
fn last_tick(run: &host::Host) -> u64 {
    run.records()
        .unwrap()
        .iter()
        .filter_map(|r| match r {
            Record::Handled { key, .. } | Record::Fault { key, .. } => Some(key.tick),
            _ => None,
        })
        .max()
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------------------------
// F-7: ending a run.
// ---------------------------------------------------------------------------------------------

#[test]
fn an_end_request_from_a_delivery_ends_the_run_with_its_reason() {
    // The frames of the two probers cross the wire every few ticks; the first delivery at or
    // after tick 20 asks the Host to end, through the flag it reads after the commit.
    let full = td::run(&text("note = \"full\""));
    assert_eq!(host::ended(&full), EndReason::EndReached);
    let run = td::run(&text(
        "end-from-tick = 20\nend-reason = \"non-convergence\"",
    ));
    assert_eq!(host::ended(&run), EndReason::NonConvergence);
    assert!(
        (20..40).contains(&last_tick(&run)),
        "ended soon after tick 20, at {}",
        last_tick(&run)
    );
    // The run is reproducible (the domain's reason is deterministic), and what it recorded is
    // the start of the run that was not asked to end.
    assert!(run.reproducible());
    let (a, b) = (run.records().unwrap(), full.records().unwrap());
    assert_eq!(
        a[..],
        b[..a.len()],
        "a prefix of the run without the request"
    );
    assert!(a.len() < b.len());
    // The log says so, with a value of the closed set.
    let said = run
        .log()
        .entries()
        .iter()
        .find(|e| e.notification.code == "run.ended")
        .expect("run.ended");
    assert!(
        said.notification
            .params
            .iter()
            .any(|(k, v)| k == "reason"
                && *v == florn_control::Value::Enum("non-convergence".into()))
    );
    assert!(said.notification.check().is_ok());
}

#[test]
fn the_other_reason_too() {
    let run = td::run(&text("end-from-tick = 20\nend-reason = \"actor-fault\""));
    assert_eq!(host::ended(&run), EndReason::ActorFault);
}

#[test]
fn an_end_request_as_an_action_of_an_intervention_ends_the_run() {
    let mut run = td::start(&text("note = \"x\""));
    run.intervene_map(
        Some(30),
        100,
        &map(&[
            ("kind", text_value(END)),
            ("reason", text_value("actor-fault")),
        ]),
    )
    .unwrap_or_else(|n| panic!("{}", n.code));
    run.run_to_end();
    assert_eq!(host::ended(&run), EndReason::ActorFault);
    assert!(last_tick(&run) <= 30, "{}", last_tick(&run));
}

#[test]
fn an_end_request_from_the_start_hook_ends_the_run_before_the_first_event() {
    let run = td::run(&text("end-at-start = \"non-convergence\""));
    assert_eq!(host::ended(&run), EndReason::NonConvergence);
    assert_eq!(last_tick(&run), 0, "only the inits were handled");
}

#[test]
fn an_end_request_of_a_reason_the_domain_may_not_give_is_refused() {
    for table in [
        "end-at-start = \"user\"",
        "end-reason = \"host-fault\"",
        "end-from-tick = -1",
    ] {
        assert!(
            td::try_start(&text(table), host::options()).is_err(),
            "{table}"
        );
    }
}

#[test]
fn a_run_that_asks_for_nothing_is_the_run_it_was() {
    let a = td::run(&text("note = \"a\""));
    assert_eq!(host::ended(&a), EndReason::EndReached);
}

// ---------------------------------------------------------------------------------------------
// F-14: the faulted-actor policy a domain declares.
// ---------------------------------------------------------------------------------------------

fn trap_at(run_text: &str, tick: u64, options: Options) -> host::Host {
    let mut run = td::try_start(run_text, options).unwrap_or_else(|e| panic!("{e}"));
    run.intervene_map(
        Some(tick),
        100,
        &map(&[
            ("kind", text_value(STEP)),
            ("actor", DomainValue::Integer(1)),
            ("script", text_value("open:0 eval:-1")),
        ]),
    )
    .unwrap_or_else(|n| panic!("{}", n.code));
    run.run_to_end();
    run
}

#[test]
fn a_policy_of_stop_run_ends_the_run_at_a_guest_fault_with_actor_fault() {
    let run = trap_at(&text("faulted-policy = \"stop-run\""), 40, host::options());
    assert_eq!(host::faults(&run).len(), 1);
    assert_eq!(host::ended(&run), EndReason::ActorFault);
    assert!(last_tick(&run) <= 41, "{}", last_tick(&run));
    // The notification of the fault carries the policy that was applied.
    let faulted = run
        .log()
        .entries()
        .iter()
        .find(|e| e.notification.code == "actor.faulted")
        .expect("actor.faulted");
    assert!(
        faulted
            .notification
            .params
            .iter()
            .any(|(k, v)| k == "policy" && *v == florn_control::Value::Enum("stop-run".into()))
    );
    assert!(faulted.notification.check().is_ok());
}

#[test]
fn the_default_policy_drops_the_events_and_the_run_goes_on() {
    for table in ["note = \"x\"", "faulted-policy = \"drop\""] {
        let run = trap_at(&text(table), 40, host::options());
        assert_eq!(host::faults(&run).len(), 1);
        assert_eq!(host::ended(&run), EndReason::EndReached, "{table}");
    }
}

#[test]
fn the_hosts_own_option_is_a_policy_too_and_the_more_severe_wins() {
    let stop = Options {
        fault_policy: FaultPolicy::StopRun,
        ..host::options()
    };
    // The Host's option alone.
    let run = trap_at(&text("note = \"x\""), 40, stop.clone());
    assert_eq!(host::ended(&run), EndReason::ActorFault);
    // The scenario says drop, the Host says stop: stop.
    let run = trap_at(&text("faulted-policy = \"drop\""), 40, stop);
    assert_eq!(host::ended(&run), EndReason::ActorFault);
}

#[test]
fn a_policy_that_is_not_one_is_refused() {
    for table in ["faulted-policy = \"halt\"", "faulted-policy = 3"] {
        assert!(
            td::try_start(&text(table), host::options()).is_err(),
            "{table}"
        );
    }
}

#[test]
fn the_policy_of_an_actor_of_a_world_of_two_domains_is_the_more_severe_of_theirs() {
    let bytes = td::guest_bytes_in(&second::composed_wat(), "both", COMPOSED);
    let run = |test_policy: &str, second_policy: &str| {
        let hash = florn_host::run::sha256_hex(&bytes);
        let body = format!(
            "{}{}{}\n[test]\nfaulted-policy = \"{test_policy}\"\n\n[second]\nfaulted-policy = \"{second_policy}\"\n",
            td::prober(1, 1),
            td::prober(2, 1),
            td::wire(100, &[(1, 0), (2, 0)], "")
        );
        let text = format!(
            "florn = 1\nname = \"both\"\ndomains = [\"test\", \"second\"]\n\n[time]\ntick = \"1ns\"\nend = \"100ns\"\n\n[order]\ntie = \"by-identity\"\n\n[seed]\nroot = 13\n\n[limits.default]\nmemory = \"2MiB\"\n\n[modules.prober]\nsha256 = \"{hash}\"\ncontract = \"{COMPOSED}\"\n{body}"
        );
        let manifest = td::manifest(&text);
        let seed = manifest.seed;
        let domains = second::domains();
        let mut admission = Admission::default();
        for w in domains.worlds() {
            admission.add_world(w);
        }
        let modules = [("prober".to_string(), bytes.clone())]
            .into_iter()
            .collect();
        let mut r = Run::new(
            host::engine(),
            admission,
            &mut FixedPlatform::default(),
            &domains,
            manifest,
            &modules,
            Seed::Given(seed),
            Options {
                keep_records: true,
                ..Options::default()
            },
        )
        .unwrap_or_else(|e| panic!("start: {e}"));
        r.intervene_map(
            Some(40),
            100,
            &map(&[
                ("kind", text_value(STEP)),
                ("actor", DomainValue::Integer(1)),
                ("script", text_value("open:0 eval:-1")),
            ]),
        )
        .unwrap_or_else(|n| panic!("{}", n.code));
        r.run_to_end();
        host::ended(&r)
    };
    assert_eq!(run("drop", "drop"), EndReason::EndReached);
    assert_eq!(run("drop", "stop-run"), EndReason::ActorFault);
    assert_eq!(run("stop-run", "drop"), EndReason::ActorFault);
    assert_eq!(run("stop-run", "stop-run"), EndReason::ActorFault);
}
