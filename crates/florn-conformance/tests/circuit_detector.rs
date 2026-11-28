//! PR-51 (checklist B-51): the detector. RFC 012 rule 13; RFC 013 rule 15. A `comparator` checks its
//! condition at the end of every accepted step of an advance, locates the crossing inside the step
//! that crossed and publishes a `condition` event on the microstep after the advance.
use florn_conformance::circuit_scenarios::{comparator_latch, comparator_latch_text};
use florn_conformance::host;
use florn_conformance::scenario::Scenario;
use florn_domain_circuit::{CHANNEL_CONDITION, CHANNEL_PROBE};
use florn_host::EndReason;

/// The comparator's identity in `comparator-latch`, and the voltmeter's.
const CMP: u32 = 5;
const VM: u32 = 4;

/// One condition: the tick and microstep the event was handled at, the output, the located tick.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Condition {
    tick: u64,
    microstep: u32,
    output: f64,
    located: u64,
}

fn conditions(run: &host::Host) -> Vec<Condition> {
    host::reports(run, CMP, CHANNEL_CONDITION)
        .into_iter()
        .map(|(at, b)| Condition {
            tick: at.tick,
            microstep: at.microstep,
            output: f64::from_le_bytes(b[..8].try_into().unwrap()),
            located: u64::from_le_bytes(b[8..16].try_into().unwrap()),
        })
        .collect()
}

fn variant(change: impl FnOnce(String) -> String) -> Scenario {
    Scenario::raw(
        "comparator-variant",
        6_000_000,
        change(comparator_latch_text()),
        &[],
    )
}

/// The analytic crossings of `comparator-latch` (RC = 1 ms): the capacitor charging through 0.6 V,
/// discharging through 0.4 V after 2 ms and charging through 0.6 V again after 4 ms, in ticks.
fn analytic() -> [f64; 3] {
    let e2 = (-2.0f64).exp();
    let v2 = 1.0 - e2;
    let v4 = v2 * e2;
    [
        (1.0f64 / 0.4).ln() * 1e6,
        2e6 + (v2 / 0.4).ln() * 1e6,
        4e6 + ((1.0 - v4) / 0.4).ln() * 1e6,
    ]
}

#[test]
fn the_condition_fires_at_the_advance_that_finds_the_crossing_and_locates_it_inside_the_step() {
    let run = host::run(&comparator_latch());
    assert_eq!(host::ended(&run), EndReason::EndReached);
    assert!(host::faults(&run).is_empty());
    let c = conditions(&run);
    assert_eq!(c.len(), 3, "rising, falling, rising: {c:?}");
    assert_eq!(
        c.iter().map(|x| x.output).collect::<Vec<_>>(),
        [1.0, 0.0, 1.0]
    );
    let exact = analytic();
    for (found, t) in c.iter().zip(exact) {
        // The event is the next microstep after the advance that found it (rule 13), at a tick of
        // the sensors' 100 us grid, **after** the crossing and at most `detect-interval` (the
        // default `max-step`, 100 us) after it.
        assert_eq!(found.microstep, 1, "the microstep after the advance");
        assert_eq!(
            found.tick % 100_000,
            0,
            "an advance of the voltmeter's period: {found:?}"
        );
        assert!(
            found.tick as f64 >= t && found.tick as f64 - t <= 100_000.0,
            "{found:?} for {t}"
        );
        // The located crossing is data, never the tick of the event: inside the step that crossed,
        // so no later than the event and within one step (at most `max-step`) of the crossing.
        assert!(found.located <= found.tick, "{found:?}");
        assert!(
            (found.located as f64 - t).abs() <= 100_000.0,
            "{found:?} for {t}"
        );
        assert!(found.located as f64 >= t - 100_000.0);
    }
}

#[test]
fn with_a_fixed_step_the_located_crossing_is_within_that_step_of_the_analytic_one() {
    let run = host::run(&variant(|t| {
        t.replace(
            "[limits.default]",
            "[circuit]\ncontrol = \"fixed\"\nstep = \"10us\"\n\n[limits.default]",
        )
    }));
    let c = conditions(&run);
    assert_eq!(c.len(), 3);
    for (found, t) in c.iter().zip(analytic()) {
        let off = (found.located as f64 - t).abs();
        assert!(off <= 10_000.0, "{found:?}: {off} ticks from {t}");
    }
}

#[test]
fn the_output_is_sent_on_the_port_and_held_for_the_signal() {
    let run = host::run(&comparator_latch());
    let sent: Vec<(u64, f64)> = host::sends(&run, CMP)
        .into_iter()
        .map(|(at, _port, b)| (at.tick, f64::from_le_bytes(b[..8].try_into().unwrap())))
        .collect();
    let c = conditions(&run);
    assert_eq!(sent.len(), 3);
    for (s, found) in sent.iter().zip(&c) {
        assert_eq!((s.0, s.1), (found.tick, found.output));
    }
}

#[test]
fn the_voltmeters_samples_are_the_plants_and_the_comparator_changes_nothing_of_them() {
    // The comparator reads the plant and changes none of it: with it or without, the voltmeter's
    // samples are the same.
    let with = host::run(&comparator_latch());
    let without = host::run(&variant(|t| {
        let a = t.find("\n[[actors]]\nid = 5\n").unwrap();
        let b = t.find("\n[[connections]]\nid = 100\n").unwrap();
        let t = format!("{}{}", &t[..a], &t[b..]);
        t.replace(", \"cmp.p\"", "").replace(", \"cmp.n\"", "")
    }));
    let samples = |r: &host::Host| host::reports(r, VM, CHANNEL_PROBE);
    assert_eq!(samples(&with), samples(&without));
    assert!(conditions(&without).is_empty());
}

