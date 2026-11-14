use super::*;
use florn_control::{Ends, Notification, Scope, Severity, Value, catalogue};

/// Every value the Host can give.
const ALL: [MachineCondition; 4] = [
    MachineCondition::EngineStack,
    MachineCondition::Watchdog,
    MachineCondition::EngineFailure,
    MachineCondition::HostDefect,
];

const TRAP_KINDS: [TrapKind; 5] = [
    TrapKind::Guest,
    TrapKind::HostRefused,
    TrapKind::EngineStack,
    TrapKind::Watchdog,
    TrapKind::Other,
];

#[test]
fn every_host_fault_the_host_can_build_gives_a_value_of_the_closed_set() {
    // RFC 007 section 4.5: `machine-condition` is a closed set. The Host builds a Host fault from a
    // trap of the engine, from a failure of the engine to do what it was asked, and from a domain's
    // defect; each gives a value of the type, whose names are all in the set, and each makes a
    // `host.not-reproducible` the catalogue accepts.
    let set = catalogue::enumeration("machine-condition").expect("the enumeration exists");
    for condition in ALL {
        assert!(set.contains(&condition.as_str()), "{condition:?}");
        let n = Notification::new(
            "host.not-reproducible",
            Severity::Error,
            Scope::actor(1),
            Ends::Run,
        )
        .with("reason", Value::Enum(condition.as_str().into()))
        .with("text", Value::Text(String::new()));
        assert!(n.check().is_ok(), "{condition:?}");
    }
    // Every trap kind is either the guest's (no Host fault) or maps to a value.
    for kind in TRAP_KINDS {
        match MachineCondition::of_trap(kind) {
            Some(c) => assert!(set.contains(&c.as_str()), "{kind:?}"),
            None => assert!(matches!(kind, TrapKind::Guest | TrapKind::HostRefused)),
        }
    }
}

#[test]
fn each_trap_gives_its_own_value() {
    assert_eq!(
        MachineCondition::of_trap(TrapKind::EngineStack),
        Some(MachineCondition::EngineStack)
    );
    assert_eq!(
        MachineCondition::of_trap(TrapKind::Watchdog),
        Some(MachineCondition::Watchdog)
    );
    assert_eq!(
        MachineCondition::of_trap(TrapKind::Other),
        Some(MachineCondition::EngineFailure)
    );
    // The names are distinct, so that no value stands for another.
    let mut names: Vec<&str> = ALL.iter().map(|c| c.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), ALL.len());
}
