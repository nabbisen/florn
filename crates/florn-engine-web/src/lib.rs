//! The browser engine adapter (RFC 005 rules 6 to 8 and 10). The Host runs as one WebAssembly
//! module inside a worker; the worker's script (`crates/florn-ui-web/www/worker.js`) holds the
//! guest instances and offers this adapter a small set of imports under the module name `florn`.
//!
//! How the rules are met:
//!
//! - Rule 6, views: a payload crosses as one copy between the Host's memory and the guest's
//!   (`engine_memory_read`, `engine_memory_write`), through views the worker keeps per instance
//!   and renews only when a memory grew. No object is made per crossing.
//! - Rule 7, typed references: an export is resolved once into an index of the Host module's own
//!   function table, which the worker grows and fills; the Host calls it with `call_indirect`
//!   from [`florn_call`], a call from WebAssembly to WebAssembly with no JavaScript value in
//!   between. A guest's imports are bound to this module's exports `florn_import_*` the same way.
//! - Rule 8, the budget: every guest shares one Host-owned global; the Host reads and sets it
//!   through `budget_get` and `budget_set`, exports of a six-instruction module the worker
//!   instantiates over the same global, so no JavaScript conversion of the 64-bit value occurs.
//! - Rule 10: the worker instantiates synchronously, binds the import table once per world,
//!   and turns every failure into a status and a text; a trap in the guest unwinds to the
//!   worker's `try` around [`florn_call`], never through the Host's Rust frames.
//!
//! `unsafe` appears in four places, each with its invariant: the handler the Host lends for the
//! duration of a call, kept as a raw pointer while the guest runs (as the Wasmtime adapter does);
//! the function-table index cast to a function pointer in [`florn_call`]; the imports from the
//! worker; and the pointers the worker hands back into this module's memory.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

pub mod sig;

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::{Func, Inst, WebEngine, florn_call, user_agent};
