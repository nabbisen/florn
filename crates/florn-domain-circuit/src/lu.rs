//! Dense LU with partial pivoting (RFC 013 rule 22): the most deterministic thing available, no
//! reordering heuristics and no fill-in decisions. One pivot rule, which depends on values only
//! (rule 19): the row of the largest magnitude in the column, the lowest index on ties. The
//! arithmetic is IEEE `+ - × ÷` in a fixed order and nothing fused.

/// A square matrix in row-major order.
#[derive(Clone, Debug, PartialEq)]
pub struct Matrix {
    n: usize,
    a: Vec<f64>,
}

impl Matrix {
    pub fn new(n: usize) -> Self {
        Self {
            n,
            a: vec![0.0; n * n],
        }
    }
    pub fn size(&self) -> usize {
        self.n
    }
    pub fn get(&self, row: usize, col: usize) -> f64 {
        self.a[row * self.n + col]
    }
    pub fn add(&mut self, row: usize, col: usize, x: f64) {
        self.a[row * self.n + col] += x;
    }
    pub fn set(&mut self, row: usize, col: usize, x: f64) {
        self.a[row * self.n + col] = x;
    }

    /// Solve `A·x = b` in place: `b` becomes `x`, and the matrix is destroyed. `Err(k)` is the
    /// column of the first zero pivot (the unknown a notification names): a pivot that is zero,
    /// or not a number.
    pub fn solve(&mut self, b: &mut [f64]) -> Result<(), usize> {
        let n = self.n;
        assert_eq!(b.len(), n, "the right-hand side has the matrix's size");
        for k in 0..n {
            // The largest magnitude in the column, the lowest row on a tie.
            let mut pivot = k;
            let mut best = self.a[k * n + k].abs();
            for i in k + 1..n {
                let m = self.a[i * n + k].abs();
                if m > best {
                    best = m;
                    pivot = i;
                }
            }
            if best.is_nan() || best <= 0.0 || best.is_infinite() {
                return Err(k);
            }
            if pivot != k {
                for j in 0..n {
                    self.a.swap(k * n + j, pivot * n + j);
                }
                b.swap(k, pivot);
            }
            let d = self.a[k * n + k];
            for i in k + 1..n {
                let f = self.a[i * n + k] / d;
                if f != 0.0 {
                    for j in k + 1..n {
                        let t = f * self.a[k * n + j];
                        self.a[i * n + j] -= t;
                    }
                    let t = f * b[k];
                    b[i] -= t;
                }
                self.a[i * n + k] = 0.0;
            }
        }
        for i in (0..n).rev() {
            let mut s = b[i];
            for (j, x) in b.iter().enumerate().skip(i + 1) {
                let t = self.a[i * n + j] * x;
                s -= t;
            }
            b[i] = s / self.a[i * n + i];
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
