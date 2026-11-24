//! The checks of PR-42's verifier session (layer B), committed with PR-43's first commit as the owner
//! decided for PR-38's. Written by the verifier, before it read the implementation; changed here
//! only where a ruling of the architect (2026-11-25) changed an expectation, each marked `Ruling`.
//!
//! PR-42 verifier, layer B: the table of RFC 013 section 11 (Comparison), row by row, over the
//! kit's test domain. Expectations are those of `checks.md`, written from RFC 013 rules 20 and 21,
//! RFC 012 F-5 and RFC 001 section 1.1 before the comparison's code was read.
//!
//! The recorded side is doctored (trace rows, payload bytes, the stored chain, the environment);
//! the replayed side is a genuine run of the same scenario. A `level` report is 16 bytes: an f64
//! in `khop` (factor 1000, so the comparison is in SI) and a whole `located` tick.

use std::collections::BTreeMap;

use florn_admission::Admission;
use florn_conformance::host;
use florn_conformance::test_domain as td;
use florn_control::Value;
use florn_host::run::Replayed;
use florn_host::{Advance, Intervention, Options, Run, RunToml};
use florn_kernel::Record;
use florn_kernel::trace::CommittedEffect;
use florn_model::canonical::Encoder;
use florn_model::manifest::{DomainMap, DomainValue};
use florn_platform::FixedPlatform;

type Digest = [u8; 32];

#[derive(Clone)]
struct Doc {
    text: String,
    toml: RunToml,
    trace: Vec<Record>,
    payloads: BTreeMap<Digest, Vec<u8>>,
}

fn payload(level: f64, located: u64) -> Vec<u8> {
    let mut v = level.to_le_bytes().to_vec();
    v.extend(located.to_le_bytes());
    v
}

fn scenario(atol: &str, rtol: &str) -> String {
    td::text(
        "tier2",
        600,
        7,
        &format!(
            "{}{}{}\n[test]\natol = {atol}\nrtol = {rtol}\ncontinuous-actors = [1]\n",
            td::prober(1, 1),
            td::prober(2, 1),
            td::wire(100, &[(1, 0), (2, 0)], "")
        ),
    )
}

const TICKS: [u64; 4] = [100, 200, 300, 400];

/// A genuine run; `levels[i]` is delivered to actor 1 at `TICKS[i]` with `located = 7 + i`.
fn record_with(text: &str, levels: [f64; 4]) -> Doc {
    let mut run = td::start(text);
    for (i, level) in levels.iter().enumerate() {
        run.advance(Advance::ToTick(TICKS[i]));
        run.intervene(
            None,
            &Intervention::Message {
                actor: 1,
                port: 0,
                payload: payload(*level, 7 + i as u64),
            },
        );
    }
    run.run_to_end();
    Doc {
        text: text.to_string(),
        toml: run.run_toml(),
        trace: run.records().expect("records kept").to_vec(),
        payloads: run.payloads().clone(),
    }
}

fn base() -> Doc {
    record_with(&scenario("1e-9", "1e-6"), [1.0, 2.0, 3.0, 4.0])
}

/// The indexes of the records that report a `level` (channel 1, 16 bytes).
fn level_records(trace: &[Record]) -> Vec<usize> {
    trace
        .iter()
        .enumerate()
        .filter(|(_, r)| matches!(r, Record::Handled { effects, .. }
            if effects.iter().any(|e| matches!(e, CommittedEffect::Report { channel: 1, len: 16, .. }))))
        .map(|(i, _)| i)
        .collect()
}

/// The indexes of the records that report a 4-byte frame on channel 1.
fn frame_records(trace: &[Record]) -> Vec<usize> {
    trace
        .iter()
        .enumerate()
        .filter(|(_, r)| matches!(r, Record::Handled { effects, .. }
            if effects.iter().any(|e| matches!(e, CommittedEffect::Report { channel: 1, len: 4, .. }))))
        .map(|(i, _)| i)
        .collect()
}

