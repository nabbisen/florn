//! The result of a replay (RFC 001 rule 50; RFC 013 rule 20; RFC 012 finding F-5): `matched`,
//! `within-tolerance`, `diverged` or `needs-payloads`, with the notifications that say which. It is
//! a result of the replay, not a field of the record: the formats stay frozen.

use std::collections::BTreeMap;

use florn_control::{Ends, Notification, Scope, Severity, Value};
use florn_kernel::trace::Comparison;
use florn_kernel::{Digest, Record};

use crate::compare::{
    self, Agreement, Difference, Missing, OfDomains, Reading, Tier, compare as compare_rows, rows,
    tier,
};
use crate::identity::Environment;

use super::Run;

/// What the replay was compared with: the record's chain and environment, and, when the record
/// held them, its trace and payloads.
pub(super) struct Reference {
    pub chain: Vec<(u64, Digest)>,
    pub environment: Environment,
    /// Whether the recorded run had ended: a record exported before its end holds a prefix.
    pub ended: bool,
    pub trace: Option<Vec<Record>>,
    pub payloads: BTreeMap<Digest, Vec<u8>>,
}

/// Which run lacks what the comparison needs (`enum(replay-side)` of RFC 007).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Recorded,
    Replayed,
    Both,
}

impl Side {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Recorded => "recorded",
            Self::Replayed => "replayed",
            Self::Both => "both",
        }
    }
}

/// How a replay compares with its record.
#[derive(Clone, Debug, PartialEq)]
pub enum Replayed {
    /// Equal at every stored chain point.
    Matched { points: u64 },
    /// The chains differ and the traces agree record by record within the tolerance.
    WithinTolerance(Agreement),
    /// The chains differ at the interval from `from` to `to` records; `difference` is the first
    /// record that differs, when the comparison of Tier 2 could name it.
    Diverged {
        from: u64,
        to: u64,
        difference: Option<Difference>,
    },
    /// The chains differ across Hosts and a trace or a payload of one run is missing.
    NeedsPayloads(Side),
}

fn quantity((number, unit): &(String, String)) -> Value {
    Value::Quantity(number.clone(), unit.clone())
}

impl Replayed {
    /// The notifications that say it (RFC 007 section 4.4), all with the scope run, ending nothing.
    /// A tolerance comparison that diverged says the first differing record in place of
    /// `replay.diverged`.
    pub fn notifications(&self) -> Vec<Notification> {
        let new =
            |code: &str, severity| Notification::new(code, severity, Scope::run(), Ends::Nothing);
        vec![match self {
            Self::Matched { points } => {
                new("replay.matched", Severity::Info).with("points", Value::Count(*points))
            }
            // Nothing compared differs and only an exempt value does (rule 20, item 4): there is
            // no field to name.
            Self::WithinTolerance(a) if a.differing == 0 => {
                new("replay.exempt-only", Severity::Info).with("compared", Value::Count(a.compared))
            }
            Self::WithinTolerance(a) => {
                let (worst, largest, at) = match &a.worst {
                    Some(w) => (w.field.clone(), quantity(&w.largest), w.at),
                    None => (String::new(), Value::Quantity("0".into(), String::new()), 0),
                };
                new("replay.within-tolerance", Severity::Info)
                    .with("compared", Value::Count(a.compared))
                    .with("differing", Value::Count(a.differing))
                    .with("worst", Value::Name(worst))
                    .with("largest", largest)
                    .with("at", Value::Tick(at))
            }
            Self::Diverged {
                difference: None,
                from,
                to,
            } => new("replay.diverged", Severity::Error)
                .with("from", Value::Count(*from))
                .with("to", Value::Count(*to)),
            Self::Diverged {
                difference:
                    Some(Difference::Value {
                        record,
                        field,
                        recorded,
                        replayed,
                    }),
                ..
            } => new("replay.value-differs", Severity::Error)
                .with("record", Value::Count(*record))
                .with("field", Value::Name(field.clone()))
                .with("recorded", quantity(recorded))
                .with("replayed", quantity(replayed)),
            Self::Diverged {
                difference: Some(Difference::Record { record, what }),
                ..
            } => new("replay.record-differs", Severity::Error)
                .with("record", Value::Count(*record))
                .with("what", Value::Enum(what.as_str().into())),
            Self::NeedsPayloads(side) => new("replay.needs-payloads", Severity::Error)
                .with("side", Value::Enum(side.as_str().into())),
        }]
    }
}

