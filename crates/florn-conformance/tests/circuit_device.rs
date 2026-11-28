//! PR-49 (checklist B-49): the guest device model. RFC 012 rules 12, 17, 19 to 21, 25 and 27; RFC
//! 013 rules 8, 9, 16 and 17. `guest-diode` against `diode-clip`; each mode of `bad-device`; the
//! records of the compound activation; the initial solve; the one-device limit; a checkpoint across
//! the loss of the device. The solver's own calls, with a native model in place of the guest, are
//! `florn-domain-circuit`'s `transient::tests::device`.
use florn_conformance::circuit::refusals;
use florn_conformance::circuit_scenarios::{
    diode_clip, guest_diode, hostile_diverge, hostile_stamp, hostile_text,
};
use florn_conformance::host;
use florn_conformance::scenario::Scenario;
use florn_domain_circuit::CHANNEL_PROBE;
use florn_host::{Advance, Checkpoint, EndReason, Options, Run};
use florn_kernel::FaultKind;
use florn_kernel::trace::Record;

fn f64s(bytes: &[u8]) -> Vec<f64> {
    bytes
        .chunks(8)
        .map(|c| f64::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

/// The voltmeter's samples (actor 4): (tick, volts).
fn samples(run: &host::Host) -> Vec<(u64, f64)> {
    host::reports(run, 4, CHANNEL_PROBE)
        .into_iter()
        .map(|(at, b)| (at.tick, f64s(&b)[0]))
        .collect()
}

/// The device's (actor 3) records: (key, number of effects).
fn device_records(run: &host::Host) -> Vec<(florn_kernel::EventKey, usize)> {
    run.records()
        .expect("records kept")
        .iter()
        .filter_map(|r| match r {
            Record::Handled {
                key,
                target: 3,
                effects,
                ..
            } => Some((*key, effects.len())),
            _ => None,
        })
        .collect()
}

/// The codes of the log, in order.
fn codes(run: &host::Host) -> Vec<String> {
    run.log()
        .entries()
        .iter()
        .map(|e| e.notification.code.clone())
        .collect()
}

fn count(run: &host::Host, code: &str) -> usize {
    codes(run).iter().filter(|c| *c == code).count()
}

/// `hostile` with the device config and profile given, over the modules of a fresh build.
fn variant(config: &str, profile: &str) -> Scenario {
    Scenario::raw(
        "hostile-variant",
        1_000_000,
        hostile_text("hostile-variant", 1_000_000, config, profile),
        &["bad_device"],
    )
}

// --- `guest-diode` against `diode-clip` ---------------------------------------------------------

#[test]
fn guest_diode_and_the_built_in_diode_agree_on_the_voltmeters_samples() {
    let (guest, builtin) = (host::run(&guest_diode()), host::run(&diode_clip()));
    assert_eq!(host::ended(&guest), EndReason::EndReached);
    assert_eq!(host::ended(&builtin), EndReason::EndReached);
    assert!(host::faults(&guest).is_empty());
    let (a, b) = (samples(&guest), samples(&builtin));
    assert_eq!(a.len(), 160, "a sample every 10 us to 1.6 ms");
    assert_eq!(a.len(), b.len());
    // The profile's tolerance (RFC 013 rule 20: atol 1e-9, rtol 1e-6), pair by pair.
    let mut worst = 0.0f64;
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(x.0, y.0, "the same ticks");
        let allowed = 1e-9 + 1e-6 * x.1.abs().max(y.1.abs());
        worst = worst.max((x.1 - y.1).abs());
        assert!(
            (x.1 - y.1).abs() <= allowed,
            "at {}: {} against {}",
            x.0,
            x.1,
            y.1
        );
    }
    // The plant is the diode clip: forward near 0.6 V, reverse at −5 V... through the resistor, the
    // voltmeter across the diode reads the junction.
    let forward = a.iter().find(|s| s.0 == 100_000).unwrap().1;
    assert!((0.5..0.8).contains(&forward), "{forward}");
    let reverse = a.iter().find(|s| s.0 == 600_000).unwrap().1;
    assert!(reverse < -4.9, "{reverse}");
    // Reported, not required: the figures are bit-identical where the iterations are the same.
    println!("the largest difference of the two voltmeters: {worst:e} V");
}

#[test]
fn the_device_leaves_one_record_per_accepted_step_keyed_by_the_advance_with_the_device_as_target() {
    let run = host::run(&guest_diode());
    let records = device_records(&run);
    // The `init` of the guest and the initial solve at tick 0 (the key of its own `init`), then one
    // record per accepted step, each with the key of the advance that caused it.
    assert_eq!(records[0].0.tick, 0);
    assert_eq!(records[0].1, 0, "the guest's own init: no effect");
    assert_eq!(
        records[1].0, records[0].0,
        "the initial solve is keyed as the device's init"
    );
    assert_eq!(records[1].1, 1, "and reports once, at the accept");
    assert_eq!(records.iter().filter(|r| r.0.tick == 0).count(), 2);
    let steps: Vec<_> = records.iter().skip(2).collect();
    assert!(steps.len() > 160, "{} accepted steps", steps.len());
    for (key, effects) in &steps {
        assert_eq!(key.class, 6, "the class of `advance`");
        assert_eq!(key.microstep, 0);
        assert_eq!(*effects, 1, "one report per accepted step");
    }
    // Several accepted steps of one advance share the advance's key, in the order of the steps.
    let first = steps[0].0;
    assert!(steps.iter().filter(|s| s.0 == first).count() > 1);
}

#[test]
fn the_device_reports_the_state_of_the_accepted_steps_only() {
    // The voltmeter at a tick reads the state at `solved`, the end of the last accepted step; the
    // device's last report of that advance is the junction voltage of the same step. A step the
    // solver rejects reports nothing, so the two agree at every sample.
    let run = host::run(&guest_diode());
    let sample: std::collections::BTreeMap<u64, f64> = samples(&run).into_iter().collect();
    let mut last_v = std::collections::BTreeMap::new();
    for (at, b) in host::reports(&run, 3, 1) {
        last_v.insert(at.tick, f64s(&b)[0]);
    }
    let mut checked = 0;
    for (tick, v) in &sample {
        if let Some(d) = last_v.get(tick) {
            assert_eq!(d.to_bits(), v.to_bits(), "at {tick}");
            checked += 1;
        }
    }
    assert!(
        checked >= 150,
        "{checked} samples had a report at their tick"
    );
}

#[test]
fn a_rejected_step_leaves_what_the_guest_reported_as_it_was() {
    // The diode with a capacitor and an error tolerance that rejects attempts: the guest's reports
    // are the accepted steps' and the voltmeter reads the same junction at every sample.
    let mut text = guest_text();
    text = text.replace(
        "[[connections]]\nid = 101\nname = \"mid\"\ntype = \"circuit/net\"\nports = [\"r.b\", \"d.a\", \"vm.p\"]",
        "[[connections]]\nid = 101\nname = \"mid\"\ntype = \"circuit/net\"\nports = [\"r.b\", \"d.a\", \"vm.p\", \"c.a\"]",
    );
    text = text.replace(
        "ports = [\"vs.n\", \"d.k\", \"vm.n\"]",
        "ports = [\"vs.n\", \"d.k\", \"vm.n\", \"c.b\"]",
    );
    text = text.replace(
        "\n[[actors]]\nid = 4",
        "\n[[actors]]\nid = 5\nname = \"c\"\nkind = \"builtin\"\nbuiltin = \"circuit/capacitor\"\nports = [{ id = 0, name = \"a\", type = \"circuit/pin\" }, { id = 1, name = \"b\", type = \"circuit/pin\" }]\n[actors.config]\nc = \"10nF\"\n\n[[actors]]\nid = 4",
    );
    text = text.replace(
        "[limits.default]",
        "[circuit]\nltol = 1e-6\n\n[limits.default]",
    );
    let s = Scenario::raw("guest-diode-rc", 1_600_000, text, &["guest_diode"]);
    let run = host::run(&s);
    assert_eq!(host::ended(&run), EndReason::EndReached);
    assert!(host::faults(&run).is_empty());
    assert!(
        count(&run, "circuit.step-rejected") > 0,
        "the error test rejects attempts: {:?}",
        codes(&run)
    );
    let sample: std::collections::BTreeMap<u64, f64> = samples(&run).into_iter().collect();
    let mut last_v = std::collections::BTreeMap::new();
    for (at, b) in host::reports(&run, 3, 1) {
        last_v.insert(at.tick, f64s(&b)[0]);
    }
    let mut checked = 0;
    for (tick, v) in &sample {
        if let Some(d) = last_v.get(tick) {
            assert_eq!(d.to_bits(), v.to_bits(), "at {tick}");
            checked += 1;
        }
    }
    assert!(checked >= 100, "{checked}");
}

/// The text of `guest-diode`, for the variants of a test.
fn guest_text() -> String {
    guest_diode().raw.expect("a text scenario").0
}

// --- the contract violations and the loss of the device ---------------------------------------------

/// What a run of a hostile device came to: the fault, the notifications, the plant's voltage.
struct Lost {
    run: host::Host,
}

impl Lost {
    fn of(config: &str, profile: &str) -> Self {
        Self {
            run: host::run(&variant(config, profile)),
        }
    }
    fn fault(&self) -> (FaultKind, u32) {
        let f = host::faults(&self.run);
        assert_eq!(f.len(), 1, "one Fault record: {f:?}");
        assert_eq!(f[0].0, 3, "the device's");
        (f[0].1, f[0].2)
    }
}

#[test]
fn each_result_that_is_no_stamp_is_a_contract_violation_with_the_parameter_12020() {
    for (mode, why) in [
        ("nan", "not finite"),
        ("short", "n + n² + 1"),
        ("long", "longer"),
        ("flag", "neither 0 nor 1"),
    ] {
        let l = Lost::of(&format!("mode = \"{mode}\"\nat = 5"), "");
        assert_eq!(l.fault(), (FaultKind::Contract, 12_020), "{mode}");
        assert_eq!(
            host::ended(&l.run),
            EndReason::EndReached,
            "{mode}: the plant continues"
        );
        // `circuit.stamp-invalid` says how, `circuit.device-faulted` says once that it is gone.
        assert_eq!(count(&l.run, "circuit.stamp-invalid"), 1, "{mode}");
        assert_eq!(count(&l.run, "circuit.device-faulted"), 1, "{mode}");
        let reason = host::log_param(&l.run, "circuit.stamp-invalid", "reason").unwrap();
        assert!(reason.contains(why), "{mode}: {reason}");
        assert_eq!(
            host::log_param(&l.run, "circuit.stamp-invalid", "actor").as_deref(),
            Some("Name(\"d\")"),
            "{mode}"
        );
    }
}

#[test]
fn the_fault_record_is_keyed_by_the_advance_and_the_device_is_dead_after_it() {
    let l = Lost::of("mode = \"nan\"\nat = 5", "");
    let faulted_at = l
        .run
        .records()
        .unwrap()
        .iter()
        .find_map(|r| match r {
            Record::Fault { key, actor: 3, .. } => Some(*key),
            _ => None,
        })
        .expect("a Fault record");
    assert_eq!(faulted_at.class, 6, "the class of `advance`");
    assert_eq!(faulted_at.microstep, 0);
    // The accepted steps before it left their records, with the same key; none after.
    let device = device_records(&l.run);
    assert!(device.iter().any(|r| r.0 == faulted_at && r.1 == 1));
    assert!(
        device.iter().all(|r| r.0 <= faulted_at),
        "the device is not activated after its fault"
    );
    // The plant continues without it: the node is the source's voltage (nothing loads it), where
    // it was 2.5 V through the device's 1 mS and the resistor's 1 kohm.
    let s = samples(&l.run);
    assert!((s.iter().find(|x| x.0 == 0).unwrap().1 - 2.5).abs() < 1e-6);
    assert_eq!(s.len(), 10, "the sensors keep sampling to the end");
    assert!((s.last().unwrap().1 - 5.0).abs() < 1e-6, "{:?}", s.last());
}

#[test]
fn a_timer_and_a_send_in_a_device_are_the_contract_violation_of_rule_19() {
    for mode in ["timer", "send"] {
        let l = Lost::of(&format!("mode = \"{mode}\"\nat = 5"), "");
        assert_eq!(l.fault(), (FaultKind::Contract, 12_019), "{mode}");
        // A fault of the guest's, not a result the domain judged: no `circuit.stamp-invalid`.
        assert_eq!(count(&l.run, "circuit.stamp-invalid"), 0, "{mode}");
        assert_eq!(count(&l.run, "circuit.device-faulted"), 1, "{mode}");
        assert_eq!(host::ended(&l.run), EndReason::EndReached, "{mode}");
    }
}

#[test]
fn a_trap_in_a_device_is_a_fault_and_the_plant_goes_on_without_it() {
    let l = Lost::of("mode = \"trap\"\nat = 5", "");
    assert_eq!(l.fault().0, FaultKind::Trap);
    assert_eq!(count(&l.run, "circuit.device-faulted"), 1);
    assert_eq!(count(&l.run, "circuit.stamp-invalid"), 0);
    assert_eq!(host::ended(&l.run), EndReason::EndReached);
}

#[test]
fn a_fault_in_the_initial_solve_is_keyed_as_the_devices_init_and_the_solve_is_made_again() {
    // The first evaluation of the run is the initial solve's.
    let l = Lost::of("mode = \"nan\"\nat = 1", "");
    assert_eq!(l.fault(), (FaultKind::Contract, 12_020));
    let key = l
        .run
        .records()
        .unwrap()
        .iter()
        .find_map(|r| match r {
            Record::Fault { key, actor: 3, .. } => Some(*key),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        (key.tick, key.microstep, key.class, key.origin, key.seq),
        (0, 0, 0, 3, 0)
    );
    // The run starts, without the device: the node is the source's voltage from the first sample.
    let s = samples(&l.run);
    assert!((s[0].1 - 5.0).abs() < 1e-6, "{:?}", s[0]);
    assert_eq!(host::ended(&l.run), EndReason::EndReached);
}

#[test]
fn under_stop_run_a_device_fault_ends_the_run_with_actor_fault() {
    let l = Lost::of("mode = \"nan\"\nat = 5", "faulted-policy = \"stop-run\"");
    assert_eq!(host::ended(&l.run), EndReason::ActorFault);
    assert_eq!(l.fault(), (FaultKind::Contract, 12_020));
    // The advance is abandoned where it stands: no sample of the voltmeter after the fault's tick.
    assert!(samples(&l.run).iter().all(|s| s.0 <= 100_000));
}

#[test]
fn a_hostile_scenario_is_the_same_on_every_run() {
    for s in [hostile_stamp(), hostile_diverge()] {
        let (a, b) = (host::run(&s), host::run(&s));
        assert_eq!(a.chain_hex(), b.chain_hex(), "{}", s.name);
    }
}

// --- non-convergence ---------------------------------------------------------------------------

#[test]
fn stamps_that_never_converge_end_the_run_with_non_convergence_under_stop_run() {
    let run = host::run(&hostile_diverge());
    assert_eq!(host::ended(&run), EndReason::NonConvergence);
    assert_eq!(count(&run, "circuit.non-convergence"), 1);
    // The attempts of the failing step are bounded: `max-rejections`, plus the first (rule 12).
    let attempts: u64 = host::log_param(&run, "circuit.step-rejected", "attempts")
        .unwrap()
        .parse()
        .unwrap();
    assert!(attempts <= 61, "{attempts}");
    assert!(
        host::faults(&run).is_empty(),
        "the device did not fault: it never converged"
    );
}

#[test]
fn under_hold_the_plant_freezes_at_the_last_accepted_step_and_the_sensors_keep_sampling() {
    let s = Scenario::raw(
        "diverge-hold",
        1_000_000,
        hostile_text(
            "diverge-hold",
            1_000_000,
            "mode = \"diverge\"\nat = 3",
            "on-non-convergence = \"hold\"",
        ),
        &["bad_device"],
    );
    let run = host::run(&s);
    assert_eq!(host::ended(&run), EndReason::EndReached);
    assert_eq!(count(&run, "circuit.non-convergence"), 1, "said once");
    let v = samples(&run);
    assert_eq!(v.len(), 10);
    // Frozen at the operating point of tick 0: every sample is the same state.
    assert!(v.iter().all(|x| x.1.to_bits() == v[0].1.to_bits()), "{v:?}");
}

#[test]
fn a_device_that_reports_limiting_changes_nothing_the_solver_would_not_have_found() {
    // `limit` reports the flag at the first evaluation of every attempt and behaves as a
    // conductance. The first evaluation of a linear plant never converges anyway, so the plant is
    // the same as `good`'s; that a flag raised where the criteria hold costs one iteration is
    // `florn-domain-circuit`'s test of the solver with a device (RFC 013 rule 8).
    let (good, limiting) = (
        host::run(&variant("mode = \"good\"", "")),
        host::run(&variant("mode = \"limit\"", "")),
    );
    assert_eq!(samples(&good), samples(&limiting));
    assert!(host::faults(&limiting).is_empty());
}

// --- the one-device limit --------------------------------------------------------------------------

#[test]
fn two_devices_are_refused_with_the_device_limit_from_the_start_hook_before_any_event() {
    let text = guest_text().replace(
        "\n[[actors]]\nid = 4",
        "\n[[actors]]\nid = 5\nname = \"d2\"\nkind = \"module\"\nmodule = \"guest_diode\"\nports = [{ id = 0, name = \"a\", type = \"circuit/pin\" }, { id = 1, name = \"k\", type = \"circuit/pin\" }]\n\n[[actors]]\nid = 4",
    )
    .replace("ports = [\"r.b\", \"d.a\", \"vm.p\"]", "ports = [\"r.b\", \"d.a\", \"vm.p\", \"d2.a\"]")
    .replace("ports = [\"vs.n\", \"d.k\", \"vm.n\"]", "ports = [\"vs.n\", \"d.k\", \"vm.n\", \"d2.k\"]");
    let s = Scenario::raw("two-devices", 1_600_000, text, &["guest_diode"]);
    // The scenario validates (it is a plant of the domain's own kinds); the start hook reads the
    // guests and refuses.
    let started = host::try_start(&s, host::options());
    let notes = refusals(started);
    assert_eq!(
        notes.iter().map(|n| n.code.as_str()).collect::<Vec<_>>(),
        ["circuit.device-limit"]
    );
    // And the run never began: nothing is in a trace that was not started.
    assert!(try_start_is_refused(&s));
}

fn try_start_is_refused(s: &Scenario) -> bool {
    host::try_start(s, host::options()).is_err()
}

// --- a checkpoint across the loss of the device -------------------------------------------------

/// The chain of the run restored from a checkpoint at `at` and carried to the end, and how many
/// times the restored run said that the device was lost.
fn finish_from(s: &Scenario, at: u64) -> (String, usize) {
    let options = || Options {
        checkpoint_interval: 0,
        checkpoint_memory: u64::MAX,
        ..host::options()
    };
    let mut a = host::try_start(s, options()).expect("starts");
    a.advance(Advance::ToTick(at));
    assert!(
        a.request_checkpoint() || {
            a.advance(Advance::OneInstant);
            !a.kept().is_empty()
        }
    );
    let c =
        Checkpoint::decode_file(&a.kept().last().unwrap().checkpoint().encode(), u64::MAX).unwrap();
    let mut b = Run::from_checkpoint(
        host::engine(),
        host::admission(),
        &mut florn_platform::FixedPlatform::default(),
        &host::domains(),
        s.manifest(),
        &s.modules(),
        &c,
        options(),
    )
    .expect("restores");
    b.run_to_end();
    let said = b
        .log()
        .entries()
        .iter()
        .filter(|e| e.notification.code == "circuit.device-faulted")
        .count();
    (b.chain_hex(), said)
}

#[test]
fn a_run_restored_before_and_after_the_loss_of_the_device_continues_to_the_same_digest() {
    for s in [guest_diode(), hostile_stamp()] {
        let whole = host::run(&s).chain_hex();
        // `hostile-stamp` loses its device at 100 us: restore before (50 us), at the fault's tick
        // and after (500 us); `guest-diode` has its device throughout.
        for at in [50_000u64, 100_000, 500_000, 900_000] {
            assert_eq!(finish_from(&s, at).0, whole, "{} restored at {at}", s.name);
        }
    }
}

#[test]
fn a_run_restored_after_the_loss_does_not_have_the_device_back_and_does_not_say_it_again() {
    // The device was lost before the checkpoint: the restored plant has none (the state says so),
    // and the continuation never calls the dead actor, so nothing is lost again and nothing is said.
    let s = hostile_stamp();
    assert_eq!(finish_from(&s, 500_000).1, 0, "restored after the loss");
    assert_eq!(finish_from(&s, 900_000).1, 0, "restored after the loss");
    // Restored before the fault, the continuation loses it and says so once.
    assert_eq!(finish_from(&s, 50_000).1, 1, "restored before the loss");
}

#[test]
fn a_replay_of_a_run_with_a_device_begins_as_the_run_did_and_matches() {
    for s in [guest_diode(), hostile_stamp()] {
        let mut run = host::start(&s);
        run.run_to_end();
        let record = run.run_toml();
        let mut replay = Run::replay(
            host::engine(),
            host::admission(),
            &mut florn_platform::FixedPlatform::default(),
            &host::domains(),
            s.manifest(),
            &s.modules(),
            &record,
            host::options(),
        )
        .unwrap();
        replay.run_to_end();
        assert!(matches!(
            replay.comparison(),
            Some(florn_kernel::trace::Comparison::Matched { .. })
        ));
        assert_eq!(replay.chain_hex(), run.chain_hex(), "{}", s.name);
    }
}

// --- the post-return (finding F-16) ----------------------------------------------------------------

#[test]
fn a_device_called_tens_of_thousands_of_times_leaves_no_result_buffer_in_the_guests_memory() {
    // Every `evaluate` returns a list the guest allocates; the Host calls `cabi_post_evaluate` after
    // lifting it. Without that the buffers would stay until the 2 MiB limit of the scenario traps
    // the device: 3 s of 100 us samples are 30,000 advances and more than 60,000 evaluations of 7
    // values.
    let s = Scenario::raw(
        "device-long",
        3_000_000_000,
        hostile_text("device-long", 3_000_000_000, "mode = \"good\"", ""),
        &["bad_device"],
    );
    let run = host::run(&s);
    assert_eq!(host::ended(&run), EndReason::EndReached);
    assert!(host::faults(&run).is_empty(), "{:?}", host::faults(&run));
    let evaluations = device_records(&run).len();
    assert!(
        evaluations > 30_000,
        "{evaluations} activations of the device"
    );
}

// --- the domain's view of its device ----------------------------------------------------------------

#[test]
fn the_device_is_of_a_continuous_class_and_the_domain_knows_it_from_the_guests_of_the_start_hook() {
    // RFC 013 rule 20, item 4: the consumed work and the draws of the guest device are exempt from
    // the comparison within a tolerance, as a controller's and a sensor's are.
    use florn_admission::ClassLimits;
    use florn_host::compound::NoCalls;
    use florn_host::domain::{Domain, Guest};
    let s = guest_diode();
    let bytes = s.modules().remove("guest_diode").unwrap();
    let admitted = host::admission()
        .admit_under("guest_diode", &bytes, &ClassLimits::defaults(), 1)
        .expect("guest_diode is admitted");
    let guest = Guest {
        actor: 3,
        module: "guest_diode".into(),
        admitted,
    };
    let mut d = florn_domain_circuit::CircuitDomain::new(florn_domain_circuit::Limits::native());
    d.compose(&s.manifest(), 1);
    assert!(
        !d.continuous_actor(3),
        "not before the start hook has read the guests"
    );
    // A handle that refuses every call: the device is absent from the start, which leaves the
    // plant a plant without one; the guest is a device all the same.
    d.start_calling(&mut NoCalls, &[guest])
        .expect("the plant starts");
    assert!(d.continuous_actor(3));
    assert!(d.continuous_actor(4), "the voltmeter is a sensor");
    assert!(!d.continuous_actor(1), "a source is no continuous actor");
    assert_eq!(d.guests().device(), Some(3));
}
