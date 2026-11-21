//! The results file of a run record (RFC 013 rule 21; AUTO-004): `florn-conformance results
//! <record> <scenario> [--modules <dir>] [--out <file>]` writes the text that the browser scripts
//! and the native comparison read, from a run record exported with its trace and payloads.
//!
//! The scenario and the modules are needed because a domain reads its payloads (which fields are
//! numbers, in what unit) from the scenario it composes and the modules it validates.

use florn_control::{Command, Envelope, Message};
use florn_host::compare::{results, rows};
use florn_host::record;
use florn_host::{Controller, Domains};

use crate::host;

/// The results text of `record_bytes`, a run record with its trace and payloads, of the scenario
/// `scenario` and its `modules`, read with the domains of `domains`.
pub fn from_record(
    domains: Domains,
    record_bytes: &[u8],
    scenario: &[u8],
    modules: &[Vec<u8>],
) -> Result<String, String> {
    let read = record::read(record_bytes)?;
    let trace = read
        .trace
        .as_deref()
        .ok_or("the record holds no trace: export it with the trace and the payloads")?;
    if read.payloads.is_empty() && !trace.is_empty() {
        return Err(
            "the record holds no payloads: export it with the trace and the payloads".into(),
        );
    }
    let mut c = Controller::new(
        host::engine(),
        florn_admission::Admission::default(),
        Box::new(florn_platform::FixedPlatform::default()),
        domains,
        host::options(),
    );
    let mut send = |m: Message| c.handle(Envelope::new(m));
    let refused = |replies: &[Message], what: &str| -> Result<(), String> {
        let codes: Vec<&str> = replies
            .iter()
            .filter_map(|m| match m {
                Message::Notification(n) if n.severity == florn_control::Severity::Error => {
                    Some(n.code.as_str())
                }
                _ => None,
            })
            .collect();
        if codes.is_empty() {
            Ok(())
        } else {
            Err(format!("{what}: {}", codes.join(", ")))
        }
    };
    let replies = send(Message::Command(Command::LoadScenario {
        bytes: scenario.to_vec(),
    }));
    refused(&replies, "the scenario was not loaded")?;
    for bytes in modules {
        let replies = send(Message::Command(Command::LoadModule {
            bytes: bytes.clone(),
        }));
        refused(&replies, "a module was not loaded")?;
    }
    let replies = send(Message::Command(Command::Replay {
        record: record_bytes.to_vec(),
    }));
    refused(&replies, "the record was not started")?;
    let run = c.run().ok_or("the record was not started")?;
    let rows = rows(Some(trace), &read.payloads, &run.reading())
        .map_err(|m| format!("the record lacks what the results need: {m:?}"))?;
    Ok(results::write(&rows))
}
