//! Time, queue, order, activation, effects, trace, digest, replay (RFC 001; RFC 005 section 5).
//! No platform, no engine, no I/O: the kernel is driven by the Host, which owns the engine and
//! the world. The kernel's job is the logical world's rules: logical time in ticks and instants,
//! the total order of events, the activation that records effects and commits or discards them,
//! the seeded streams, interventions as recorded inputs, the trace with its digest chain, and the
//! comparison of two runs.
//!
//! Implemented by PR-03 of `rfcs/handoffs/005-host-architecture/task-breakdown-pr-plan.md`.
#![forbid(unsafe_code)]

pub mod effect;
mod kernel;
pub mod key;
pub mod stream;
pub mod time;
pub mod trace;

pub use effect::{Effect, Quotas, Sink, SinkRefusal};
pub use kernel::*;
pub use key::{EventKey, Identity, TiePolicy};
pub use stream::{Stream, StreamKey};
pub use time::{Instant, MAX_TICK, Tick};
pub use trace::{Comparison, Digest, FaultKind, Record, Trace, TraceSnapshot};

/// The names recorded in the run identity (RFC 001 rule 40 item 7; RFC 005 rule 22).
pub const GENERATOR_NAME: &str = "chacha8";
pub const DERIVATION_NAME: &str = "blake3-derive-key";
pub const DIGEST_NAME: &str = "blake3";
/// The kernel's own version, recorded with the run (the contracts' versions are RFC 002's).
pub const KERNEL_VERSION: u32 = 1;

/// The reserved origins of RFC 004 rule 8.
pub const ORIGIN_INTERVENTIONS: Identity = 0xF000_0000;
pub const ORIGIN_SCHEDULED_CHANGES: Identity = 0xF000_0001;
pub const ORIGIN_WORLD_BASE: Identity = 0xF000_0010;
/// Class 0 is reserved for interventions and scheduled changes (RFC 001 rule 15).
pub const CLASS_CONTROL: u8 = 0;
