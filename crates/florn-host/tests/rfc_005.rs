//! The Host under Wasmtime with hand-written guests: AC-002 (faults contained), AC-003 (replay;
//! an altered module refused), AC-004 (ties and declaration order), AC-006 (export and restore),
//! interventions replayed, the run record, capacity before instantiation.
mod support;

use std::collections::BTreeMap;

use florn_admission::Admission;
use florn_engine::Engine;
use florn_host::record::{self, Contents};
use florn_host::{Advance, Checkpoint, EndReason, Intervention, Options, Run, RunToml, Seed};
use florn_kernel::trace::Comparison;
use florn_kernel::{FaultKind, Record};
use florn_platform::{Capacity, FixedPlatform};
use support::{Guest, ScenarioSpec, actor, engine, new_run, options};

fn echo_and_pinger(end: u64) -> ScenarioSpec {
    ScenarioSpec {
        end,
        tie: "by-identity",
        actors: vec![actor(1, Guest::Pinger, 1), actor(2, Guest::Echo, 1)],
        connections: vec![vec![(1, 0), (2, 0)]],
        seed: 42,
    }
}

fn faults(run: &Run<florn_engine_wasmtime::WasmtimeEngine>) -> Vec<(u32, FaultKind, u64)> {
    run.records()
        .unwrap()
        .iter()
        .filter_map(|r| match r {
            Record::Fault {
                actor,
                kind,
                metered,
                ..
            } => Some((*actor, *kind, *metered)),
            _ => None,
        })
        .collect()
}

#[test]
fn ac_002_hostile_guests_fault_at_one_point_and_the_neighbour_continues() {
    let expected = [
        (Guest::Loop, FaultKind::Budget),
        (Guest::Recurse, FaultKind::Stack),
        (Guest::Grow, FaultKind::Trap),
        (Guest::Flood, FaultKind::Quota),
        (Guest::BadPointer, FaultKind::Contract),
    ];
    // The benign neighbour alone: its chain is the reference.
    let alone = ScenarioSpec {
        end: 1_000,
        tie: "by-identity",
        actors: vec![actor(2, Guest::Echo, 0)],
        connections: vec![],
        seed: 1,
    };
    let mut reference = new_run(&alone, options());
    assert_eq!(reference.run_to_end(), EndReason::EndReached);
    let reference_chain = reference.actor_chain(2).unwrap();
    for (guest, kind) in expected {
        let spec = ScenarioSpec {
            end: 1_000,
            tie: "by-identity",
            actors: vec![actor(1, guest, 1), actor(2, Guest::Echo, 0)],
            connections: vec![],
            seed: 1,
        };
        let mut a = new_run(&spec, options());
        assert_eq!(
            a.run_to_end(),
            EndReason::EndReached,
            "{guest:?}: the run continues"
        );
        let f = faults(&a);
        assert_eq!(f.len(), 1, "{guest:?}: one fault: {f:?}");
        assert_eq!(f[0].0, 1);
        assert_eq!(f[0].1, kind, "{guest:?}");
        assert!(f[0].2 > 0, "{guest:?}: metered work at the fault");
        if kind == FaultKind::Budget {
            assert_eq!(f[0].2, 10_000_000, "{guest:?}: the whole budget");
        }
        assert!(!a.alive(1), "{guest:?}: the actor handles nothing further");
        assert!(a.alive(2));
        assert_eq!(
            a.actor_chain(2).unwrap(),
            reference_chain,
            "{guest:?}: the neighbour's records are unchanged"
        );
        let codes = a.log().codes();
        assert!(codes.contains(&"actor.faulted"), "{codes:?}");
        // The same point again: the metered work is a property of the inputs.
        let mut b = new_run(&spec, options());
        b.run_to_end();
        assert_eq!(faults(&b), f, "{guest:?}: same fault, same metered work");
        assert_eq!(a.chain(), b.chain());
    }
}

