//! The display stream (RFC 005 rule 14): the high-rate telemetry a front end draws from, as
//! fixed-size binary records of sixteen bytes, written by the Host after it reduced (OBS-003,
//! OBS-008) and carried by a transport the reader does not see: a ring in shared memory, or
//! one message per Host tick. The trace is never affected by this path.
//!
//! The stream is a sequence of batches, one per Host tick (a slice of a paced advance or one
//! `advance` command); a batch ends with the instant the Host reached. A record stands alone:
//! a reader needs no state to decode it.
//!
//! ```text
//! offset  size  field   instant        flight                          activity
//! 0       1     kind    0              1                               2
//! 1       1     byte    0              the sender's port index         bit 0: a fault occurred
//! 2       2     half    0              the connection's index          activations (saturating)
//! 4       4     word    0              the delay in ticks, as an f32   the actor
//! 8       8     tick    the instant    the instant sent                the last activation
//! ```
//!
//! Every field is little-endian. A connection's index is its position among the scenario's
//! connections in id order, and a port's is its position in the connection's ports (255 for
//! one beyond the 255th); the front end has both from the `scenario` query. The delay is a
//! 32-bit float because the display interpolates a flight's position and a float keeps every
//! delay a scenario can state; the exact ticks are in the trace.
use std::collections::BTreeMap;

/// The size of one record.
pub const RECORD_BYTES: usize = 16;
/// The stream's layout version, carried by the transport, not by each record.
pub const VERSION: u32 = 1;

/// One record of the display stream.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Record {
    /// The instant the Host reached: the last record of a batch.
    Instant { tick: u64 },
    /// A delivery in flight: on `connection`, from its port `port`, for `delay` ticks from
    /// `sent`. Written when the delivery is scheduled, so that the display can draw it moving.
    Flight {
        connection: u16,
        port: u8,
        delay: f32,
        sent: u64,
    },
    /// An actor's activations in the batch, `count` of them (saturating), the last at `last`;
    /// `faulted` when one of them faulted.
    Activity {
        actor: u32,
        count: u16,
        faulted: bool,
        last: u64,
    },
}

const KIND_INSTANT: u8 = 0;
const KIND_FLIGHT: u8 = 1;
const KIND_ACTIVITY: u8 = 2;

impl Record {
    /// The sixteen bytes.
    pub fn encode(&self) -> [u8; RECORD_BYTES] {
        let (kind, byte, half, word, tick) = match *self {
            Record::Instant { tick } => (KIND_INSTANT, 0, 0, 0, tick),
            Record::Flight {
                connection,
                port,
                delay,
                sent,
            } => (KIND_FLIGHT, port, connection, delay.to_bits(), sent),
            Record::Activity {
                actor,
                count,
                faulted,
                last,
            } => (KIND_ACTIVITY, u8::from(faulted), count, actor, last),
        };
        let mut b = [0u8; RECORD_BYTES];
        b[0] = kind;
        b[1] = byte;
        b[2..4].copy_from_slice(&half.to_le_bytes());
        b[4..8].copy_from_slice(&word.to_le_bytes());
        b[8..16].copy_from_slice(&tick.to_le_bytes());
        b
    }

    /// The record of sixteen bytes, or `None` for a short slice or an unknown kind.
    pub fn decode(b: &[u8]) -> Option<Self> {
        if b.len() < RECORD_BYTES {
            return None;
        }
        let byte = b[1];
        let half = u16::from_le_bytes([b[2], b[3]]);
        let word = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
        let tick = u64::from_le_bytes(b[8..16].try_into().ok()?);
        Some(match b[0] {
            KIND_INSTANT => Record::Instant { tick },
            KIND_FLIGHT => Record::Flight {
                connection: half,
                port: byte,
                delay: f32::from_bits(word),
                sent: tick,
            },
            KIND_ACTIVITY => Record::Activity {
                actor: word,
                count: half,
                faulted: byte & 1 == 1,
                last: tick,
            },
            _ => return None,
        })
    }
}

/// The records as one byte string, in order.
pub fn encode_all(records: &[Record]) -> Vec<u8> {
    let mut out = Vec::with_capacity(records.len() * RECORD_BYTES);
    for r in records {
        out.extend_from_slice(&r.encode());
    }
    out
}

