//! The distributions of RFC 001 rule 30 over the Host's uniform 64-bit integers, by algorithms
//! florn specifies here, so that every guest computes the same value from the same draw.
//!
//! | Function | Algorithm |
//! |---|---|
//! | `below(n)` | Lemire's multiply-and-reject: `x·n` in 128 bits; reject while the low word is below `2^64 mod n`; one draw per attempt |
//! | `f64()` | The high 53 bits of one draw, times `2^-53`: uniform on `[0, 1)` |
//! | `chance(p)` | `f64() < p` |
//! | `exponential(rate)` | `-ln(1 - f64()) / rate`, `ln` from `libm` |
//! | `normal(mean, sd)` | Box–Muller: `u1 = 1 - f64()`, `u2 = f64()`; `mean + sd · sqrt(-2 ln u1) · cos(2π u2)`; two draws |
//!
//! Floating-point results are deterministic because `libm`'s functions are pure code, the same on
//! every engine (RFC 003 §9).

/// A source of uniform 64-bit integers: the Host's `random`, or a fixed sequence in tests.
pub trait Source {
    fn next(&mut self) -> u64;
}

/// The Host's stream of this actor (RFC 002 rule 30).
#[derive(Clone, Copy, Debug, Default)]
pub struct Host;

impl Source for Host {
    fn next(&mut self) -> u64 {
        crate::host::random()
    }
}

/// The distributions over a source.
#[derive(Clone, Debug, Default)]
pub struct Random<S: Source = Host> {
    source: S,
}

impl Random<Host> {
    /// Over the Host's stream.
    pub const fn host() -> Self {
        Self { source: Host }
    }
}

impl<S: Source> Random<S> {
    pub fn over(source: S) -> Self {
        Self { source }
    }

    /// One uniform 64-bit draw.
    pub fn u64(&mut self) -> u64 {
        self.source.next()
    }

    /// Uniform on `0..n`; `n = 0` is a contract violation and traps.
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            crate::actor::trap();
        }
        let threshold = n.wrapping_neg() % n;
        loop {
            let x = self.source.next();
            let m = (x as u128) * (n as u128);
            if (m as u64) >= threshold {
                return (m >> 64) as u64;
            }
        }
    }

    /// Uniform on `lo..hi`; an empty range traps.
    pub fn range(&mut self, lo: u64, hi: u64) -> u64 {
        if hi <= lo {
            crate::actor::trap();
        }
        lo + self.below(hi - lo)
    }

    /// Uniform on `[0, 1)` with 53 bits.
    pub fn f64(&mut self) -> f64 {
        (self.source.next() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// `true` with probability `p`.
    pub fn chance(&mut self, p: f64) -> bool {
        self.f64() < p
    }

    /// Exponentially distributed with the given rate.
    pub fn exponential(&mut self, rate: f64) -> f64 {
        -libm::log(1.0 - self.f64()) / rate
    }

    /// Normally distributed, by Box–Muller over two draws.
    pub fn normal(&mut self, mean: f64, sd: f64) -> f64 {
        let u1 = 1.0 - self.f64();
        let u2 = self.f64();
        let r = libm::sqrt(-2.0 * libm::log(u1));
        mean + sd * r * libm::cos(2.0 * core::f64::consts::PI * u2)
    }

    /// One element of a non-empty slice.
    pub fn choose<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECORDED: [u64; 6] = [
        7806831264735756412,
        5,
        164,
        4600568673609213140,
        4605322125149212083,
        13831055547662332364,
    ];
    const CHOSEN: &str = "a";

    struct Counter(u64);
    impl Source for Counter {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0
        }
    }

    #[test]
    fn the_algorithms_are_fixed() {
        let mut r = Random::over(Counter(1));
        // Recorded once from this implementation: a change to any algorithm changes them.
        let recorded = [
            r.u64(),
            r.below(10),
            r.range(100, 200),
            r.f64().to_bits(),
            r.exponential(2.0).to_bits(),
            r.normal(0.0, 1.0).to_bits(),
        ];
        assert_eq!(recorded, RECORDED);
        assert_eq!(*r.choose(&["a", "b", "c"]), CHOSEN);
        let f = f64::from_bits(recorded[3]);
        assert!((0.0..1.0).contains(&f));
        // `below` is unbiased in distribution: every residue appears, none twice as often.
        let mut counts = [0u32; 7];
        for _ in 0..70_000 {
            counts[r.below(7) as usize] += 1;
        }
        assert!(
            counts.iter().all(|c| (9_000..11_000).contains(c)),
            "{counts:?}"
        );
    }
}