#[test]
fn ac_003_replay_matches_and_an_altered_module_is_refused_by_hash() {
    let spec = echo_and_pinger(5_000);
    let mut run = new_run(&spec, options());
    assert_eq!(run.run_to_end(), EndReason::EndReached);
    assert!(run.kernel().trace.records() > 500);
    let contents = Contents {
        run: Some(run.run_toml()),
        trace: run.records().map(|r| r.to_vec()),
        payloads: Some(run.payloads().clone()),
        log: run.log().clone(),
        readme: None,
    };
    let bytes = record::write(&contents).unwrap();
    let read = record::read(&bytes).unwrap();
    assert_eq!(read.run, run.run_toml(), "run.toml round-trips");
    assert_eq!(
        read.trace.as_deref(),
        run.records(),
        "the trace round-trips"
    );
    assert_eq!(read.payloads.len(), run.payloads().len());
    assert!(
        read.entries
            .iter()
            .all(|e| !e.ends_with(".wasm") && e != "scenario.toml"),
        "no module bytes, no scenario text: {:?}",
        read.entries
    );
    assert!(read.log_jsonl.lines().count() >= 3);
    assert!(read.run.chain.len() >= 2);
    assert_eq!(read.run.ended, Some(EndReason::EndReached));
    assert!(read.run.reproducible);
    // Replay from the record's inputs: matched at every stored point.
    let mut replay = Run::replay(
        engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        spec.manifest(),
        &spec.modules(),
        &read.run,
        options(),
    )
    .unwrap();
    replay.run_to_end();
    assert_eq!(
        replay.comparison(),
        Some(Comparison::Matched {
            points: read.run.chain.len() as u64
        })
    );
    assert!(replay.log().codes().contains(&"replay.matched"));
    // One byte of the module altered: refused by hash, naming both.
    let mut modules = spec.modules();
    let echo = modules.get_mut("echo").unwrap();
    wasm_encoder::Section::append_to(
        &wasm_encoder::CustomSection {
            name: "altered".into(),
            data: [1u8].as_slice().into(),
        },
        echo,
    );
    let err = Run::new(
        engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        spec.manifest(),
        &modules,
        Seed::Given(42),
        options(),
    )
    .err()
    .expect("refused");
    match err {
        florn_host::HostError::Refused(e) => {
            assert_eq!(e.codes(), ["module.hash-mismatch"]);
            let params = e.0[0].params();
            assert_eq!(params[0].0, "module");
            assert_eq!(params[1].0, "declared");
            assert_eq!(params[2].0, "actual");
        }
        other => panic!("{other}"),
    }
    // A record of another scenario is refused for replay.
    let other = ScenarioSpec {
        end: 100,
        tie: "by-identity",
        actors: vec![actor(1, Guest::Echo, 0)],
        connections: vec![],
        seed: 42,
    };
    let err = Run::replay(
        engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        other.manifest(),
        &other.modules(),
        &read.run,
        options(),
    )
    .err()
    .expect("refused");
    assert!(
        matches!(
            err,
            florn_host::HostError::CheckpointMismatch {
                what: "scenario-hash",
                ..
            }
        ),
        "{err}"
    );
}

/// The winners of the races at the judge: for each instant with two messages, the origin handled first.
fn winners(run: &Run<florn_engine_wasmtime::WasmtimeEngine>, judge: u32) -> Vec<u32> {
    let mut by_tick: BTreeMap<u64, Vec<u32>> = BTreeMap::new();
    for r in run.records().unwrap() {
        if let Record::Handled { key, target, .. } = r
            && *target == judge
            && key.class == florn_host::CLASS_MESSAGE
        {
            by_tick.entry(key.tick).or_default().push(key.origin);
        }
    }
    by_tick
        .values()
        .filter(|v| v.len() == 2)
        .map(|v| v[0])
        .collect()
}

