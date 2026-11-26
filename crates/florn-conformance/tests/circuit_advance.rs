//! PR-46 (checklist B-46): the plant advanced on demand under the Host. RFC 012 rules 7 to 11, 14,
//! 15 and 29; RFC 013 rule 17. The `advance` event, the single pending advance, the sensors'
//! samples, the breakpoints and their bound, held signals, scheduled changes and interventions,
//! and the checkpoint state.
use florn_conformance::circuit::{self, Deck, rc_step};
use florn_conformance::host;
use florn_domain_circuit::CHANNEL_PROBE;

/// `rc-step` of RFC 012 rule 27, with its scheduled change to 0 V at 2 ms.
fn rc_step_with_change() -> Deck {
    rc_step().change("2ms", "world:circuit", "actor = 1\nv = \"0V\"")
}

fn probe_values(run: &circuit::Host, actor: u32) -> Vec<(u64, f64)> {
    host::reports(run, actor, CHANNEL_PROBE)
        .into_iter()
        .map(|(at, bytes)| {
            let v = f64::from_le_bytes(bytes.as_slice().try_into().expect("eight bytes"));
            (at.tick, v)
        })
        .collect()
}

/// The analytic voltage across the capacitor (RC = 1 ms), the source stepped at `fall` seconds.
fn analytic(t: f64, fall: f64) -> f64 {
    if t <= fall {
        1.0 - (-t / 1e-3).exp()
    } else {
        (1.0 - (-fall / 1e-3).exp()) * (-(t - fall) / 1e-3).exp()
    }
}

#[test]
fn rc_step_through_the_host_samples_the_analytic_solution_within_four_ltol_v() {
    let mut run = circuit::start(&rc_step_with_change());
    run.run_to_end();
    assert_eq!(host::ended(&run), florn_host::EndReason::EndReached);
    let samples = probe_values(&run, 4);
    // Every 100 us from tick 0 to 5 ms (the end is exclusive): 50 samples.
    assert_eq!(samples.len(), 50, "{samples:?}");
    for (k, (tick, v)) in samples.iter().enumerate() {
        assert_eq!(*tick, k as u64 * 100_000);
        let want = analytic(*tick as f64 * 1e-9, 2e-3);
        assert!(
            (v - want).abs() <= 4e-3,
            "sample {k} at {tick}: {v} against {want}"
        );
    }
    // The source's step at T leaves the sample at T as it was and changes the next (rule 9).
    let at = |t: u64| samples.iter().find(|s| s.0 == t).unwrap().1;
    assert!((at(2_000_000) - analytic(2e-3, f64::INFINITY)).abs() <= 4e-3);
    assert!(at(2_100_000) < at(2_000_000));
}

use std::collections::BTreeMap;

use florn_host::{Advance, Checkpoint, Options, Run};

fn options() -> Options {
    Options {
        checkpoint_interval: 0,
        checkpoint_memory: u64::MAX,
        ..host::options()
    }
}

/// A checkpoint taken at the first boundary at or after `tick`.
fn checkpoint_at(deck: &Deck, tick: u64) -> (circuit::Host, Checkpoint) {
    let mut run = circuit::try_start_in(&circuit::domains(), deck, options()).unwrap();
    while run.kernel().now().tick < tick {
        run.advance(Advance::OneInstant);
    }
    assert!(
        run.request_checkpoint() || {
            run.advance(Advance::OneInstant);
            run.kept().len() == 2
        }
    );
    let kept = run.kept().last().unwrap();
    let decoded = Checkpoint::decode_file(&kept.checkpoint().encode(), u64::MAX).unwrap();
    (run, decoded)
}

fn restored(deck: &Deck, c: &Checkpoint) -> circuit::Host {
    Run::from_checkpoint(
        host::engine(),
        host::admission(),
        &mut florn_platform::FixedPlatform::default(),
        &circuit::domains(),
        deck.manifest(),
        &BTreeMap::new(),
        c,
        options(),
    )
    .unwrap()
}

