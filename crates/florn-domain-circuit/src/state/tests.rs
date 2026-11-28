//! RFC 012 rule 29: the state of the plant in a checkpoint, written and read back.

use super::*;
use crate::dc::initial_solve;
use crate::tests::Deck;

fn started() -> (crate::plant::Plant, Live, Solution) {
    let plant = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"1V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "c1", "capacitor", "c = \"1uF\"\nic = \"0V\"")
        .builtin(4, "vm", "voltmeter", "period = \"100us\"")
        .guest(10, "ctl", &[("in", "circuit/signal-in")])
        .net("top", &["v1.p", "r1.a"])
        .net("cap", &["r1.b", "c1.a", "vm.p"])
        .ground("gnd", &["v1.n", "c1.b", "vm.n"])
        .signal("volts", &["vm.v", "ctl.in"], "")
        .plant();
    let solution = initial_solve(&plant).unwrap();
    let mut live = Live::new(&plant);
    live.start(&plant, &solution, Vec::new());
    (plant, live, solution)
}

#[test]
fn the_state_reads_back_to_the_bytes_it_was_written_as() {
    let (plant, mut live, solution) = started();
    for t in [0, 100_000, 200_000] {
        live.on_advance(&plant, t, &mut florn_host::compound::NoCalls);
    }
    live.record(&plant, 300_000, Input::Source(1), 0.25, 1);
    live.freeze(0, true);
    live.samplers = vec![crate::controllers::Sampler {
        actor: 9,
        period: 1_000,
        next_due: 205_000,
        index: 205,
    }];
    let bytes = encode(&live, Some(&solution));
    // The recorded version of this encoding: 4 holds the detectors' state (PR-51); 3 held whether
    // the guest device was lost (PR-49).
    assert_eq!(VERSION, 4);
    assert_eq!(bytes[0], 4);
    let (back, sol) = decode(&plant, &bytes).expect("it fits the plant");
    assert_eq!(encode(&back, sol.as_ref()), bytes);
    assert_eq!(back.pending.len(), live.pending.len());
    assert_eq!(back.last_advance, 200_000);
    assert_eq!(
        back.samplers, live.samplers,
        "each controller's period, next sample and index"
    );
    assert_eq!(
        back.transient.as_ref().unwrap().x(),
        live.transient.as_ref().unwrap().x()
    );
    assert_eq!(sol.unwrap().stage, solution.stage);
}

#[test]
fn the_loss_of_the_guest_device_is_in_the_state_and_the_old_version_is_refused() {
    let (plant, mut live, solution) = started();
    assert!(!live.guest_lost);
    live.guest_lost = true;
    let bytes = encode(&live, Some(&solution));
    let (back, _) = decode(&plant, &bytes).expect("it fits the plant");
    assert!(back.guest_lost, "the loss survives a checkpoint");
    // The same plant without the loss differs in that one byte, after the version, the last advance
    // and its pending tick, the status and the notified mark.
    live.guest_lost = false;
    let other = encode(&live, Some(&solution));
    let differing: Vec<usize> = (0..bytes.len())
        .filter(|i| bytes[*i] != other[*i])
        .collect();
    assert_eq!(differing.len(), 1, "{differing:?}");
    // The versions before, 2 as PR-47 wrote it and 3 as PR-49 did, are not read.
    for old in [2, 3] {
        let mut v = other.clone();
        v[0] = old;
        assert!(decode(&plant, &v).is_none(), "version {old}");
    }
}

#[test]
fn bytes_that_are_not_this_domains_or_do_not_fit_the_plant_are_refused() {
    let (plant, live, solution) = started();
    let bytes = encode(&live, Some(&solution));
    assert!(decode(&plant, &[]).is_none());
    let mut other = bytes.clone();
    other[0] = VERSION + 1;
    assert!(decode(&plant, &other).is_none(), "another version");
    assert!(
        decode(&plant, &bytes[..bytes.len() - 1]).is_none(),
        "cut short"
    );
    let mut longer = bytes.clone();
    longer.push(0);
    assert!(decode(&plant, &longer).is_none(), "trailing bytes");
    // The state of another plant (one more sensor) does not fit.
    let bigger = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"1V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(4, "vm", "voltmeter", "period = \"100us\"")
        .builtin(5, "am", "ammeter", "period = \"100us\"")
        .net("top", &["v1.p", "am.a"])
        .net("mid", &["am.b", "r1.a", "vm.p"])
        .ground("gnd", &["v1.n", "r1.b", "vm.n"])
        .plant();
    assert!(decode(&bigger, &bytes).is_none());
}

#[test]
fn every_float_is_canonical_in_the_state() {
    let (plant, mut live, solution) = started();
    live.held[0].value = f64::from_bits(0x7ff8_0000_0000_0001);
    let bytes = encode(&live, Some(&solution));
    let (back, _) = decode(&plant, &bytes).unwrap();
    assert_eq!(back.held[0].value.to_bits(), 0x7ff8_0000_0000_0000);
}