#[test]
fn ac_004_ties_follow_the_policy_and_not_the_declaration_order() {
    let races = 10_000u64;
    let period = 7u64;
    let mk = |tie: &'static str, reversed: bool| {
        let mut actors = vec![
            actor(1, Guest::Pinger, 1),
            actor(2, Guest::Pinger, 1),
            actor(3, Guest::Judge, 2),
        ];
        for a in &mut actors {
            if a.guest == Guest::Pinger {
                a.period = Some(period);
            }
        }
        if reversed {
            actors.reverse();
        }
        ScenarioSpec {
            end: period * races + 1,
            tie,
            actors,
            connections: vec![vec![(1, 0), (3, 0)], vec![(2, 0), (3, 1)]],
            seed: 99,
        }
    };
    // By identity: origin 1 always first; the reversed declaration gives the same digest.
    let mut a = new_run(&mk("by-identity", false), options());
    a.run_to_end();
    let w = winners(&a, 3);
    assert_eq!(w.len() as u64, races);
    assert!(w.iter().all(|o| *o == 1));
    let mut b = new_run(&mk("by-identity", true), options());
    b.run_to_end();
    assert_eq!(a.chain(), b.chain());
    // By seed: the order is a function of the seed, equal for both declaration orders, and fair.
    let mut c = new_run(&mk("by-seed", false), options());
    c.run_to_end();
    let mut d = new_run(&mk("by-seed", true), options());
    d.run_to_end();
    assert_eq!(c.chain(), d.chain());
    let w = winners(&c, 3);
    assert_eq!(w.len() as u64, races);
    let ones = w.iter().filter(|o| **o == 1).count() as f64 / races as f64;
    assert!(
        (0.45..=0.55).contains(&ones),
        "origin 1 won {:.1}% of {races} races",
        ones * 100.0
    );
    assert_ne!(a.chain(), c.chain(), "the policy is a run input");
}

