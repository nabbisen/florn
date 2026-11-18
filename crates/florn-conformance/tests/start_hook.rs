//! PR-40, finding F-15 (RFC 012 rule 30): the start hook. After every guest's `init` has run at
//! tick 0 and before the first event is handled, the Host calls the domain's `start_calling` with
//! the call-in handle; the activations it makes are keyed as the actor's own `init` event; and
//! the domain may refuse the run. The kit's test domain does what its `[test]` table says.
use florn_conformance::host;
use florn_conformance::test_domain::{self as td};
use florn_host::HostError;
use florn_kernel::trace::CommittedEffect;
use florn_kernel::{EventKey, FaultKind, Record};

fn scenario(table: &str) -> String {
    td::text(
        "start",
        50,
        11,
        &format!(
            "{}{}{}\n[test]\n{table}\n",
            td::prober(1, 1),
            td::prober(2, 1),
            td::wire(100, &[(1, 0), (2, 0)], "")
        ),
    )
}

fn init_key(actor: u32) -> EventKey {
    EventKey {
        tick: 0,
        microstep: 0,
        class: florn_kernel::CLASS_CONTROL,
        tie: 0,
        origin: actor,
        seq: 0,
    }
}

/// The records in the trace, in order, as (what, actor, key).
fn trace(run: &host::Host) -> Vec<(&'static str, u32, EventKey)> {
    run.records()
        .unwrap()
        .iter()
        .filter_map(|r| match r {
            Record::Handled { key, target, .. } => Some(("handled", *target, *key)),
            Record::Fault { key, actor, .. } => Some(("fault", *actor, *key)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_hook_runs_after_every_init_and_before_the_first_event_keyed_as_the_actors_init() {
    let run = td::run(&scenario(
        "start-actor = 1\nstart-script = \"open:2 eval:3 commit\"",
    ));
    let t = trace(&run);
    // Both inits first, in ascending identity; then the hook's one record, keyed as actor 1's
    // `init` (the same key as the init activation of that actor); then the events.
    assert_eq!(t[0], ("handled", 1, init_key(1)), "{t:?}");
    assert_eq!(t[1], ("handled", 2, init_key(2)), "{t:?}");
    assert_eq!(t[2], ("handled", 1, init_key(1)), "{t:?}");
    assert!(
        t[3..].iter().all(|(_, _, k)| k.tick > 0 || k.class != 0),
        "no event before the hook is done: {t:?}"
    );
    // The hook's record holds the compound activation's effects, in call order.
    let Record::Handled { effects, .. } = &run.records().unwrap()[2] else {
        panic!("a Handled record")
    };
    let channels: Vec<u32> = effects
        .iter()
        .filter_map(|e| match e {
            CommittedEffect::Report { channel, .. } => Some(*channel),
            _ => None,
        })
        .collect();
    assert_eq!(channels, vec![3, 2, 0]);
    // What the domain said about it reaches the log.
    assert!(
        run.log().entries().iter().any(|e| {
            e.notification.code == "domain.event"
                && format!("{:?}", e.notification.params).contains("open=opened")
        }),
        "the start report"
    );
}

#[test]
fn a_step_discarded_in_the_hook_leaves_the_run_of_one_that_had_no_hook() {
    let with = td::run(&scenario(
        "start-actor = 1\nstart-script = \"open:2 eval:3 discard\"",
    ));
    let without = td::run(&scenario("note = \"no hook\""));
    assert_eq!(trace(&with), trace(&without));
    assert_eq!(with.chain_hex(), without.chain_hex());
}

#[test]
fn a_fault_in_the_hook_is_one_fault_keyed_as_the_actors_init_and_the_run_goes_on() {
    let run = td::run(&scenario(
        "start-actor = 1\nstart-script = \"open:0 eval:-1\"",
    ));
    let faults = host::faults(&run);
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!((faults[0].0, faults[0].1), (1, FaultKind::Trap));
    let t = trace(&run);
    assert_eq!(t[2], ("fault", 1, init_key(1)), "{t:?}");
    // The other actor was not stopped by it.
    assert!(
        t.iter()
            .any(|(w, a, k)| *w == "handled" && *a == 2 && k.tick > 0),
        "{t:?}"
    );
    assert_eq!(host::ended(&run), florn_host::EndReason::EndReached);
}

#[test]
fn a_domain_may_refuse_the_run_before_the_first_event() {
    let text = scenario("refuse-start = true");
    match td::try_start(&text, host::options()) {
        Err(HostError::Domain(refusals)) => {
            assert_eq!(refusals.len(), 1);
            assert_eq!(refusals[0].code, "manifest.invalid");
            assert!(refusals[0].check().is_ok(), "{:?}", refusals[0]);
        }
        Err(other) => panic!("{other}"),
        Ok(_) => panic!("the run started"),
    }
}

#[test]
fn a_hook_that_does_nothing_changes_no_digest() {
    // The default start hook of a domain does nothing: a scenario with a table that only names
    // a note has the digest of the same scenario without the hook's keys.
    let a = td::run(&scenario("note = \"a\""));
    let b = td::run(&scenario("note = \"b\""));
    assert_eq!(a.chain_hex(), b.chain_hex());
}

#[test]
fn the_table_is_checked() {
    for table in [
        "refuse-start = 1",
        "start-script = 3",
        "start-actor = \"one\"",
        "start-actor = -1",
        "nope = 1",
    ] {
        let r = td::try_start(&scenario(table), host::options());
        assert!(matches!(r, Err(HostError::Domain(_))), "{table}");
    }
}
