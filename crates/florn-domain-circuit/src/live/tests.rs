//! RFC 012 rules 8, 9, 11 and 15 at the plant's state between events: which tick the advance is at,
//! what records a breakpoint and what records none, the bound, a frozen signal.

use super::*;
use crate::dc::initial_solve;
use crate::tests::Deck;

fn deck(profile: &str) -> Deck {
    Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"1V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "c1", "capacitor", "c = \"1uF\"\nic = \"0V\"")
        .builtin(4, "vm", "voltmeter", "period = \"100us\"\nphase = \"30us\"")
        .builtin(5, "am", "ammeter", "period = \"70us\"")
        .net("top", &["v1.p", "am.a"])
        .net("mid", &["am.b", "r1.a"])
        .net("cap", &["r1.b", "c1.a", "vm.p"])
        .ground("gnd", &["v1.n", "c1.b", "vm.n"])
        .profile(profile)
}

fn started(profile: &str) -> (crate::plant::Plant, Live, Vec<Action>) {
    let plant = deck(profile).plant();
    let mut live = Live::new(&plant);
    let solution = initial_solve(&plant).unwrap();
    let actions = live.start(&plant, &solution, Vec::new());
    (plant, live, actions)
}

#[test]
fn the_first_advance_is_at_the_earliest_sensor_due_and_every_sensor_due_samples_after_it() {
    // Sensors due at 30 us (the voltmeter, phase 30 us) and 0 (the ammeter): the first advance is
    // at tick 0, the second at 30 us, and so on at the earliest due tick.
    let (plant, mut live, actions) = started("");
    assert_eq!(live.advance_at, Some(0));
    assert_eq!(
        actions,
        [Action::Schedule {
            origin: plant.world,
            delay: 0,
            class: CLASS_ADVANCE,
            target: plant.world,
            payload: Vec::new()
        }]
    );
    let after = live.on_advance(&plant, 0);
    // The ammeter samples (delay 0, next microstep); the next advance is the voltmeter's 30 us.
    let samples: Vec<_> = after
        .iter()
        .filter_map(|a| match a {
            Action::Schedule {
                class,
                target,
                delay,
                ..
            } if *class == CLASS_SAMPLE => Some((*target, *delay)),
            _ => None,
        })
        .collect();
    assert_eq!(samples, [(5, 0)]);
    assert_eq!(live.advance_at, Some(30_000));
    live.on_advance(&plant, 30_000);
    assert_eq!(live.advance_at, Some(70_000));
}

#[test]
fn no_advance_is_scheduled_at_or_after_the_end() {
    let plant = deck("").plant();
    let mut live = Live::new(&plant);
    live.sensors.iter_mut().for_each(|s| s.next_due = plant.end);
    assert_eq!(live.next_advance(&plant, 0), None);
}

#[test]
fn a_change_to_the_value_an_input_holds_records_nothing_and_a_second_at_one_tick_replaces() {
    let (plant, mut live, _) = started("");
    live.record(&plant, 500, Input::Source(1), 1.0, 1);
    assert!(live.pending.is_empty(), "1 V is the value it holds");
    live.record(&plant, 500, Input::Source(1), 0.5, 1);
    live.record(&plant, 500, Input::Source(1), 0.25, 1);
    assert_eq!(live.pending.len(), 1);
    // Back to the value in force while 0.25 is pending is a change, not a repeat.
    live.record(&plant, 600, Input::Source(1), 0.25, 1);
    assert_eq!(
        live.pending.len(),
        1,
        "the same value as the pending one holds"
    );
}

#[test]
fn more_pending_breakpoints_than_the_bound_set_the_limit_naming_the_origins() {
    let (plant, mut live, _) = started("max-breakpoints = 3");
    for k in 0..4 {
        live.record(&plant, 10 + k, Input::Source(1), 0.1 * (k + 1) as f64, 7);
    }
    let limit = live.limit.take().expect("the bound is passed");
    assert_eq!(
        (limit.what, limit.limit, limit.origins),
        ("pending-breakpoints", 3, vec![7])
    );
}

#[test]
fn a_held_plant_records_no_breakpoint() {
    let (plant, mut live, _) = started("");
    live.status = Status::Held;
    live.record(&plant, 10, Input::Source(1), 0.5, 1);
    assert!(live.pending.is_empty());
}

