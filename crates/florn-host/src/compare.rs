//! The comparison of Tier 2 (RFC 001 section 1.1 and rules 41, 50 and 62; RFC 013 rules 20 and
//! 21; RFC 012 finding F-5): two runs of one florn version that differ in Host, engine or
//! architecture, of a scenario whose domain is continuous, agree when their traces do record by
//! record, the numeric fields within the tolerance and everything else exactly.
//!
//! A trace is first reduced to [`Row`]s, the form the results file holds (`results`) and rule 20
//! reads: the record's kind, its whole key and its target; its effects' kinds, sequence numbers,
//! ports or channels and payload lengths; for each numeric field the bits of the `f64`; for each
//! other payload its digest; and the consumed work and the draws. Two row sequences are then
//! compared by position.

use std::collections::BTreeMap;

use florn_kernel::trace::{CommittedEffect, Record};
use florn_kernel::{Digest, EventKey, Identity};

use crate::domain::{Domain, Numeric, Site, Tolerance};
use crate::identity::Environment;

pub mod golden;
pub mod results;

/// The tier of a comparison (RFC 001 section 1.1, rule 41): the environments decide it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    /// Nothing differs: the traces are identical, bit for bit.
    One,
    /// Host, engine, engine version or architecture differs, the florn version does not.
    Two,
    /// The florn version differs: nothing is promised before 1.0.0.
    Three,
}

/// The tier two runs are compared under.
pub fn tier(recorded: &Environment, replayed: &Environment) -> Tier {
    if recorded == replayed {
        Tier::One
    } else if recorded.florn_version == replayed.florn_version {
        Tier::Two
    } else {
        Tier::Three
    }
}

/// What the comparison asks of the domains of a run (the methods of [`Domain`] that say how its
/// runs are compared), over the scenario's domains in their order: the first that answers wins.
pub trait Reading {
    fn tolerance(&self) -> Option<Tolerance>;
    fn continuous_actor(&self, actor: Identity) -> bool;
    fn numeric_fields(&self, actor: Identity, site: Site, payload: &[u8]) -> Option<Numeric>;
}

/// The domains of a run, read in the scenario's order.
pub struct OfDomains<'a>(pub &'a [Box<dyn Domain>]);

impl Reading for OfDomains<'_> {
    fn tolerance(&self) -> Option<Tolerance> {
        self.0.iter().find_map(|d| d.tolerance())
    }
    fn continuous_actor(&self, actor: Identity) -> bool {
        self.0.iter().any(|d| d.continuous_actor(actor))
    }
    fn numeric_fields(&self, actor: Identity, site: Site, payload: &[u8]) -> Option<Numeric> {
        self.0
            .iter()
            .find_map(|d| d.numeric_fields(actor, site, payload))
    }
}

/// A numeric field of a row: the `f64`s as bits, so that a row is exactly what the results file
/// holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Number {
    pub name: String,
    pub unit: String,
    /// The unit's factor to the SI unit, as `f64` bits.
    pub factor: u64,
    pub value: u64,
}

impl Number {
    fn value(&self) -> f64 {
        f64::from_bits(self.value)
    }
    fn si(&self) -> f64 {
        f64::from_bits(self.value) * f64::from_bits(self.factor)
    }
}

/// A payload of a row: a digest, or numeric fields with the digest of the rest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Payload {
    Digest(Digest),
    Numbers { fields: Vec<Number>, rest: Digest },
}

/// An effect of a row (RFC 013 rule 21: kinds, sequence numbers, ports or channels, lengths).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Send {
        seq: u64,
        port: u32,
        len: u32,
        payload: Payload,
    },
    SetTimer {
        seq: u64,
        timer: u32,
        delay: u64,
    },
    CancelTimer {
        seq: u64,
        timer: u32,
    },
    Report {
        seq: u64,
        channel: u32,
        len: u32,
        payload: Payload,
    },
}

/// One trace record as the comparison reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    Handled {
        key: EventKey,
        target: Identity,
        /// The consumed work and the draws: compared exactly, but not for an actor of a continuous
        /// class (rule 20, item 4).
        work: u64,
        draws: u32,
        effects: Vec<Effect>,
    },
    Fault {
        key: EventKey,
        actor: Identity,
        kind: u8,
        parameter: u32,
        work: u64,
    },
    Intervention {
        key: EventKey,
        digest: Digest,
        len: u32,
    },
}

