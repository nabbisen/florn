//! The acceptance criteria AC-001 to AC-006 of the requirements specification §22.1, as the
//! checklist runs them, natively over the kit's guests and shipped scenarios. AC-007 is this
//! crate's whole test run.
use std::collections::BTreeMap;
use std::time::Instant;

use florn_admission::Admission;
use florn_conformance::host::{self, admitted_hashes, ended, engine, faults, handled, options};
use florn_conformance::scenario::Scenario;
use florn_conformance::{guests, shipped};
use florn_control::{Answer, Command, Envelope, Message};
use florn_engine::Engine;
use florn_host::record::{self, Contents};
use florn_host::{Advance, Checkpoint, Controller, EndReason, HostError, Run, Seed};
use florn_kernel::FaultKind;
use florn_kernel::trace::Comparison;
use florn_model::{HostMaxima, Manifest};
use florn_platform::FixedPlatform;
use florn_platform_native::NativePlatform;

fn shipped_named(name: &str) -> Scenario {
    shipped::all()
        .into_iter()
        .find(|s| s.scenario.name == name)
        .unwrap_or_else(|| panic!("shipped scenario {name}"))
        .scenario
}

#[test]
fn ac_001_every_shipped_scenario_produces_its_recorded_digest() {
    let dir = shipped::directory();
    let recorded = shipped::Digests::read(&dir).expect("scenarios/digests.toml");
    let hashes = guests::hashes();
    assert_eq!(
        hashes,
        recorded.modules,
        "the SDK's guests build to the recorded bytes on {} (regenerate with `florn-conformance scenarios` after a change to the SDK or a guest)",
        std::env::consts::ARCH
    );
    for s in shipped::all() {
        let name = s.scenario.name.clone();
        // The file is the kit's text, and the run reads the file as the headless runner does.
        let path = dir.join(format!("{name}.toml"));
        let text =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            text,
            shipped::header(&name) + &s.scenario.text(&hashes),
            "{name}: the file differs from the kit's text; regenerate"
        );
        let manifest = Manifest::parse(&text, &HostMaxima::native())
            .unwrap_or_else(|e| panic!("{name}: {:?}", e.codes()));
        let r = recorded
            .runs
            .get(&name)
            .unwrap_or_else(|| panic!("{name}: no recorded digest"));
        let mut run = Run::new(
            engine(),
            Admission::default(),
            &mut NativePlatform::new(),
            manifest,
            &s.scenario.modules(),
            Seed::Given(r.seed),
            options(),
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        run.run_to_end();
        assert_eq!(
            run.chain_hex(),
            r.chain,
            "{name}: the chain (recorded on {})",
            recorded.arch
        );
        assert_eq!(
            run.kernel().trace.records(),
            r.records,
            "{name}: the record count"
        );
        assert_eq!(ended(&run).as_str(), r.ended, "{name}: how the run ended");
        assert_eq!(
            s.ends_normally,
            ended(&run) == EndReason::EndReached,
            "{name}: ends as the kit says"
        );
        assert_eq!(
            admitted_hashes(&run).len(),
            s.scenario.module_names().len(),
            "{name}: every module admitted"
        );
    }
    // Tier 1: the same scenario twice under one Host gives one digest.
    let a = host::run(&shipped_named("echo-pair"));
    let b = host::run(&shipped_named("echo-pair"));
    assert_eq!(a.chain(), b.chain());
    assert_eq!(a.records(), b.records());
}

#[test]
fn ac_002_the_hostile_scenarios_fault_at_one_point_and_the_neighbour_continues() {
    let reference = host::run(&shipped_named("neighbour"));
    let reference_chain = reference.actor_chain(2).expect("the neighbour's chain");
    for (name, kind) in [
        ("hostile-loop", FaultKind::Budget),
        ("hostile-recurse", FaultKind::Stack),
        ("hostile-grow", FaultKind::Trap),
        ("hostile-flood", FaultKind::Quota),
        ("hostile-bad-pointer", FaultKind::Contract),
    ] {
        let s = shipped_named(name);
        let a = host::run(&s);
        assert_eq!(
            ended(&a),
            EndReason::EndReached,
            "{name}: the Host exits normally"
        );
        let f = faults(&a);
        assert_eq!(f.len(), 1, "{name}: one fault: {f:?}");
        let (actor, found, parameter, metered) = f[0];
        assert_eq!(actor, 1, "{name}");
        assert_eq!(found, kind, "{name}");
        assert!(metered > 0, "{name}: metered work at the fault");
        match kind {
            FaultKind::Budget => assert_eq!(metered, 10_000_000, "{name}: the whole budget"),
            FaultKind::Quota => assert_eq!(parameter, 4, "{name}: the effects quota"),
            FaultKind::Contract => assert_eq!(parameter, 8, "{name}: RFC 002 rule 8"),
            _ => {}
        }
        assert!(!a.alive(1) && a.alive(2), "{name}");
        assert_eq!(
            a.actor_chain(2).expect("chain"),
            reference_chain,
            "{name}: the neighbour's records equal its records without the hostile actor"
        );
        assert!(a.log().codes().contains(&"actor.faulted"), "{name}");
        // The same point again: the fault kind and the metered work are properties of the inputs.
        let b = host::run(&s);
        assert_eq!(faults(&b), f, "{name}");
        assert_eq!(a.chain(), b.chain(), "{name}");
    }
}

