//! PR-45's first commit, finding F-18 of RFC 012: only an error of `Domain::validate` refuses a
//! run; a warning is logged when the run starts and returned with the validation a front end
//! asks for; and the start hook reads each guest actor's admitted module, read-only. The kit's
//! test domain returns a warning from its validation with `warn-validate` and says what the hook
//! was given with `list-guests`.
use florn_admission::Admission;
use florn_conformance::host;
use florn_conformance::test_domain::{self as td};
use florn_control::{Answer, Command, Envelope, Message, Severity};
use florn_host::{Controller, HostError};
use florn_platform::FixedPlatform;

fn scenario(table: &str) -> String {
    td::text(
        "notes",
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

fn controller() -> Controller<florn_engine_wasmtime::WasmtimeEngine> {
    Controller::new(
        host::engine(),
        Admission::default(),
        Box::new(FixedPlatform::default()),
        td::domains(),
        host::options(),
    )
}

fn domain_warnings(run: &host::Host) -> usize {
    run.log()
        .entries()
        .iter()
        .filter(|e| {
            e.notification.code == "host.capacity-warning"
                && e.notification.severity == Severity::Warning
                && format!("{:?}", e.notification.params).contains("Count(1)")
        })
        .count()
}

#[test]
fn a_warning_of_the_validation_does_not_refuse_the_run_and_is_in_its_log() {
    let with = td::run(&scenario("warn-validate = true"));
    assert_eq!(domain_warnings(&with), 1, "{:?}", with.log().entries());
    // The run is the run of the scenario without the warning.
    let without = td::run(&scenario("note = \"quiet\""));
    assert_eq!(domain_warnings(&without), 0);
    assert_eq!(with.chain_hex(), without.chain_hex());
}

#[test]
fn an_error_of_the_validation_still_refuses_and_only_the_errors_are_the_refusals() {
    // An unknown key in the table is an error; the warning the same validation returned is not
    // among the refusals.
    match td::try_start(
        &scenario("warn-validate = true\nbogus = 1"),
        host::options(),
    ) {
        Err(HostError::Domain(refusals)) => {
            assert!(
                refusals.iter().all(|n| n.severity == Severity::Error),
                "only errors refuse: {refusals:?}"
            );
            assert!(refusals.iter().any(|n| n.code == "manifest.unknown-key"));
        }
        Err(other) => panic!("{other}"),
        Ok(_) => panic!("the run started"),
    }
}

#[test]
fn a_front_end_validating_a_draft_is_told_the_warning_and_the_scenario_loads() {
    let mut c = controller();
    let out = c.handle(Envelope::new(Message::Command(Command::LoadScenario {
        bytes: scenario("warn-validate = true").into_bytes(),
    })));
    let warned = out.iter().any(|m| {
        matches!(m, Message::Notification(n)
            if n.code == "host.capacity-warning" && n.severity == Severity::Warning)
    });
    assert!(warned, "the warning comes with the validation: {out:?}");
    assert!(
        out.iter()
            .any(|m| matches!(m, Message::Answer(Answer::Done))),
        "the scenario is loaded, not refused: {out:?}"
    );
    // The same scenario with an error is refused and says so.
    let out = c.handle(Envelope::new(Message::Command(Command::LoadScenario {
        bytes: scenario("warn-validate = true\nbogus = 1").into_bytes(),
    })));
    assert!(
        !out.iter()
            .any(|m| matches!(m, Message::Answer(Answer::Done))),
        "{out:?}"
    );
}

#[test]
fn the_start_hook_reads_every_guest_actor_read_only_in_ascending_identity() {
    let run = td::run(&scenario("list-guests = true"));
    let said: Vec<String> = run
        .log()
        .entries()
        .iter()
        .filter(|e| e.notification.code == "domain.event")
        .map(|e| format!("{:?}", e.notification.params))
        .filter(|p| p.contains("guests"))
        .collect();
    assert_eq!(said.len(), 1, "{said:?}");
    let world = td::WORLD;
    let first = said[0].find(&format!("1=prober@{world}")).expect("actor 1");
    let second = said[0].find(&format!("2=prober@{world}")).expect("actor 2");
    assert!(first < second, "ascending identity: {}", said[0]);
}
