//! The headless runner end to end (RFC 005 rule 31; AC-005; AUTO-001, AUTO-002): `admit` on a
//! module without a manifest and on a guest; `run` prints the chain and writes the record; the
//! record `replay`s to a match and a second `run` with the same seed prints the same chain;
//! `probe` prints the capacity.
use std::path::{Path, PathBuf};
use std::process::Command;

use florn_test_guests::{Guest, ScenarioSpec, actor};

fn florn() -> Command {
    Command::new(env!("CARGO_BIN_EXE_florn"))
}

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("florn-run-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write_scenario(d: &Path) -> (PathBuf, PathBuf) {
    let spec = ScenarioSpec {
        end: 3_000,
        tie: "by-seed",
        actors: vec![actor(1, Guest::Pinger, 1), actor(2, Guest::Echo, 1)],
        connections: vec![vec![(1, 0), (2, 0)]],
        seed: 5,
    };
    let scenario = d.join("two.toml");
    std::fs::write(&scenario, spec.text()).unwrap();
    let modules = d.join("modules");
    std::fs::create_dir_all(&modules).unwrap();
    for (name, bytes) in spec.modules() {
        std::fs::write(modules.join(format!("{name}.wasm")), bytes).unwrap();
    }
    (scenario, modules)
}

fn value<'a>(stdout: &'a str, key: &str) -> Option<&'a str> {
    stdout
        .lines()
        .find_map(|l| l.strip_prefix(&format!("{key} = ")))
}