/// The records of a byte string; `None` when its length is no multiple of a record's or a
/// record is of no known kind.
pub fn decode_all(bytes: &[u8]) -> Option<Vec<Record>> {
    if !bytes.len().is_multiple_of(RECORD_BYTES) {
        return None;
    }
    bytes
        .as_chunks::<RECORD_BYTES>()
        .0
        .iter()
        .map(|c| Record::decode(c))
        .collect()
}

/// `enum(transport)` of RFC 007: what carries the stream to the display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    /// The ring in shared memory, where the page is cross-origin isolated.
    Shared,
    /// One transferred message per Host tick.
    Messages,
}

impl Transport {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Messages => "messages",
        }
    }
    /// The path records are dropped on, for `telemetry.dropped`.
    pub fn path(self) -> DisplayPath {
        match self {
            Self::Shared => DisplayPath::Ring,
            Self::Messages => DisplayPath::Messages,
        }
    }
}

/// `enum(display-path)` of RFC 007: where records were dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayPath {
    Ring,
    Messages,
}

impl DisplayPath {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ring => "ring",
            Self::Messages => "messages",
        }
    }
}

/// An actor's activity folded over the batches read since the display last drew.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Activity {
    pub count: u32,
    pub faulted: bool,
    pub last: u64,
}

/// The reduction above the transport (rule 14), the same for both: the batches a display read
/// since it last drew, folded into one update. Every flight is kept, in order (what to draw of
/// them, to the display's ceiling, is the view model's choice); an actor's activity is summed;
/// the instant is the last reached.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reduced {
    pub instant: Option<u64>,
    pub flights: Vec<Record>,
    pub activity: BTreeMap<u32, Activity>,
}

impl Reduced {
    /// Fold records into this reduction, in their order.
    pub fn fold(&mut self, records: &[Record]) {
        for r in records {
            match *r {
                Record::Instant { tick } => self.instant = Some(tick),
                Record::Flight { .. } => self.flights.push(*r),
                Record::Activity {
                    actor,
                    count,
                    faulted,
                    last,
                } => {
                    let a = self.activity.entry(actor).or_default();
                    a.count = a.count.saturating_add(u32::from(count));
                    a.faulted |= faulted;
                    a.last = a.last.max(last);
                }
            }
        }
    }
    /// Clear what was drawn, keeping nothing.
    pub fn clear(&mut self) {
        self.instant = None;
        self.flights.clear();
        self.activity.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_round_trips_in_sixteen_bytes() {
        let records = [
            Record::Instant { tick: u64::MAX },
            Record::Flight {
                connection: 65_535,
                port: 255,
                delay: 1.5e12,
                sent: 7,
            },
            Record::Activity {
                actor: 42,
                count: 65_535,
                faulted: true,
                last: 9,
            },
        ];
        for r in records {
            let b = r.encode();
            assert_eq!(b.len(), RECORD_BYTES);
            assert_eq!(Record::decode(&b), Some(r));
        }
        let bytes = encode_all(&records);
        assert_eq!(decode_all(&bytes).as_deref(), Some(&records[..]));
        assert_eq!(decode_all(&bytes[..17]), None);
        let mut unknown = records[0].encode();
        unknown[0] = 9;
        assert_eq!(Record::decode(&unknown), None);
        assert_eq!(Record::decode(&unknown[..15]), None);
    }

    #[test]
    fn the_reduction_folds_batches() {
        let mut r = Reduced::default();
        r.fold(&[
            Record::Activity {
                actor: 1,
                count: 2,
                faulted: false,
                last: 5,
            },
            Record::Flight {
                connection: 0,
                port: 1,
                delay: 10.0,
                sent: 3,
            },
            Record::Instant { tick: 5 },
        ]);
        r.fold(&[
            Record::Activity {
                actor: 1,
                count: 3,
                faulted: true,
                last: 8,
            },
            Record::Instant { tick: 9 },
        ]);
        assert_eq!(r.instant, Some(9));
        assert_eq!(r.flights.len(), 1);
        assert_eq!(
            r.activity.get(&1),
            Some(&Activity {
                count: 5,
                faulted: true,
                last: 8
            })
        );
        r.clear();
        assert_eq!(r, Reduced::default());
    }
}
