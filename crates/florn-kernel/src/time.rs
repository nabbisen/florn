//! Logical time (RFC 001 §2, §3): an unsigned 64-bit tick count and, within a tick, a microstep.

/// Logical time in ticks (rule 1). The scenario declares the tick length; the kernel never knows it.
pub type Tick = u64;

/// The largest tick (rule 6): an event scheduled beyond it is refused.
pub const MAX_TICK: Tick = u64::MAX - 1;

/// An instant: a tick and a microstep (rule 10). Microsteps start at zero in every tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant {
    pub tick: Tick,
    pub microstep: u32,
}

impl Instant {
    pub const ZERO: Instant = Instant {
        tick: 0,
        microstep: 0,
    };

    pub const fn new(tick: Tick, microstep: u32) -> Self {
        Self { tick, microstep }
    }

    /// The instant an effect with delay `delay` produces from this one (rule 11): zero delay is
    /// the next microstep of the same tick; a positive delay is `tick + delay`, microstep zero.
    /// `None` when the result would be beyond the largest tick (rule 6).
    pub fn after(self, delay: Tick) -> Option<Instant> {
        if delay == 0 {
            Some(Instant {
                tick: self.tick,
                microstep: self.microstep.checked_add(1)?,
            })
        } else {
            let tick = self.tick.checked_add(delay)?;
            if tick > MAX_TICK {
                None
            } else {
                Some(Instant { tick, microstep: 0 })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delays() {
        let i = Instant::new(10, 3);
        assert_eq!(i.after(0), Some(Instant::new(10, 4)));
        assert_eq!(i.after(5), Some(Instant::new(15, 0)));
        assert_eq!(Instant::new(MAX_TICK, 0).after(1), None);
        assert_eq!(
            Instant::new(MAX_TICK - 1, 0).after(1),
            Some(Instant::new(MAX_TICK, 0))
        );
        assert!(Instant::new(1, 0) < Instant::new(1, 1));
        assert!(Instant::new(1, 9) < Instant::new(2, 0));
    }
}