impl Doc {
    /// Replace the bytes of the (single) report of record `i` on channel 1.
    fn set_report(&mut self, i: usize, bytes: Vec<u8>) {
        let digest = *blake3::hash(&bytes).as_bytes();
        let len = bytes.len() as u32;
        self.payloads.insert(digest, bytes);
        if let Record::Handled { effects, .. } = &mut self.trace[i] {
            for e in effects {
                if let CommittedEffect::Report {
                    channel: 1,
                    len: l,
                    digest: d,
                    ..
                } = e
                {
                    *l = len;
                    *d = digest;
                }
            }
        }
    }
    fn level(&self, i: usize) -> (f64, u64) {
        let Record::Handled { effects, .. } = &self.trace[i] else {
            panic!()
        };
        for e in effects {
            if let CommittedEffect::Report {
                channel: 1, digest, ..
            } = e
            {
                let b = &self.payloads[digest];
                return (
                    f64::from_le_bytes(b[..8].try_into().unwrap()),
                    u64::from_le_bytes(b[8..16].try_into().unwrap()),
                );
            }
        }
        panic!("no report")
    }
    fn set_level(&mut self, i: usize, v: f64) {
        let (_, located) = self.level(i);
        self.set_report(i, payload(v, located));
    }
    fn set_located(&mut self, i: usize, located: u64) {
        let (v, _) = self.level(i);
        self.set_report(i, payload(v, located));
    }
    fn flip_chain(&mut self) {
        self.toml.chain.last_mut().unwrap().1[0] ^= 1;
    }
    fn across(&mut self) {
        self.toml.environment.host_build = "browser".into();
    }
    fn metered(&mut self, i: usize, by: u64) {
        if let Record::Handled { metered, .. } = &mut self.trace[i] {
            *metered += by;
        }
    }
    fn draws(&mut self, i: usize, by: u32) {
        if let Record::Handled { draws, .. } = &mut self.trace[i] {
            *draws += by;
        }
    }
}

struct Out {
    run: host::Host,
}

fn go(
    doc: &Doc,
    trace: Option<Vec<Record>>,
    payloads: BTreeMap<Digest, Vec<u8>>,
    options: Options,
) -> Out {
    let mut r = Run::replay(
        host::engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        &td::domains(),
        td::manifest(&doc.text),
        &td::modules(),
        &doc.toml,
        options,
    )
    .expect("the replay starts");
    r.record_trace(trace, payloads);
    r.run_to_end();
    Out { run: r }
}

fn compare(doc: &Doc) -> Out {
    go(
        doc,
        Some(doc.trace.clone()),
        doc.payloads.clone(),
        host::options(),
    )
}

impl Out {
    fn kind(&self) -> &'static str {
        match self.run.replayed().expect("a replay that ended") {
            Replayed::Matched { .. } => "matched",
            Replayed::WithinTolerance(_) => "within-tolerance",
            Replayed::Diverged { .. } => "diverged",
            Replayed::NeedsPayloads(_) => "needs-payloads",
        }
    }
    fn codes(&self) -> Vec<String> {
        self.run
            .log()
            .entries()
            .iter()
            .map(|e| e.notification.code.clone())
            .filter(|c| c.starts_with("replay."))
            .collect()
    }
    fn param(&self, code: &str, key: &str) -> Option<Value> {
        self.run
            .log()
            .entries()
            .iter()
            .find(|e| e.notification.code == code)
            .and_then(|e| e.notification.params.iter().find(|(k, _)| k == key))
            .map(|(_, v)| v.clone())
    }
    fn text(&self, code: &str, key: &str) -> String {
        match self.param(code, key) {
            Some(Value::Enum(t)) | Some(Value::Text(t)) | Some(Value::Name(t)) => t,
            Some(Value::Tick(t)) => t.to_string(),
            Some(Value::Count(c)) => c.to_string(),
            Some(other) => format!("{other:?}"),
            None => "<absent>".into(),
        }
    }
}

macro_rules! expect {
    ($out:expr, $kind:expr, $codes:expr) => {{
        let o = &$out;
        assert_eq!(o.kind(), $kind, "outcome; codes {:?}", o.codes());
        let want: Vec<&str> = $codes.to_vec();
        assert_eq!(o.codes(), want, "codes");
    }};
}

mod outcomes;
mod results;
mod structure;
mod values;
