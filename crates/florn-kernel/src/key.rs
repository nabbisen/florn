//! The order of events (RFC 001 §4): the key of rule 14, the tie policies of rule 18, and the
//! property that no part of the key depends on arrival, the wall clock, or a thread (rule 19).

use crate::time::{Instant, Tick};

/// The stable identity of an origin or a target (RFC 004 rule 7; RFC 001 rule 16).
pub type Identity = u32;

/// The tie policy a scenario selects (rule 18).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TiePolicy {
    /// Tie value zero: equal-time events of one class are ordered by origin.
    ByIdentity,
    /// Tie value: a keyed hash of the root seed, tick, microstep, class, and origin.
    BySeed,
}

impl TiePolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ByIdentity => "by-identity",
            Self::BySeed => "by-seed",
        }
    }
}

/// The key of rule 14, compared field by field in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventKey {
    pub tick: Tick,
    pub microstep: u32,
    pub class: u8,
    pub tie: u64,
    pub origin: Identity,
    pub seq: u64,
}

impl EventKey {
    pub fn instant(&self) -> Instant {
        Instant {
            tick: self.tick,
            microstep: self.microstep,
        }
    }
}

/// The keyed hash of the `by-seed` policy. The key is derived from the root seed once per run.
#[derive(Clone, Copy, Debug)]
pub struct TieHasher {
    key: [u8; 32],
}

impl TieHasher {
    pub const CONTEXT: &'static str = "florn:core/tie@0.1.0";

    pub fn new(root_seed: u64) -> Self {
        Self {
            key: blake3::derive_key(Self::CONTEXT, &root_seed.to_le_bytes()),
        }
    }

    /// The tie value of an event at `instant` with `class` from `origin` (rule 18).
    pub fn tie(&self, instant: Instant, class: u8, origin: Identity) -> u64 {
        let mut material = [0u8; 17];
        material[..8].copy_from_slice(&instant.tick.to_le_bytes());
        material[8..12].copy_from_slice(&instant.microstep.to_le_bytes());
        material[12] = class;
        material[13..17].copy_from_slice(&origin.to_le_bytes());
        let h = blake3::keyed_hash(&self.key, &material);
        u64::from_le_bytes(h.as_bytes()[..8].try_into().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_order_is_field_order() {
        let base = EventKey {
            tick: 5,
            microstep: 1,
            class: 2,
            tie: 0,
            origin: 7,
            seq: 3,
        };
        assert!(
            EventKey {
                tick: 4,
                microstep: 9,
                ..base
            } < base
        );
        assert!(
            EventKey {
                microstep: 0,
                class: 9,
                ..base
            } < base
        );
        assert!(
            EventKey {
                class: 1,
                origin: 99,
                ..base
            } < base
        );
        assert!(
            EventKey {
                tie: 0,
                origin: 6,
                ..base
            } < base
        );
        assert!(EventKey { seq: 2, ..base } < base);
    }

    #[test]
    fn tie_is_a_function_of_seed_instant_class_origin() {
        let a = TieHasher::new(42);
        let b = TieHasher::new(42);
        let c = TieHasher::new(43);
        let i = Instant::new(10, 0);
        assert_eq!(a.tie(i, 1, 1), b.tie(i, 1, 1));
        assert_ne!(a.tie(i, 1, 1), c.tie(i, 1, 1));
        assert_ne!(a.tie(i, 1, 1), a.tie(i, 1, 2));
        assert_ne!(a.tie(i, 1, 1), a.tie(Instant::new(11, 0), 1, 1));
    }
}
