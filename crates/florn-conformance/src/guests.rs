//! The kit's guests: the SDK's example modules, built for `wasm32-unknown-unknown` with the Rust
//! toolchain alone (RFC 002 rule 51), and two modules written in text for what a Rust guest
//! cannot express.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use wasm_encoder::{CustomSection, Section};

/// The SDK's example modules, by example name; the module key of a scenario is this name.
pub const SDK: &[&str] = &[
    "echo",
    "probe",
    "hostile_loop",
    "hostile_recurse",
    "hostile_grow",
    "hostile_flood",
    "hostile_bad_pointer",
    "hostile_wasi",
    "pinger",
    "bench_protocol",
    "bench_compute",
    "timers",
    "chatter",
    "heap",
    "clock",
    "net_echo",
];

/// The workspace root.
pub fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root")
}

/// The directory holding the built guests: `FLORN_GUESTS` when set, else a build into
/// `target/guests` of the workspace, made once per process.
pub fn dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| match std::env::var_os("FLORN_GUESTS") {
        Some(d) => PathBuf::from(d),
        None => build(&workspace().join("target").join("guests")),
    })
}

/// Build the SDK's examples for the guest target into `target_dir`; the examples directory.
pub fn build(target_dir: &Path) -> PathBuf {
    let cargo = option_env!("CARGO").unwrap_or("cargo");
    let status = Command::new(cargo)
        .current_dir(workspace())
        .args([
            "build",
            "--quiet",
            "--locked",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
            "-p",
            "florn-guest",
            "-p",
            "florn-guest-network",
            "--examples",
            "--target-dir",
        ])
        .arg(target_dir)
        .status()
        .expect("cargo runs");
    assert!(
        status.success(),
        "the SDK's example guests build with the Rust toolchain alone"
    );
    target_dir
        .join("wasm32-unknown-unknown")
        .join("release")
        .join("examples")
}

/// The bytes of a built guest.
pub fn bytes(name: &str) -> Vec<u8> {
    let path = dir().join(format!("{name}.wasm"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The SHA-256 of every SDK guest, by name (RFC 004 rule 16).
pub fn hashes() -> BTreeMap<String, String> {
    SDK.iter()
        .map(|n| (n.to_string(), florn_host::run::sha256_hex(&bytes(n))))
        .collect()
}

fn with_sections(wat: &str, kind: &str) -> Vec<u8> {
    let mut bytes = wat::parse_str(wat).expect("the guest parses");
    CustomSection {
        name: "component-type:florn-core-guest".into(),
        data: florn_test_guests::contract_section().into(),
    }
    .append_to(&mut bytes);
    let manifest = format!(
        "florn-module = 1\nworld = \"florn:core/guest@0.1.0\"\nkind = \"{kind}\"\n\n[[ports]]\nkind = \"p\"\ntype = \"core/port\"\nmin = 0\nmax = 8\n\n[resources]\nmemory = \"256KiB\"\n"
    );
    CustomSection {
        name: "florn.manifest".into(),
        data: manifest.into_bytes().into(),
    }
    .append_to(&mut bytes);
    bytes
}

/// A conforming module without either custom section: refused as `module.manifest-missing`.
pub fn bare() -> Vec<u8> {
    wat::parse_str(
        r#"
(module
  (memory (export "memory") 1)
  (func (export "cabi_realloc") (param i32 i32 i32 i32) (result i32) (i32.const 1024))
  (func (export "florn:core/actor@0.1.0#init") (param i64 i32 i32 i32 i32 i32))
  (func (export "florn:core/actor@0.1.0#on-message") (param i64 i32 i32 i32))
  (func (export "florn:core/actor@0.1.0#on-timer") (param i64 i32)))
"#,
    )
    .expect("the bare module parses")
}

/// A guest whose `cabi_realloc` loops for ever: the Host's lowering of the first message runs
/// it under the budget (RFC 003 rule 11), so the activation faults as `budget`.
pub fn realloc_loop() -> Vec<u8> {
    with_sections(
        r#"
(module
  (import "florn:core/host@0.1.0" "set-timer" (func $set_timer (param i32 i64)))
  (memory (export "memory") 1)
  (global $n (mut i32) (i32.const 0))
  (func (export "cabi_realloc") (param i32 i32 i32 i32) (result i32)
    (loop $l (global.set $n (i32.add (global.get $n) (i32.const 1))) (br $l))
    (i32.const 1024))
  (func (export "florn:core/actor@0.1.0#init") (param i64 i32 i32 i32 i32 i32))
  (func (export "florn:core/actor@0.1.0#on-message") (param i64 i32 i32 i32))
  (func (export "florn:core/actor@0.1.0#on-timer") (param i64 i32)))
"#,
        "realloc-loop",
    )
}

/// The crossing benchmark's guest (RFC 005 rule 11): `on-timer` returns at once (the call
/// floor); `on-message` sends its payload back through the same port (one crossing in, one
/// out). Its allocator hands out one fixed buffer, so the memory does not grow with the calls.
pub fn crossing() -> Vec<u8> {
    with_sections(
        r#"
(module
  (import "florn:core/host@0.1.0" "send" (func $send (param i32 i32 i32)))
  (import "florn:core/host@0.1.0" "set-timer" (func $set_timer (param i32 i64)))
  (memory (export "memory") 2)
  (func (export "cabi_realloc") (param i32 i32 i32 i32) (result i32)
    (block $ok (loop $grow
      (br_if $ok (i32.le_u (i32.add (i32.const 4096) (local.get 3)) (i32.mul (memory.size) (i32.const 65536))))
      (drop (memory.grow (i32.const 1)))
      (br $grow)))
    (i32.const 4096))
  (func (export "florn:core/actor@0.1.0#init") (param i64 i32 i32 i32 i32 i32))
  (func (export "florn:core/actor@0.1.0#on-message") (param i64 i32 i32 i32)
    (call $send (local.get 1) (local.get 2) (local.get 3)))
  (func (export "florn:core/actor@0.1.0#on-timer") (param i64 i32)))
"#,
        "crossing",
    )
}