impl Row {
    fn key(&self) -> EventKey {
        match self {
            Row::Handled { key, .. } | Row::Fault { key, .. } | Row::Intervention { key, .. } => {
                *key
            }
        }
    }
}

/// What a trace lacks for the comparison (`replay.needs-payloads`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Missing {
    /// The records were not kept or not exported.
    Trace,
    /// A payload's bytes are not there.
    Payloads,
}

/// Reduce a trace to rows. Every payload of a `send` or a `report` is looked up by its digest in
/// `payloads`; one that is missing makes the trace unreadable for the comparison.
pub fn rows(
    records: Option<&[Record]>,
    payloads: &BTreeMap<Digest, Vec<u8>>,
    reading: &dyn Reading,
) -> Result<Vec<Row>, Missing> {
    let records = records.ok_or(Missing::Trace)?;
    let payload = |actor: Identity, site: Site, digest: &Digest| -> Result<Payload, Missing> {
        let bytes = payloads.get(digest).ok_or(Missing::Payloads)?;
        Ok(match reading.numeric_fields(actor, site, bytes) {
            Some(n) => Payload::Numbers {
                fields: n
                    .fields
                    .into_iter()
                    .map(|f| Number {
                        name: f.name,
                        unit: f.unit,
                        factor: f.factor.to_bits(),
                        value: f.value.to_bits(),
                    })
                    .collect(),
                rest: *blake3::hash(&n.exact).as_bytes(),
            },
            None => Payload::Digest(*digest),
        })
    };
    let mut out = Vec::with_capacity(records.len());
    for r in records {
        out.push(match r {
            Record::Handled {
                key,
                target,
                metered,
                draws,
                effects,
            } => {
                let mut es = Vec::with_capacity(effects.len());
                for e in effects {
                    es.push(match e {
                        CommittedEffect::Send {
                            seq,
                            port,
                            len,
                            digest,
                        } => Effect::Send {
                            seq: *seq,
                            port: *port,
                            len: *len,
                            payload: payload(*target, Site::Send { port: *port }, digest)?,
                        },
                        CommittedEffect::SetTimer { seq, timer, delay } => Effect::SetTimer {
                            seq: *seq,
                            timer: *timer,
                            delay: *delay,
                        },
                        CommittedEffect::CancelTimer { seq, timer } => Effect::CancelTimer {
                            seq: *seq,
                            timer: *timer,
                        },
                        CommittedEffect::Report {
                            seq,
                            channel,
                            len,
                            digest,
                        } => Effect::Report {
                            seq: *seq,
                            channel: *channel,
                            len: *len,
                            payload: payload(*target, Site::Report { channel: *channel }, digest)?,
                        },
                    });
                }
                Row::Handled {
                    key: *key,
                    target: *target,
                    work: *metered,
                    draws: *draws,
                    effects: es,
                }
            }
            Record::Fault {
                key,
                actor,
                kind,
                parameter,
                metered,
            } => Row::Fault {
                key: *key,
                actor: *actor,
                kind: *kind as u8,
                parameter: *parameter,
                work: *metered,
            },
            Record::Intervention {
                key,
                content_digest,
                content_len,
            } => Row::Intervention {
                key: *key,
                digest: *content_digest,
                len: *content_len,
            },
        });
    }
    Ok(out)
}

/// How a pair of records differs, when no numeric field is outside the tolerance
/// (`enum(record-difference)` of RFC 007).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordDifference {
    /// The key or the target differs.
    Key,
    /// The kind of record, the effects' kinds, sequence numbers, ports, channels or timers, or
    /// the consumed work or draws of an actor of a discrete class.
    Structure,
    /// A payload that is not a numeric field, or the rest of one that is.
    Payload,
    /// One trace holds more records than the other.
    Length,
}

impl RecordDifference {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Structure => "structure",
            Self::Payload => "payload",
            Self::Length => "length",
        }
    }
}

/// The first record at which two traces do not agree (RFC 013 rule 20).
#[derive(Clone, Debug, PartialEq)]
pub enum Difference {
    /// The records differ in more than a numeric value, or are not as many.
    Record { record: u64, what: RecordDifference },
    /// A numeric field outside the tolerance, with both values in the field's own unit.
    Value {
        record: u64,
        field: String,
        recorded: (String, String),
        replayed: (String, String),
    },
}