#[test]
fn ac_003_a_run_replays_from_its_record_and_an_altered_module_is_refused() {
    let s = shipped_named("echo-pair");
    let mut run = host::run(&s);
    assert_eq!(ended(&run), EndReason::EndReached);
    let contents = Contents {
        run: Some(run.run_toml()),
        trace: run.records().map(|r| r.to_vec()),
        payloads: Some(run.payloads().clone()),
        log: run.log().clone(),
        readme: None,
    };
    let bytes = record::write(&contents).expect("the record");
    let read = record::read(&bytes).expect("the record reads");
    assert!(
        read.entries
            .iter()
            .all(|e| !e.ends_with(".wasm") && e != "scenario.toml"),
        "no module bytes, no scenario text: {:?}",
        read.entries
    );
    let mut replay = Run::replay(
        engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        s.manifest(),
        &s.modules(),
        &read.run,
        options(),
    )
    .expect("the replay starts");
    replay.run_to_end();
    assert_eq!(
        replay.comparison(),
        Some(Comparison::Matched {
            points: read.run.chain.len() as u64
        }),
        "matched at every stored point"
    );
    assert_eq!(replay.chain(), run.chain());
    // One byte of the echo module altered (inside its manifest's description, so that the module
    // stays valid): refused with `module.hash-mismatch` naming both hashes.
    let mut modules = s.modules();
    let echo = modules.get_mut("echo").expect("echo");
    let at = echo
        .windows(6)
        .position(|w| w == b"Echoes")
        .expect("the description");
    echo[at] = b'e';
    let err = Run::new(
        engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        s.manifest(),
        &modules,
        Seed::Given(s.seed),
        options(),
    )
    .err()
    .expect("refused");
    match err {
        HostError::Refused(e) => {
            assert_eq!(e.codes(), ["module.hash-mismatch"]);
            let params = e.0[0].params();
            let names: Vec<&str> = params.iter().map(|(n, _)| *n).collect();
            assert_eq!(names, ["module", "declared", "actual"]);
        }
        other => panic!("{other}"),
    }
    let _ = run.take_telemetry();
}

