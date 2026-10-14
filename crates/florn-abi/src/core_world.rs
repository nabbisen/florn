//! The signatures of `florn:core@0.1.0` (RFC 002 §4) as the Host sees them: the import module
//! name, the mangled export names (rule 20), and each function's parameter and result types.
//! Written by hand from the WIT; the test in `tests/` checks them against the parsed contract.

use crate::ty::Ty;

pub const PACKAGE: &str = "florn:core@0.1.0";
/// The import module name of the `host` interface.
pub const HOST_MODULE: &str = "florn:core/host@0.1.0";
/// The prefix of the mangled export names of the `actor` interface.
pub const ACTOR_EXPORT_PREFIX: &str = "florn:core/actor@0.1.0#";

pub const EXPORT_INIT: &str = "florn:core/actor@0.1.0#init";
pub const EXPORT_ON_MESSAGE: &str = "florn:core/actor@0.1.0#on-message";
pub const EXPORT_ON_TIMER: &str = "florn:core/actor@0.1.0#on-timer";
pub const EXPORT_MEMORY: &str = "memory";
pub const EXPORT_REALLOC: &str = "cabi_realloc";

/// A function's signature in the type subset.
#[derive(Clone, Debug, PartialEq)]
pub struct Signature {
    pub name: &'static str,
    pub params: Vec<(&'static str, Ty)>,
    pub result: Option<Ty>,
}

impl Signature {
    pub fn param_types(&self) -> Vec<Ty> {
        self.params.iter().map(|(_, t)| t.clone()).collect()
    }
}

/// `record port { id: port-id, kind: u32, name: string }`
pub fn port() -> Ty {
    Ty::record(&[("id", Ty::U32), ("kind", Ty::U32), ("name", Ty::String)])
}

/// The `host` interface: what a guest imports.
pub fn host_functions() -> Vec<Signature> {
    vec![
        Signature {
            name: "send",
            params: vec![("port", Ty::U32), ("payload", Ty::list(Ty::U8))],
            result: None,
        },
        Signature {
            name: "set-timer",
            params: vec![("timer", Ty::U32), ("delay", Ty::U64)],
            result: None,
        },
        Signature {
            name: "cancel-timer",
            params: vec![("timer", Ty::U32)],
            result: None,
        },
        Signature {
            name: "random",
            params: vec![],
            result: Some(Ty::U64),
        },
        Signature {
            name: "report",
            params: vec![("channel", Ty::U32), ("payload", Ty::list(Ty::U8))],
            result: None,
        },
    ]
}

/// The `actor` interface: what a guest exports.
pub fn actor_functions() -> Vec<Signature> {
    vec![
        Signature {
            name: "init",
            params: vec![
                ("now", Ty::U64),
                ("actor", Ty::U32),
                ("ports", Ty::list(port())),
                ("config", Ty::list(Ty::U8)),
            ],
            result: None,
        },
        Signature {
            name: "on-message",
            params: vec![
                ("now", Ty::U64),
                ("port", Ty::U32),
                ("payload", Ty::list(Ty::U8)),
            ],
            result: None,
        },
        Signature {
            name: "on-timer",
            params: vec![("now", Ty::U64), ("timer", Ty::U32)],
            result: None,
        },
    ]
}
