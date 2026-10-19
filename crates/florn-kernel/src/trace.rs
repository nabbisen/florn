//! The trace and the digest (RFC 001 §10; RFC 005 §5): records in the canonical encoding with
//! payload digests computed once, a BLAKE3 chain stored at fixed intervals and at the end, the
//! comparison of two runs' stored chain values, and per-actor chains computed from kept records.

use crate::effect::Effect;
use crate::key::{EventKey, Identity};

/// A 32-byte BLAKE3 digest.
pub type Digest = [u8; 32];

/// The kinds of fault (RFC 002 rule 41), as the trace encodes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FaultKind {
    Trap = 1,
    Budget = 2,
    Stack = 3,
    Memory = 4,
    Quota = 5,
    Contract = 6,
}

/// A record of the trace (RFC 001 rule 44; RFC 005 rule 20).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Record {
    /// A handled event and what it committed.
    Handled {
        key: EventKey,
        target: Identity,
        metered: u64,
        draws: u32,
        effects: Vec<CommittedEffect>,
    },
    /// An activation that ended in a fault: nothing committed.
    Fault {
        key: EventKey,
        actor: Identity,
        kind: FaultKind,
        parameter: u32,
        metered: u64,
    },
    /// An intervention applied at the instant boundary.
    Intervention {
        key: EventKey,
        content_digest: Digest,
        content_len: u32,
    },
}

/// A committed effect with its sequence number (RFC 001 rule 17); payloads as digests (RFC 005 rule 21).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommittedEffect {
    Send {
        seq: u64,
        port: u32,
        len: u32,
        digest: Digest,
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
        digest: Digest,
    },
}

impl CommittedEffect {
    /// From an effect and its sequence number; the payload is hashed here, once.
    pub fn from_effect(seq: u64, e: &Effect) -> Self {
        match e {
            Effect::Send { port, payload } => Self::Send {
                seq,
                port: *port,
                len: payload.len() as u32,
                digest: *blake3::hash(payload).as_bytes(),
            },
            Effect::SetTimer { timer, delay } => Self::SetTimer {
                seq,
                timer: *timer,
                delay: *delay,
            },
            Effect::CancelTimer { timer } => Self::CancelTimer { seq, timer: *timer },
            Effect::Report { channel, payload } => Self::Report {
                seq,
                channel: *channel,
                len: payload.len() as u32,
                digest: *blake3::hash(payload).as_bytes(),
            },
        }
    }
    pub fn seq(&self) -> u64 {
        match self {
            Self::Send { seq, .. }
            | Self::SetTimer { seq, .. }
            | Self::CancelTimer { seq, .. }
            | Self::Report { seq, .. } => *seq,
        }
    }
}

