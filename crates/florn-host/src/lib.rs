//! The Host: kernel, admission, model, and domains over the engine and platform adapter traits;
//! the run loop, the instance pool, actor and checkpoint export, the diagnostic log, the run
//! identity and the run record (RFC 005 §4 to §6; RFC 003 rules 11 to 13, 17, 20 to 22;
//! RFC 001 §9). One implementation for both Hosts: nothing here names a platform or an engine.
//!
//! Implemented by PR-07 of `rfcs/handoffs/005-host-architecture/task-breakdown-pr-plan.md`.
#![forbid(unsafe_code)]

pub mod controller;
pub mod domain;
pub mod identity;
pub mod log;
pub mod record;
pub mod run;
pub mod snapshot;
pub mod world;

pub use controller::Controller;
pub use domain::{Action, BuiltinEvent, Domain, DomainEvent, DomainWorld, Domains};
pub use florn_kernel::Advance;
pub use identity::{EndReason, Environment, ModuleInput, RunInputs, RunToml};
pub use log::DiagnosticLog;
pub use run::{FaultPolicy, HostError, Kept, Options, Run, Seed, Telemetry};
pub use snapshot::{ActorState, Checkpoint};
pub use world::{CLASS_MESSAGE, CLASS_TIMER, CoreWorld, Intervention};
