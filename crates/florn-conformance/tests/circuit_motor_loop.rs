//! PR-47 (checklist B-47): the sampled controller and `motor-loop`, the closed loop of the exit
//! criterion, headless. RFC 012 rules 10, 11, 16 to 18, 25 and 26; RFC 014 rule 3. The digest of
//! `motor-loop` under both Hosts is the shipped scenarios' test (`ladder`) and the browser kit's;
//! here are the loop, the order of samples, the held signals and the faulted controller.
use florn_conformance::circuit_scenarios::{motor_loop, motor_loop_text};
use florn_conformance::host;
use florn_conformance::scenario::Scenario;
use florn_control::Severity;
use florn_domain_circuit::CHANNEL_PROBE;
use florn_host::EndReason;
use florn_kernel::FaultKind;

/// `motor-loop` with the text changed by `change`, over the modules of a fresh build.
fn variant(modules: &'static [&'static str], change: impl FnOnce(String) -> String) -> Scenario {
    Scenario::raw(
        "motor-loop-variant",
        1_500_000_000,
        change(motor_loop_text()),
        modules,
    )
}

fn f64s(bytes: &[u8]) -> Vec<f64> {
    bytes
        .chunks(8)
        .map(|c| f64::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

/// The motor's `speed` samples: (tick, rad/s).
fn speeds(run: &host::Host) -> Vec<(u64, f64)> {
    host::reports(run, 2, CHANNEL_PROBE)
        .into_iter()
        .map(|(at, b)| (at.tick, f64s(&b)[0]))
        .collect()
}

/// A controller's `control` reports on `channel`: (tick, input, output, error).
fn controls(run: &host::Host, actor: u32, channel: u32) -> Vec<(u64, f64, f64, f64)> {
    host::reports(run, actor, channel)
        .into_iter()
        .map(|(at, b)| {
            let v = f64s(&b);
            (at.tick, v[0], v[1], v[2])
        })
        .collect()
}

fn at(s: &[(u64, f64)], ms: u64) -> f64 {
    s.iter().find(|x| x.0 == ms * 1_000_000).unwrap().1
}

#[test]
fn the_loop_settles_to_the_set_point_and_recovers_from_the_load_step() {
    let run = host::run(&motor_loop());
    assert_eq!(host::ended(&run), EndReason::EndReached);
    assert!(host::faults(&run).is_empty());
    let s = speeds(&run);
    assert_eq!(s.len(), 1500, "a sample every millisecond");
    // Settled well before the load step at one second.
    for ms in [800, 900, 999] {
        assert!((at(&s, ms) - 100.0).abs() < 2.0, "{ms} ms: {}", at(&s, ms));
    }
    // The load step at 1 s pulls the speed down and the integral brings it back.
    let dip = (1000..1100)
        .map(|ms| at(&s, ms))
        .fold(f64::INFINITY, f64::min);
    assert!(dip < 99.9 && dip > 90.0, "the dip: {dip}");
    for ms in [1400, 1499] {
        assert!((at(&s, ms) - 100.0).abs() < 0.5, "{ms} ms: {}", at(&s, ms));
    }
}

#[test]
fn a_controller_with_phase_zero_reads_the_previous_sample_and_with_phase_one_the_current() {
    // RFC 012 rule 10 and section 10 (Sample order). The motor's sensor samples at k ms; the
    // controller at k ms + phase. At a tick where both sample the controller (class 7, microstep 0)
    // runs before the sensor's sample (microstep 1): it reads the previous one.
    let run = host::run(&motor_loop());
    let (s, c) = (speeds(&run), controls(&run, 5, 1));
    // phase = 1 ns: the controller's k-th sample is one tick after the sensor's k-th.
    for k in 1..50 {
        assert_eq!(c[k].0, k as u64 * 1_000_000 + 1);
        assert_eq!(
            c[k].1, s[k].1,
            "reads the sensor's sample of the same millisecond"
        );
    }
    let zero = host::run(&variant(&["pid"], |t| {
        t.replace("phase = \"1ns\"", "phase = \"0ns\"")
    }));
    let (s, c) = (speeds(&zero), controls(&zero, 5, 1));
    for k in 1..50 {
        assert_eq!(c[k].0, k as u64 * 1_000_000);
        assert_eq!(c[k].1, s[k - 1].1, "reads the previous sample");
    }
}

#[test]
fn a_controller_keeps_running_under_a_frozen_signal_and_reads_the_frozen_value() {
    // RFC 012 rule 15: the `speed` signal (connection 102) frozen at 500 ms: the sensor's samples
    // are ignored, the controller keeps sampling and reads the frozen value.
    let mut run = host::start(&motor_loop());
    run.advance(florn_host::Advance::ToTick(500_000_000));
    let content = {
        let m: florn_model::manifest::DomainMap = [
            (
                "kind".to_string(),
                florn_model::manifest::DomainValue::Text("signal".into()),
            ),
            (
                "held".to_string(),
                florn_model::manifest::DomainValue::Boolean(true),
            ),
        ]
        .into_iter()
        .collect();
        let mut e = florn_model::canonical::Encoder::default();
        florn_model::canonical::domain_map(&mut e, &m);
        e.finish()
    };
    run.intervene_map(Some(500_000_000), 102, &content).unwrap();
    run.run_to_end();
    let c = controls(&run, 5, 1);
    let after: Vec<_> = c.iter().filter(|x| x.0 > 501_000_000).collect();
    assert!(after.len() > 900, "the controller samples to the end");
    assert!(
        after.iter().all(|x| x.1 == after[0].1),
        "it reads one value for ever"
    );
    // The plant went on under that controller: the speed no longer follows the set point.
    let s = speeds(&run);
    assert!(
        s.iter()
            .any(|x| x.0 > 1_400_000_000 && (x.1 - 100.0).abs() > 0.5)
    );
}

#[test]
fn a_set_output_of_the_value_already_held_records_no_breakpoint() {
    // RFC 012 rule 11 and B-46: a controller that sets 0.3 at every sample sets a change at the
    // first and nothing after: from the second millisecond no breakpoint is ever pending.
    let mut run = host::start(&faulty("value = 0.3", ""));
    let world = florn_model::manifest::WORLD_IDENTITY_BASE + 1;
    let pending = |run: &host::Host| {
        let i = run.inspect(world).expect("the world");
        i.fields
            .iter()
            .find_map(|f| match (f.name.as_str(), &f.value) {
                (
                    "pending-breakpoints",
                    florn_control::message::InspectValue::Real { value, .. },
                ) => Some(*value),
                _ => None,
            })
            .unwrap()
    };
    let mut seen_one = false;
    while run.kernel().now().tick < 20_000_000 {
        run.advance(florn_host::Advance::OneInstant);
        if run.kernel().now().tick < 1_000_000 {
            seen_one |= pending(&run) == 1.0;
        } else {
            assert_eq!(pending(&run), 0.0, "at {}", run.kernel().now().tick);
        }
    }
    assert!(seen_one, "the first sample's change was a breakpoint");
}

#[test]
fn a_controller_that_traps_keeps_its_last_outputs_held_and_the_run_goes_on() {
    let run = host::run(&faulty("trap-at = 5\nvalue = 0.5", ""));
    assert_eq!(host::ended(&run), EndReason::EndReached);
    let faults = host::faults(&run);
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!((faults[0].0, faults[0].1), (5, FaultKind::Trap));
    // Four samples before the trap, and none after: it is not sampled again (rule 17).
    assert_eq!(controls(&run, 5, 1).len(), 4);
    // The command it set stays held: the motor runs at the speed 0.5 × 12 V gives, to the end.
    let s = speeds(&run);
    let late = at(&s, 1490);
    assert!(late > 80.0 && (at(&s, 1400) - late).abs() < 1.5, "{late}");
}

#[test]
fn under_stop_run_a_faulted_controller_ends_the_run_with_actor_fault() {
    let run = host::run(&faulty(
        "trap-at = 5\nvalue = 0.5",
        "faulted-policy = \"stop-run\"",
    ));
    assert_eq!(host::ended(&run), EndReason::ActorFault);
}

#[test]
fn a_value_that_is_not_finite_is_a_contract_violation_of_rule_18() {
    let run = host::run(&faulty("nan-at = 3\nvalue = 0.5", ""));
    let faults = host::faults(&run);
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!(
        (faults[0].0, faults[0].1, faults[0].2),
        (5, FaultKind::Contract, 12018)
    );
    // Nothing of the faulted activation was applied: the output held is the second sample's.
    assert_eq!(controls(&run, 5, 1).len(), 2);
}

#[test]
fn a_period_of_zero_ticks_refuses_the_run_naming_the_actor() {
    let s = variant(&["pid"], |t| {
        t.replace(
            "period = \"1ms\"\nphase = \"1ns\"",
            "period = \"0ns\"\nphase = \"1ns\"",
        )
    });
    let refused = host::refused(&s);
    assert_eq!(refused, ["circuit.period-zero"]);
}

#[test]
fn an_input_on_no_signal_reads_none_and_the_guest_falls_back_to_its_parameter() {
    // The set point's signal is not there: `read-input` answers none and the parameter `setpoint`
    // is the set point (RFC 012 rules 18 and 26). It is the same 100 rad/s, so the loop is the same
    // loop, bit for bit.
    let alone = variant(&["pid"], |t| {
        let a = t.find("[[connections]]\nid = 103").unwrap();
        let b = t.find("[[connections]]\nid = 104").unwrap();
        format!("{}{}", &t[..a], &t[b..]).replace("kp = 0.004", "setpoint = 100.0\nkp = 0.004")
    });
    let (with, without) = (host::run(&motor_loop()), host::run(&alone));
    assert_eq!(host::ended(&without), EndReason::EndReached);
    let (a, b) = (speeds(&with), speeds(&without));
    assert_eq!(a.len(), b.len());
    assert!(
        a.iter()
            .zip(&b)
            .all(|(x, y)| x.1.to_bits() == y.1.to_bits()),
        "the same loop"
    );
}

/// `motor-loop` with the faulty controller in the pid's place (it reads the speed on `input` and
/// sets `output`), its parameters and the `[circuit]` table's lines given.
fn faulty(params: &str, circuit: &str) -> Scenario {
    let params = params.to_string();
    let circuit = circuit.to_string();
    variant(&["bad_controller"], move |t| {
        let (head, rest) = t.split_once("[[actors]]\nid = 3").unwrap();
        let (_, rest) = rest.split_once("[[actors]]\nid = 4").unwrap();
        let (sp_and_tl, rest) = (
            format!(
                "[[actors]]\nid = 4{}",
                rest.split_once("[[actors]]\nid = 5").unwrap().0
            ),
            rest.split_once("[[actors]]\nid = 5").unwrap().1,
        );
        let (_, connections) = rest.split_once("[[connections]]").unwrap();
        let connections = format!("[[connections]]{connections}");
        // Drop the set point's connection; the controller's ports are `input` and `output`.
        let (before, after) = connections.split_once("[[connections]]\nid = 103").unwrap();
        let (_, after) = after.split_once("[[connections]]\nid = 104").unwrap();
        let connections = format!("{before}[[connections]]\nid = 104{after}")
            .replace("pid.measurement", "pid.input");
        let head = head.replace(
            "[modules.pid]\nsha256 = \"@pid@\"",
            "[modules.bad_controller]\nsha256 = \"@bad_controller@\"",
        );
        format!(
            "{head}{sp_and_tl}[[actors]]\nid = 5\nname = \"pid\"\nkind = \"module\"\nmodule = \"bad_controller\"\nports = [{{ id = 0, name = \"input\", type = \"circuit/signal-in\" }}, {{ id = 1, name = \"output\", type = \"circuit/signal-out\" }}]\n[actors.config]\nperiod = \"1ms\"\n{params}\n\n{connections}\n[circuit]\n{circuit}\n"
        )
    })
}

#[test]
fn nothing_in_the_log_is_worse_than_a_warning_for_the_good_loop() {
    let run = host::run(&motor_loop());
    assert!(
        run.log()
            .entries()
            .iter()
            .all(|e| e.notification.severity != Severity::Error),
        "no error in the log"
    );
}

use florn_control::display::Record;
use florn_host::{Advance, Checkpoint, Options, Run};

fn drawn(run: &mut host::Host, to: u64) -> Vec<(u8, u16, f32)> {
    run.advance(Advance::ToTick(to));
    run.take_display()
        .into_iter()
        .filter_map(|r| match r {
            Record::Sample {
                index,
                actor,
                value,
                ..
            } => Some((index, actor, value)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_guest_with_two_control_channels_numbers_its_series_across_both() {
    // RFC 014 rule 3: the index of an `f64` field among the actor's declared `f64` fields, counted
    // in channel order and then field order: the first `control` channel's three are 0 to 2 and the
    // second's are 3 to 5. The faulty controller reports a constant on both at each sample.
    let mut run = host::try_start(
        &faulty("value = 0.25", ""),
        Options {
            display: true,
            ..host::options()
        },
    )
    .expect("starts");
    let samples = drawn(&mut run, 3_500_000);
    // The controller is the fourth actor in id order (1, 2, 4, 5): place 3.
    let mine: Vec<_> = samples.iter().filter(|s| s.1 == 3).collect();
    let mut indices: Vec<u8> = mine.iter().map(|s| s.0).collect();
    indices.sort_unstable();
    indices.dedup();
    assert_eq!(indices, [0, 1, 2, 3, 4, 5], "{mine:?}");
    // The values: the output is 0.25 at every sample, in both channels (indices 1 and 4).
    assert!(
        mine.iter()
            .filter(|s| s.0 == 1 || s.0 == 4)
            .all(|s| s.2 == 0.25)
    );
}

#[test]
fn a_built_in_sensors_samples_are_in_the_display_as_its_reports_are_in_the_trace() {
    // RFC 014 rules 2 and 3: the Host writes a `sample` record for each `f64` a domain draws in a
    // committed report, a built-in sensor's among them (found by the view model's series, which
    // had the controller's and not the motor's). The motor is the second actor in id order
    // (1, 2, 3, 4, 5): place 1, and its `speed` is the one field, index 0.
    let mut run = host::try_start(
        &motor_loop(),
        Options {
            display: true,
            ..host::options()
        },
    )
    .expect("starts");
    let samples = drawn(&mut run, 20_000_000);
    let motor: Vec<f32> = samples.iter().filter(|s| s.1 == 1).map(|s| s.2).collect();
    let reports = speeds(&run);
    assert_eq!(motor.len(), reports.len(), "one record for each report");
    assert!(motor.len() >= 19, "a sample every millisecond");
    assert!(samples.iter().all(|s| s.1 != 1 || s.0 == 0));
    for (record, (_, exact)) in motor.iter().zip(&reports) {
        assert_eq!(*record, *exact as f32);
    }
}

#[test]
fn the_series_index_of_a_guest_survives_a_checkpoint() {
    // RFC 012 finding F-18, extended: a run continuing from a checkpoint is told the guests as the
    // start hook was, so the continuation draws the same series with the same indices.
    let scenario = faulty("value = 0.25", "");
    let options = || Options {
        display: true,
        checkpoint_interval: 0,
        checkpoint_memory: u64::MAX,
        ..host::options()
    };
    let mut a = host::try_start(&scenario, options()).expect("starts");
    a.advance(Advance::ToTick(2_000_000));
    assert!(
        a.request_checkpoint() || {
            a.advance(Advance::OneInstant);
            a.kept().len() == 2
        }
    );
    let c =
        Checkpoint::decode_file(&a.kept().last().unwrap().checkpoint().encode(), u64::MAX).unwrap();
    let mut b = Run::from_checkpoint(
        host::engine(),
        host::admission(),
        &mut florn_platform::FixedPlatform::default(),
        &host::domains(),
        scenario.manifest(),
        &scenario.modules(),
        &c,
        options(),
    )
    .unwrap();
    let samples = drawn(&mut b, 4_500_000);
    let mut indices: Vec<u8> = samples.iter().filter(|s| s.1 == 3).map(|s| s.0).collect();
    indices.sort_unstable();
    indices.dedup();
    assert_eq!(indices, [0, 1, 2, 3, 4, 5], "after a restore");
}

#[test]
fn a_replay_of_the_loop_begins_as_the_run_did_and_matches() {
    // The start hook runs when a run begins again from its start, in a replay as in a restore
    // (RFC 012 finding F-15): without it a replay has no advance and no first sample.
    let scenario = motor_loop();
    let mut run = host::start(&scenario);
    run.run_to_end();
    let record = run.run_toml();
    let mut replay = florn_host::Run::replay(
        host::engine(),
        host::admission(),
        &mut florn_platform::FixedPlatform::default(),
        &host::domains(),
        scenario.manifest(),
        &scenario.modules(),
        &record,
        host::options(),
    )
    .unwrap();
    replay.run_to_end();
    assert!(matches!(
        replay.comparison(),
        Some(florn_kernel::trace::Comparison::Matched { .. })
    ));
    assert_eq!(replay.chain_hex(), run.chain_hex());
}

#[test]
fn the_domain_answers_read_input_from_the_held_values_and_knows_its_controllers() {
    // The domain as the Host composes it, given the guests the start hook gets: `read-input` is
    // the held value in the unit of the signal (RFC 012 rule 18), none for a port on no signal;
    // the controller is of a continuous class (RFC 013 rule 20, item 4).
    use florn_abi::Val;
    use florn_admission::ClassLimits;
    use florn_host::compound::NoCalls;
    use florn_host::domain::{Domain, Guest};
    let s = motor_loop();
    let bytes = s.modules().remove("pid").unwrap();
    let admitted = host::admission()
        .admit_under("pid", &bytes, &ClassLimits::defaults(), 1)
        .expect("pid is admitted");
    let guest = Guest {
        actor: 5,
        module: "pid".into(),
        admitted,
    };
    let mut d = florn_domain_circuit::CircuitDomain::new(florn_domain_circuit::Limits::native());
    d.compose(&s.manifest(), 1);
    d.start_calling(&mut NoCalls, &[guest])
        .expect("the plant starts");
    let read = |d: &mut florn_domain_circuit::CircuitDomain, port| {
        d.answer(5, "read-input", &[Val::U32(port)])
    };
    // Port 1 is `setpoint`, on the signal of the 100 rad/s constant; port 0 `measurement`, on the
    // motor's speed (0 at rest); port 99 is on no signal.
    assert_eq!(
        read(&mut d, 1),
        Some(Val::Option(Some(Box::new(Val::F64(100.0)))))
    );
    assert_eq!(
        read(&mut d, 0),
        Some(Val::Option(Some(Box::new(Val::F64(0.0)))))
    );
    assert_eq!(read(&mut d, 99), Some(Val::Option(None)));
    assert!(
        d.continuous_actor(5),
        "a controller is of a continuous class"
    );
    assert!(d.continuous_actor(2), "so is a sensor");
    assert!(!d.continuous_actor(1), "a source is not");
}
