//! The initial solve (RFC 013 rule 9), in the start hook after the held values are set: the
//! dynamic states are taken in their direct-current form (assemble.rs), and the algebraic unknowns
//! are solved by Newton from zero. **Stages**: the plain solve; on failure `gmin` stepping, the
//! same solve with `gmin` at 1 mS and then at each decade below while it stays above both the
//! profile's `gmin` and 1e-15 S, the last level being the profile's `gmin` itself, each level
//! starting from the previous solution; on failure source stepping, every independent source
//! scaled by λ = 0.1, 0.2, … 1 in ten levels, each starting from the previous. A singular matrix, or
//! the failure of all three stages, refuses the run (`circuit.no-operating-point`).

use crate::assemble::memory;
use crate::newton::{self, Failure};
use crate::plant::{Plant, UnknownKind};

/// Which stage gave the operating point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Plain,
    GminStepping,
    SourceStepping,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::GminStepping => "gmin-stepping",
            Self::SourceStepping => "source-stepping",
        }
    }
}

/// The operating point at tick 0: the unknowns, which stage found them, and the iterations the
/// stages took in all.
#[derive(Clone, Debug, PartialEq)]
pub struct Solution {
    pub x: Vec<f64>,
    pub stage: Stage,
    pub iterations: u32,
}

/// The run has no operating point: the unknown named and what it is (rule 28).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoOperatingPoint {
    pub unknown: String,
    pub kind: UnknownKind,
    pub iterations: u32,
}

/// The decades of `gmin` stepping, from 1 mS down.
const DECADES: [f64; 13] = [
    1e-3, 1e-4, 1e-5, 1e-6, 1e-7, 1e-8, 1e-9, 1e-10, 1e-11, 1e-12, 1e-13, 1e-14, 1e-15,
];

/// The levels of `gmin` stepping for a profile's `gmin`: the decades above both it and 1e-15,
/// then the profile's `gmin` itself (zero included).
pub fn gmin_levels(gmin: f64) -> Vec<f64> {
    let mut levels: Vec<f64> = DECADES
        .iter()
        .copied()
        .filter(|&g| g > gmin && g > 1e-15)
        .collect();
    levels.push(gmin);
    levels
}

fn refused(plant: &Plant, f: &Failure, total: u32) -> NoOperatingPoint {
    let u = &plant.unknowns[f.unknown.min(plant.unknowns.len().saturating_sub(1))];
    NoOperatingPoint {
        unknown: u.name.clone(),
        kind: u.kind,
        iterations: total.max(f.iterations),
    }
}

/// Solve for the operating point at tick 0.
pub fn initial_solve(plant: &Plant) -> Result<Solution, NoOperatingPoint> {
    let n = plant.size();
    let gmin = plant.profile.gmin;
    if n == 0 {
        return Ok(Solution {
            x: Vec::new(),
            stage: Stage::Plain,
            iterations: 0,
        });
    }
    let mut spent = 0u32;
    let mut run = |x0: Vec<f64>, g: f64, lambda: f64, mem: &mut _| {
        newton::solve(plant, x0, g, lambda, mem).inspect(|c| spent += c.iterations)
    };
    // The plain solve.
    let mut mem = memory(plant);
    let mut last = match run(vec![0.0; n], gmin, 1.0, &mut mem) {
        Ok(c) => {
            return Ok(Solution {
                x: c.x,
                stage: Stage::Plain,
                iterations: spent,
            });
        }
        Err(f) if f.singular => return Err(refused(plant, &f, spent)),
        Err(f) => f,
    };
    // `gmin` stepping.
    let mut mem = memory(plant);
    let mut x = vec![0.0; n];
    let mut ok = true;
    for g in gmin_levels(gmin) {
        match run(x.clone(), g, 1.0, &mut mem) {
            Ok(c) => x = c.x,
            Err(f) if f.singular => return Err(refused(plant, &f, spent)),
            Err(f) => {
                last = f;
                ok = false;
                break;
            }
        }
    }
    if ok {
        return Ok(Solution {
            x,
            stage: Stage::GminStepping,
            iterations: spent,
        });
    }
    // Source stepping.
    let mut mem = memory(plant);
    let mut x = vec![0.0; n];
    for k in 1..=10u32 {
        match run(x.clone(), gmin, f64::from(k) / 10.0, &mut mem) {
            Ok(c) => x = c.x,
            Err(f) if f.singular => return Err(refused(plant, &f, spent)),
            Err(f) => return Err(refused(plant, &f, spent)),
        }
    }
    let _ = last;
    Ok(Solution {
        x,
        stage: Stage::SourceStepping,
        iterations: spent,
    })
}

#[cfg(test)]
mod tests;