/// The canonical encoding of a record (RFC 004 rule 30 applied to RFC 005 rule 20's formats).
pub fn encode(r: &Record) -> Vec<u8> {
    let mut w = Vec::with_capacity(96);
    fn key(w: &mut Vec<u8>, k: &EventKey) {
        w.extend_from_slice(&k.tick.to_le_bytes());
        w.extend_from_slice(&k.microstep.to_le_bytes());
        w.push(k.class);
        w.extend_from_slice(&k.tie.to_le_bytes());
        w.extend_from_slice(&k.origin.to_le_bytes());
        w.extend_from_slice(&k.seq.to_le_bytes());
    }
    match r {
        Record::Handled {
            key: k,
            target,
            metered,
            draws,
            effects,
        } => {
            w.push(1);
            key(&mut w, k);
            w.extend_from_slice(&target.to_le_bytes());
            w.extend_from_slice(&metered.to_le_bytes());
            w.extend_from_slice(&draws.to_le_bytes());
            w.extend_from_slice(&(effects.len() as u32).to_le_bytes());
            for e in effects {
                match e {
                    CommittedEffect::Send {
                        seq,
                        port,
                        len,
                        digest,
                    } => {
                        w.push(1);
                        w.extend_from_slice(&seq.to_le_bytes());
                        w.extend_from_slice(&port.to_le_bytes());
                        w.extend_from_slice(&len.to_le_bytes());
                        w.extend_from_slice(digest);
                    }
                    CommittedEffect::SetTimer { seq, timer, delay } => {
                        w.push(2);
                        w.extend_from_slice(&seq.to_le_bytes());
                        w.extend_from_slice(&timer.to_le_bytes());
                        w.extend_from_slice(&delay.to_le_bytes());
                    }
                    CommittedEffect::CancelTimer { seq, timer } => {
                        w.push(3);
                        w.extend_from_slice(&seq.to_le_bytes());
                        w.extend_from_slice(&timer.to_le_bytes());
                    }
                    CommittedEffect::Report {
                        seq,
                        channel,
                        len,
                        digest,
                    } => {
                        w.push(4);
                        w.extend_from_slice(&seq.to_le_bytes());
                        w.extend_from_slice(&channel.to_le_bytes());
                        w.extend_from_slice(&len.to_le_bytes());
                        w.extend_from_slice(digest);
                    }
                }
            }
        }
        Record::Fault {
            key: k,
            actor,
            kind,
            parameter,
            metered,
        } => {
            w.push(2);
            key(&mut w, k);
            w.extend_from_slice(&actor.to_le_bytes());
            w.push(*kind as u8);
            w.extend_from_slice(&parameter.to_le_bytes());
            w.extend_from_slice(&metered.to_le_bytes());
        }
        Record::Intervention {
            key: k,
            content_digest,
            content_len,
        } => {
            w.push(3);
            key(&mut w, k);
            w.extend_from_slice(&content_len.to_le_bytes());
            w.extend_from_slice(content_digest);
        }
    }
    w
}

/// The inverse of [`encode`]: `None` for anything that is not exactly one record.
pub fn decode(bytes: &[u8]) -> Option<Record> {
    struct R<'a>(&'a [u8], usize);
    impl R<'_> {
        fn take(&mut self, n: usize) -> Option<&[u8]> {
            let s = self.0.get(self.1..self.1.checked_add(n)?)?;
            self.1 += n;
            Some(s)
        }
        fn u8(&mut self) -> Option<u8> {
            Some(self.take(1)?[0])
        }
        fn u32(&mut self) -> Option<u32> {
            Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
        }
        fn u64(&mut self) -> Option<u64> {
            Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
        }
        fn digest(&mut self) -> Option<Digest> {
            self.take(32)?.try_into().ok()
        }
        fn key(&mut self) -> Option<EventKey> {
            Some(EventKey {
                tick: self.u64()?,
                microstep: self.u32()?,
                class: self.u8()?,
                tie: self.u64()?,
                origin: self.u32()?,
                seq: self.u64()?,
            })
        }
    }
    let mut r = R(bytes, 0);
    let record = match r.u8()? {
        1 => {
            let key = r.key()?;
            let target = r.u32()?;
            let metered = r.u64()?;
            let draws = r.u32()?;
            let n = r.u32()? as usize;
            if n > bytes.len() {
                return None;
            }
            let mut effects = Vec::with_capacity(n);
            for _ in 0..n {
                effects.push(match r.u8()? {
                    1 => CommittedEffect::Send {
                        seq: r.u64()?,
                        port: r.u32()?,
                        len: r.u32()?,
                        digest: r.digest()?,
                    },
                    2 => CommittedEffect::SetTimer {
                        seq: r.u64()?,
                        timer: r.u32()?,
                        delay: r.u64()?,
                    },
                    3 => CommittedEffect::CancelTimer {
                        seq: r.u64()?,
                        timer: r.u32()?,
                    },
                    4 => CommittedEffect::Report {
                        seq: r.u64()?,
                        channel: r.u32()?,
                        len: r.u32()?,
                        digest: r.digest()?,
                    },
                    _ => return None,
                });
            }
            Record::Handled {
                key,
                target,
                metered,
                draws,
                effects,
            }
        }
        2 => {
            let key = r.key()?;
            let actor = r.u32()?;
            let kind = match r.u8()? {
                1 => FaultKind::Trap,
                2 => FaultKind::Budget,
                3 => FaultKind::Stack,
                4 => FaultKind::Memory,
                5 => FaultKind::Quota,
                6 => FaultKind::Contract,
                _ => return None,
            };
            Record::Fault {
                key,
                actor,
                kind,
                parameter: r.u32()?,
                metered: r.u64()?,
            }
        }
        3 => {
            let key = r.key()?;
            let content_len = r.u32()?;
            Record::Intervention {
                key,
                content_digest: r.digest()?,
                content_len,
            }
        }
        _ => return None,
    };
    (r.1 == bytes.len()).then_some(record)
}

