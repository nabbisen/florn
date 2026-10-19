//! The control interface: commands, queries, subscriptions, notifications, and the code catalogue
//! with typed parameters (RFC 005 §6; RFC 007).
//!
//! PR-07 provides the notification of RFC 007 rule 3, which the Host's diagnostic log needs; the
//! commands, queries, subscriptions, their encoding, and the version arrive with PR-08 of
//! `rfcs/handoffs/005-host-architecture/task-breakdown-pr-plan.md`.
#![forbid(unsafe_code)]

pub mod notification;

pub use notification::{At, Ends, Notification, Scope, Severity, Value};