#[test]
fn a_checkpoint_restores_the_plant_and_the_run_continues_to_the_same_digest() {
    // Before the change, on the change's own instant (a breakpoint pending), and after it: the
    // plant's whole state is in the checkpoint (RFC 012 rule 29).
    let deck = rc_step_with_change();
    let mut reference = circuit::try_start_in(&circuit::domains(), &deck, options()).unwrap();
    reference.run_to_end();
    for tick in [1_000_000, 2_000_000, 3_050_000] {
        let (_, c) = checkpoint_at(&deck, tick);
        let mut b = restored(&deck, &c);
        b.run_to_end();
        assert_eq!(b.chain(), reference.chain(), "restored at {tick}");
        assert_eq!(
            b.kernel().trace.records(),
            reference.kernel().trace.records()
        );
    }
}

#[test]
fn the_state_of_the_plant_is_what_the_world_inspector_shows_after_a_restore() {
    let deck = rc_step_with_change();
    let (a, c) = checkpoint_at(&deck, 2_000_000);
    let b = restored(&deck, &c);
    let world = florn_model::manifest::WORLD_IDENTITY_BASE + 1;
    let show = |r: &circuit::Host| {
        let i = r.inspect(world).expect("the world");
        i.fields
            .iter()
            .map(|f| format!("{}={:?}", f.name, f.value))
            .collect::<Vec<_>>()
    };
    assert_eq!(show(&a), show(&b));
}

#[test]
fn checkpoints_and_inspectors_are_observation_the_digest_is_that_of_a_run_without() {
    let deck = rc_step_with_change();
    let mut plain = circuit::try_start_in(&circuit::domains(), &deck, options()).unwrap();
    plain.run_to_end();
    let mut watched = circuit::try_start_in(
        &circuit::domains(),
        &deck,
        Options {
            checkpoint_interval: 1,
            ..options()
        },
    )
    .unwrap();
    while watched.ended().is_none() {
        watched.advance(Advance::OneInstant);
        // Every inspector open at every boundary: the world, a net, a signal-less element.
        for id in [florn_model::manifest::WORLD_IDENTITY_BASE + 1, 100, 101, 4] {
            let _ = watched.inspect(id);
        }
        let _ = watched.request_checkpoint();
    }
    assert_eq!(watched.chain(), plain.chain());
    assert_eq!(
        watched.kernel().trace.records(),
        plain.kernel().trace.records()
    );
}

use florn_control::Severity;
use florn_model::manifest::DomainValue;

const WORLD: u32 = florn_model::manifest::WORLD_IDENTITY_BASE + 1;

fn world_field(run: &circuit::Host, name: &str) -> f64 {
    let i = run.inspect(WORLD).expect("the world");
    for f in &i.fields {
        if f.name == name
            && let florn_control::message::InspectValue::Real { value, .. } = f.value
        {
            return value;
        }
    }
    panic!("no {name} in {i:?}")
}

fn map(entries: &[(&str, DomainValue)]) -> Vec<u8> {
    let m: florn_model::manifest::DomainMap = entries
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    let mut e = florn_model::canonical::Encoder::default();
    florn_model::canonical::domain_map(&mut e, &m);
    e.finish()
}

#[test]
fn two_changes_of_one_input_at_one_tick_leave_one_breakpoint_and_an_equal_value_none() {
    // RFC 012 rule 9 and section 10: at 2.05 ms (between two samples) two changes of the source;
    // before, a change to the value it already holds. After the instant of the changes the pending
    // advance is at 2.1 ms, so the breakpoint is still pending.
    let deck = rc_step()
        .change("1ms", "world:circuit", "actor = 1\nv = \"1V\"")
        .change("2050us", "world:circuit", "actor = 1\nv = \"0.5V\"")
        .change("2050us", "world:circuit", "actor = 1\nv = \"0V\"");
    let mut run = circuit::start(&deck);
    run.advance(Advance::ToTick(1_000_000));
    while run.kernel().now().tick < 1_000_000
        || run.kernel().has_next() && run.kernel().now().tick == 1_000_000
    {
        if run.kernel().now().tick > 1_000_000 {
            break;
        }
        run.advance(Advance::OneInstant);
    }
    assert_eq!(
        world_field(&run, "pending-breakpoints"),
        0.0,
        "1 V is the value it holds"
    );
    while run.kernel().now().tick < 2_050_000 {
        run.advance(Advance::OneInstant);
    }
    assert_eq!(world_field(&run, "pending-breakpoints"), 1.0);
}

