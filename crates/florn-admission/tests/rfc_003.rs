//! RFC 003 §10, the rows that need no run: transformed bytes recorded for the admission version,
//! custom sections preserved, every refusal of §2 with its code, the estimate before anything
//! exists; and RFC 002 §3: the manifest schema and the config encoder.

use std::collections::BTreeMap;

use florn_admission::config::{NoUnits, Units, encode_config};
use florn_admission::estimate::{Capacity, ClassLimits, ModuleFacts, estimate};
use florn_admission::manifest::parse_manifest;
use florn_admission::validate::ModuleMaxima;
use florn_admission::{Admission, Errors, FaultClass, GlobalKind, WorldSpec, classify, sha256};
use florn_model::{Decimal, DomainValue, HostMaxima, Manifest, TickLength};
use wasm_encoder::{CustomSection, Section};

/// A well-formed guest of the core world: the exports with their canonical signatures, a mutable
/// global, a function table, a data segment, and nothing it should not have.
const GOOD: &str = r#"
(module
  (import "florn:core/host@0.1.0" "send" (func $send (param i32 i32 i32)))
  (import "florn:core/host@0.1.0" "random" (func $random (result i64)))
  (memory (export "memory") 2 16)
  (data (i32.const 1024) "florn")
  (global $counter (mut i32) (i32.const 0))
  (global $ro i32 (i32.const 7))
  (global $acc (mut i64) (i64.const 0))
  (table 2 2 funcref)
  (elem (i32.const 0) $f0 $f1)
  (type $ret (func (result i32)))
  (func $f0 (result i32) (i32.const 10))
  (func $f1 (result i32) (i32.const 20))
  (func (export "cabi_realloc") (param i32 i32 i32 i32) (result i32) (i32.const 2048))
  (func (export "florn:core/actor@0.1.0#init") (param i64 i32 i32 i32 i32 i32))
  (func (export "florn:core/actor@0.1.0#on-message") (param i64 i32 i32 i32)
    (global.set $counter (i32.add (global.get $counter) (i32.const 1)))
    (global.set $acc (i64.add (global.get $acc) (call $random)))
    (drop (call_indirect (type $ret) (i32.const 1)))
    (call $send (local.get 1) (local.get 2) (local.get 3)))
  (func (export "florn:core/actor@0.1.0#on-timer") (param i64 i32)))
"#;

const MANIFEST: &str = r#"
florn-module = 1
world = "florn:core/guest@0.1.0"
kind = "echo"
description = "Echoes every message"
sdk = "hand-written"

[[ports]]
kind = "eth"
type = "network/ethernet"
min = 1
max = 4

[[params]]
name = "label"
type = "text"
required = true

[[params]]
name = "hello"
type = "ticks"
default = "10ms"

[[params]]
name = "cost"
type = "u64"
default = 10

[[params]]
name = "price"
type = "integer"
unit = "0.01USD"
default = "12.34USD"

[[params]]
name = "flag"
type = "bool"

[[telemetry]]
channel = 1
name = "state"
fields = [{ name = "neighbour", type = "u64" }, { name = "up", type = "bool" }]

[resources]
memory = "1MiB"
pending-timers = 16
"#;

/// The contract-identity section as the bindings generator writes it, from the contract of record.
fn contract_section() -> Vec<u8> {
    let wit = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../contracts/florn-core/wit/core.wit"
    ))
    .expect("the contract of record");
    contract_section_of(&wit)
}

fn contract_section_of(wit: &str) -> Vec<u8> {
    let mut resolve = wit_parser::Resolve::default();
    let pkg = resolve
        .push_str("core.wit", wit)
        .expect("the contract parses");
    let world = resolve
        .select_world(&[pkg], Some("guest"))
        .expect("world guest");
    wit_component::metadata::encode(
        &resolve,
        world,
        wit_component::StringEncoding::UTF8,
        None,
        false,
    )
    .expect("encoded")
}

fn with_sections(wat: &str, manifest: Option<&str>, contract: bool) -> Vec<u8> {
    let mut bytes = wat::parse_str(wat).expect("the guest parses");
    if contract {
        CustomSection {
            name: "component-type:florn-core-guest".into(),
            data: contract_section().into(),
        }
        .append_to(&mut bytes);
    }
    if let Some(m) = manifest {
        CustomSection {
            name: "florn.manifest".into(),
            data: m.as_bytes().into(),
        }
        .append_to(&mut bytes);
    }
    bytes
}