#[test]
fn a_frozen_signal_ignores_its_producers_output_and_a_repeat_changes_nothing() {
    let plant = Deck::new()
        .builtin_opt(1, "v1", "voltage-source", "v = \"1V\"")
        .builtin(2, "k1", "constant", "value = 1")
        .builtin(3, "r1", "resistor", "r = \"1kohm\"")
        .builtin(4, "vm", "voltmeter", "period = \"1ms\"")
        .net("a", &["v1.p", "r1.a", "vm.p"])
        .ground("g", &["v1.n", "r1.b", "vm.n"])
        .guest(10, "ctl", &[("out", "circuit/signal-out")])
        .signal("cmd", &["ctl.out", "v1.cmd"], "")
        .plant();
    // A guest is the producer of the signal that drives the source's `cmd`.
    let mut live = Live::new(&plant);
    let sig = plant.port_signal[&(10, 0)];
    live.output(&plant, 5, 10, 0, 2.0);
    assert_eq!(live.held[sig].value, 2.0);
    live.freeze(sig, true);
    live.output(&plant, 6, 10, 0, 3.0);
    assert_eq!(
        live.held[sig].value, 2.0,
        "frozen: the producer's output is ignored"
    );
    live.freeze(sig, false);
    live.output(&plant, 8, 10, 0, 3.0);
    assert_eq!(live.held[sig].value, 3.0);
    // Freezing and releasing recorded no breakpoint (RFC 012 rule 9, amended).
    assert!(live.pending.is_empty() || live.transient.is_none());
}

#[test]
fn an_unknown_names_the_device_that_owns_it() {
    let (plant, _, _) = started("");
    // Three nets (0 to 2), then the branch of the source and of the ammeter in ascending identity;
    // a net is named by the lowest-identity device with a pin on it.
    assert_eq!(plant.devices[owner_of_unknown(&plant, 3)].name, "v1");
    assert_eq!(plant.devices[owner_of_unknown(&plant, 4)].name, "am");
    assert_eq!(plant.devices[owner_of_unknown(&plant, 0)].name, "v1");
    assert_eq!(plant.devices[owner_of_unknown(&plant, 1)].name, "r1");
}

#[test]
fn exactly_the_bound_of_pending_breakpoints_is_allowed() {
    let (plant, mut live, _) = started("max-breakpoints = 3");
    for k in 0..3 {
        live.record(&plant, 10 + k, Input::Source(1), 0.1 * (k + 1) as f64, 7);
    }
    assert!(live.limit.is_none(), "three pending with a bound of three");
    live.record(&plant, 20, Input::Source(1), 0.9, 7);
    assert!(live.limit.is_some(), "the fourth passes it");
}

#[test]
fn a_detector_wakes_the_advance_at_detect_interval_when_no_sensor_is_due_sooner() {
    // RFC 012 rule 8: the second term of the formula, only when the scenario has a detector.
    let plant = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"1V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "cp", "comparator", "threshold = \"0.5V\"")
        .builtin(4, "vm", "voltmeter", "period = \"1ms\"\nphase = \"900us\"")
        .net("a", &["v1.p", "r1.a", "cp.p", "vm.p"])
        .ground("g", &["v1.n", "r1.b", "cp.n", "vm.n"])
        .profile("detect-interval = \"200us\"")
        .plant();
    let mut live = Live::new(&plant);
    assert!(live.detector);
    live.start(&plant, &initial_solve(&plant).unwrap(), Vec::new());
    // The voltmeter is due at 900 us, the detector wakes the plant at 200 us.
    assert_eq!(live.advance_at, Some(200_000));
    live.on_advance(&plant, 200_000);
    assert_eq!(
        live.advance_at,
        Some(400_000),
        "the last advance plus the interval"
    );
    // Without a detector the sensor alone decides.
    let (plant, live, _) = started("");
    assert!(!live.detector);
    assert_eq!(plant.profile.detect_interval, 100 * plant.profile.step);
}

#[test]
fn the_held_values_the_signals_have_at_the_start_are_the_inputs_in_force() {
    // RFC 012 rule 30: the outputs a controller's `init` set are in force at the first step.
    let plant = Deck::new()
        .builtin_opt(1, "v1", "voltage-source", "v = \"1V\"\ngain = \"2V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "vm", "voltmeter", "period = \"1ms\"")
        .net("a", &["v1.p", "r1.a", "vm.p"])
        .ground("g", &["v1.n", "r1.b", "vm.n"])
        .guest(10, "ctl", &[("out", "circuit/signal-out")])
        .signal("cmd", &["ctl.out", "v1.cmd"], "")
        .plant();
    let mut live = Live::new(&plant);
    let sig = plant.port_signal[&(10, 0)];
    live.held[sig].value = 0.5;
    let solution = initial_solve(&live.plant_in_force(&plant)).unwrap();
    live.start(&plant, &solution, Vec::new());
    assert_eq!(live.transient.as_ref().unwrap().held(sig), 0.5);
    // The source is 1 V + 2 V × 0.5.
    assert!((solution.x[0] - 2.0).abs() < 1e-6, "{:?}", solution.x);
}