#[test]
fn a_source_intervention_between_samples_acts_from_its_tick() {
    let mut run = circuit::start(&rc_step());
    run.advance(Advance::ToTick(1_000_000));
    let content = map(&[
        ("kind", DomainValue::Text("source".into())),
        ("actor", DomainValue::Integer(1)),
        ("v", DomainValue::Text("0V".into())),
    ]);
    run.intervene_map(Some(2_050_000), WORLD, &content).unwrap();
    run.run_to_end();
    let s = probe_values(&run, 4);
    let at = |t: u64| s.iter().find(|x| x.0 == t).unwrap().1;
    // The samples at 2.0 ms and 2.1 ms: the source went to 0 V at 2.05 ms, so the step the plant
    // takes from 2.0 ms still charges and the sample at 2.1 ms shows 50 us of discharge only.
    let charge = |t: f64| 1.0 - (-t / 1e-3_f64).exp();
    let at_change = charge(2.05e-3);
    let want = at_change * (-(0.05e-3_f64) / 1e-3).exp();
    assert!(
        (at(2_100_000) - want).abs() <= 4e-3,
        "{} against {want}",
        at(2_100_000)
    );
    assert!(at(2_000_000) < at_change);
}

#[test]
fn a_scheduled_change_of_the_wrong_kind_or_dimension_is_refused_naming_the_field() {
    let refused = |payload: &str| {
        let deck = rc_step().change("2ms", "world:circuit", payload);
        let notes = circuit::refusals(circuit::try_start(&deck));
        notes.into_iter().map(|n| n.code).collect::<Vec<_>>()
    };
    assert_eq!(refused("actor = 99\nv = \"1V\""), ["reference.unresolved"]);
    assert_eq!(
        refused("actor = 2\nr = \"1kohm\""),
        ["circuit.param-unknown"]
    );
    assert_eq!(refused("actor = 1\nv = \"1A\""), ["quantity.dimension"]);
    assert_eq!(refused("actor = 1\nv = 3"), ["quantity.missing-unit"]);
    assert!(refused("actor = 1\nv = \"-2V\"").is_empty());
}

#[test]
fn more_pending_breakpoints_than_the_bound_end_the_run_with_limit_exceeded() {
    // `max-breakpoints = 4` and changes every microsecond against a sensor period of 1 ms: the
    // fifth is pending before the next advance (RFC 012 rule 9).
    let mut deck = rc_step().profile("max-breakpoints = 4");
    for k in 1..=8u64 {
        let v = if k % 2 == 0 { "1V" } else { "0.5V" };
        deck = deck.change(
            &format!("{}us", 2000 + k),
            "world:circuit",
            &format!("actor = 1\nv = \"{v}\""),
        );
    }
    let mut run = circuit::start(&deck);
    run.run_to_end();
    assert_eq!(host::ended(&run), florn_host::EndReason::HostFault);
    let n = run
        .log()
        .entries()
        .iter()
        .find(|e| e.notification.code == "run.limit-exceeded")
        .expect("the limit is reported");
    assert!(format!("{:?}", n.notification.params).contains("pending-breakpoints"));
    assert_eq!(n.notification.severity, Severity::Error);
}

/// A diode circuit that Newton cannot solve in three iterations once the source goes to 5 V.
fn hostile(profile: &str) -> Deck {
    Deck::new("hostile", "1ms")
        .profile(&format!("newton.max-iterations = 3\n{profile}"))
        .part(1, "v1", "voltage-source", &[], "v = \"0V\"")
        .part(2, "r1", "resistor", &[], "r = \"1kohm\"")
        .part(3, "d1", "diode", &[], "")
        .part(4, "vm", "voltmeter", &[], "period = \"50us\"")
        .net("top", &["v1.p", "r1.a"])
        .net("a", &["r1.b", "d1.a", "vm.p"])
        .ground("g", &["v1.n", "d1.k", "vm.n"])
        .change("100us", "world:circuit", "actor = 1\nv = \"5V\"")
}

#[test]
fn non_convergence_under_stop_run_ends_the_run_and_says_where() {
    let mut run = circuit::start(&hostile(""));
    run.run_to_end();
    assert_eq!(host::ended(&run), florn_host::EndReason::NonConvergence);
    let codes = run.log().codes();
    assert_eq!(
        codes
            .iter()
            .filter(|c| **c == "circuit.non-convergence")
            .count(),
        1
    );
    assert!(codes.contains(&"circuit.step-rejected"), "{codes:?}");
    // The notification counts the rejected attempts of the advance, as the inspection does.
    let said = run
        .log()
        .entries()
        .iter()
        .find(|e| e.notification.code == "circuit.step-rejected")
        .map(|e| format!("{:?}", e.notification.params))
        .unwrap();
    let rejected = world_field(&run, "rejected") as u64;
    assert!(
        rejected > 0 && said.contains(&format!("Count({rejected})")),
        "{said} against {rejected}"
    );
}

