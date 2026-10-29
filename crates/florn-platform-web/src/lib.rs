//! The browser platform adapter (RFC 005 rules 12 and 13): the wall clock and the entropy of the
//! worker, reached through imports under the module name `florn` that the worker's script
//! provides; the capacity the page measured or assumed; the only clock the benchmarks read.
//! Transport is the worker's exports (`florn-ui-web`); storage is 0.2.
//!
//! `unsafe` appears only at the three imports, each a call that takes nothing and returns a
//! number: the invariant is the worker's script, which defines them before the Host starts.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use florn_platform::{Capacity, Platform};

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "florn")]
unsafe extern "C" {
    fn wall_clock_ms() -> f64;
    fn entropy() -> u32;
    /// `performance.now()` in seconds: the pacing clock, never a run input.
    fn perf_now() -> f64;
}

/// Milliseconds since the Unix epoch, from the worker.
pub fn clock_ms() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        // SAFETY: the import takes nothing and returns a number.
        unsafe { wall_clock_ms() }.max(0.0) as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0
    }
}

/// The pacing clock in seconds (RFC 001 rule 9: it decides when the Host works, never what).
pub fn now_seconds() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        // SAFETY: as above.
        unsafe { perf_now() }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0.0
    }
}

fn entropy_word() -> u32 {
    #[cfg(target_arch = "wasm32")]
    {
        // SAFETY: as above.
        unsafe { entropy() }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0
    }
}

/// The browser Host in this worker.
#[derive(Clone, Debug)]
pub struct WebPlatform {
    capacity: Capacity,
}

impl WebPlatform {
    /// The capacity the page gives: the family's measured ceiling with a margin
    /// (RFC 003 rule 20; the ceilings measured by the conformance kit's probe).
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

impl Platform for WebPlatform {
    fn capacity(&mut self) -> Capacity {
        self.capacity
    }

    /// Two words of the worker's entropy: the seed is then an input (rule 13).
    fn new_seed(&mut self) -> u64 {
        (u64::from(entropy_word()) << 32) | u64::from(entropy_word())
    }

    fn wall_clock_ms(&mut self) -> u64 {
        clock_ms()
    }
    fn monotonic_us(&mut self) -> u64 {
        (now_seconds() * 1_000_000.0).max(0.0) as u64
    }
}
