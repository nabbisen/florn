//! The formats of release 0.1, frozen: the trace record encoding (RFC 005 rule 20), the control
//! message encoding (rule 25), `run.toml` and the checkpoint (rule 29; RFC 003 rule 22). Each
//! test holds the recorded encoding of fixed values; a change here is a change of format and
//! needs a new version number, never a silent edit of the record.
use std::collections::BTreeMap;

use florn_conformance::host;
use florn_control::{
    Command, Envelope, ExportKind, Message, Query, SeedChoice, Subscription, Unit,
};
use florn_host::{Environment, Options, Run, Seed};
use florn_kernel::trace::{CommittedEffect, decode, encode};
use florn_kernel::{EventKey, FaultKind, Record};
use florn_platform::FixedPlatform;
use florn_test_guests::{Guest, ScenarioSpec, actor};

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn key() -> EventKey {
    EventKey {
        tick: 5,
        microstep: 1,
        class: 2,
        tie: 7,
        origin: 1,
        seq: 3,
    }
}

thread_local! {
    static MISSING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Compare with the record, or print it when the record is empty (the first run).
fn frozen(what: &str, found: &str, recorded: &str) {
    if recorded.is_empty() {
        eprintln!("{what}: {found}");
        MISSING.with(|m| m.set(true));
        return;
    }
    assert_eq!(found, recorded, "{what}: the format changed");
}

/// Every value of the test had a record.
fn complete() {
    assert!(
        !MISSING.with(|m| m.get()),
        "a format has no recorded value yet"
    );
}

const HANDLED: &str = "01050000000000000001000000020700000000000000010000000300000000000000020000006400000000000000010000000400000001000000000000000000000000030000000101010101010101010101010101010101010101010101010101010101010101020100000000000000000000000a000000000000000302000000000000000100000004030000000000000001000000080000000202020202020202020202020202020202020202020202020202020202020202";
const FAULT: &str = "020500000000000000010000000207000000000000000100000003000000000000000100000002000000008096980000000000";
const INTERVENTION: &str = "03050000000000000001000000020700000000000000010000000300000000000000090000000303030303030303030303030303030303030303030303030303030303030303";

#[test]
fn trace_records_encode_as_recorded() {
    let handled = Record::Handled {
        key: key(),
        target: 2,
        metered: 100,
        draws: 1,
        effects: vec![
            CommittedEffect::Send {
                seq: 0,
                port: 0,
                len: 3,
                digest: [1u8; 32],
            },
            CommittedEffect::SetTimer {
                seq: 1,
                timer: 0,
                delay: 10,
            },
            CommittedEffect::CancelTimer { seq: 2, timer: 1 },
            CommittedEffect::Report {
                seq: 3,
                channel: 1,
                len: 8,
                digest: [2u8; 32],
            },
        ],
    };
    let fault = Record::Fault {
        key: key(),
        actor: 1,
        kind: FaultKind::Budget,
        parameter: 0,
        metered: 10_000_000,
    };
    let intervention = Record::Intervention {
        key: key(),
        content_digest: [3u8; 32],
        content_len: 9,
    };
    for (what, r, recorded) in [
        ("handled", &handled, HANDLED),
        ("fault", &fault, FAULT),
        ("intervention", &intervention, INTERVENTION),
    ] {
        let bytes = encode(r);
        assert_eq!(decode(&bytes).as_ref(), Some(r), "{what} round-trips");
        frozen(what, &hex(&bytes), recorded);
    }
    complete();
}

const NEW_RUN: &str = "010000000003002a00000000000000";
const ADVANCE: &str = "01000000000503e803000000000000";
const RUN_STATE: &str = "010000000101";
const EVENTS: &str = "010000000201";
/// The `display` subscription of 0.2 (RFC 005 rule 14): an addition, the rest unchanged.
const DISPLAY: &str = "010000000203";
// RFC 009 section 8 (0.3): the checkpoint commands and query, within control version 1.
const CHECKPOINT_CMD: &str = "01000000000b";
const RESTORE: &str = "01000000000c03000000";
const LOAD_CHECKPOINT: &str = "01000000000d03000000020000";
const EXPORT_CHECKPOINT: &str = "010000000009030100000000";
const CHECKPOINTS: &str = "010000000108";
// RFC 009 section 4 (0.3): the branch command and the parent's record, within control version 1.
const BRANCH: &str = "01000000000f";
const EXPORT_PARENT: &str = "0100000000090401";

#[test]
fn control_messages_encode_as_recorded() {
    for (what, m, recorded) in [
        (
            "new-run",
            Message::Command(Command::NewRun {
                seed: SeedChoice::Given(42),
            }),
            NEW_RUN,
        ),
        (
            "advance",
            Message::Command(Command::Advance {
                unit: Unit::ToTick(1_000),
            }),
            ADVANCE,
        ),
        ("run-state", Message::Query(Query::RunState), RUN_STATE),
        ("events", Message::Subscribe(Subscription::Events), EVENTS),
        (
            "display",
            Message::Subscribe(Subscription::Display),
            DISPLAY,
        ),
        (
            "checkpoint",
            Message::Command(Command::Checkpoint),
            CHECKPOINT_CMD,
        ),
        (
            "restore",
            Message::Command(Command::Restore { checkpoint: 3 }),
            RESTORE,
        ),
        (
            "load-checkpoint",
            Message::Command(Command::LoadCheckpoint {
                bytes: vec![2, 0, 0],
            }),
            LOAD_CHECKPOINT,
        ),
        (
            "export checkpoint",
            Message::Command(Command::Export {
                kind: ExportKind::Checkpoint { index: 1 },
                confirmed: false,
            }),
            EXPORT_CHECKPOINT,
        ),
        (
            "checkpoints",
            Message::Query(Query::Checkpoints),
            CHECKPOINTS,
        ),
        ("branch", Message::Command(Command::Branch), BRANCH),
        (
            "export parent-record",
            Message::Command(Command::Export {
                kind: ExportKind::ParentRecord,
                confirmed: true,
            }),
            EXPORT_PARENT,
        ),
    ] {
        let env = Envelope::new(m);
        let bytes = florn_control::encode(&env);
        assert_eq!(florn_control::decode(&bytes).ok().as_ref(), Some(&env));
        frozen(what, &hex(&bytes), recorded);
    }
    complete();
}

const RUN_TOML: &str = "b5067636665393f4946d6091605a1e4bcfe27ad83a17e32a4f70011299c46709";
// Version 2 since 0.3.0 (RFC 009 rule 1): the origins' sequence counters joined the layout.
const CHECKPOINT: &str = "6dd48fef2cf8dbd71519b2a5439b7da9a3eb603958a0144e7bb59c6da5da89c6";
const ENTRIES: &str = "README.md diagnostic.bin diagnostic.log payloads/<64-hex-digest> run.toml trace/0.bin trace/1.bin";
/// The same run branched at tick 100 (RFC 009 rule 12): `run.toml` with its `[parent]` table.
const RUN_TOML_BRANCH: &str = "a72bbb516195aee4be4d25d70fc212dbf93e557933e9d7119d176c68ac261eb4";

/// A fixed run of the hand-written guests under a fixed platform: its `run.toml` (with the
/// environment, which names the machine, replaced by a fixed one), its checkpoint at the end,
/// and the entries of its run record.
#[test]
fn the_run_record_and_the_checkpoint_of_a_fixed_run_are_recorded() {
    let spec = ScenarioSpec {
        end: 200,
        tie: "by-identity",
        actors: vec![actor(1, Guest::Pinger, 1), actor(2, Guest::Echo, 1)],
        connections: vec![vec![(1, 0), (2, 0)]],
        seed: 1,
    };
    let engine =
        florn_engine_wasmtime::WasmtimeEngine::new(florn_engine_wasmtime::Limits::default())
            .unwrap();
    let mut run = Run::new(
        engine,
        florn_admission::Admission::default(),
        &mut FixedPlatform::default(),
        &host::domains(),
        spec.manifest(),
        &spec.modules(),
        Seed::Given(1),
        Options {
            keep_records: true,
            keep_payloads: true,
            chain_interval: 50,
            ..Options::default()
        },
    )
    .unwrap();
    run.run_to_end();
    let mut toml = run.run_toml();
    toml.environment = Environment {
        florn_version: "0".into(),
        host_build: "native".into(),
        engine: "engine".into(),
        engine_version: "0".into(),
        arch: "arch".into(),
    };
    let text = toml.to_toml();
    assert_eq!(florn_host::RunToml::parse(&text).as_ref(), Some(&toml));
    frozen(
        "run.toml (blake3 of the text)",
        blake3::hash(text.as_bytes()).to_hex().as_ref(),
        RUN_TOML,
    );
    let checkpoint = run.checkpoint().unwrap();
    let bytes = checkpoint.encode();
    assert_eq!(
        florn_host::Checkpoint::decode(&bytes).as_ref(),
        Some(&checkpoint)
    );
    frozen(
        "checkpoint (blake3)",
        blake3::hash(&bytes).to_hex().as_ref(),
        CHECKPOINT,
    );
    let contents = florn_host::record::Contents {
        run: Some(run.run_toml()),
        trace: run.records().map(|r| r.to_vec()),
        payloads: Some(run.payloads().clone()),
        log: run.log().clone(),
        readme: None,
    };
    let record = florn_host::record::write(&contents).unwrap();
    let read = florn_host::record::read(&record).unwrap();
    let mut entries: Vec<String> = read
        .entries
        .iter()
        .map(|e| {
            if let Some(rest) = e.strip_prefix("payloads/") {
                format!("payloads/<{}-hex-digest>", rest.len())
            } else {
                e.clone()
            }
        })
        .collect();
    entries.sort();
    entries.dedup();
    frozen("entries", &entries.join(" "), ENTRIES);
    let _: BTreeMap<String, String> = BTreeMap::new();
    // The branch: the same run to tick 100, branched there, run to the end.
    let mut platform = FixedPlatform::default();
    let mut run = Run::new(
        florn_engine_wasmtime::WasmtimeEngine::new(florn_engine_wasmtime::Limits::default())
            .unwrap(),
        florn_admission::Admission::default(),
        &mut platform,
        &host::domains(),
        spec.manifest(),
        &spec.modules(),
        Seed::Given(1),
        Options {
            keep_records: true,
            chain_interval: 50,
            ..Options::default()
        },
    )
    .unwrap();
    run.advance(florn_host::Advance::ToTick(100));
    let parent = run.branch(&mut platform).unwrap();
    assert_eq!(
        parent.run.as_ref().unwrap().records,
        run.parent().unwrap().records
    );
    run.run_to_end();
    let mut branch = run.run_toml();
    branch.environment = toml.environment.clone();
    let text = branch.to_toml();
    assert!(text.contains("[parent]"));
    assert_eq!(florn_host::RunToml::parse(&text).as_ref(), Some(&branch));
    frozen(
        "run.toml of a branch (blake3 of the text)",
        blake3::hash(text.as_bytes()).to_hex().as_ref(),
        RUN_TOML_BRANCH,
    );
    complete();
}