/// The field that used the largest share of its tolerance.
#[derive(Clone, Debug, PartialEq)]
pub struct Worst {
    pub field: String,
    /// The absolute difference in the field's own unit, as a number text and the unit symbol.
    pub largest: (String, String),
    /// The tick of its record.
    pub at: u64,
}

/// Two traces that agree record by record (`replay.within-tolerance`).
#[derive(Clone, Debug, PartialEq)]
pub struct Agreement {
    /// The pairs compared.
    pub compared: u64,
    /// The pairs with a numeric field that is not the same number.
    pub differing: u64,
    /// The numeric field that came nearest its tolerance, when any differs.
    pub worst: Option<Worst>,
    /// The pairs of an actor of a continuous class whose consumed work or draws differ: exempt,
    /// reported so that the exemption rests on evidence (rule 20, item 4).
    pub consumption_differing: u64,
}

/// A number as text: its shortest form that reads back as the same value, with an exponent when
/// it is very large or very small.
pub fn number_text(v: f64) -> String {
    if v == 0.0 {
        "0".into()
    } else if (1e-3..1e15).contains(&v.abs()) {
        format!("{v}")
    } else {
        format!("{v:e}")
    }
}

/// Whether two values of one field agree: `|a − b| ≤ atol + rtol·max(|a|, |b|)`, every NaN being
/// one value and two infinities of one sign equal (RFC 013 rule 20, item 2).
fn agree(a: f64, b: f64, tolerance: Tolerance) -> bool {
    if a.is_nan() || b.is_nan() {
        return a.is_nan() && b.is_nan();
    }
    if a.is_infinite() || b.is_infinite() {
        return a == b;
    }
    (a - b).abs() <= tolerance.atol + tolerance.rtol * a.abs().max(b.abs())
}

/// How far apart two values are: zero for two that are the same number (`-0` and `+0`, equal
/// infinities, any two NaNs), so that a pair that differs only in its bits has no distance.
fn distance(a: f64, b: f64) -> f64 {
    if a == b || (a.is_nan() && b.is_nan()) {
        0.0
    } else {
        (a - b).abs()
    }
}

/// The share of its tolerance a difference uses, for finding the field nearest it.
fn share(a: f64, b: f64, tolerance: Tolerance) -> f64 {
    let allowed = tolerance.atol + tolerance.rtol * a.abs().max(b.abs());
    let difference = distance(a, b);
    if difference == 0.0 {
        0.0
    } else if allowed > 0.0 {
        difference / allowed
    } else {
        f64::INFINITY
    }
}