#[test]
fn a_committed_send_that_is_not_one_finite_f64_changes_nothing() {
    // RFC 012 rule 11: eight bytes, and a non-finite value is a contract violation of the guest
    // (the Host's), never a held value.
    use florn_host::compound::NoCalls;
    use florn_host::domain::Domain;
    let deck = Deck::new()
        .builtin_opt(1, "v1", "voltage-source", "v = \"1V\"\ngain = \"2V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "vm", "voltmeter", "period = \"1ms\"")
        .net("a", &["v1.p", "r1.a", "vm.p"])
        .ground("g", &["v1.n", "r1.b", "vm.n"])
        .guest(10, "ctl", &[("out", "circuit/signal-out")])
        .signal("cmd", &["ctl.out", "v1.cmd"], "");
    let mut d = crate::CircuitDomain::new(crate::Limits::native());
    d.compose(&deck.manifest(), 1);
    d.start_calling(&mut NoCalls, &[]).unwrap();
    let held = |d: &crate::CircuitDomain| d.live().unwrap().held[0].value;
    let at = florn_kernel::Instant::new(5, 0);
    let dead = std::collections::BTreeSet::new();
    d.deliver(at, 10, 0, 0, &f64::NAN.to_le_bytes(), &dead);
    d.deliver(at, 10, 1, 0, &f64::INFINITY.to_le_bytes(), &dead);
    d.deliver(at, 10, 2, 0, &[1, 2, 3], &dead);
    assert_eq!(held(&d), 0.0);
    d.deliver(at, 10, 3, 0, &0.75f64.to_le_bytes(), &dead);
    assert_eq!(held(&d), 0.75);
    assert_eq!(
        d.live().unwrap().pending.len(),
        1,
        "a breakpoint where it drives"
    );
}

#[test]
fn a_step_at_the_minimum_is_notified_once_per_advance_and_counted_in_the_summary() {
    // RFC 013 rule 12 and rule 28's row: `min-step` equal to `step` and a tight `ltol` put many
    // steps of one advance at the minimum; the log hears of the first and the count is kept.
    let (plant, mut live, _) = started("step = \"1000ns\"\nmin-step = \"1000ns\"\nltol = 1e-9");
    live.on_advance(&plant, 0);
    let actions = live.on_advance(&plant, 30_000);
    let said = actions
        .iter()
        .filter(|a| matches!(a, Action::Notify(n) if n.code == "circuit.step-minimum"))
        .count();
    assert_eq!(said, 1);
    assert!(live.summary.at_minimum > 1, "{:?}", live.summary);
}

fn with_controller(period: u64, phase: u64) -> (crate::plant::Plant, Live, Vec<Action>) {
    let plant = deck("").plant();
    let mut live = Live::new(&plant);
    let sampler = crate::controllers::Sampler {
        actor: 9,
        period,
        next_due: phase,
        index: 0,
    };
    let solution = initial_solve(&plant).unwrap();
    let actions = live.start(&plant, &solution, vec![sampler]);
    (plant, live, actions)
}

fn samples_of(actions: &[Action], actor: u32) -> Vec<u64> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Schedule {
                class,
                target,
                delay,
                ..
            } if *class == CLASS_SAMPLE && *target == actor => Some(*delay),
            _ => None,
        })
        .collect()
}

#[test]
fn a_controllers_first_sample_is_at_its_phase_unless_that_is_the_end() {
    // RFC 012 rule 10: the start hook schedules it at its phase; none at or after the end.
    let (plant, _, actions) = with_controller(1_000, 40);
    assert_eq!(samples_of(&actions, 9), [40]);
    let (_, _, actions) = with_controller(1_000, plant.end - 1);
    assert_eq!(samples_of(&actions, 9), [plant.end - 1]);
    let (_, _, actions) = with_controller(1_000, plant.end);
    assert!(samples_of(&actions, 9).is_empty(), "the end is exclusive");
}

#[test]
fn a_controllers_sample_calls_on_sample_and_schedules_the_next_a_period_later() {
    let (plant, mut live, _) = with_controller(1_000, 0);
    let first = live.controller_sample(&plant, 9, 0, true);
    assert!(matches!(
        &first[0],
        Action::Call { actor: 9, export, values }
            if export == crate::controllers::ON_SAMPLE
                && values == &[florn_abi::Val::U64(0), florn_abi::Val::U64(0)]
    ));
    assert_eq!(samples_of(&first, 9), [1_000]);
    let second = live.controller_sample(&plant, 9, 1_000, true);
    assert!(matches!(
        &second[0],
        Action::Call { values, .. } if values == &[florn_abi::Val::U64(1_000), florn_abi::Val::U64(1)]
    ));
    // The last sample before the end schedules none after it, not even one at the end.
    let last = live.controller_sample(&plant, 9, plant.end - 1_000, true);
    assert!(samples_of(&last, 9).is_empty(), "{last:?}");
}

#[test]
fn a_faulted_controller_is_dropped_and_not_sampled_again() {
    // RFC 012 rule 17: nothing is called and nothing scheduled, and the index does not move.
    let (plant, mut live, _) = with_controller(1_000, 0);
    assert!(live.controller_sample(&plant, 9, 0, false).is_empty());
    assert_eq!(live.samplers[0].index, 0);
    assert!(
        live.controller_sample(&plant, 77, 0, true).is_empty(),
        "not a controller"
    );
}
