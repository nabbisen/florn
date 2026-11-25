//! PR-45's first commit, finding F-18 (b) of RFC 012: the circuit domain's start hook reads each
//! guest actor's admitted module. The circuit worlds are not registered yet (PR-47, PR-49), so the
//! guests here are stubs of the core world whose manifests carry the `[circuit]` table and the
//! ports of a device or a controller. Checklist B-49's `two-devices`, in the form the hook gives.
use florn_conformance::circuit::{Deck, refusals, start, try_start};
use florn_conformance::guests;
use florn_control::{Severity, Value};

/// A divider with a voltmeter and `devices` device modules across its middle net and the ground.
fn plant_with(devices: u32) -> Deck {
    let mut deck = Deck::new("two-devices", "1ms")
        .part(1, "v1", "voltage-source", &[], "v = \"10V\"")
        .part(2, "r1", "resistor", &[], "r = \"1kohm\"")
        .part(3, "r2", "resistor", &[], "r = \"3kohm\"")
        .part(4, "vm", "voltmeter", &[], "period = \"1ms\"");
    let mut mid = vec!["r1.b".to_string(), "r2.a".to_string(), "vm.p".to_string()];
    let mut gnd = vec!["v1.n".to_string(), "r2.b".to_string(), "vm.n".to_string()];
    for n in 0..devices {
        let name = format!("d{n}");
        deck = deck.guest(
            10 + n,
            &name,
            "device",
            guests::circuit_stub("device", ""),
            &[("p", "circuit/pin"), ("n", "circuit/pin")],
        );
        mid.push(format!("{name}.p"));
        gnd.push(format!("{name}.n"));
    }
    let mid: Vec<&str> = mid.iter().map(String::as_str).collect();
    let gnd: Vec<&str> = gnd.iter().map(String::as_str).collect();
    deck.net("top", &["v1.p", "r1.a"])
        .net("mid", &mid)
        .ground("gnd", &gnd)
}

#[test]
fn one_device_module_starts() {
    let run = start(&plant_with(1));
    assert!(run.ended().is_none(), "the run is live");
}

#[test]
fn a_second_device_module_is_refused_from_the_start_hook_naming_the_count_and_the_maximum() {
    let refused = refusals(try_start(&plant_with(2)));
    assert_eq!(refused.len(), 1, "{refused:?}");
    let n = &refused[0];
    assert_eq!(n.code, "circuit.device-limit");
    assert_eq!(n.severity, Severity::Error);
    assert!(n.check().is_ok(), "{n:?}");
    let params: Vec<&Value> = n.params.iter().map(|(_, v)| v).collect();
    assert_eq!(params, [&Value::Count(2), &Value::Count(1)]);
    // Three are three.
    let refused = refusals(try_start(&plant_with(3)));
    assert_eq!(refused[0].params[0].1, Value::Count(3));
}

#[test]
fn a_controller_module_is_not_a_device_and_does_not_count() {
    let deck = plant_with(1).guest(
        30,
        "ctl",
        "controller",
        guests::circuit_stub("controller", ""),
        &[("in", "circuit/signal-in"), ("out", "circuit/signal-out")],
    );
    // The controller's ports are on no signal, which is allowed; the run starts.
    let refused = refusals(try_start(&deck));
    assert!(
        !refused.iter().any(|n| n.code == "circuit.device-limit"),
        "{refused:?}"
    );
}
