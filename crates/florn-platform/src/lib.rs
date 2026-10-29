//! The platform adapter trait (RFC 005 rules 12 and 13): the only place where a wall clock or a
//! source of entropy exists. Both reach the kernel only as recorded inputs or as metadata.
//!
//! PR-07 provides the operations the Host core needs for a run: the capacity probe, the seed of a
//! new run, and the wall clock for the diagnostic log's metadata. Transport and storage are the
//! front ends' and arrive with the headless runner and the browser Host.
#![forbid(unsafe_code)]

/// What this Host build on this machine can hold (RFC 003 rule 20).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capacity {
    /// Resident instances.
    pub instances: u64,
    /// Memory that may be reserved for instances, in bytes.
    pub memory_bytes: u64,
    /// The measured cost of one swap, in microseconds per KB, when the platform measured it.
    pub swap_cost_us_per_kb: Option<u64>,
}

pub trait Platform {
    /// The probe of RFC 003 rule 20, run at start-up.
    fn capacity(&mut self) -> Capacity;
    /// Entropy for the root seed of a new run, which then becomes a recorded input (rule 13).
    fn new_seed(&mut self) -> u64;
    /// The wall clock, in milliseconds since the Unix epoch: metadata only (RFC 001 rule 42).
    fn wall_clock_ms(&mut self) -> u64;
    /// A monotonic clock in microseconds, for the Host's own measurements (RFC 009 rule 14: the
    /// cost of a swap); never a run input. The wall clock's milliseconds where nothing finer
    /// exists.
    fn monotonic_us(&mut self) -> u64 {
        self.wall_clock_ms().saturating_mul(1_000)
    }
}

/// A platform for tests: fixed capacity, a seed sequence, a clock that only moves when told.
#[derive(Clone, Debug)]
pub struct FixedPlatform {
    pub capacity: Capacity,
    pub seeds: Vec<u64>,
    pub clock_ms: u64,
}

impl Default for FixedPlatform {
    fn default() -> Self {
        Self {
            capacity: Capacity {
                instances: 10_000,
                memory_bytes: 1 << 40,
                swap_cost_us_per_kb: None,
            },
            seeds: vec![7],
            clock_ms: 0,
        }
    }
}

impl Platform for FixedPlatform {
    fn capacity(&mut self) -> Capacity {
        self.capacity
    }
    fn new_seed(&mut self) -> u64 {
        if self.seeds.len() > 1 {
            self.seeds.remove(0)
        } else {
            self.seeds.first().copied().unwrap_or(0)
        }
    }
    fn wall_clock_ms(&mut self) -> u64 {
        self.clock_ms
    }
}
