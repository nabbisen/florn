//! Newton–Raphson (RFC 013 rule 8): from a starting point, each iteration assembles the system at
//! the trial values, solves `J·dx = −f` by the dense LU of rule 22, and forms `x + dx`. The solver
//! changes no trial value: a junction is limited inside the device that has it. Converged when
//! every unknown's update is within `reltol·max(|x_i|, |x_i + dx_i|) + tol_i`, every net's
//! residual within `abstol + reltol·s_i`, and no device limited in the last iteration. More than
//! `newton.max-iterations` iterations is a convergence failure (rule 16).

use crate::assemble::{Memory, assemble};
use crate::math::fmax;
use crate::plant::{Plant, UnknownKind};

/// A converged solve.
#[derive(Clone, Debug, PartialEq)]
pub struct Converged {
    pub x: Vec<f64>,
    pub iterations: u32,
}

/// Why a solve failed.
#[derive(Clone, Debug, PartialEq)]
pub struct Failure {
    /// The unknown named: the column of the first zero pivot, or the one furthest from the
    /// criteria (the largest share of its tolerance) at the last iteration.
    pub unknown: usize,
    pub iterations: u32,
    /// The matrix was singular (a zero pivot), as against the iteration not converging.
    pub singular: bool,
}

/// The absolute tolerance of an unknown (rule 8): `vntol` for a voltage or a speed, `abstol` for a
/// current.
fn tolerance(plant: &Plant, i: usize) -> f64 {
    match plant.unknowns[i].kind {
        UnknownKind::Current => plant.profile.abstol,
        UnknownKind::Voltage | UnknownKind::State => plant.profile.vntol,
    }
}

/// Run Newton from `x0` with the conductance `gmin` and the sources at `lambda`.
pub fn solve(
    plant: &Plant,
    x0: Vec<f64>,
    gmin: f64,
    lambda: f64,
    memory: &mut Memory,
) -> Result<Converged, Failure> {
    let p = &plant.profile;
    let n = plant.size();
    let nets = plant.nets.len();
    let mut x = x0;
    let mut worst = 0;
    for iteration in 1..=p.max_iterations {
        let mut s = assemble(plant, &x, gmin, lambda, memory);
        let mut dx: Vec<f64> = s.f.iter().map(|f| -f).collect();
        let finite = s.f.iter().all(|f| f.is_finite());
        if finite && let Err(column) = s.j.solve(&mut dx) {
            return Err(Failure {
                unknown: column,
                iterations: iteration,
                singular: true,
            });
        }
        if !finite || dx.iter().any(|d| !d.is_finite()) {
            let bad = s.f.iter().position(|f| !f.is_finite()).unwrap_or(0);
            return Err(Failure {
                unknown: bad,
                iterations: iteration,
                singular: false,
            });
        }
        let mut converged = !s.limited;
        let mut largest = 0.0;
        worst = 0;
        for i in 0..n {
            let new = x[i] + dx[i];
            let allowed = p.reltol * fmax(x[i].abs(), new.abs()) + tolerance(plant, i);
            let mut share = dx[i].abs() / allowed;
            if i < nets {
                let bound = p.abstol + p.reltol * s.scale[i];
                share = fmax(share, s.f[i].abs() / bound);
            }
            if share > 1.0 {
                converged = false;
            }
            if share > largest {
                largest = share;
                worst = i;
            }
        }
        for i in 0..n {
            x[i] += dx[i];
        }
        if converged {
            return Ok(Converged {
                x,
                iterations: iteration,
            });
        }
    }
    Err(Failure {
        unknown: worst,
        iterations: p.max_iterations,
        singular: false,
    })
}

#[cfg(test)]
mod tests;
