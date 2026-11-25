//! The arithmetic of the solver (RFC 013 rule 19). Every float function that IEEE 754 does not
//! round exactly comes from `libm`, one pure-Rust implementation compiled into both Hosts, never
//! from the platform's math library; `fmin` and `fmax` are by comparison, because `f64::min` and
//! `f64::max` leave the result for a pair of signed zeros to the target. The crate's
//! `clippy.toml` forbids the `std` methods these replace.

/// `e^x`.
pub fn exp(x: f64) -> f64 {
    libm::exp(x)
}

/// The natural logarithm.
pub fn ln(x: f64) -> f64 {
    libm::log(x)
}

/// The smaller of two numbers, by comparison: `b` when `b < a`, else `a`. Where a NaN is one of
/// them the result is `a`; the solver never passes one (a NaN is a convergence failure first).
pub fn fmin(a: f64, b: f64) -> f64 {
    if b < a { b } else { a }
}

/// The larger of two numbers, by comparison: `b` when `b > a`, else `a`.
pub fn fmax(a: f64, b: f64) -> f64 {
    if b > a { b } else { a }
}

/// A value that leaves the solver, made canonical (RFC 001 rule 59): every NaN is one NaN.
pub fn canonical(x: f64) -> f64 {
    florn_abi::canonical_f64(x)
}

#[cfg(test)]
mod tests;
