//! The control interface: commands, queries, subscriptions, notifications, and the code catalogue
//! with typed parameters (RFC 005 §6; RFC 007).
//!
//! One definition, as Rust types, with the canonical binary encoding over every transport and
//! a version (RFC 005 rule 25); the catalogue of codes with typed parameters (RFC 007 §4). No
//! text: the front end's catalogue renders. Implemented by PR-07 (the notification) and PR-08 of
//! `rfcs/handoffs/005-host-architecture/task-breakdown-pr-plan.md`.
#![forbid(unsafe_code)]

pub mod catalogue;
pub mod display;
pub mod message;
pub mod notification;

pub use catalogue::{CATALOGUE, Code, Entry, FaultCode, ParamType, ScopeKind};
pub use display::{DisplayPath, Record as DisplayRecord, Reduced, Transport};
pub use message::{
    Answer, CONTROL_VERSION, Command, DecodeError, Envelope, ExportKind, FileKind, Message, Pacing,
    Progress, Query, RunState, SeedChoice, Subscription, SubscriptionKind, Unit, decode, encode,
};
pub use notification::{At, Ends, Notification, Scope, Severity, Value};