#[test]
fn ac_006_export_and_restore_equal_the_uninterrupted_run() {
    let spec = echo_and_pinger(100_000);
    let mut reference = new_run(&spec, options());
    reference.run_to_end();
    let total = reference.kernel().handled();
    assert!(total >= 10_000, "{total} activations");
    let half = total / 2;

    // Export after half the activations into a fresh instance, in the same run.
    let mut a = new_run(&spec, options());
    while a.kernel().handled() < half {
        a.advance(Advance::OneEvent);
    }
    let state = a.export_actor(2).unwrap();
    assert!(state.memory.len() >= 65_536 * 2);
    assert!(!state.globals.is_empty());
    assert_eq!(state.tables.len(), 1);
    let encoded = state.encode();
    let decoded = florn_host::ActorState::decode(&encoded).unwrap();
    assert_eq!(decoded, state);
    a.restore_actor(2, &decoded).unwrap();
    a.run_to_end();
    assert_eq!(a.chain(), reference.chain(), "export into a fresh instance");

    // A checkpoint to bytes, restored into a new run (a new process would read the same bytes).
    let mut b = new_run(&spec, options());
    while b.kernel().handled() < half {
        b.advance(Advance::OneEvent);
    }
    let checkpoint = b.checkpoint().unwrap();
    let bytes = checkpoint.encode();
    let decoded = Checkpoint::decode(&bytes).unwrap();
    assert_eq!(decoded, checkpoint);
    let chain_at_half = b.chain();
    let mut c = Run::from_checkpoint(
        engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        spec.manifest(),
        &spec.modules(),
        &decoded,
        options(),
    )
    .unwrap();
    assert_eq!(c.chain(), chain_at_half);
    assert_eq!(c.kernel().handled(), half);
    c.run_to_end();
    assert_eq!(c.chain(), reference.chain(), "restore in a new run");
    assert_eq!(c.kernel().trace.stored(), reference.kernel().trace.stored());

    // Move backward: the first run continues, then restores the same checkpoint again.
    b.run_to_end();
    assert_eq!(b.chain(), reference.chain());
    let mut d = Run::from_checkpoint(
        engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        spec.manifest(),
        &spec.modules(),
        &checkpoint,
        options(),
    )
    .unwrap();
    d.run_to_end();
    assert_eq!(
        d.chain(),
        reference.chain(),
        "restore after moving backward"
    );
    assert_eq!(
        d.engine().compiled_count(),
        2,
        "one compilation per module, never per restore"
    );

    // A checkpoint of another scenario is refused with the field named.
    let other = ScenarioSpec {
        end: 100,
        ..echo_and_pinger(100)
    };
    let err = Run::from_checkpoint(
        engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        other.manifest(),
        &other.modules(),
        &checkpoint,
        options(),
    )
    .err()
    .expect("refused");
    assert!(
        matches!(
            err,
            florn_host::HostError::CheckpointMismatch {
                what: "scenario-hash",
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn interventions_are_inputs_and_replay_applies_them_at_their_instants() {
    let spec = echo_and_pinger(2_000);
    let mut run = new_run(&spec, options());
    run.advance(Advance::ToTick(500));
    run.intervene(
        None,
        &Intervention::Message {
            actor: 2,
            port: 0,
            payload: vec![7, 7, 7],
        },
    );
    run.advance(Advance::OneTick);
    run.intervene(
        Some(1_200),
        &Intervention::Message {
            actor: 2,
            port: 0,
            payload: vec![8],
        },
    );
    run.run_to_end();
    let applied = run.kernel().applied_interventions();
    assert_eq!(applied.len(), 2);
    assert_eq!(applied[1].instant.tick, 1_200);
    assert!(
        applied[0].instant.tick <= 500 && applied[0].instant.tick > 400,
        "{applied:?}"
    );
    let codes = run.log().codes();
    assert_eq!(
        codes
            .iter()
            .filter(|c| **c == "intervention.applied")
            .count(),
        2
    );
    // The echo answered the intervention: a message with the payload came back to the pinger.
    let echoed = run.records().unwrap().iter().any(|r| matches!(r, Record::Handled { target: 1, key, .. } if key.class == florn_host::CLASS_MESSAGE));
    assert!(echoed);
    let toml = run.run_toml();
    assert_eq!(toml.inputs.interventions.len(), 2);
    let text = toml.to_toml();
    let parsed = RunToml::parse(&text).unwrap();
    assert_eq!(parsed, toml);
    let mut replay = Run::replay(
        engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        spec.manifest(),
        &spec.modules(),
        &parsed,
        options(),
    )
    .unwrap();
    replay.run_to_end();
    assert_eq!(replay.chain(), run.chain());
    assert!(matches!(
        replay.comparison(),
        Some(Comparison::Matched { .. })
    ));
    // The same run without the interventions differs.
    let mut plain = new_run(&spec, options());
    plain.run_to_end();
    assert_ne!(plain.chain(), run.chain());
}

#[test]
fn telemetry_and_payloads_are_committed_effects() {
    let spec = echo_and_pinger(100);
    let mut run = new_run(&spec, options());
    run.run_to_end();
    let t = run.take_telemetry();
    assert!(!t.is_empty());
    assert!(
        t.iter()
            .all(|r| r.actor == 2 && r.channel == 1 && r.payload.len() == 8)
    );
    assert_eq!(run.telemetry_dropped(), 0);
    // Every committed send and report has its payload by digest.
    for r in run.records().unwrap() {
        if let Record::Handled { effects, .. } = r {
            for e in effects {
                match e {
                    florn_kernel::trace::CommittedEffect::Send { digest, len, .. }
                    | florn_kernel::trace::CommittedEffect::Report { digest, len, .. } => {
                        assert_eq!(run.payload(digest).map(|p| p.len() as u32), Some(*len));
                    }
                    _ => {}
                }
            }
        }
    }
}

#[test]
fn the_estimate_refuses_before_anything_is_instantiated() {
    let spec = echo_and_pinger(100);
    let mut platform = FixedPlatform {
        capacity: Capacity {
            instances: 1,
            memory_bytes: 1 << 40,
            swap_cost_us_per_kb: None,
        },
        ..Default::default()
    };
    let engine = engine();
    let err = Run::new(
        engine,
        Admission::default(),
        &mut platform,
        spec.manifest(),
        &spec.modules(),
        Seed::Given(1),
        options(),
    )
    .err()
    .expect("refused");
    match err {
        florn_host::HostError::Refused(e) => assert_eq!(e.codes(), ["host.capacity-exceeded"]),
        other => panic!("{other}"),
    }
    // A chosen seed becomes an input.
    let mut platform = FixedPlatform {
        seeds: vec![123_456],
        ..Default::default()
    };
    let run = Run::new(
        support::engine(),
        Admission::default(),
        &mut platform,
        spec.manifest(),
        &spec.modules(),
        Seed::Choose,
        Options::default(),
    )
    .unwrap();
    assert_eq!(run.inputs().root_seed, 123_456);
    assert_eq!(run.environment().engine, "wasmtime");
    assert!(
        run.log()
            .codes()
            .starts_with(&["host.capacity", "admission.result"])
    );
    assert_eq!(run.inputs().modules.len(), 2);
    assert!(
        run.scenario()
            .actors
            .iter()
            .all(|a| matches!(a.kind, florn_model::ActorKind::Module(_)))
    );
    assert!(run.engine().compiled_count() <= 2);
}
