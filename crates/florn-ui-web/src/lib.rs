//! The browser front end of release 0.1: the Host as a WebAssembly module in a dedicated worker
//! (RFC 005 rule 32), with a test page that runs the conformance kit's scenarios and benchmarks
//! and reports to the Playwright driver in CI. The shell of RFC 005 section 8 follows in 0.2.
//!
//! The worker module has a C surface (`florn_*` exports) that the worker's script
//! (`www/worker.js`) calls; every run-related call goes through the control interface's
//! [`florn_host::Controller`], as the headless runner's do. Replies come back as JSON text, and
//! one export takes an encoded control message as the transport will carry it.
//!
//! `unsafe` appears at the pointers the worker hands in and out of this module's memory, each
//! with its invariant stated; the crate is a browser adapter composition in the sense of the
//! handoff's hygiene rule.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code, unused_imports))]

#[cfg(target_arch = "wasm32")]
mod worker;
