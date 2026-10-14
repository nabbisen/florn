//! Manifest, package, canonical encoding, scenario hash, validation (RFC 004).
//!
//! The core reads a small, closed set of fields of a scenario manifest and carries everything else
//! opaquely to the domain it belongs to. This crate turns the TOML text of a manifest into the
//! [`Manifest`] model, reporting every validation error it can find in one pass as a typed
//! [`Error`] with the code of RFC 007 §4.1; computes the canonical encoding of the parsed model and
//! the scenario hash over it (RFC 004 §5); and reads and writes the package that bundles a manifest
//! with the modules it references (RFC 004 §6).
//!
//! Implemented by PR-02 of `rfcs/handoffs/005-host-architecture/task-breakdown-pr-plan.md`.
#![forbid(unsafe_code)]

pub mod canonical;
pub mod error;
pub mod manifest;
pub mod package;
mod parse;
pub mod quantity;

pub use error::{Error, Errors, Kind, Param};
pub use manifest::*;
pub use package::{Package, PackageMeta};
pub use quantity::{Decimal, Quantity, TickLength};

/// The maxima that bound every allocation a manifest could size (RFC 004 rule 39, RFC 006 §7).
/// Host constants; provisional until Phase 1 measures (RFC 003 open question 1).
pub mod maxima {
    /// Bytes of manifest text.
    pub const MANIFEST_BYTES: usize = 4 * 1024 * 1024;
    pub const DOMAINS: usize = 16;
    pub const MODULES: usize = 10_000;
    pub const ACTORS: usize = 100_000;
    pub const PORTS_PER_ACTOR: usize = 4_096;
    pub const CONNECTIONS: usize = 1_000_000;
    pub const PORTS_PER_CONNECTION: usize = 4_096;
    pub const CHANGES: usize = 100_000;
    pub const LIMIT_CLASSES: usize = 1_024;
    /// Bytes of one name or text field.
    pub const NAME_BYTES: usize = 256;
}

/// The Host's maxima for the limits of `[limits]` (RFC 004 rule 15, RFC 003 rule 19).
/// A manifest may only lower them. The defaults are the browser Host's provisional values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostMaxima {
    pub budget: u64,
    pub stack: u64,
    pub memory_bytes: u64,
    pub effects_per_activation: u64,
    pub payload_bytes: u64,
    pub pending_timers: u64,
    pub telemetry_per_activation_bytes: u64,
    pub microsteps_per_tick: u64,
    pub events_per_instant: u64,
}

impl Default for HostMaxima {
    fn default() -> Self {
        Self::browser()
    }
}

impl HostMaxima {
    pub const fn browser() -> Self {
        Self {
            budget: 1_000_000_000,
            stack: 65_536,
            memory_bytes: 64 * 1024 * 1024,
            effects_per_activation: 100_000,
            payload_bytes: 16 * 1024 * 1024,
            pending_timers: 65_536,
            telemetry_per_activation_bytes: 16 * 1024 * 1024,
            microsteps_per_tick: 1_000_000,
            events_per_instant: 10_000_000,
        }
    }
    pub const fn native() -> Self {
        Self {
            memory_bytes: 1024 * 1024 * 1024,
            ..Self::browser()
        }
    }
}
