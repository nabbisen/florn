use super::*;

/// RFC 005 rule 31, RFC 012 finding F-7: the runner's exit code follows the end reason, whatever
/// the reason is. Every value of the closed set `end-reason` but `end-reached` is a fault ending
/// (exit code 3), with `--to` or without; the two a domain can give among them.
#[test]
fn every_end_reason_but_the_end_is_a_fault_ending() {
    let reasons = florn_control::catalogue::enumeration("end-reason").expect("the set");
    assert!(reasons.contains(&"non-convergence") && reasons.contains(&"actor-fault"));
    for reason in reasons {
        for to_given in [false, true] {
            assert_eq!(
                ends_as_fault(Some(reason), to_given),
                reason != &"end-reached",
                "{reason} (--to {to_given})"
            );
        }
    }
}

#[test]
fn a_run_with_no_reason_is_judged_by_whether_it_was_asked_to_stop_early() {
    assert!(!ends_as_fault(None, true), "stopped by --to, no reason yet");
    assert!(
        ends_as_fault(None, false),
        "asked for to its end, and has none"
    );
}
