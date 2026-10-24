//! Hand-written guests of the core world for tests: an echo, a pinger, a judge, and the five
//! hostile guests of RFC 003 §10, each with a global and a table for the export tests; a scenario
//! builder over them. The Host's, the runner's and the conformance kit's tests use them; the
//! crate is never published.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use florn_model::{HostMaxima, Manifest};
use wasm_encoder::{CustomSection, Section};

/// The module around a behaviour: imports, memory, a bump allocator that grows the memory, a
/// table used indirectly, and `init` that reads a `u64` period from the config and remembers
/// the actor's identity.
fn module(init: &str, on_message: &str, on_timer: &str) -> String {
    format!(
        r#"
(module
  (import "florn:core/host@0.1.0" "send" (func $send (param i32 i32 i32)))
  (import "florn:core/host@0.1.0" "set-timer" (func $set_timer (param i32 i64)))
  (import "florn:core/host@0.1.0" "cancel-timer" (func $cancel (param i32)))
  (import "florn:core/host@0.1.0" "random" (func $random (result i64)))
  (import "florn:core/host@0.1.0" "report" (func $report (param i32 i32 i32)))
  (memory (export "memory") 2)
  (type $ret (func (result i32)))
  (global $bump (mut i32) (i32.const 4096))
  (global $counter (mut i32) (i32.const 0))
  (global $period (mut i64) (i64.const 10))
  (global $me (mut i32) (i32.const 0))
  (table 2 2 funcref)
  (elem (i32.const 0) $f0 $f1)
  (func $f0 (result i32) (i32.const 10))
  (func $f1 (result i32) (i32.const 20))
  (func $rec (param i32) (result i32)
    (global.set $counter (i32.add (global.get $counter) (i32.const 1)))
    (i32.add (local.get 0) (call $rec (i32.add (local.get 0) (i32.const 1)))))
  (func (export "cabi_realloc") (param i32 i32 i32 i32) (result i32)
    (local $p i32)
    (local.set $p (i32.and (i32.add (global.get $bump) (i32.sub (local.get 2) (i32.const 1)))
                           (i32.sub (i32.const 0) (local.get 2))))
    (global.set $bump (i32.add (local.get $p) (local.get 3)))
    (block $ok (loop $grow
      (br_if $ok (i32.le_u (global.get $bump) (i32.mul (memory.size) (i32.const 65536))))
      (drop (memory.grow (i32.const 1)))
      (br $grow)))
    (local.get $p))
  (func (export "florn:core/actor@0.1.0#init") (param i64 i32 i32 i32 i32 i32)
    (global.set $me (local.get 1))
    (if (i32.ge_u (local.get 5) (i32.const 8))
      (then (global.set $period (i64.load (local.get 4)))))
    {init})
  (func (export "florn:core/actor@0.1.0#on-message") (param i64 i32 i32 i32)
    (global.set $counter (i32.add (global.get $counter) (i32.const 1)))
    {on_message})
  (func (export "florn:core/actor@0.1.0#on-timer") (param i64 i32)
    (global.set $counter (i32.add (global.get $counter) (i32.const 1)))
    {on_timer}))
"#
    )
}

const ARM: &str = "(call $set_timer (i32.const 0) (global.get $period))";
const ARM_SOON: &str = "(call $set_timer (i32.const 0) (i64.const 5))";

/// What a guest does; each is one module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Guest {
    /// Echoes every message through its port; on its timer reports a random value and re-arms.
    Echo,
    /// Sends its identity through port 0 on every timer, period from the config.
    Pinger,
    /// Reports every message's payload on channel 1.
    Judge,
    /// On its timer: loops for ever (budget).
    Loop,
    /// On its timer: recurses without end (stack).
    Recurse,
    /// On its timer: grows the memory until refused, then traps (trap at the limit).
    Grow,
    /// On its timer: sends until refused (quota).
    Flood,
    /// On its timer: sends from an address outside the memory (contract).
    BadPointer,
}

