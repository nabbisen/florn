//! The conformance kit (AUTO-007): the SDK's guests built with the toolchain alone, scenarios with
//! recorded digests, the verification tables of RFC 001 §14, RFC 002 §10, RFC 003 §10 and
//! RFC 005 §11 as tests, and the crossing and cost benchmarks of RFC 005 rule 11 and RFC 003 §7.
//!
//! The tests run under the native Host (`cargo test -p florn-conformance`); the browser Host runs
//! the same scenarios through its test page (PR-12). The benchmarks' published figures come from
//! `cargo run --release -p florn-conformance -- figures`; the shipped scenarios and their digests
//! from `cargo run --release -p florn-conformance -- scenarios`.
//!
//! The readme maps every row of the four tables to its test.
#![forbid(unsafe_code)]

pub mod bench;
pub mod guests;
pub mod host;
pub mod scenario;
pub mod shipped;