#[test]
fn a_comparator_already_beyond_its_threshold_at_start_reports_it_at_the_first_advance_that_takes_a_step()
 {
    // Threshold −0.1 V with hysteresis 0.2 V: the capacitor at 0 V is above −0.1 + 0.1 = 0 V? Its
    // rising level is 0 V, so use −1 V: the condition holds at tick 0.
    let run = host::run(&variant(|t| {
        t.replace("threshold = \"0.5V\"", "threshold = \"-1V\"")
    }));
    let c = conditions(&run);
    assert_eq!(c.len(), 1, "{c:?}");
    assert_eq!(c[0].output, 1.0);
    // The advance at tick 0 takes no step; the first one that does is the voltmeter's at 100 us, and
    // the located crossing is the start of the first step (rule 15: φ = 0).
    assert_eq!(c[0].tick, 100_000);
    assert_eq!(c[0].located, 0);
}

#[test]
fn a_threshold_moved_by_a_scheduled_change_is_located_at_the_start_of_its_step() {
    // At 3 ms the capacitor is at 0.66·e^(…) well above 0.1 V; the threshold goes to 0 V at 3.0 ms
    // with its hysteresis 0.2 V: the falling level is −0.1 V, the rising 0.1 V, and the output is
    // already high, so nothing changes; then to −1 V... A threshold change that makes the output go
    // high from low is the case: move it down at 2.8 ms, when the output has just gone low (0.4 V).
    let run = host::run(&variant(|t| {
        t.replace(
            "\n[[changes]]\nat = \"4000000ns\"",
            "\n[[changes]]\nat = \"3050000ns\"\ntarget = \"world:circuit\"\n[changes.payload]\nactor = 5\nthreshold = \"0.1V\"\n\n[[changes]]\nat = \"4000000ns\"",
        )
    }));
    let c = conditions(&run);
    // Rising 0.9 ms, falling 2.77 ms; then at 3.05 ms the threshold is 0.1 V: its rising level is
    // 0.2 V, which the capacitor (about 0.35 V) is above: the output goes high at once, located at
    // the start of the first step after the breakpoint.
    let after: Vec<&Condition> = c.iter().filter(|x| x.tick > 3_050_000).collect();
    assert!(!after.is_empty(), "{c:?}");
    assert_eq!(after[0].output, 1.0);
    assert_eq!(after[0].located, 3_050_000, "{after:?}");
}

#[test]
fn the_condition_drives_the_plant_from_its_tick_a_relaxation_oscillator() {
    // The comparator's output on a signal that drives the source: `v = 1 V − 1 V·y`. While the
    // output is low the capacitor charges, past 0.6 V the output goes high, the source goes to
    // 0 V and the capacitor discharges, below 0.4 V the output goes low again.
    let run = host::run(&variant(|t| {
        // No scheduled change: the loop is the only thing that moves the source.
        let t = t[..t.find("\n[[changes]]").unwrap()].to_string();
        t.replacen("v = \"1V\"", "v = \"1V\"\ngain = \"-1V\"", 1)
            .replace(
                "[[connections]]\nid = 100\n",
                "[[connections]]\nid = 103\nname = \"latch\"\ntype = \"circuit/signal\"\nports = [\"cmp.y\", \"vs.cmd\"]\n\n[[connections]]\nid = 100\n",
            )
            .replace(
                "ports = [{ id = 0, name = \"p\", type = \"circuit/pin\" }, { id = 1, name = \"n\", type = \"circuit/pin\" }]\n[actors.config]\nv = \"1V\"",
                "ports = [{ id = 0, name = \"p\", type = \"circuit/pin\" }, { id = 1, name = \"n\", type = \"circuit/pin\" }, { id = 2, name = \"cmd\", type = \"circuit/signal-in\" }]\n[actors.config]\nv = \"1V\"",
            )
    }));
    assert_eq!(host::ended(&run), EndReason::EndReached);
    let c = conditions(&run);
    assert!(c.len() >= 8, "it oscillates: {c:?}");
    for pair in c.windows(2) {
        assert_ne!(
            pair[0].output, pair[1].output,
            "the outputs alternate: {pair:?}"
        );
    }
    // A half period is the time to go from 0.4 V to 0.6 V towards 1 V, `RC·ln(0.6/0.4)` = 0.405 ms,
    // and the detection adds up to a step at each end.
    for pair in c.windows(2).skip(1) {
        let half = (pair[1].located - pair[0].located) as f64;
        assert!((half - 405_465.0).abs() < 250_000.0, "{half}");
    }
}

#[test]
fn a_run_restored_between_the_crossing_and_its_event_and_elsewhere_continues_to_the_same_digest() {
    use florn_host::{Advance, Checkpoint, Options, Run};
    let s = comparator_latch();
    let whole = host::run(&s);
    let options = || Options {
        checkpoint_interval: 0,
        checkpoint_memory: u64::MAX,
        ..host::options()
    };
    // 0.95 ms: the capacitor has crossed 0.6 V at 0.916 ms and the advance that finds it is at 1.0 ms,
    // so the checkpoint holds a detector that has not found it; 1.0 ms: the advance has run and the
    // condition event is pending; 3 ms and 5 ms: after.
    for at in [500_000u64, 950_000, 1_000_000, 3_000_000, 5_000_000] {
        let mut a = host::try_start(&s, options()).expect("starts");
        a.advance(Advance::ToTick(at));
        assert!(
            a.request_checkpoint() || {
                a.advance(Advance::OneInstant);
                !a.kept().is_empty()
            }
        );
        let c = Checkpoint::decode_file(&a.kept().last().unwrap().checkpoint().encode(), u64::MAX)
            .unwrap();
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
        assert_eq!(b.chain_hex(), whole.chain_hex(), "restored at {at}");
    }
}
