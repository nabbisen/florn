//! The initial solve (RFC 013 rule 9), in the start hook after the held values are set: the
//! dynamic states are taken in their direct-current form (assemble.rs), and the algebraic unknowns
//! are solved by Newton from zero. **Stages**: the plain solve; on failure `gmin` stepping, the
//! same solve with `gmin` at 1 mS and then at each decade below while it stays above both the
//! profile's `gmin` and 1e-15 S, the last level being the profile's `gmin` itself, each level
//! starting from the previous solution; on failure source stepping, every independent source
//! scaled by λ = 0.1, 0.2, … 1 in ten levels, each starting from the previous. A singular matrix, or
//! the failure of all three stages, refuses the run (`circuit.no-operating-point`).

use crate::assemble::{Form, Memory, memory};
use crate::device::{Device, Lost, NoDevice};
use crate::newton::{self, Converged, Failure};
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

/// The operating point, and what became of the guest device in the solve: when it was lost the
/// solve was made again without it (rule 16), and `lost` says how.
#[derive(Clone, Debug, PartialEq)]
pub struct Solved {
    pub solution: Solution,
    pub lost: Option<Lost>,
}

/// Why the initial solve gave no point: the plant has none, or the run ended in a call of the
/// device (a fault under `stop-run`).
#[derive(Clone, Debug, PartialEq)]
pub enum Stop {
    NoPoint(NoOperatingPoint),
    Ended,
}

/// What the stages came to: a result, or the device lost.
enum Staged {
    Done(Result<Solution, NoOperatingPoint>),
    Lost(Lost),
}

/// One solve of the stages, one compound activation of the guest device (RFC 013 rule 9): opened in
/// DC form, `begin-step(0, 0.0)`, closed by `accept-step` when this solve gives the result and by
/// `reject-step` otherwise. A loss of the device ends the solve with it.
fn solve_once(
    plant: &Plant,
    device: &mut dyn Device,
    x0: Vec<f64>,
    gmin: f64,
    lambda: f64,
    mem: &mut Memory,
    result: bool,
) -> Result<Converged, Failure> {
    let live = device.present() && plant.guest.is_some();
    if live && let Err(lost) = device.begin(0, 0.0) {
        return Err(Failure {
            unknown: 0,
            iterations: 0,
            singular: false,
            lost: Some(lost),
        });
    }
    let solved = newton::solve_form_with(plant, Form::Dc { lambda }, x0, gmin, mem, device);
    if live && solved.as_ref().is_err_and(|f| f.lost.is_some()) {
        return solved;
    }
    if live {
        let closed = match &solved {
            Ok(c) if result => device.accept(&plant.guest_voltages(&c.x)),
            _ => device.reject(),
        };
        if let Err(lost) = closed {
            return Err(Failure {
                unknown: 0,
                iterations: solved
                    .as_ref()
                    .map_or_else(|f| f.iterations, |c| c.iterations),
                singular: false,
                lost: Some(lost),
            });
        }
    }
    solved
}

fn stages(plant: &Plant, device: &mut dyn Device) -> Staged {
    let n = plant.size();
    let gmin = plant.profile.gmin;
    if n == 0 {
        return Staged::Done(Ok(Solution {
            x: Vec::new(),
            stage: Stage::Plain,
            iterations: 0,
        }));
    }
    let mut spent = 0u32;
    // Run one solve; a loss of the device is the answer of the whole.
    macro_rules! run {
        ($x0:expr, $g:expr, $lambda:expr, $mem:expr, $result:expr) => {
            match solve_once(plant, device, $x0, $g, $lambda, $mem, $result) {
                Err(Failure { lost: Some(l), .. }) => return Staged::Lost(l),
                other => other.inspect(|c| spent += c.iterations),
            }
        };
    }
    // The plain solve.
    let mut mem = memory(plant);
    let mut last = match run!(vec![0.0; n], gmin, 1.0, &mut mem, true) {
        Ok(c) => {
            return Staged::Done(Ok(Solution {
                x: c.x,
                stage: Stage::Plain,
                iterations: spent,
            }));
        }
        Err(f) if f.singular => return Staged::Done(Err(refused(plant, &f, spent))),
        Err(f) => f,
    };
    // `gmin` stepping: every level but the last is a solve that does not give the result.
    let mut mem = memory(plant);
    let mut x = vec![0.0; n];
    let mut ok = true;
    let levels = gmin_levels(gmin);
    for (k, g) in levels.iter().enumerate() {
        match run!(x.clone(), *g, 1.0, &mut mem, k + 1 == levels.len()) {
            Ok(c) => x = c.x,
            Err(f) if f.singular => return Staged::Done(Err(refused(plant, &f, spent))),
            Err(f) => {
                last = f;
                ok = false;
                break;
            }
        }
    }
    if ok {
        return Staged::Done(Ok(Solution {
            x,
            stage: Stage::GminStepping,
            iterations: spent,
        }));
    }
    // Source stepping: the last level gives the result.
    let mut mem = memory(plant);
    let mut x = vec![0.0; n];
    for k in 1..=10u32 {
        match run!(x.clone(), gmin, f64::from(k) / 10.0, &mut mem, k == 10) {
            Ok(c) => x = c.x,
            Err(f) => return Staged::Done(Err(refused(plant, &f, spent))),
        }
    }
    let _ = last;
    Staged::Done(Ok(Solution {
        x,
        stage: Stage::SourceStepping,
        iterations: spent,
    }))
}

/// Solve for the operating point at tick 0 with no guest device.
pub fn initial_solve(plant: &Plant) -> Result<Solution, NoOperatingPoint> {
    match initial_solve_with(plant, &mut NoDevice) {
        Ok(s) => Ok(s.solution),
        Err(Stop::NoPoint(e)) => Err(e),
        Err(Stop::Ended) => unreachable!("no device to end the run"),
    }
}

/// Solve for the operating point at tick 0 (rule 9), calling the guest device in DC form: one
/// compound activation for each solve of the three stages, the one that gives the result closed by
/// `accept-step` and every other by `reject-step`. A device lost in the analysis is absent from
/// then on, and the analysis is made again without it.
pub fn initial_solve_with(plant: &Plant, device: &mut dyn Device) -> Result<Solved, Stop> {
    match stages(plant, device) {
        Staged::Done(r) => r
            .map(|solution| Solved {
                solution,
                lost: None,
            })
            .map_err(Stop::NoPoint),
        Staged::Lost(Lost::Ended) => Err(Stop::Ended),
        Staged::Lost(lost) => {
            let mut without = plant.clone();
            without.guest = None;
            match stages(&without, &mut NoDevice) {
                Staged::Done(r) => r
                    .map(|solution| Solved {
                        solution,
                        lost: Some(lost),
                    })
                    .map_err(Stop::NoPoint),
                Staged::Lost(_) => unreachable!("no device to lose"),
            }
        }
    }
}

#[cfg(test)]
mod tests;
