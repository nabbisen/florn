//! The SDK's guests through the runner (RFC 002 §10; RFC 003 §10; AC-005, AC-007's native half):
//! the example modules are built for `wasm32-unknown-unknown` with the Rust toolchain alone,
//! then admitted and run. The echo guest echoes and reports; each hostile guest faults with its
//! kind while the neighbour continues; the WASI importer is refused at admission.
use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// Build the SDK's examples once, into a target directory of their own, and return it.
fn guests() -> PathBuf {
    let ws = workspace();
    let target = ws.join("target").join("guests");
    let status = Command::new(env!("CARGO"))
        .current_dir(&ws)
        .args([
            "build",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
            "-p",
            "florn-guest",
            "--examples",
            "--target-dir",
        ])
        .arg(&target)
        .status()
        .expect("cargo runs");
    assert!(
        status.success(),
        "the example guests build with the Rust toolchain alone"
    );
    target
        .join("wasm32-unknown-unknown")
        .join("release")
        .join("examples")
}

fn florn() -> Command {
    Command::new(env!("CARGO_BIN_EXE_florn"))
}

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("florn-sdk-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn value<'a>(stdout: &'a str, key: &str) -> Option<&'a str> {
    stdout
        .lines()
        .find_map(|l| l.strip_prefix(&format!("{key} = ")))
}

/// A scenario over SDK guests: `(id, example name, ports)`; connections as port pairs.
fn scenario(
    guests: &Path,
    end: u64,
    actors: &[(u32, &str, u32)],
    connections: &[Vec<(u32, u32)>],
) -> String {
    let mut t = format!(
        "florn = 1\nname = \"sdk\"\ndomains = [\"core\"]\n\n[time]\ntick = \"1ns\"\nend = \"{end}ns\"\n\n[order]\ntie = \"by-identity\"\n\n[seed]\nroot = 3\n\n[limits.default]\nmemory = \"2MiB\"\n"
    );
    let mut names: Vec<&str> = actors.iter().map(|a| a.1).collect();
    names.sort();
    names.dedup();
    for n in names {
        let bytes = std::fs::read(guests.join(format!("{n}.wasm"))).unwrap();
        t.push_str(&format!(
            "\n[modules.{n}]\nsha256 = \"{}\"\ncontract = \"florn:core/guest@0.1.0\"\n",
            florn_host::run::sha256_hex(&bytes)
        ));
    }
    for (id, name, ports) in actors {
        let ports: Vec<String> = (0..*ports)
            .map(|p| format!("{{ id = {p}, name = \"p{p}\", type = \"core/port\" }}"))
            .collect();
        t.push_str(&format!("\n[[actors]]\nid = {id}\nname = \"a{id}\"\nkind = \"module\"\nmodule = \"{name}\"\nports = [{}]\n", ports.join(", ")));
    }
    for (i, c) in connections.iter().enumerate() {
        let ports: Vec<String> = c.iter().map(|(a, p)| format!("\"a{a}.p{p}\"")).collect();
        t.push_str(&format!(
            "\n[[connections]]\nid = {}\nname = \"c{i}\"\nports = [{}]\n",
            100 + i,
            ports.join(", ")
        ));
    }
    t
}