#[test]
fn non_convergence_under_hold_freezes_the_plant_and_the_sensors_keep_sampling() {
    // A second change at 120 us is still pending when the solver gives up at 100 us: held drops it.
    let deck = hostile("on-non-convergence = \"hold\"").change(
        "120us",
        "world:circuit",
        "actor = 1\nv = \"4V\"",
    );
    let mut run = circuit::start(&deck);
    run.run_to_end();
    assert_eq!(host::ended(&run), florn_host::EndReason::EndReached);
    let codes = run.log().codes();
    assert_eq!(
        codes
            .iter()
            .filter(|c| **c == "circuit.non-convergence")
            .count(),
        1,
        "said once"
    );
    let s = probe_values(&run, 4);
    assert_eq!(s.len(), 20, "every 50 us to 1 ms, held or not");
    let last = s.last().unwrap().1;
    assert!(
        s.iter().rev().take(10).all(|x| x.1 == last),
        "the frozen state: {s:?}"
    );
    assert_eq!(world_field(&run, "pending-breakpoints"), 0.0);
}

#[test]
fn an_advance_leaves_no_record_and_the_sensor_samples_are_the_only_activations() {
    let mut run = circuit::start(&rc_step_with_change());
    run.run_to_end();
    let records = run.records().unwrap();
    let world = florn_model::manifest::WORLD_IDENTITY_BASE + 1;
    assert!(
        records.iter().all(|r| match r {
            florn_kernel::Record::Handled { target, .. } => *target != world,
            _ => true,
        }),
        "no record has the world as its target"
    );
}

use florn_conformance::guests;
use florn_control::message::InspectValue;

/// `rc-step` with a controller module reading the voltmeter's signal (a stub of the core world
/// that carries the `[circuit]` table: the controller's own world is PR-47's).
fn observed_signal() -> Deck {
    rc_step()
        .guest(
            20,
            "ctl",
            "ctl",
            guests::circuit_stub("controller", ""),
            &[("in", "circuit/signal-in")],
        )
        .signal("volts", &["vm.v", "ctl.in"], "")
}

fn held_of(run: &circuit::Host, signal: u32) -> (f64, String) {
    let i = run.inspect(signal).expect("the signal");
    let mut out = (f64::NAN, String::new());
    for f in &i.fields {
        match (&f.name[..], &f.value) {
            ("held", InspectValue::Real { value, .. }) => out.0 = *value,
            ("frozen", InspectValue::Text(t)) => out.1 = t.clone(),
            _ => {}
        }
    }
    out
}

#[test]
fn a_frozen_signal_ignores_its_producer_and_on_release_keeps_the_old_value_until_the_next_sample() {
    // RFC 012 rule 15 and section 10 (Interventions). The signal is connection 103.
    let signal = Deck::connection_id(3);
    let mut run = circuit::start(&observed_signal());
    let until = |run: &mut circuit::Host, tick: u64| {
        while run.kernel().now().tick < tick {
            run.advance(Advance::OneInstant);
        }
    };
    until(&mut run, 1_000_050);
    let (before, frozen) = held_of(&run, signal);
    assert_eq!(frozen, "no");
    assert!(
        (before - analytic(1e-3, f64::INFINITY)).abs() <= 4e-3,
        "the 1 ms sample: {before}"
    );
    let freeze = |held: bool| {
        map(&[
            ("kind", DomainValue::Text("signal".into())),
            ("held", DomainValue::Boolean(held)),
        ])
    };
    run.intervene_map(Some(1_500_000), signal, &freeze(true))
        .unwrap();
    run.intervene_map(Some(3_050_000), signal, &freeze(false))
        .unwrap();
    until(&mut run, 2_500_050);
    let (frozen_value, frozen) = held_of(&run, signal);
    assert_eq!(frozen, "yes");
    assert_eq!(
        world_field(&run, "pending-breakpoints"),
        0.0,
        "a freeze records no breakpoint"
    );
    assert!(
        (frozen_value - analytic(1.4e-3, f64::INFINITY)).abs() <= 4e-3,
        "the 1.4 ms sample, the last before 1.5 ms: {frozen_value}"
    );
    assert_ne!(
        frozen_value.to_bits(),
        analytic(2.5e-3, f64::INFINITY).to_bits()
    );
    // Released at 3.05 ms: the signal keeps the frozen value until the producer next sets one.
    until(&mut run, 3_050_050);
    let (released, frozen) = held_of(&run, signal);
    assert_eq!(frozen, "no");
    assert_eq!(released.to_bits(), frozen_value.to_bits());
    until(&mut run, 3_100_050);
    let (next, _) = held_of(&run, signal);
    assert_ne!(
        next.to_bits(),
        frozen_value.to_bits(),
        "the 3.1 ms sample reaches the signal"
    );
}