fn good() -> Vec<u8> {
    with_sections(GOOD, Some(MANIFEST), true)
}

fn codes(e: &Errors) -> Vec<&'static str> {
    e.codes()
}

fn custom_sections(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    wasmparser::Parser::new(0)
        .parse_all(bytes)
        .filter_map(|p| match p {
            Ok(wasmparser::Payload::CustomSection(c)) => {
                Some((c.name().to_string(), c.data().to_vec()))
            }
            _ => None,
        })
        .collect()
}

fn imports(bytes: &[u8]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for p in wasmparser::Parser::new(0).parse_all(bytes) {
        if let Ok(wasmparser::Payload::ImportSection(s)) = p {
            for i in s {
                if let wasmparser::Imports::Single(_, i) = i.unwrap() {
                    out.push((i.module.to_string(), i.name.to_string()));
                }
            }
        }
    }
    out
}

fn exports(bytes: &[u8]) -> Vec<(String, wasmparser::ExternalKind)> {
    let mut out = Vec::new();
    for p in wasmparser::Parser::new(0).parse_all(bytes) {
        if let Ok(wasmparser::Payload::ExportSection(s)) = p {
            for e in s {
                let e = e.unwrap();
                out.push((e.name.to_string(), e.kind));
            }
        }
    }
    out
}

/// The admitted hash of the kit guest under the default class limits, admission version 1,
/// cost-model version 1. A change to any pass, to the library, or to the guest changes it; that is
/// the point (RFC 003 rule 10). Update only with a new admission version.
const RECORDED_ADMITTED_HASH: &str =
    "4575dd3408bac722eae0d9bb7a5cdf92a9e2be6cd59b2810e610547fb6aa0ee5";

#[test]
fn the_good_guest_is_admitted_and_the_hash_is_recorded() {
    let source = good();
    let mut adm = Admission::default();
    let a = adm
        .admit(&source, &ClassLimits::defaults())
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(a.source_sha256, sha256(&source));
    assert_eq!(a.admission_version, 1);
    assert_eq!(a.cost_model_version, 1);
    assert_eq!(a.manifest.kind, "echo");
    assert_eq!(a.world, WorldSpec::core());
    assert_eq!(a.memory_initial_pages, 2);
    // The state layout: the two mutable source globals (not the immutable one, not the height
    // global, not the imported budget), the one table.
    assert_eq!(
        a.layout.globals,
        vec![
            ("florn.global.1".to_string(), GlobalKind::I32),
            ("florn.global.3".to_string(), GlobalKind::I64)
        ]
    );
    assert_eq!(a.layout.tables, vec![("florn.table.0".to_string(), 2)]);
    let ex = exports(&a.bytes);
    assert!(
        ex.contains(&("florn.stack".to_string(), wasmparser::ExternalKind::Global)),
        "{ex:?}"
    );
    assert!(ex.contains(&("memory".to_string(), wasmparser::ExternalKind::Memory)));
    // The admitted module imports the budget global and nothing else new.
    assert_eq!(
        imports(&a.bytes),
        [
            ("florn:core/host@0.1.0", "send"),
            ("florn:core/host@0.1.0", "random"),
            ("env", "gas_counter")
        ]
        .map(|(m, n)| (m.to_string(), n.to_string()))
    );
    let hex = a.admitted_hex();
    if RECORDED_ADMITTED_HASH.is_empty() {
        panic!("record the admitted hash: {hex}");
    }
    assert_eq!(
        hex, RECORDED_ADMITTED_HASH,
        "the transformation changed without a new admission version"
    );
    // The cache: the same source under the same limits transforms once; other limits, again.
    adm.admit(&source, &ClassLimits::defaults()).unwrap();
    assert_eq!(adm.transformed(), 1);
    let other = ClassLimits {
        stack: 2_048,
        ..ClassLimits::defaults()
    };
    let b = adm.admit(&source, &other).unwrap();
    assert_eq!(adm.transformed(), 2);
    assert_ne!(
        b.admitted_sha256, a.admitted_sha256,
        "the stack limit is baked into the module"
    );
}

