//! The table of `bit_identity` (RFC 013 rule 19, control 4): a fixed list of inputs, among them
//! signed zeros, infinities, subnormals and NaN payloads, run through the arithmetic the solver
//! uses, with the bits of every answer. The kit compares the table of one engine with the table the
//! kit recorded (`scenarios/results/bit-identity.table`), natively on every architecture and in
//! every browser; a bit that differs is a difference the tolerance of rule 20 would otherwise hide,
//! and the release notes print the table with the figures.
//!
//! What the table runs: the three `libm` functions the solver calls (through [`crate::math`]), the
//! operations it takes from `std` (`+ − × ÷`, `sqrt`, `abs`, `floor`, `ceil`, `trunc`, `copysign`),
//! its own `fmin` and `fmax`, the canonical NaN, and three pieces built on them: the diode's
//! junction (`pnjlim` and the characteristic) and the dense LU's solve. **Every answer is the value
//! as it leaves the solver, made canonical** (RFC 001 rule 59): the sign of a NaN produced by
//! arithmetic is the target's, and rule 59 removes it at the contract.
//!
//! The kit fails the table when the crate calls a `libm` function that [`LIBM`] does not list.

use crate::diode::Junction;
use crate::lu::Matrix;
use crate::math::{canonical, cbrt, exp, fmax, fmin, ln};

/// The first line of the table's file.
pub const FIRST_LINE: &str = "florn-bits 1";

/// The `libm` functions the crate may call, as `(the libm name, the name in the table)`. The kit
/// reads the crate's source and fails on a call of any other.
pub const LIBM: &[(&str, &str)] = &[("exp", "exp"), ("log", "ln"), ("cbrt", "cbrt")];

/// The functions the table runs through the crate's arithmetic; a test replaces them to show that
/// a doctored function fails.
#[derive(Clone, Copy)]
pub struct Functions {
    pub exp: fn(f64) -> f64,
    pub ln: fn(f64) -> f64,
    pub cbrt: fn(f64) -> f64,
    pub fmin: fn(f64, f64) -> f64,
    pub fmax: fn(f64, f64) -> f64,
}

impl Functions {
    /// The crate's own.
    pub const CRATE: Self = Self {
        exp,
        ln,
        cbrt,
        fmin,
        fmax,
    };
}

/// One line of the table: a function, its inputs, the bits of the answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub function: String,
    pub inputs: Vec<u64>,
    pub output: u64,
}

impl Entry {
    fn line(&self) -> String {
        let mut line = self.function.clone();
        for i in &self.inputs {
            line.push_str(&format!(" {i:016x}"));
        }
        line.push_str(&format!(" {:016x}", self.output));
        line
    }
}

/// The table under the crate's arithmetic.
pub fn table() -> Vec<Entry> {
    table_using(&Functions::CRATE)
}

/// SplitMix64: a fixed sequence of 64-bit words from integer arithmetic alone.
fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Values of every kind the engines might treat differently, as bits.
fn specials() -> Vec<u64> {
    let mut v: Vec<u64> = [
        0.0f64,
        -0.0,
        1.0,
        -1.0,
        0.5,
        -0.5,
        2.0,
        3.0,
        8.0,
        -8.0,
        10.0,
        core::f64::consts::E,
        core::f64::consts::PI,
        1e-10,
        -1e-10,
        1e10,
        1e300,
        -1e300,
        80.0,
        709.0,
        709.782_712_893_384,
        710.0,
        -708.0,
        -745.0,
        -746.0,
        f64::MAX,
        f64::MIN,
        f64::MIN_POSITIVE,
        -f64::MIN_POSITIVE,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ]
    .iter()
    .map(|x| x.to_bits())
    .collect();
    v.extend([
        // The smallest and the largest subnormal.
        0x0000_0000_0000_0001,
        0x8000_0000_0000_0001,
        0x000f_ffff_ffff_ffff,
        0x800f_ffff_ffff_ffff,
        // NaNs: the default, its negative, a payload, a signalling one, every bit set.
        0x7ff8_0000_0000_0000,
        0xfff8_0000_0000_0000,
        0x7ff8_0000_0000_0001,
        0x7ff0_0000_0000_0001,
        0xfff0_0000_0000_0001,
        0x7fff_ffff_ffff_ffff,
        0xffff_ffff_ffff_ffff,
    ]);
    v
}

/// A fixed sweep of ordinary values between about 2⁻³⁰ and 2³⁰ in magnitude, either sign.
fn sweep(count: usize, seed: u64) -> Vec<u64> {
    let mut state = seed;
    (0..count)
        .map(|_| {
            let a = splitmix(&mut state);
            let exponent = 1023 - 30 + (a >> 52) % 61;
            let sign = (splitmix(&mut state) & 1) << 63;
            sign | (exponent << 52) | (a & 0x000f_ffff_ffff_ffff)
        })
        .collect()
}

/// The pairs of the binary operations: every one of the first values with every other.
fn pairs(values: &[u64]) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    for a in values {
        for b in values {
            out.push((*a, *b));
        }
    }
    out
}