/// Compare two row sequences by position (RFC 013 rule 20): `Ok` when every pair agrees and there
/// are as many, else the first difference. `rows` holds the same layout in both: a field is a
/// number in the one when it is in the other, as one scenario's domains read both.
pub fn compare(
    recorded: &[Row],
    replayed: &[Row],
    tolerance: Tolerance,
    reading: &dyn Reading,
) -> Result<Agreement, Box<Difference>> {
    let mut agreement = Agreement {
        compared: 0,
        differing: 0,
        worst: None,
        consumption_differing: 0,
    };
    let mut worst_share = 0.0f64;
    for (n, (a, b)) in recorded.iter().zip(replayed).enumerate() {
        // Records are counted from 1, as the stored points count records (RFC 013 rule 20).
        let record = n as u64 + 1;
        let record_difference = |what| Box::new(Difference::Record { record, what });
        if std::mem::discriminant(a) != std::mem::discriminant(b) {
            return Err(record_difference(RecordDifference::Structure));
        }
        if a.key() != b.key() {
            return Err(record_difference(RecordDifference::Key));
        }
        let mut differs = false;
        let mut consumption = false;
        match (a, b) {
            (
                Row::Handled {
                    target,
                    work,
                    draws,
                    effects,
                    ..
                },
                Row::Handled {
                    target: other_target,
                    work: other_work,
                    draws: other_draws,
                    effects: other_effects,
                    ..
                },
            ) => {
                if target != other_target {
                    return Err(record_difference(RecordDifference::Key));
                }
                if effects.len() != other_effects.len() {
                    return Err(record_difference(RecordDifference::Structure));
                }
                for (x, y) in effects.iter().zip(other_effects) {
                    let (px, py) = match (x, y) {
                        (
                            Effect::Send {
                                seq,
                                port,
                                len,
                                payload,
                            },
                            Effect::Send {
                                seq: s2,
                                port: p2,
                                len: l2,
                                payload: q,
                            },
                        ) if seq == s2 && port == p2 && len == l2 => (Some(payload), Some(q)),
                        (
                            Effect::Report {
                                seq,
                                channel,
                                len,
                                payload,
                            },
                            Effect::Report {
                                seq: s2,
                                channel: c2,
                                len: l2,
                                payload: q,
                            },
                        ) if seq == s2 && channel == c2 && len == l2 => (Some(payload), Some(q)),
                        (a, b)
                            if matches!(
                                (a, b),
                                (Effect::SetTimer { .. }, Effect::SetTimer { .. })
                                    | (Effect::CancelTimer { .. }, Effect::CancelTimer { .. })
                            ) && a == b =>
                        {
                            (None, None)
                        }
                        _ => return Err(record_difference(RecordDifference::Structure)),
                    };
                    if let (Some(p), Some(q)) = (px, py) {
                        match (p, q) {
                            (Payload::Digest(d), Payload::Digest(e)) if d == e => {}
                            (
                                Payload::Numbers { fields, rest },
                                Payload::Numbers {
                                    fields: other,
                                    rest: other_rest,
                                },
                            ) if fields.len() == other.len()
                                && rest == other_rest
                                && fields.iter().zip(other).all(|(f, g)| {
                                    f.name == g.name && f.unit == g.unit && f.factor == g.factor
                                }) =>
                            {
                                for (f, g) in fields.iter().zip(other) {
                                    let (fa, fb) = (f.si(), g.si());
                                    if !agree(fa, fb, tolerance) {
                                        return Err(Box::new(Difference::Value {
                                            record,
                                            field: f.name.clone(),
                                            recorded: (number_text(f.value()), f.unit.clone()),
                                            replayed: (number_text(g.value()), g.unit.clone()),
                                        }));
                                    }
                                    // A field differs when its bits differ (RFC 013 rule 20): `-0`
                                    // and `+0`, or two NaNs of other bits, differ and agree.
                                    if f.value != g.value {
                                        differs = true;
                                        let used = share(fa, fb, tolerance);
                                        if agreement.worst.is_none() || used > worst_share {
                                            worst_share = used;
                                            agreement.worst = Some(Worst {
                                                field: f.name.clone(),
                                                largest: (
                                                    number_text(distance(f.value(), g.value())),
                                                    f.unit.clone(),
                                                ),
                                                at: a.key().tick,
                                            });
                                        }
                                    }
                                }
                            }
                            _ => return Err(record_difference(RecordDifference::Payload)),
                        }
                    }
                }
                if work != other_work || draws != other_draws {
                    if reading.continuous_actor(*target) {
                        consumption = true;
                    } else {
                        return Err(record_difference(RecordDifference::Structure));
                    }
                }
            }
            (
                Row::Fault {
                    actor,
                    kind,
                    parameter,
                    work,
                    ..
                },
                Row::Fault {
                    actor: other_actor,
                    kind: other_kind,
                    parameter: other_parameter,
                    work: other_work,
                    ..
                },
            ) => {
                if actor != other_actor {
                    return Err(record_difference(RecordDifference::Key));
                }
                if kind != other_kind || parameter != other_parameter {
                    return Err(record_difference(RecordDifference::Structure));
                }
                if work != other_work {
                    if reading.continuous_actor(*actor) {
                        consumption = true;
                    } else {
                        return Err(record_difference(RecordDifference::Structure));
                    }
                }
            }
            (
                Row::Intervention { digest, len, .. },
                Row::Intervention {
                    digest: other_digest,
                    len: other_len,
                    ..
                },
            ) => {
                if digest != other_digest || len != other_len {
                    return Err(record_difference(RecordDifference::Payload));
                }
            }
            _ => unreachable!("the discriminants are equal"),
        }
        agreement.compared += 1;
        agreement.differing += u64::from(differs);
        agreement.consumption_differing += u64::from(consumption);
    }
    if recorded.len() != replayed.len() {
        return Err(Box::new(Difference::Record {
            record: recorded.len().min(replayed.len()) as u64 + 1,
            what: RecordDifference::Length,
        }));
    }
    Ok(agreement)
}

#[cfg(test)]
mod tests;
