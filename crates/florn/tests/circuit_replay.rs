//! `florn replay` of a circuit scenario's record (RFC 013 rule 20; checklist B-48): the runner's
//! words and exit codes for a record another Host made, whose chains differ from this Host's.
//! PR-42 made the comparison before any circuit scenario existed; here it is the real one, the
//! closed loop `motor-loop` over the shipped modules.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use florn_host::record::{self, Contents};

fn florn() -> Command {
    Command::new(env!("CARGO_BIN_EXE_florn"))
}

fn scenarios() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios")
}

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("florn-circuit-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn word(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("replay = "))
        .unwrap_or("")
        .to_string()
}

fn run_motor_loop(out: &Path, extra: &[&str]) {
    let s = scenarios();
    let done = florn()
        .args(["run", s.join("motor-loop.toml").to_str().unwrap()])
        .args(["--modules", s.join("modules").to_str().unwrap()])
        .args(["--seed", "1", "--out", out.to_str().unwrap()])
        .args(extra)
        .output()
        .unwrap();
    assert_eq!(
        done.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&done.stderr)
    );
}

fn replay(record: &Path) -> Output {
    let s = scenarios();
    florn()
        .args(["replay", record.to_str().unwrap()])
        .arg(s.join("motor-loop.toml"))
        .args(["--modules", s.join("modules").to_str().unwrap()])
        .output()
        .unwrap()
}

/// The record with the given changes: another Host's environment and a chain that differs, as asked.
fn altered(from: &Path, to: &Path, another_host: bool, chain_bit: bool) {
    let read = record::read(&std::fs::read(from).unwrap()).unwrap();
    let mut run = read.run.clone();
    if another_host {
        run.environment.host_build = "browser".into();
        run.environment.engine = "browser-wasm".into();
    }
    if chain_bit {
        let last = run.chain.len() - 1;
        run.chain[last].1[0] ^= 1;
    }
    let bytes = record::write(&Contents {
        run: Some(run),
        trace: read.trace.clone(),
        payloads: Some(read.payloads.clone()),
        log: Default::default(),
        readme: None,
    })
    .unwrap();
    std::fs::write(to, bytes).unwrap();
}

#[test]
fn a_circuit_record_exported_without_payloads_needs_them_across_hosts_and_exits_1() {
    let d = dir("needs");
    let record = d.join("loop.florn-run");
    // Exported with the trace but without the payloads: the preview of RFC 005 rule 30.
    run_motor_loop(&record, &["--trace"]);
    // Its own Host's replay agrees: the chains agree, so the payloads are not needed.
    let out = replay(&record);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(word(&out), "matched");
    // Another Host's: its chain differs, and the comparison within a tolerance needs both runs'
    // payload bytes, which this record does not hold.
    let other = d.join("browser.florn-run");
    altered(&record, &other, true, true);
    let out = replay(&other);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(word(&out), "needs-payloads");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("replay.needs-payloads"), "{err}");
}

#[test]
fn the_same_record_on_one_build_is_diverged_and_with_payloads_nothing_explains_the_chain() {
    let d = dir("one-build");
    let bare = d.join("bare.florn-run");
    run_motor_loop(&bare, &["--trace"]);
    // The same build: no tolerance is asked for, so a chain that differs is `diverged` (exit 2).
    let same = d.join("same.florn-run");
    altered(&bare, &same, false, true);
    let out = replay(&same);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(word(&out), "diverged");
    // With the payloads, across Hosts, a chain bit that no record explains is `diverged` too.
    let full = d.join("full.florn-run");
    run_motor_loop(&full, &["--trace", "--payloads"]);
    let other = d.join("other.florn-run");
    altered(&full, &other, true, true);
    let out = replay(&other);
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(word(&out), "diverged");
    // Another Host with the chain intact is a match, whatever its environment says.
    let intact = d.join("intact.florn-run");
    altered(&full, &intact, true, false);
    let out = replay(&intact);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(word(&out), "matched");
}