#[test]
fn the_sdk_guests_admit_and_run() {
    let guests = guests();
    let d = dir("guests");
    // AC-005 on an SDK guest: the declared parameters are listed.
    let out = florn()
        .args(["admit", guests.join("echo.wasm").to_str().unwrap()])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(value(&stdout, "kind"), Some("echo"));
    assert!(stdout.contains("param period: u64, default 10"), "{stdout}");
    assert!(
        stdout.contains("param label: text, default echo"),
        "{stdout}"
    );
    assert!(stdout.contains("channel 1: draw"), "{stdout}");
    assert!(stdout.contains("needs pending-timers = 4"), "{stdout}");
    assert!(stdout.contains("needs memory = 1048576"), "{stdout}");
    // The WASI importer is refused at admission (RFC 002 §10, no ambient access).
    let out = florn()
        .args(["admit", guests.join("hostile_wasi.wasm").to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "module.import-refused: The module imports wasi:cli/environment@0.2.0.get-environment"
        ),
        "{err}"
    );
    // Two echoes connected: messages bounce for ever once a probe sends one; here the timers
    // report draws and the trace fills. The run ends normally and both stay alive.
    let text = scenario(
        &guests,
        1_000,
        &[(1, "echo", 1), (2, "echo", 1), (3, "probe", 0)],
        &[vec![(1, 0), (2, 0)]],
    );
    let scen = d.join("echo.toml");
    std::fs::write(&scen, text).unwrap();
    let record = d.join("echo.florn-run");
    let out = florn()
        .args([
            "run",
            scen.to_str().unwrap(),
            "--modules",
            guests.to_str().unwrap(),
            "--seed",
            "1",
            "--out",
            record.to_str().unwrap(),
            "--trace",
            "--payloads",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    assert_eq!(value(&stdout, "ended"), Some("end-reached"));
    let chain = value(&stdout, "chain").unwrap().to_string();
    assert!(!stderr.contains("actor.faulted"), "{stderr}");
    let records: u64 = value(&stdout, "records").unwrap().parse().unwrap();
    assert!(
        records > 200,
        "{records}: the echoes' timers fired every 10 ticks and the probe once"
    );
    // The record replays to a match: the SDK guest is deterministic under the Host (AC-001's native half).
    let replay = florn()
        .args([
            "replay",
            record.to_str().unwrap(),
            scen.to_str().unwrap(),
            "--modules",
            guests.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let rs = String::from_utf8_lossy(&replay.stdout);
    assert_eq!(
        value(&rs, "replay"),
        Some("matched"),
        "{}",
        String::from_utf8_lossy(&replay.stderr)
    );
    assert_eq!(value(&rs, "chain"), Some(chain.as_str()));
    // The NaN probe reported five patterns; they are in the record's payloads.
    let read = florn_host::record::read(&std::fs::read(&record).unwrap()).unwrap();
    assert!(
        read.payloads
            .values()
            .filter(|p| p.len() > 8 && p[..4] == 3u32.to_le_bytes() && &p[4..7] == b"0/0")
            .count()
            == 1,
        "the probe's 0/0 report"
    );
    // Each hostile guest faults with its kind; the echo neighbour continues to the end.
    for (name, fault) in [
        ("hostile_loop", "fault.budget"),
        ("hostile_recurse", "fault.stack"),
        ("hostile_grow", "fault.trap"),
        ("hostile_flood", "fault.quota"),
        ("hostile_bad_pointer", "fault.contract"),
    ] {
        let text = scenario(&guests, 200, &[(1, name, 1), (2, "echo", 0)], &[]);
        let scen = d.join(format!("{name}.toml"));
        std::fs::write(&scen, text).unwrap();
        let out = florn()
            .args([
                "run",
                scen.to_str().unwrap(),
                "--modules",
                guests.to_str().unwrap(),
                "--seed",
                "1",
                "--out",
                d.join(format!("{name}.florn-run")).to_str().unwrap(),
            ])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(0), "{name}: {stderr}");
        assert_eq!(value(&stdout, "ended"), Some("end-reached"), "{name}");
        let line = stderr
            .lines()
            .find(|l| l.contains("actor.faulted"))
            .unwrap_or_else(|| panic!("{name}: no fault\n{stderr}"));
        assert!(
            line.contains(&format!("Actor 1 faulted with {fault} after")),
            "{name}: {line}"
        );
        assert!(line.contains("the policy is drop"), "{line}");
        assert!(
            stderr
                .lines()
                .filter(|l| l.contains("actor.faulted"))
                .count()
                == 1,
            "{name}: one fault\n{stderr}"
        );
    }
}
