use super::*;

#[test]
fn a_rule_of_rfc_002_keeps_its_number_and_another_rfcs_is_times_a_thousand_plus_the_rule() {
    // RFC 012 rule 20: the Fault record's parameter for a contract violation.
    for (rule, parameter) in [
        ("RFC 002 rule 23", 23),
        ("RFC 002 rule 8", 8),
        ("RFC 002 rule 44", 44),
        ("RFC 012 rule 19", 12019),
        ("RFC 005 rule 20", 5020),
        ("RFC 008 rule 1", 8001),
    ] {
        assert_eq!(contract_parameter(rule), parameter, "{rule}");
    }
    // Text that names no rule gives zero, as it always did.
    for rule in ["", "no rule", "RFC rule x"] {
        assert_eq!(contract_parameter(rule), 0, "{rule}");
    }
    assert_eq!(
        contract_parameter("rule 44"),
        44,
        "the old form: its last word"
    );
}

#[test]
fn the_fault_parameter_of_a_refusal_follows_the_rule_it_names() {
    let contract = |rule| SinkRefusal::Contract {
        rule,
        detail: String::new(),
    };
    assert_eq!(fault_parameter(&contract("RFC 002 rule 32")), 32);
    assert_eq!(fault_parameter(&contract("RFC 012 rule 19")), 12019);
    let quota = |quota| SinkRefusal::Quota { quota, limit: 1 };
    assert_eq!(fault_parameter(&quota("effects-per-activation")), 4);
    assert_eq!(fault_parameter(&quota("pending-timers")), 6);
}