/// The side that lacks its trace or a payload, when one does (`replay.needs-payloads`).
fn lacking(recorded: bool, replayed: bool) -> Option<Side> {
    match (recorded, replayed) {
        (false, false) => None,
        (true, false) => Some(Side::Recorded),
        (false, true) => Some(Side::Replayed),
        (true, true) => Some(Side::Both),
    }
}

impl<E: florn_engine::Engine> Run<E> {
    /// Give the replay the record's trace and payloads, when it held them: what the comparison of
    /// Tier 2 needs of the recorded run (RFC 013 rule 20).
    pub fn record_trace(
        &mut self,
        trace: Option<Vec<Record>>,
        payloads: BTreeMap<Digest, Vec<u8>>,
    ) {
        if let Some(reference) = &mut self.replay_against {
            reference.trace = trace;
            reference.payloads = payloads;
        }
    }

    /// The domains' way of reading a trace for the comparison.
    pub fn reading(&self) -> impl Reading + '_ {
        OfDomains(self.world.domains())
    }

    /// This run's trace as rows, from the records it kept and the payloads it holds.
    pub fn rows(&self) -> Result<Vec<compare::Row>, Missing> {
        let kept = self
            .kernel
            .trace
            .kept_records()
            .filter(|k| k.len() as u64 == self.kernel.trace.records());
        rows(kept, &self.payloads, &self.reading())
    }

    /// How the replay compares with its record, once it has ended (RFC 001 rule 50; RFC 013 rule
    /// 20). `None` for a run that is no replay or has not ended.
    pub fn replayed(&self) -> Option<Replayed> {
        let reference = self.replay_against.as_ref()?;
        self.ended?;
        let (from, to) = match florn_kernel::trace::compare_reference(
            &self.kernel.trace.stored(),
            &reference.chain,
        ) {
            Comparison::Matched { points } => return Some(Replayed::Matched { points }),
            // Stored points at different positions: unchanged, reported as `replay.diverged`.
            Comparison::Incomparable => (0, 0),
            Comparison::Diverged { from, to } => (from, to),
        };
        let plain = Replayed::Diverged {
            from,
            to,
            difference: None,
        };
        // Only runs of one florn version that differ in Host, engine or architecture are compared
        // within a tolerance, and only for a continuous domain (RFC 013 rule 20).
        if tier(&reference.environment, &self.environment) != Tier::Two {
            return Some(plain);
        }
        let reading = self.reading();
        let Some(tolerance) = reading.tolerance() else {
            return Some(plain);
        };
        let recorded = rows(reference.trace.as_deref(), &reference.payloads, &reading);
        let replayed = self.rows();
        if let Some(side) = lacking(recorded.is_err(), replayed.is_err()) {
            return Some(Replayed::NeedsPayloads(side));
        }
        let (mut recorded, mut replayed) = (recorded.ok()?, replayed.ok()?);
        // A record exported before its run ended holds a prefix of the replay's trace.
        if !reference.ended {
            replayed.truncate(recorded.len());
        }
        recorded.shrink_to_fit();
        Some(
            match compare_rows(&recorded, &replayed, tolerance, &reading) {
                // The chains differ and no record does, exempt values included: nothing explains
                // the difference, so it is not agreement (RFC 013 rule 20).
                Ok(agreement)
                    if agreement.differing == 0 && agreement.consumption_differing == 0 =>
                {
                    plain
                }
                Ok(agreement) => Replayed::WithinTolerance(agreement),
                Err(difference) => Replayed::Diverged {
                    from,
                    to,
                    difference: Some(*difference),
                },
            },
        )
    }
}

#[cfg(test)]
mod tests;