#[test]
fn ac_005_admit_prints_the_refusal_or_the_manifest() {
    let d = dir("admit");
    // A valid guest of the core world without its two custom sections.
    let bare = d.join("bare.wasm");
    std::fs::write(&bare, wat::parse_str(Guest::Echo.wat()).unwrap()).unwrap();
    let out = florn()
        .args(["admit", bare.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "error: module.manifest-missing: The module has no contract identity section."
        ),
        "{err}"
    );
    assert!(
        err.contains("error: module.manifest-missing: The module has no florn manifest section."),
        "{err}"
    );
    let echo = d.join("echo.wasm");
    std::fs::write(&echo, Guest::Echo.bytes()).unwrap();
    let out = florn()
        .args(["admit", echo.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(value(&stdout, "contract"), Some("florn:core/guest@0.1.0"));
    assert_eq!(value(&stdout, "kind"), Some("echo"));
    assert!(
        stdout.contains("port-kind p: type core/port, 0 to 8"),
        "{stdout}"
    );
    assert!(stdout.contains("param period: u64, default 10"), "{stdout}");
    assert!(stdout.contains("channel 1: r"), "{stdout}");
    assert!(stdout.contains("needs memory = 131072"), "{stdout}");
    assert_eq!(value(&stdout, "admitted").map(str::len), Some(64));
    assert!(String::from_utf8_lossy(&out.stderr).contains("info: admission.result:"));
}

#[test]
fn run_replay_and_a_second_run_agree() {
    let d = dir("run");
    let (scenario, modules) = write_scenario(&d);
    let record = d.join("two.florn-run");
    let out = florn()
        .args([
            "run",
            scenario.to_str().unwrap(),
            "--modules",
            modules.to_str().unwrap(),
            "--seed",
            "5",
            "--out",
            record.to_str().unwrap(),
            "--trace",
        ])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let chain = value(&stdout, "chain").expect("chain").to_string();
    assert_eq!(chain.len(), 64);
    assert_eq!(value(&stdout, "seed"), Some("5"));
    assert_eq!(value(&stdout, "ended"), Some("end-reached"));
    assert!(record.is_file());
    // The same inputs, the same chain (AUTO-002).
    let again = florn()
        .args([
            "run",
            scenario.to_str().unwrap(),
            "--modules",
            modules.to_str().unwrap(),
            "--seed",
            "5",
            "--out",
            d.join("again.florn-run").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(
        value(&String::from_utf8_lossy(&again.stdout), "chain"),
        Some(chain.as_str())
    );
    // Another seed, another chain; a chosen seed is printed.
    let other = florn()
        .args([
            "run",
            scenario.to_str().unwrap(),
            "--modules",
            modules.to_str().unwrap(),
            "--out",
            d.join("other.florn-run").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let other_out = String::from_utf8_lossy(&other.stdout);
    assert!(value(&other_out, "seed").is_some());
    assert_ne!(value(&other_out, "chain"), Some(chain.as_str()));
    // Replay the record: matched, exit 0.
    let replay = florn()
        .args([
            "replay",
            record.to_str().unwrap(),
            scenario.to_str().unwrap(),
            "--modules",
            modules.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let rs = String::from_utf8_lossy(&replay.stdout);
    assert_eq!(
        replay.status.code(),
        Some(0),
        "{}\n{}",
        rs,
        String::from_utf8_lossy(&replay.stderr)
    );
    assert_eq!(value(&rs, "replay"), Some("matched"));
    assert_eq!(value(&rs, "chain"), Some(chain.as_str()));
    assert!(
        String::from_utf8_lossy(&replay.stderr)
            .contains("info: replay.matched: The replay matched the record at every one of")
    );
    // An altered module: its bytes hash to something the scenario does not reference.
    let mut echo = std::fs::read(modules.join("echo.wasm")).unwrap();
    echo.extend_from_slice(&[0, 1, 0]); // a custom section: valid, different bytes
    let altered = d.join("altered");
    std::fs::create_dir_all(&altered).unwrap();
    std::fs::write(altered.join("echo.wasm"), echo).unwrap();
    std::fs::copy(modules.join("pinger.wasm"), altered.join("pinger.wasm")).unwrap();
    let out = florn()
        .args([
            "run",
            scenario.to_str().unwrap(),
            "--modules",
            altered.to_str().unwrap(),
            "--seed",
            "5",
            "--out",
            d.join("x.florn-run").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("missing module:"), "{err}");
    assert!(
        err.contains("echo.wasm hashes to")
            && err.contains("which the scenario does not reference"),
        "{err}"
    );
    // Missing modules are named.
    let out = florn()
        .args(["run", scenario.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("missing module:"));
}

#[test]
fn probe_prints_the_capacity_and_bad_arguments_print_usage() {
    let out = florn()
        .args(["probe", "--instances", "12", "--memory", "1024"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(value(&stdout, "instances"), Some("12"));
    assert_eq!(value(&stdout, "memory"), Some("1024"));
    assert!(value(&stdout, "engine").unwrap().starts_with("wasmtime "));
    let out = florn().args(["dance"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("usage:"));
    let out = florn()
        .args(["run", "x.toml", "--seed", "abc"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn a_diverged_replay_exits_with_2() {
    let d = dir("diverged");
    let (scen, modules) = write_scenario(&d);
    let record = d.join("a.florn-run");
    let out = florn()
        .args([
            "run",
            scen.to_str().unwrap(),
            "--modules",
            modules.to_str().unwrap(),
            "--seed",
            "5",
            "--out",
            record.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Alter one stored chain point and write the record back.
    let read = florn_host::record::read(&std::fs::read(&record).unwrap()).unwrap();
    let mut run = read.run.clone();
    let n = run.chain.len();
    assert!(n >= 1, "the end point is always stored");
    run.chain[n - 1].1[0] ^= 1;
    let bytes = florn_host::record::write(&florn_host::record::Contents {
        run: Some(run),
        trace: None,
        payloads: None,
        log: florn_host::DiagnosticLog::default(),
        readme: None,
    })
    .unwrap();
    let altered = d.join("altered.florn-run");
    std::fs::write(&altered, bytes).unwrap();
    let out = florn()
        .args([
            "replay",
            altered.to_str().unwrap(),
            scen.to_str().unwrap(),
            "--modules",
            modules.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        value(&String::from_utf8_lossy(&out.stdout), "replay"),
        Some("diverged")
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("replay.diverged"));
}

#[test]
fn help_prints_the_usage_and_exits_with_0() {
    let out = florn().arg("help").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("florn run") && text.contains("exit codes"),
        "{text}"
    );
}