#[test]
fn custom_sections_are_preserved_byte_for_byte() {
    let source = good();
    let a = Admission::default()
        .admit(&source, &ClassLimits::defaults())
        .unwrap();
    let before = custom_sections(&source);
    let after = custom_sections(&a.bytes);
    // The text format adds a `name` section; the two sections of RFC 002 rule 11 follow it.
    assert_eq!(
        before.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        ["name", "component-type:florn-core-guest", "florn.manifest"]
    );
    assert_eq!(before, after);
}

#[test]
fn the_memory_maximum_is_capped_and_a_lower_declared_maximum_is_kept() {
    let limits = ClassLimits {
        memory_bytes: 8 * 65_536,
        ..ClassLimits::defaults()
    };
    let a = Admission::default().admit(&good(), &limits).unwrap();
    let max = wasmparser::Parser::new(0)
        .parse_all(&a.bytes)
        .find_map(|p| match p {
            Ok(wasmparser::Payload::MemorySection(s)) => {
                s.into_iter().next().map(|m| m.unwrap().maximum)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(max, Some(8));
    let a = Admission::default()
        .admit(&good(), &ClassLimits::defaults())
        .unwrap();
    let max = wasmparser::Parser::new(0)
        .parse_all(&a.bytes)
        .find_map(|p| match p {
            Ok(wasmparser::Payload::MemorySection(s)) => {
                s.into_iter().next().map(|m| m.unwrap().maximum)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(
        max,
        Some(16),
        "the declared maximum of 16 pages is below the 64-page limit"
    );
}

fn refused(wat: &str) -> Vec<&'static str> {
    let bytes = with_sections(wat, Some(MANIFEST), true);
    codes(
        &Admission::default()
            .admit(&bytes, &ClassLimits::defaults())
            .expect_err("refused"),
    )
}

#[test]
fn every_refusal_of_section_2_has_its_code() {
    // A start section.
    assert_eq!(
        refused(&GOOD.replace(
            "(table 2 2 funcref)",
            "(table 2 2 funcref) (start $f2) (func $f2)"
        )),
        ["module.start-section"]
    );
    // A second memory.
    assert_eq!(
        refused(&GOOD.replace(
            "(memory (export \"memory\") 2 16)",
            "(memory (export \"memory\") 2 16) (memory 1)"
        )),
        ["module.memory"]
    );
    // A `wasi:` import.
    let wasi = GOOD.replace("(memory (export", "(import \"wasi:cli/environment@0.2.0\" \"get-environment\" (func (param i32)))\n  (memory (export");
    assert_eq!(refused(&wasi), ["module.import-refused"]);
    // A SIMD instruction.
    let simd = GOOD.replace(
        "(global.set $counter (i32.add (global.get $counter) (i32.const 1)))",
        "(drop (i32x4.extract_lane 0 (v128.const i32x4 1 2 3 4)))",
    );
    assert_eq!(refused(&simd), ["module.feature-refused"]);
    // An import named `florn.stack`, and the budget import.
    let stack = GOOD.replace(
        "(memory (export",
        "(import \"env\" \"florn.stack\" (global (mut i32)))\n  (memory (export",
    );
    assert_eq!(refused(&stack), ["module.import-refused"]);
    let gas = GOOD.replace(
        "(memory (export",
        "(import \"env\" \"gas_counter\" (global (mut i64)))\n  (memory (export",
    );
    assert_eq!(refused(&gas), ["module.import-refused"]);
    // An export under `florn.`.
    assert_eq!(
        refused(&GOOD.replace(
            "(global $ro i32",
            "(global $ro (export \"florn.global.9\") i32"
        )),
        ["module.export-reserved"]
    );
    // `table.set`.
    let tset = GOOD.replace(
        "(drop (call_indirect (type $ret) (i32.const 1)))",
        "(table.set 0 (i32.const 0) (ref.func $f1))",
    );
    assert_eq!(refused(&tset), ["module.table-mutation"]);
    // Initial memory above the class limit.
    let big = GOOD.replace(
        "(memory (export \"memory\") 2 16)",
        "(memory (export \"memory\") 100)",
    );
    assert_eq!(refused(&big), ["module.initial-memory-above-limit"]);
    // A missing export, a wrong signature, a wrong import signature.
    assert_eq!(
        refused(&GOOD.replace(
            "(func (export \"florn:core/actor@0.1.0#on-timer\") (param i64 i32))",
            ""
        )),
        ["module.export-missing"]
    );
    assert_eq!(
        refused(&GOOD.replace(
            "(func (export \"florn:core/actor@0.1.0#on-timer\") (param i64 i32))",
            "(func (export \"florn:core/actor@0.1.0#on-timer\") (param i64 i64))"
        )),
        ["module.export-missing"]
    );
    assert_eq!(
        refused(
            &GOOD
                .replace(
                    "\"random\" (func $random (result i64))",
                    "\"random\" (func $random (result i32))"
                )
                .replace(
                    "(i64.add (global.get $acc) (call $random))",
                    "(i64.extend_i32_u (call $random))"
                )
        ),
        ["module.import-refused"]
    );
    // Shared memory is a feature; an imported memory is a memory rule.
    assert_eq!(
        refused(&GOOD.replace(
            "(memory (export \"memory\") 2 16)",
            "(memory (export \"memory\") 2 16 shared)"
        )),
        ["module.feature-refused"]
    );
    assert_eq!(refused(&GOOD.replace("(memory (export \"memory\") 2 16)", "(import \"florn:core/host@0.1.0\" \"memory\" (memory 2 16)) (export \"memory\" (memory 0))")), ["module.memory", "module.import-refused"]);
    // Not exported as `memory`.
    assert_eq!(
        refused(&GOOD.replace(
            "(memory (export \"memory\") 2 16)",
            "(memory (export \"mem\") 2 16)"
        )),
        ["module.memory"]
    );
    // Not a module at all.
    let e = Admission::default()
        .admit(b"\0asm\x01\0\0\0\x05", &ClassLimits::defaults())
        .expect_err("refused");
    assert_eq!(e.codes(), ["module.invalid"]);
    // Too large.
    let mut adm = Admission::new(ModuleMaxima {
        bytes: 16,
        ..ModuleMaxima::default()
    });
    assert_eq!(
        adm.admit(&good(), &ClassLimits::defaults())
            .expect_err("refused")
            .codes(),
        ["module.too-large"]
    );
    let mut adm = Admission::new(ModuleMaxima {
        functions: 3,
        ..ModuleMaxima::default()
    });
    assert_eq!(
        adm.admit(&good(), &ClassLimits::defaults())
            .expect_err("refused")
            .codes(),
        ["module.too-large"]
    );
}

#[test]
fn the_manifest_sections_are_required_and_checked() {
    let no_manifest = with_sections(GOOD, None, true);
    let e = Admission::default()
        .admit(&no_manifest, &ClassLimits::defaults())
        .expect_err("refused");
    assert_eq!(e.codes(), ["module.manifest-missing"]);
    assert_eq!(
        e.0[0].params()[0].1,
        florn_model::Param::Enum("florn-manifest")
    );
    let no_contract = with_sections(GOOD, Some(MANIFEST), false);
    let e = Admission::default()
        .admit(&no_contract, &ClassLimits::defaults())
        .expect_err("refused");
    assert_eq!(e.codes(), ["module.manifest-missing"]);
    assert_eq!(
        e.0[0].params()[0].1,
        florn_model::Param::Enum("contract-identity")
    );
    let neither = with_sections(GOOD, None, false);
    assert_eq!(
        Admission::default()
            .admit(&neither, &ClassLimits::defaults())
            .expect_err("refused")
            .codes(),
        ["module.manifest-missing", "module.manifest-missing"]
    );
    // The manifest names a world this Host does not support.
    let other = MANIFEST.replace("florn:core/guest@0.1.0", "florn:network/router@0.1.0");
    let e = Admission::default()
        .admit(
            &with_sections(GOOD, Some(&other), true),
            &ClassLimits::defaults(),
        )
        .expect_err("refused");
    assert_eq!(e.codes(), ["module.contract-unsupported"]);
    // The contract identity encodes other interfaces than the world the manifest names.
    let narrower = "package florn:core@0.1.0;\ninterface host { send: func(port: u32, payload: list<u8>); }\nworld guest { import host; }\n";
    let mut bytes = wat::parse_str(GOOD).unwrap();
    CustomSection {
        name: "component-type:x".into(),
        data: contract_section_of(narrower).into(),
    }
    .append_to(&mut bytes);
    CustomSection {
        name: "florn.manifest".into(),
        data: MANIFEST.as_bytes().into(),
    }
    .append_to(&mut bytes);
    let e = Admission::default()
        .admit(&bytes, &ClassLimits::defaults())
        .expect_err("refused");
    assert_eq!(e.codes(), ["module.contract-mismatch"]);
    assert_eq!(
        e.0[0].params()[1].1,
        florn_model::Param::Text("imports florn:core/host@0.1.0; exports ".into())
    );
    // Unknown keys and a wrong schema version are schema errors, all reported.
    let bad = MANIFEST.replace("florn-module = 1", "florn-module = 2\nauthor = \"x\"");
    let e = Admission::default()
        .admit(
            &with_sections(GOOD, Some(&bad), true),
            &ClassLimits::defaults(),
        )
        .expect_err("refused");
    assert_eq!(
        e.codes(),
        ["module.manifest-schema", "module.manifest-schema"]
    );
}

#[test]
fn the_manifest_schema_of_rule_12() {
    let m = parse_manifest(MANIFEST).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(m.ports.len(), 1);
    assert_eq!(m.ports[0].ty, "network/ethernet");
    assert_eq!(
        m.params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        ["label", "hello", "cost", "price", "flag"]
    );
    assert_eq!(m.params[3].unit.as_deref(), Some("0.01USD"));
    assert_eq!(m.telemetry[0].fields.len(), 2);
    assert_eq!(m.resources.memory_bytes, Some(1024 * 1024));
    assert_eq!(m.resources.pending_timers, Some(16));
    let errs = |t: &str| {
        parse_manifest(t)
            .expect_err("refused")
            .0
            .into_iter()
            .map(|e| (e.code(), e.field.unwrap_or_default()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        errs(&MANIFEST.replace("type = \"text\"", "type = \"string\"")),
        [("module.manifest-schema", "params[0].type".to_string())]
    );
    assert_eq!(
        errs(&MANIFEST.replace("default = 10", "default = \"ten\"")),
        [("module.manifest-schema", "params[2].default".to_string())]
    );
    assert_eq!(
        errs(&MANIFEST.replace("channel = 1", "channel = 0")),
        [("module.manifest-schema", "telemetry[0].channel".to_string())]
    );
    assert_eq!(
        errs(&MANIFEST.replace("min = 1", "min = 9")),
        [("module.manifest-schema", "ports[0].min".to_string())]
    );
    assert_eq!(
        errs(&MANIFEST.replace("unit = \"0.01USD\"\n", "")),
        [("module.manifest-schema", "params[3].unit".to_string())]
    );
    assert_eq!(
        errs(&MANIFEST.replace("memory = \"1MiB\"", "memory = \"1.5KiB\"")),
        [("module.manifest-schema", "resources.memory".to_string())]
    );
}

const SCENARIO: &str = r#"
florn = 1
name = "two echoes"
domains = ["network"]

[time]
tick = "1ns"
end = "1s"

[order]
tie = "by-identity"

[seed]
root = 1

[limits.default]
memory = "4MiB"

[limits.small]
memory = "512KiB"
pending-timers = 8

[modules.echo]
sha256 = "SHA"
contract = "florn:core/guest@0.1.0"

[[actors]]
id = 1
name = "a"
kind = "module"
module = "echo"
ports = [{ id = 0, name = "eth0", type = "network/ethernet" }]
config = { label = "first", hello = "5ms", price = "0.07USD" }

[[actors]]
id = 2
name = "b"
kind = "module"
module = "echo"
class = "small"
ports = [{ id = 0, name = "eth0", type = "network/ethernet" }, { id = 1, name = "x", type = "network/serial" }]
config = { label = "second" }
"#;

fn scenario(text: &str, source: &[u8]) -> Manifest {
    let hex = florn_model::canonical::hex(&sha256(source));
    Manifest::parse(&text.replace("SHA", &hex), &HostMaxima::native())
        .unwrap_or_else(|e| panic!("{:?}", e.codes()))
}

fn facts(source: &[u8]) -> BTreeMap<String, ModuleFacts> {
    let a = Admission::default()
        .admit(source, &ClassLimits::defaults())
        .unwrap();
    BTreeMap::from([(
        "echo".to_string(),
        ModuleFacts {
            source_sha256: a.source_sha256,
            manifest: a.manifest.clone(),
            memory_initial_pages: a.memory_initial_pages,
        },
    )])
}

#[test]
fn the_estimate_checks_needs_ports_and_capacity_before_anything_exists() {
    let source = good();
    let s = scenario(SCENARIO, &source);
    let f = facts(&source);
    let host = HostMaxima::native();
    let big = Capacity {
        instances: 1_000,
        memory_bytes: 1 << 40,
    };
    let e = estimate(&s, &f, &ClassLimits::defaults(), &host, big).expect_err("refused");
    let got: Vec<(&str, String)> =
        e.0.iter()
            .map(|x| (x.code(), x.field.clone().unwrap_or_default()))
            .collect();
    // Actor 2: memory 512 KiB below the need of 1 MiB; pending timers 8 below 16; a port of unknown type.
    assert_eq!(
        got,
        [
            ("limit.below-need", "actors[1]".to_string()),
            ("limit.below-need", "actors[1]".to_string()),
            (
                "module.port-type-unknown",
                "actors[1].ports[1].type".to_string()
            )
        ]
    );
    // Fixed: both admitted; the totals are two instances and the sum of the class limits.
    let fixed = SCENARIO
        .replace(
            "memory = \"512KiB\"\npending-timers = 8",
            "memory = \"2MiB\"",
        )
        .replace(", { id = 1, name = \"x\", type = \"network/serial\" }", "");
    let s = scenario(&fixed, &source);
    let est =
        estimate(&s, &f, &ClassLimits::defaults(), &host, big).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(est.instances, 2);
    assert_eq!(est.memory_bytes, 4 * 1024 * 1024 + 2 * 1024 * 1024);
    assert_eq!(est.actors[1].2.memory_bytes, 2 * 1024 * 1024);
    // Capacity: one instance short; memory short.
    let e = estimate(
        &s,
        &f,
        &ClassLimits::defaults(),
        &host,
        Capacity {
            instances: 1,
            memory_bytes: 1 << 40,
        },
    )
    .expect_err("refused");
    assert_eq!(e.codes(), ["host.capacity-exceeded"]);
    let e = estimate(
        &s,
        &f,
        &ClassLimits::defaults(),
        &host,
        Capacity {
            instances: 10,
            memory_bytes: 5 * 1024 * 1024,
        },
    )
    .expect_err("refused");
    assert_eq!(e.codes(), ["host.capacity-exceeded"]);
    // Port count: zero ports of a kind with min 1.
    let none = fixed.replace("ports = [{ id = 0, name = \"eth0\", type = \"network/ethernet\" }]\nconfig = { label = \"second\" }", "ports = []\nconfig = { label = \"second\" }");
    let e = estimate(
        &scenario(&none, &source),
        &f,
        &ClassLimits::defaults(),
        &host,
        big,
    )
    .expect_err("refused");
    assert_eq!(e.codes(), ["module.port-count"]);
    // The scenario's hash and contract against the module's.
    let wrong_hash = fixed.replace("SHA", &"0".repeat(64));
    let e = estimate(
        &Manifest::parse(&wrong_hash, &host).unwrap(),
        &f,
        &ClassLimits::defaults(),
        &host,
        big,
    )
    .expect_err("refused");
    assert_eq!(e.codes(), ["module.hash-mismatch"]);
    let wrong_contract = fixed.replace(
        "contract = \"florn:core/guest@0.1.0\"",
        "contract = \"florn:network/router@0.1.0\"",
    );
    let e = estimate(
        &scenario(&wrong_contract, &source),
        &f,
        &ClassLimits::defaults(),
        &host,
        big,
    )
    .expect_err("refused");
    assert_eq!(e.codes(), ["module.contract-mismatch"]);
}

struct Ohms;
impl Units for Ohms {
    fn to_si(&self, unit: &str) -> Option<Decimal> {
        match unit {
            "ohm" => Decimal::parse("1"),
            "kohm" => Decimal::parse("1000"),
            _ => None,
        }
    }
}

#[test]
fn the_config_encoder_of_rules_13_and_14() {
    let m = parse_manifest(MANIFEST).unwrap();
    let cfg = |pairs: &[(&str, DomainValue)]| -> florn_model::DomainMap {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    };
    let text = |s: &str| DomainValue::Text(s.to_string());
    // All given: text, ticks at 1 ns, u64 default, integer with unit, bool default zero.
    let bytes = encode_config(
        1,
        &m,
        &cfg(&[
            ("label", text("ab")),
            ("hello", text("5ms")),
            ("price", text("0.07USD")),
            ("flag", DomainValue::Boolean(true)),
        ]),
        TickLength::Ns,
        &NoUnits,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let mut expect = Vec::new();
    expect.extend_from_slice(&2u32.to_le_bytes());
    expect.extend_from_slice(b"ab");
    expect.extend_from_slice(&5_000_000u64.to_le_bytes());
    expect.extend_from_slice(&10u64.to_le_bytes());
    expect.extend_from_slice(&7i64.to_le_bytes());
    expect.push(1);
    assert_eq!(bytes, expect);
    // Defaults fill: hello 10 ms, price 12.34 USD → 1234, flag absent → 0.
    let bytes = encode_config(
        1,
        &m,
        &cfg(&[("label", text(""))]),
        TickLength::Ns,
        &NoUnits,
    )
    .unwrap();
    assert_eq!(&bytes[4..12], &10_000_000u64.to_le_bytes());
    assert_eq!(&bytes[20..28], &1234i64.to_le_bytes());
    assert_eq!(bytes[28], 0);
    // Errors, all reported: missing required, unknown, wrong type, not whole ticks, not whole unit.
    let e = encode_config(
        1,
        &m,
        &cfg(&[
            ("nope", text("x")),
            ("hello", text("1.5ns")),
            ("cost", text("ten")),
            ("price", text("0.005USD")),
        ]),
        TickLength::Ns,
        &NoUnits,
    )
    .expect_err("refused");
    assert_eq!(
        e.codes(),
        [
            "config.unknown-parameter",
            "config.missing-parameter",
            "quantity.not-whole-ticks",
            "config.type",
            "quantity.not-whole-unit"
        ]
    );
    // A real with a unit through the domain's registry.
    let r = parse_manifest("florn-module = 1\nworld = \"w\"\nkind = \"k\"\n[[params]]\nname = \"r\"\ntype = \"real\"\nunit = \"ohm\"\n").unwrap();
    let bytes = encode_config(
        1,
        &r,
        &cfg(&[("r", text("1.5kohm"))]),
        TickLength::Ns,
        &Ohms,
    )
    .unwrap();
    assert_eq!(bytes, 1500f64.to_le_bytes());
    assert_eq!(
        encode_config(
            1,
            &r,
            &cfg(&[("r", text("1.5kohm"))]),
            TickLength::Ns,
            &NoUnits
        )
        .expect_err("refused")
        .codes(),
        ["quantity.unknown-unit"]
    );
    // Bytes from hexadecimal.
    let b = parse_manifest("florn-module = 1\nworld = \"w\"\nkind = \"k\"\n[[params]]\nname = \"b\"\ntype = \"bytes\"\n").unwrap();
    assert_eq!(
        encode_config(
            1,
            &b,
            &cfg(&[("b", text("00ff10"))]),
            TickLength::Ns,
            &NoUnits
        )
        .unwrap(),
        [3, 0, 0, 0, 0x00, 0xff, 0x10]
    );
    assert_eq!(
        encode_config(1, &b, &cfg(&[("b", text("0g"))]), TickLength::Ns, &NoUnits)
            .expect_err("refused")
            .codes(),
        ["config.type"]
    );
}

#[test]
fn classification_reads_globals_only() {
    assert_eq!(classify(-1, 0, 1024), FaultClass::Budget);
    assert_eq!(classify(5, 1024, 1024), FaultClass::Stack);
    assert_eq!(classify(5, 2000, 1024), FaultClass::Stack);
    assert_eq!(classify(5, 10, 1024), FaultClass::Trap);
    assert_eq!(
        classify(-1, 2000, 1024),
        FaultClass::Budget,
        "budget first (RFC 003 rule 13)"
    );
}