/// For each instant at the echo with two arrivals, the origin handled first.
fn winners(run: &host::Host) -> Vec<u32> {
    let mut by_tick: BTreeMap<u64, Vec<u32>> = BTreeMap::new();
    for key in handled(run, 3) {
        if key.class == florn_host::CLASS_MESSAGE {
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
fn ac_004_ties_follow_the_policy_whatever_the_declaration_order() {
    let a = host::run(&shipped_named("race-by-identity"));
    let w = winners(&a);
    assert_eq!(w.len(), 10_000, "ten thousand races");
    assert!(w.iter().all(|o| *o == 1), "by identity: origin 1 first");
    let b = host::run(&shipped_named("race-by-identity-reversed"));
    assert_eq!(
        a.chain(),
        b.chain(),
        "the declaration order changes nothing"
    );
    let c = host::run(&shipped_named("race-by-seed"));
    let d = host::run(&shipped_named("race-by-seed-reversed"));
    assert_eq!(
        c.chain(),
        d.chain(),
        "by seed: the same digest for both orders"
    );
    let w = winners(&c);
    assert_eq!(w.len(), 10_000);
    let share = w.iter().filter(|o| **o == 1).count() as f64 / w.len() as f64;
    assert!(
        (0.45..=0.55).contains(&share),
        "by seed is fair: origin 1 won {:.1}% of 10,000 races",
        share * 100.0
    );
    assert_ne!(a.chain(), c.chain(), "the policy is a run input");
}

fn admit(bytes: Vec<u8>) -> Vec<Message> {
    let mut c = Controller::new(
        engine(),
        florn_host::run::default_admission(),
        Box::new(NativePlatform::new()),
        options(),
    );
    c.handle(Envelope::new(Message::Command(Command::Admit { bytes })))
}

#[test]
fn ac_005_admit_prints_the_refusal_or_the_manifest() {
    // A module without a manifest.
    let replies = admit(guests::bare());
    let codes: Vec<String> = replies
        .iter()
        .filter_map(|m| match m {
            Message::Notification(n) => Some(n.code.clone()),
            _ => None,
        })
        .collect();
    assert!(
        codes.contains(&"module.manifest-missing".to_string()),
        "{codes:?}"
    );
    assert!(!replies.iter().any(|m| matches!(m, Message::Answer(_))));
    // The echo guest: the manifest and the admitted hash.
    let recorded = shipped::Digests::read(&shipped::directory()).unwrap();
    let replies = admit(guests::bytes("echo"));
    let Some(Message::Answer(Answer::Admitted {
        module,
        admitted_sha256,
    })) = replies.iter().find(|m| matches!(m, Message::Answer(_)))
    else {
        panic!("no answer: {replies:?}");
    };
    assert_eq!(module.contract, "florn:core/guest@0.1.0");
    assert_eq!(module.kind, "echo");
    assert_eq!(
        module.port_kinds,
        vec![("p".to_string(), "core/port".to_string(), 0, 8)]
    );
    let params: Vec<(String, String, bool, Option<String>)> = module
        .params
        .iter()
        .map(|p| (p.name.clone(), p.ty.clone(), p.required, p.default.clone()))
        .collect();
    assert_eq!(
        params,
        vec![
            ("period".into(), "u64".into(), false, Some("10".into())),
            ("label".into(), "text".into(), false, Some("echo".into())),
        ]
    );
    assert_eq!(module.channels, vec![(1, "draw".to_string())]);
    assert_eq!(
        module.needs,
        vec![
            ("memory".to_string(), 1_048_576),
            ("pending-timers".to_string(), 4)
        ]
    );
    assert_eq!(&module.source_sha256, &recorded.modules["echo"]);
    // The command admits under the Host's default limits; the recorded hash is for the kit's.
    let direct = Admission::default()
        .admit(
            "echo",
            &guests::bytes("echo"),
            &florn_admission::ClassLimits::defaults(),
        )
        .unwrap();
    assert_eq!(admitted_sha256, &direct.admitted_hex());
    let kit = Admission::default()
        .admit("echo", &guests::bytes("echo"), &host::kit_limits())
        .unwrap();
    assert_eq!(kit.admitted_hex(), recorded.admitted["echo"]);
}

#[test]
fn ac_006_export_and_restore_equal_the_uninterrupted_run() {
    let s = shipped_named("heap");
    let reference = host::run(&s);
    let total = reference.kernel().handled();
    assert!(total >= 10_000, "{total} activations");
    let half = total / 2;

    // Export after half the activations into a fresh instance of the same run.
    let mut a = host::start(&s);
    while a.kernel().handled() < half {
        a.advance(Advance::OneEvent);
    }
    let started = Instant::now();
    let state = a.export_actor(1).expect("export");
    let export_time = started.elapsed();
    let mb = state.memory.len() as f64 / (1024.0 * 1024.0);
    assert!(
        state.memory.len() > 5_000 * 64,
        "the heap grew: {} bytes",
        state.memory.len()
    );
    let encoded = state.encode();
    assert_eq!(
        florn_host::ActorState::decode(&encoded).as_ref(),
        Some(&state)
    );
    let started = Instant::now();
    a.restore_actor(1, &state).expect("restore");
    let restore_time = started.elapsed();
    a.run_to_end();
    assert_eq!(a.chain(), reference.chain(), "export into a fresh instance");
    let per_mb = |t: std::time::Duration| t.as_secs_f64() * 1e3 / mb;
    eprintln!(
        "export {:.2} ms/MB, restore {:.2} ms/MB over {mb:.2} MB",
        per_mb(export_time),
        per_mb(restore_time)
    );
    if !cfg!(debug_assertions) {
        assert!(per_mb(export_time) < 5.0, "export under 5 ms per MB");
        assert!(per_mb(restore_time) < 5.0, "restore under 5 ms per MB");
    }

    // A checkpoint to bytes, restored in a new run (a new process reads the same bytes).
    let mut b = host::start(&s);
    while b.kernel().handled() < half {
        b.advance(Advance::OneEvent);
    }
    let checkpoint = b.checkpoint().expect("checkpoint");
    let bytes = checkpoint.encode();
    let decoded = Checkpoint::decode(&bytes).expect("decodes");
    assert_eq!(decoded, checkpoint);
    let mut c = Run::from_checkpoint(
        engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        s.manifest(),
        &s.modules(),
        &decoded,
        options(),
    )
    .expect("restores");
    assert_eq!(c.kernel().handled(), half);
    c.run_to_end();
    assert_eq!(c.chain(), reference.chain(), "restore in a new run");

    // Backward: the first run continues to the end, then the checkpoint is restored again.
    b.run_to_end();
    assert_eq!(b.chain(), reference.chain());
    let mut d = Run::from_checkpoint(
        engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        s.manifest(),
        &s.modules(),
        &checkpoint,
        options(),
    )
    .expect("restores again");
    d.run_to_end();
    assert_eq!(
        d.chain(),
        reference.chain(),
        "restore after moving backward"
    );
    assert_eq!(
        d.engine().compiled_count(),
        1,
        "one compilation, never per restore"
    );
}