/// The actor a record concerns, for per-actor chains (RFC 005 rule 24).
pub fn actor_of(r: &Record) -> Option<Identity> {
    match r {
        Record::Handled { target, .. } => Some(*target),
        Record::Fault { actor, .. } => Some(*actor),
        Record::Intervention { .. } => None,
    }
}

/// The chain's state at a boundary: enough to continue the trace (RFC 001 rule 52).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceSnapshot {
    pub chain: Digest,
    pub records: u64,
    pub stored: Vec<(u64, Digest)>,
}

/// The trace of a run: the chain, the stored chain values, and optionally the records themselves.
#[derive(Clone, Debug)]
pub struct Trace {
    chain: Digest,
    records: u64,
    interval: u64,
    /// (record count at which it was stored, chain value) — at every multiple of `interval` and at the end.
    stored: Vec<(u64, Digest)>,
    recording: Option<Vec<Record>>,
}

impl Trace {
    /// The provisional chain interval (RFC 005 rule 23).
    pub const DEFAULT_INTERVAL: u64 = 10_000;

    pub fn new(interval: u64, record: bool) -> Self {
        Self {
            chain: [0; 32],
            records: 0,
            interval: interval.max(1),
            stored: Vec::new(),
            recording: if record { Some(Vec::new()) } else { None },
        }
    }

    /// Append a record: hash it into the chain (RFC 001 rule 48), store the chain at the interval.
    pub fn push(&mut self, r: Record) {
        let bytes = encode(&r);
        let mut h = blake3::Hasher::new();
        h.update(&self.chain);
        h.update(&bytes);
        self.chain = *h.finalize().as_bytes();
        self.records += 1;
        if self.records.is_multiple_of(self.interval) {
            self.stored.push((self.records, self.chain));
        }
        if let Some(v) = &mut self.recording {
            v.push(r);
        }
    }

    /// The chain's state at a boundary, for a checkpoint (RFC 001 rule 51).
    pub fn snapshot(&self) -> TraceSnapshot {
        TraceSnapshot {
            chain: self.chain,
            records: self.records,
            stored: self.stored.clone(),
        }
    }

    /// A trace continuing from a snapshot (RFC 001 rule 52). Kept records start empty.
    pub fn restore(interval: u64, record: bool, s: TraceSnapshot) -> Self {
        Self {
            chain: s.chain,
            records: s.records,
            interval: interval.max(1),
            stored: s.stored,
            recording: if record { Some(Vec::new()) } else { None },
        }
    }

    /// The chain value now.
    pub fn chain(&self) -> Digest {
        self.chain
    }
    pub fn chain_hex(&self) -> String {
        hex(&self.chain)
    }
    pub fn records(&self) -> u64 {
        self.records
    }
    pub fn interval(&self) -> u64 {
        self.interval
    }
    /// The stored values, with the end added (RFC 001 rule 48).
    pub fn stored(&self) -> Vec<(u64, Digest)> {
        let mut v = self.stored.clone();
        if v.last().map(|(n, _)| *n) != Some(self.records) {
            v.push((self.records, self.chain));
        }
        v
    }
    pub fn kept_records(&self) -> Option<&[Record]> {
        self.recording.as_deref()
    }

    /// The chain over the records of one actor, from kept records (RFC 005 rule 24).
    pub fn actor_chain(&self, actor: Identity) -> Option<Digest> {
        let records = self.recording.as_ref()?;
        let mut chain = [0u8; 32];
        for r in records.iter().filter(|r| actor_of(r) == Some(actor)) {
            let mut h = blake3::Hasher::new();
            h.update(&chain);
            h.update(&encode(r));
            chain = *h.finalize().as_bytes();
        }
        Some(chain)
    }

