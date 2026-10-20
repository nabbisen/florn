//! The native platform adapter (RFC 005 rules 12 and 13): the wall clock for pacing and for the
//! log's metadata, entropy for the seed of a new run, and the capacity probe. Files and standard
//! streams are the runner's own; the watchdog is the engine adapter's epoch.
#![forbid(unsafe_code)]

use std::hash::{BuildHasher, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

use florn_platform::{Capacity, Platform};

/// The native Host on this machine. The instance count is bounded by the engine's store limits
/// (measured: 20,000 instances of the echo guest in one store at 5.3 µs each, the probe's own
/// bound), the memory by what the runner is told it may use; both are overridable from the
/// command line.
#[derive(Clone, Debug)]
pub struct NativePlatform {
    capacity: Capacity,
}

impl NativePlatform {
    /// Provisional: 100,000 instances, 4 GiB.
    pub const DEFAULT_INSTANCES: u64 = 100_000;
    pub const DEFAULT_MEMORY_BYTES: u64 = 4 * 1024 * 1024 * 1024;

    pub fn new() -> Self {
        Self::with_capacity(Self::DEFAULT_INSTANCES, Self::DEFAULT_MEMORY_BYTES)
    }

    pub fn with_capacity(instances: u64, memory_bytes: u64) -> Self {
        Self {
            capacity: Capacity {
                instances,
                memory_bytes,
                swap_cost_us_per_kb: None,
            },
        }
    }
}

impl Default for NativePlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl Platform for NativePlatform {
    fn capacity(&mut self) -> Capacity {
        self.capacity
    }

    /// Entropy from the operating system, through the standard library's randomly keyed hasher,
    /// mixed with the clock: the seed is then an input, never consulted again (rule 13).
    fn new_seed(&mut self) -> u64 {
        let mut h = std::hash::RandomState::new().build_hasher();
        h.write_u128(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        );
        h.finish()
    }

    fn wall_clock_ms(&mut self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_differ_and_the_clock_moves_forward() {
        let mut p = NativePlatform::new();
        let a = p.new_seed();
        let b = p.new_seed();
        assert_ne!(a, b);
        assert!(p.wall_clock_ms() > 1_700_000_000_000);
        assert_eq!(p.capacity().instances, NativePlatform::DEFAULT_INSTANCES);
    }
}
