//! A second domain for the kit (PR-39; RFC 002 rule 48; RFC 012 finding F-9): it owns nothing but
//! a world, so that a world of **two** domains has two to span. Its guests export one function,
//! `on-ping`, which nothing calls: the kit proves that such a module is admitted or refused, and
//! that it runs.
//!
//! Three worlds exist in `wit/test-world.wit`: the test domain's `guest`, this domain's `pinger`,
//! and `both`, which includes `guest` and adds `pings`: the composed world `COMPOSED`.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};

use florn_abi::Ty;
use florn_abi::Val;
use florn_abi::core_world::Signature;
use florn_control::Notification;
use florn_host::Domains;
use florn_host::domain::{Action, BuiltinEvent, Domain, DomainEvent, DomainWorld, RecordedImport};
use florn_kernel::{Delivery, Identity, Instant, Sink};
use florn_model::Manifest;
use florn_model::manifest::DomainMap;

use super::{WORLD as TEST_WORLD, domains as test_domains};

pub const NAME: &str = "second";
pub const PINGS_ID: &str = "test:kit/pings@0.1.0";
pub const LINES_ID: &str = "test:kit/lines@0.1.0";
/// This domain's own world: the core and `pings`.
pub const WORLD: &str = "test:kit/pinger@0.1.0";
/// The world of a module that is of both domains: `guest` and `pings`.
pub const COMPOSED: &str = "test:kit/both@0.1.0";

/// How many imports of this domain's world the Host has asked it to answer, in this process.
/// An import that records is never asked.
pub static ASKED: AtomicUsize = AtomicUsize::new(0);

#[derive(Default)]
pub struct SecondDomain;

impl SecondDomain {
    pub fn new() -> Self {
        Self
    }
}

impl Domain for SecondDomain {
    fn name(&self) -> &'static str {
        NAME
    }
    fn connection_types(&self) -> Vec<&'static str> {
        Vec::new()
    }
    fn port_types(&self) -> Vec<&'static str> {
        Vec::new()
    }
    fn worlds(&self) -> Vec<DomainWorld> {
        vec![DomainWorld {
            name: WORLD.to_string(),
            imports_ids: vec![LINES_ID.to_string()],
            exports_ids: vec![PINGS_ID.to_string()],
            imports: vec![(
                LINES_ID.to_string(),
                vec![Signature {
                    name: "set-level",
                    params: vec![("port", Ty::U32), ("value", Ty::F64)],
                    result: None,
                }],
            )],
            exports: vec![(
                PINGS_ID.to_string(),
                vec![Signature {
                    name: "on-ping",
                    params: vec![("now", Ty::U64), ("port", Ty::U32), ("up", Ty::Bool)],
                    result: None,
                }],
            )],
            compounds: vec![],
            // `set-level` is recorded as a send and the domain is not asked (RFC 012 finding
            // F-11); a value that is not finite is the contract violation of rule 18.
            records: vec![RecordedImport {
                function: "set-level",
                port: 0,
                value: 1,
                non_finite: Some("RFC 012 rule 18"),
            }],
        }]
    }
    fn validate(&self, _: &Manifest) -> Vec<Notification> {
        Vec::new()
    }
    fn compose(&mut self, _: &Manifest, _: u64) {}
    fn deliver(
        &mut self,
        _: Instant,
        _: Identity,
        _: u64,
        _: u32,
        _: &[u8],
        _: &BTreeSet<Identity>,
    ) -> Vec<Delivery> {
        Vec::new()
    }
    fn change(&mut self, _: Instant, _: Identity, _: &DomainMap) -> Vec<Action> {
        Vec::new()
    }
    fn check_intervention(
        &self,
        _: Identity,
        _: &str,
        _: &DomainMap,
    ) -> Result<(), Box<Notification>> {
        Err(Box::new(Notification::new(
            "control.invalid",
            florn_control::Severity::Error,
            florn_control::Scope::run(),
            florn_control::Ends::Nothing,
        )))
    }
    fn intervene(&mut self, _: Instant, _: Identity, _: &str, _: &DomainMap) -> Vec<Action> {
        Vec::new()
    }
    fn event(&mut self, _: &DomainEvent, _: bool) -> Vec<Action> {
        Vec::new()
    }
    fn answer(&mut self, _: Identity, _: &str, _: &[Val]) -> Option<Val> {
        ASKED.fetch_add(1, Ordering::SeqCst);
        None
    }
    fn builtin(&mut self, _: Identity, _: &str, _: &BuiltinEvent<'_>, _: &mut Sink) {}
}

/// The Host's domains with both of the kit's and the world that spans them.
pub fn domains() -> Domains {
    test_domains()
        .with(SecondDomain::new)
        .with_composed(COMPOSED, &[("test", TEST_WORLD), (NAME, WORLD)])
}

/// The prober's text with the export `on-ping` of `pings`, as a module of `COMPOSED` needs it.
pub fn composed_wat() -> String {
    let wat = super::guest_wat();
    let body = wat.trim_end().strip_suffix(')').expect("a module");
    format!("{body}\n  (func (export \"{PINGS_ID}#on-ping\") (param i64 i32 i32))\n)")
}

/// The same for `WORLD` alone: the core's exports, `lines` and `on-ping`, nothing of the test
/// domain's. At tick 1 its timer sets the level `1.5` on port 0.
pub fn pinger_wat() -> String {
    pinger_setting("0", "1.5")
}

/// As [`pinger_wat`], setting the level `value` (a WAT `f64` text: `1.5`, `nan`, `inf`) on `port`.
pub fn pinger_setting(port: &str, value: &str) -> String {
    format!(
        r#"
(module
  (import "florn:core/host@0.1.0" "send" (func $send (param i32 i32 i32)))
  (import "florn:core/host@0.1.0" "set-timer" (func $set_timer (param i32 i64)))
  (import "florn:core/host@0.1.0" "cancel-timer" (func $cancel (param i32)))
  (import "florn:core/host@0.1.0" "random" (func $random (result i64)))
  (import "florn:core/host@0.1.0" "report" (func $report (param i32 i32 i32)))
  (import "{LINES_ID}" "set-level" (func $set_level (param i32 f64)))
  (memory (export "memory") 2)
  (global $bump (mut i32) (i32.const 4096))
  (func (export "cabi_realloc") (param i32 i32 i32 i32) (result i32)
    (local $p i32)
    (local.set $p (i32.and (i32.add (global.get $bump) (i32.sub (local.get 2) (i32.const 1)))
                           (i32.sub (i32.const 0) (local.get 2))))
    (global.set $bump (i32.add (local.get $p) (local.get 3)))
    (local.get $p))
  (func (export "florn:core/actor@0.1.0#init") (param i64 i32 i32 i32 i32 i32)
    (call $set_timer (i32.const 0) (i64.const 1)))
  (func (export "florn:core/actor@0.1.0#on-message") (param i64 i32 i32 i32))
  (func (export "florn:core/actor@0.1.0#on-timer") (param i64 i32)
    (call $set_level (i32.const {port}) (f64.const {value})))
  (func (export "{PINGS_ID}#on-ping") (param i64 i32 i32)))
"#
    )
}