    /// Compare this run's stored values with another's (RFC 001 rule 50).
    pub fn compare(&self, other: &[(u64, Digest)]) -> Comparison {
        compare(&self.stored(), other)
    }
}

/// The result of comparing two runs' stored chain values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Comparison {
    /// Equal at every stored point.
    Matched { points: u64 },
    /// The first stored point that differs lies after `from` records and at `to`.
    Diverged { from: u64, to: u64 },
    /// One run has stored points the other has not (different interval or length).
    Incomparable,
}

pub fn compare(a: &[(u64, Digest)], b: &[(u64, Digest)]) -> Comparison {
    let mut last_equal = 0u64;
    let mut points = 0u64;
    for (x, y) in a.iter().zip(b) {
        if x.0 != y.0 {
            return Comparison::Incomparable;
        }
        if x.1 != y.1 {
            return Comparison::Diverged {
                from: last_equal,
                to: x.0,
            };
        }
        last_equal = x.0;
        points += 1;
    }
    if a.len() != b.len() {
        Comparison::Incomparable
    } else {
        Comparison::Matched { points }
    }
}

pub fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(seq: u64) -> Record {
        Record::Handled {
            key: EventKey {
                tick: seq,
                microstep: 0,
                class: 1,
                tie: 0,
                origin: 1,
                seq,
            },
            target: 1,
            metered: 7,
            draws: 0,
            effects: vec![],
        }
    }

    #[test]
    fn chain_interval_and_comparison() {
        let mut a = Trace::new(3, false);
        let mut b = Trace::new(3, false);
        for i in 0..7 {
            a.push(rec(i));
            b.push(rec(i));
        }
        assert_eq!(
            a.stored().iter().map(|s| s.0).collect::<Vec<_>>(),
            vec![3, 6, 7]
        );
        assert_eq!(a.compare(&b.stored()), Comparison::Matched { points: 3 });
        let mut c = Trace::new(3, false);
        for i in 0..7 {
            c.push(rec(if i == 4 { 99 } else { i }));
        }
        assert_eq!(
            a.compare(&c.stored()),
            Comparison::Diverged { from: 3, to: 6 }
        );
        let mut d = Trace::new(2, false);
        d.push(rec(0));
        assert_eq!(a.compare(&d.stored()), Comparison::Incomparable);
    }

    #[test]
    fn payload_hashed_once_and_encoding_is_fixed() {
        let e = CommittedEffect::from_effect(
            1,
            &Effect::Send {
                port: 0,
                payload: vec![1, 2, 3],
            },
        );
        match &e {
            CommittedEffect::Send { len, digest, .. } => {
                assert_eq!(*len, 3);
                assert_eq!(digest, blake3::hash(&[1, 2, 3]).as_bytes());
            }
            _ => panic!(),
        }
        let r = Record::Handled {
            key: EventKey {
                tick: 1,
                microstep: 2,
                class: 3,
                tie: 4,
                origin: 5,
                seq: 6,
            },
            target: 7,
            metered: 8,
            draws: 9,
            effects: vec![e],
        };
        let bytes = encode(&r);
        assert_eq!(bytes.len(), 1 + 33 + 4 + 8 + 4 + 4 + (1 + 8 + 4 + 4 + 32));
        assert_eq!(&bytes[..9], &[1, 1, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn per_actor_chain_from_kept_records() {
        let mut t = Trace::new(100, true);
        t.push(rec(0));
        t.push(Record::Handled {
            key: EventKey {
                tick: 1,
                microstep: 0,
                class: 1,
                tie: 0,
                origin: 2,
                seq: 0,
            },
            target: 2,
            metered: 0,
            draws: 0,
            effects: vec![],
        });
        t.push(rec(2));
        let one = t.actor_chain(1).unwrap();
        let two = t.actor_chain(2).unwrap();
        assert_ne!(one, two);
        assert_ne!(one, [0; 32]);
        assert_eq!(Trace::new(100, false).actor_chain(1), None);
    }
}