/// The table, with `f` as the functions the solver calls.
pub fn table_using(f: &Functions) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut push = |name: &str, inputs: &[u64], answer: f64| {
        out.push(Entry {
            function: name.to_string(),
            inputs: inputs.to_vec(),
            output: canonical(answer).to_bits(),
        });
    };
    let specials = specials();
    let mut values = specials.clone();
    values.extend(sweep(300, 1));
    for bits in &values {
        let x = f64::from_bits(*bits);
        push("exp", &[*bits], (f.exp)(x));
        push("ln", &[*bits], (f.ln)(x));
        push("cbrt", &[*bits], (f.cbrt)(x));
        push("sqrt", &[*bits], x.sqrt());
        push("abs", &[*bits], x.abs());
        push("floor", &[*bits], x.floor());
        push("ceil", &[*bits], x.ceil());
        push("trunc", &[*bits], x.trunc());
        push("neg", &[*bits], -x);
    }
    // The pairs: the nasty values against each other, and the sweep against itself in part.
    let mut nasty: Vec<u64> = specials.iter().copied().step_by(2).collect();
    nasty.extend(&specials[1..4]);
    nasty.extend([
        0x8000_0000_0000_0000,
        0x7ff8_0000_0000_0001,
        0xfff8_0000_0000_0000,
    ]);
    for (a, b) in pairs(&nasty) {
        let (x, y) = (f64::from_bits(a), f64::from_bits(b));
        push("add", &[a, b], x + y);
        push("sub", &[a, b], x - y);
        push("mul", &[a, b], x * y);
        push("div", &[a, b], x / y);
        push("fmin", &[a, b], (f.fmin)(x, y));
        push("fmax", &[a, b], (f.fmax)(x, y));
        push("copysign", &[a, b], x.copysign(y));
    }
    let ordinary = sweep(40, 7);
    for pair in ordinary.chunks(2) {
        let (a, b) = (pair[0], pair[1]);
        let (x, y) = (f64::from_bits(a), f64::from_bits(b));
        push("add", &[a, b], x + y);
        push("sub", &[a, b], x - y);
        push("mul", &[a, b], x * y);
        push("div", &[a, b], x / y);
    }
    // The diode's junction at the default constants.
    let junction = Junction::new(1e-14, 1.0, 1e-12);
    let volts = [
        -5.0f64, -1.0, -0.3, 0.0, 0.3, 0.6, 0.7, 0.8, 1.0, 2.0, 2.1, 5.0,
    ];
    for v in volts {
        let (i, g) = junction.characteristic(v);
        push("diode.i", &[v.to_bits()], i);
        push("diode.g", &[v.to_bits()], g);
    }
    for v_new in volts {
        for v_old in volts {
            let (limited, flag) = junction.limit(v_new, v_old);
            let inputs = [v_new.to_bits(), v_old.to_bits()];
            push("pnjlim", &inputs, limited);
            push("pnjlim.flag", &inputs, if flag { 1.0 } else { 0.0 });
        }
    }
    // The dense LU: a matrix with a tie for the pivot, one that needs every row swapped, one
    // from the sweep, and a singular one (the answer is the column of the first zero pivot).
    for (name, n, entries, rhs) in lu_cases() {
        let mut a = Matrix::new(n);
        for (k, e) in entries.iter().enumerate() {
            a.set(k / n, k % n, *e);
        }
        let mut b = rhs;
        match a.solve(&mut b) {
            Ok(()) => {
                for (k, x) in b.iter().enumerate() {
                    push(&format!("lu.{name}.x{k}"), &[], *x);
                }
            }
            Err(column) => push(&format!("lu.{name}.err"), &[], column as f64),
        }
    }
    out
}

/// The matrices of the LU's rows: a name, the size, the entries row by row, the right-hand side.
fn lu_cases() -> Vec<(&'static str, usize, Vec<f64>, Vec<f64>)> {
    let mut state = 11;
    let mut ordinary = |count: usize| -> Vec<f64> {
        (0..count)
            .map(|_| f64::from_bits(sweep(1, splitmix(&mut state))[0]))
            .collect()
    };
    vec![
        (
            "tie",
            3,
            vec![2.0, -2.0, 1.0, 2.0, 2.0, -1.0, -2.0, 1.0, 3.0],
            vec![1.0, 2.0, 3.0],
        ),
        (
            "swap",
            4,
            vec![
                0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0, 0.0, 0.0, 3.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0,
            ],
            vec![1.0, 2.0, 3.0, 4.0],
        ),
        ("sweep", 6, ordinary(36), ordinary(6)),
        (
            "singular",
            3,
            vec![1.0, 2.0, 3.0, 2.0, 4.0, 6.0, 1.0, 1.0, 1.0],
            vec![1.0, 2.0, 3.0],
        ),
    ]
}

/// The table as text: the first line, then one line per entry (`function inputs… answer`, each as
/// 16 hex digits).
pub fn render(table: &[Entry]) -> String {
    let mut text = String::from(FIRST_LINE);
    text.push('\n');
    for e in table {
        text.push_str(&e.line());
        text.push('\n');
    }
    text
}

/// A table against the file the kit recorded: the number of lines that agree, or what differs
/// (at most the first eight lines that do, and the counts).
pub fn compare(file: &str, now: &[Entry]) -> Result<usize, Vec<String>> {
    let mut lines = file.lines();
    if lines.next() != Some(FIRST_LINE) {
        return Err(vec![format!("the file does not begin with `{FIRST_LINE}`")]);
    }
    let recorded: Vec<&str> = lines.collect();
    let mut problems = Vec::new();
    for (n, entry) in now.iter().enumerate() {
        let line = entry.line();
        match recorded.get(n) {
            Some(r) if *r == line => {}
            Some(r) => problems.push(format!(
                "line {}: the file has `{r}`, this engine `{line}`",
                n + 2
            )),
            None => problems.push(format!(
                "line {}: the file has none, this engine `{line}`",
                n + 2
            )),
        }
    }
    if recorded.len() > now.len() {
        problems.push(format!(
            "the file has {} lines, this engine {}",
            recorded.len(),
            now.len()
        ));
    }
    if problems.is_empty() {
        Ok(now.len())
    } else {
        let total = problems.len();
        problems.truncate(8);
        if total > 8 {
            problems.push(format!("{total} differences in all"));
        }
        Err(problems)
    }
}

#[cfg(test)]
mod tests;