#[test]
fn a_sensor_reports_and_sends_in_the_unit_of_its_signal() {
    // RFC 012 rule 5: one division by the unit's factor; the signal `mV` holds a thousand volts'
    // worth of the voltmeter's reading.
    let deck = rc_step()
        .guest(
            20,
            "ctl",
            "ctl",
            guests::circuit_stub("controller", ""),
            &[("in", "circuit/signal-in")],
        )
        .signal("volts", &["vm.v", "ctl.in"], "unit = \"mV\"");
    let mut run = circuit::start(&deck);
    run.advance(Advance::ToTick(1_100_000));
    let s = probe_values(&run, 4);
    let at_1ms = s.iter().find(|x| x.0 == 1_000_000).unwrap().1;
    assert!(
        (at_1ms - 1000.0 * analytic(1e-3, f64::INFINITY)).abs() <= 4.0,
        "{at_1ms}"
    );
    let sent = host::sends(&run, 4)
        .into_iter()
        .find(|s| s.0.tick == 1_000_000)
        .unwrap();
    assert_eq!(
        f64::from_le_bytes(sent.2.as_slice().try_into().unwrap()),
        at_1ms
    );
}

#[test]
fn a_breakpoint_at_the_tick_of_an_advance_stays_pending_until_the_next_advance() {
    // RFC 012 rule 9: the change at 2 ms comes before the advance at 2 ms in the instant; the
    // advance leaves it pending, and the state at `solved` is the limit from the left.
    let mut run = circuit::start(&rc_step_with_change());
    while run.kernel().now().tick < 2_000_000 {
        run.advance(Advance::OneInstant);
    }
    run.advance(Advance::OneInstant);
    assert_eq!(run.kernel().now().tick, 2_000_000);
    assert_eq!(world_field(&run, "pending-breakpoints"), 1.0);
    assert_eq!(world_field(&run, "solved"), 2_000_000.0);
}

#[test]
fn the_value_of_a_constant_is_the_held_value_of_its_signal_from_its_tick() {
    // RFC 012 rule 15: "for a `constant`, sets the held value of its signal".
    let deck = Deck::new("constant", "3ms")
        .part(
            1,
            "v1",
            "voltage-source",
            &["cmd"],
            "v = \"1V\"\ngain = \"2V\"",
        )
        .part(2, "k1", "constant", &[], "value = 1")
        .part(3, "r1", "resistor", &[], "r = \"1kohm\"")
        .part(4, "vm", "voltmeter", &[], "period = \"1ms\"")
        .net("a", &["v1.p", "r1.a", "vm.p"])
        .ground("g", &["v1.n", "r1.b", "vm.n"])
        .signal("cmd", &["k1.y", "v1.cmd"], "")
        .change("1500us", "world:circuit", "actor = 2\nvalue = 0.25");
    let mut run = circuit::start(&deck);
    assert_eq!(held_of(&run, Deck::connection_id(2)).0, 1.0);
    run.run_to_end();
    assert_eq!(held_of(&run, Deck::connection_id(2)).0, 0.25);
    // 1 V + 2 V × 0.25 on the source, seen from the step that starts at 1.5 ms: the sample at 2 ms.
    let s = probe_values(&run, 4);
    assert!((s.iter().find(|x| x.0 == 2_000_000).unwrap().1 - 1.5).abs() < 1e-6);
    assert!((s.iter().find(|x| x.0 == 1_000_000).unwrap().1 - 3.0).abs() < 1e-6);
}