impl Guest {
    pub fn kind(self) -> &'static str {
        match self {
            Self::Echo => "echo",
            Self::Pinger => "pinger",
            Self::Judge => "judge",
            Self::Loop => "loop",
            Self::Recurse => "recurse",
            Self::Grow => "grow",
            Self::Flood => "flood",
            Self::BadPointer => "bad-pointer",
        }
    }

    pub fn wat(self) -> String {
        match self {
            Self::Echo => module(
                ARM,
                "(call $send (local.get 1) (local.get 2) (local.get 3))",
                &format!(
                    "(i64.store (i32.const 64) (call $random))
                     (call $report (i32.const 1) (i32.const 64) (i32.const 8))
                     (i32.store (i32.const 72) (call_indirect (type $ret) (i32.and (global.get $counter) (i32.const 1))))
                     {ARM}"
                ),
            ),
            Self::Pinger => module(
                ARM,
                "",
                &format!(
                    "(i32.store (i32.const 64) (global.get $me))
                     (call $send (i32.const 0) (i32.const 64) (i32.const 4))
                     {ARM}"
                ),
            ),
            Self::Judge => module(
                "",
                "(call $report (i32.const 1) (local.get 2) (local.get 3))",
                "",
            ),
            Self::Loop => module(
                ARM_SOON,
                "",
                "(loop $l (global.set $counter (i32.add (global.get $counter) (i32.const 1))) (br $l))",
            ),
            Self::Recurse => module(ARM_SOON, "", "(drop (call $rec (i32.const 0)))"),
            Self::Grow => module(
                ARM_SOON,
                "",
                "(loop $l
                   (if (i32.eq (memory.grow (i32.const 1)) (i32.const -1)) (then (unreachable)))
                   (global.set $counter (i32.add (global.get $counter) (i32.const 1)))
                   (br $l))",
            ),
            Self::Flood => module(
                ARM_SOON,
                "",
                "(loop $l
                   (call $send (i32.const 0) (i32.const 64) (i32.const 4))
                   (global.set $counter (i32.add (global.get $counter) (i32.const 1)))
                   (br $l))",
            ),
            Self::BadPointer => module(
                ARM_SOON,
                "",
                "(call $send (i32.const 0) (i32.const 0x7fff0000) (i32.const 16))",
            ),
        }
    }

    pub fn manifest(self) -> String {
        format!(
            r#"
florn-module = 1
world = "florn:core/guest@0.1.0"
kind = "{}"

[[ports]]
kind = "p"
type = "core/port"
min = 0
max = 8

[[params]]
name = "period"
type = "u64"
default = 10

[[telemetry]]
channel = 1
name = "r"
fields = [{{ name = "v", type = "u64" }}]

[resources]
memory = "128KiB"
"#,
            self.kind()
        )
    }

    /// The module bytes with both custom sections of RFC 002 rule 11.
    pub fn bytes(self) -> Vec<u8> {
        let mut bytes = wat::parse_str(self.wat()).expect("the guest parses");
        CustomSection {
            name: "component-type:florn-core-guest".into(),
            data: contract_section().into(),
        }
        .append_to(&mut bytes);
        CustomSection {
            name: "florn.manifest".into(),
            data: self.manifest().into_bytes().into(),
        }
        .append_to(&mut bytes);
        bytes
    }
}

/// The contract-identity section, from the contract of record.
pub fn contract_section() -> Vec<u8> {
    let wit = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../contracts/florn-core/wit/core.wit"
    ))
    .expect("the contract of record");
    let mut resolve = wit_parser::Resolve::default();
    let pkg = resolve
        .push_str("core.wit", &wit)
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

pub fn sha256_hex(bytes: &[u8]) -> String {
    florn_model::canonical::hex(&florn_admission::sha256(bytes))
}

/// An actor line of a scenario.
pub struct ActorSpec {
    pub id: u32,
    pub guest: Guest,
    pub ports: u32,
    pub period: Option<u64>,
}

pub fn actor(id: u32, guest: Guest, ports: u32) -> ActorSpec {
    ActorSpec {
        id,
        guest,
        ports,
        period: None,
    }
}

/// A scenario over the guests, with `[modules]` by guest kind, connections as (actor, port) pairs.
pub struct ScenarioSpec {
    pub end: u64,
    pub tie: &'static str,
    pub actors: Vec<ActorSpec>,
    pub connections: Vec<Vec<(u32, u32)>>,
    pub seed: u64,
}

impl ScenarioSpec {
    pub fn text(&self) -> String {
        let mut guests: Vec<Guest> = Vec::new();
        for a in &self.actors {
            if !guests.contains(&a.guest) {
                guests.push(a.guest);
            }
        }
        let mut t = format!(
            "florn = 1\nname = \"host test\"\ndomains = [\"core\"]\n\n[time]\ntick = \"1ns\"\nend = \"{}ns\"\n\n[order]\ntie = \"{}\"\n\n[seed]\nroot = {}\n\n[limits.default]\nmemory = \"1MiB\"\n",
            self.end, self.tie, self.seed
        );
        for g in &guests {
            t.push_str(&format!(
                "\n[modules.{}]\nsha256 = \"{}\"\ncontract = \"florn:core/guest@0.1.0\"\n",
                g.kind(),
                sha256_hex(&g.bytes())
            ));
        }
        for a in &self.actors {
            let ports: Vec<String> = (0..a.ports)
                .map(|p| format!("{{ id = {p}, name = \"p{p}\", type = \"core/port\" }}"))
                .collect();
            t.push_str(&format!(
                "\n[[actors]]\nid = {}\nname = \"a{}\"\nkind = \"module\"\nmodule = \"{}\"\nports = [{}]\n",
                a.id,
                a.id,
                a.guest.kind(),
                ports.join(", ")
            ));
            if let Some(p) = a.period {
                t.push_str(&format!("[actors.config]\nperiod = {p}\n"));
            }
        }
        for (i, c) in self.connections.iter().enumerate() {
            let ports: Vec<String> = c.iter().map(|(a, p)| format!("\"a{a}.p{p}\"")).collect();
            t.push_str(&format!(
                "\n[[connections]]\nid = {}\nname = \"c{}\"\nports = [{}]\n",
                100 + i as u32,
                i,
                ports.join(", ")
            ));
        }
        t
    }

    pub fn manifest(&self) -> Manifest {
        Manifest::parse(&self.text(), &HostMaxima::native())
            .unwrap_or_else(|e| panic!("scenario: {:?}\n{}", e.codes(), self.text()))
    }

    pub fn modules(&self) -> BTreeMap<String, Vec<u8>> {
        self.actors
            .iter()
            .map(|a| (a.guest.kind().to_string(), a.guest.bytes()))
            .collect()
    }
}
