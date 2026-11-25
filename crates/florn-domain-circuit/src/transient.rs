//! The transient solver (RFC 013 rules 6, 7 and 10 to 14): the plant advanced from the tick it is
//! solved at to a later one by steps of whole ticks, each an attempt of Newton on the system of a
//! step with the coefficients of the method; accepted or rejected by the local error estimate; the
//! history cut at tick 0 and at every breakpoint, after which two backward Euler steps and a
//! third with the order-1 test restart the integration.
//!
//! This is the solver and nothing of the queue: [`Transient::advance`] takes the nearest limit (a
//! breakpoint or the advance's tick) and the caller, the advance of RFC 012 rule 8 (PR-46), sets
//! the inputs and cuts the history between limits. A failure leaves the state at the last accepted
//! step (rule 17).

use crate::assemble::{Companion, Form, Memory};
use crate::control::{grow, quarter, shape, shrink};
use crate::dc::Solution;
use crate::integrate::{self, Dynamic, Formula};
use crate::math::fmax;
use crate::newton::{self, Failure};
use crate::plant::{Element, Plant, UnknownKind};
use crate::profile::Control;

/// Why an attempt was rejected (`circuit.step-rejected`'s `cause`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    /// The error test failed: `r > 1`.
    Error,
    /// Newton did not converge.
    Convergence,
}

/// How the solver gave up (rule 17: non-convergence).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// A convergence failure at the smallest step.
    AtMinimumStep,
    /// A convergence failure under `fixed`.
    Fixed,
    /// More than `max-rejections` consecutive rejections of one step.
    TooManyRejections,
    /// The matrix was singular.
    Singular,
}

/// The solver gave up at the step that starts at `tick`: the state is that of the last accepted
/// step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stuck {
    pub tick: u64,
    pub why: Why,
    /// The Newton iterations of the failing attempt.
    pub iterations: u32,
    /// The unknown furthest from its criteria, or of the zero pivot; `None` when the attempts
    /// were rejected by the error test and Newton had no complaint.
    pub unknown: Option<usize>,
    /// After too many rejections by the error test, the device that owns the state variable with
    /// the largest error ratio (an index into the plant's devices); otherwise `None`.
    pub owner: Option<usize>,
}

/// A step the error test failed at the smallest size, accepted anyway (`circuit.step-minimum`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Minimum {
    /// The tick the step starts at, and its length.
    pub tick: u64,
    pub step: u64,
}

/// What one accepted step was: for the inspection summary and the tests.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepRecord {
    pub start: u64,
    pub ticks: u64,
    /// The limit the step was shaped by.
    pub limit: u64,
    pub formula: Formula,
    /// The error ratio when the step was tested.
    pub ratio: Option<f64>,
    /// The attempts it took, the last included.
    pub attempts: u32,
}

/// What an advance did (rule 17's observation: the figures of `circuit.step-rejected` and the
/// world inspection).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Advance {
    pub steps: u32,
    pub attempts: u32,
    pub rejected: u32,
    /// The cause of the first rejection.
    pub first_cause: Option<Cause>,
    pub iterations: u32,
    pub minimum: Vec<Minimum>,
}

/// The result of an advance: what it did, and how it ended if it did not reach the limit.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub report: Advance,
    pub stuck: Option<Stuck>,
}

/// The accepted points since the last cut: the dynamic variables' values at the last three, the
/// ticks between them, and the derivative at the last.
#[derive(Clone, Debug, Default)]
struct History {
    points: Vec<Vec<f64>>,
    gaps: Vec<u64>,
    derivative: Vec<f64>,
    since_cut: u32,
}

/// The plant as the transient solver keeps it.
#[derive(Clone, Debug)]
pub struct Transient {
    plant: Plant,
    dynamics: Vec<Dynamic>,
    /// The unknowns of rule 3 at `now`.
    x: Vec<f64>,
    /// `solved`: the tick the state is valid at (RFC 012 rule 7).
    now: u64,
    history: History,
    /// The controller's proposal for the next step, in ticks (rule 10).
    proposal: u64,
    trace: Option<Vec<StepRecord>>,
}

/// The attempt of one step.
struct Attempt {
    x: Vec<f64>,
    iterations: u32,
    values: Vec<f64>,
    derivative: Vec<f64>,
    ratio: Option<f64>,
    /// The dynamic state variable that gave the ratio.
    worst: usize,
}

enum Tried {
    Done(Attempt),
    Failed(Failure),
}

impl Transient {
    /// The solver at tick 0 with the operating point of the initial solve, the history cut there.
    pub fn new(plant: &Plant, solution: &Solution) -> Self {
        let mut t = Self {
            plant: plant.clone(),
            dynamics: integrate::dynamics(plant),
            x: solution.x[..plant.transient_unknowns].to_vec(),
            now: 0,
            history: History::default(),
            proposal: plant.profile.step,
            trace: None,
        };
        t.cut();
        t
    }

    /// Keep a record of every accepted step from now on (the tests', the inspector's).
    pub fn trace_steps(&mut self) {
        self.trace = Some(Vec::new());
    }

    /// The records kept, and forget them.
    pub fn take_trace(&mut self) -> Vec<StepRecord> {
        self.trace.as_mut().map(std::mem::take).unwrap_or_default()
    }

    /// `solved`, in ticks.
    pub fn now(&self) -> u64 {
        self.now
    }

    /// The unknowns of rule 3 at `now`.
    pub fn x(&self) -> &[f64] {
        &self.x
    }

    pub fn plant(&self) -> &Plant {
        &self.plant
    }

    /// The controller's proposal for the next step, in ticks.
    pub fn proposal(&self) -> u64 {
        self.proposal
    }

    /// The steps taken since the history was last cut.
    pub fn since_cut(&self) -> u32 {
        self.history.since_cut
    }

    /// The voltage of the net with this name at `now`; the ground is 0.
    pub fn voltage(&self, net: &str) -> Option<f64> {
        let k = self.plant.nets.iter().position(|n| n.name == net)?;
        Some(self.x[k])
    }

    /// Cut the history (rule 7): the next two steps are backward Euler of the profile's `step`
    /// and the third is backward Euler with the order-1 test. Called at tick 0 and at every
    /// applied breakpoint.
    pub fn cut(&mut self) {
        let values: Vec<f64> = self.dynamics.iter().map(|d| d.value(&self.x)).collect();
        self.history = History {
            points: vec![values],
            gaps: Vec::new(),
            derivative: vec![0.0; self.dynamics.len()],
            since_cut: 0,
        };
        self.proposal = self.plant.profile.step;
    }

    /// Set the value of a settable source or threshold (RFC 012 rule 15): a `voltage-source`'s
    /// `v`, a `current-source`'s `i`, a `constant`'s value (the held value of its signal, in the
    /// signal's unit) or a `comparator`'s `threshold`. The caller cuts the history.
    pub fn set(&mut self, actor: u32, value: f64) -> bool {
        let Some(d) = self.plant.devices.iter_mut().find(|d| d.actor == actor) else {
            return false;
        };
        match &mut d.element {
            Element::VoltageSource { v, .. } => *v = value,
            Element::CurrentSource { i, .. } => *i = value,
            Element::Comparator { threshold, .. } => *threshold = value,
            Element::Constant { out, value: held } => {
                *held = value;
                if let Some(s) = out {
                    self.plant.signals[*s].held = value;
                }
            }
            _ => return false,
        }
        true
    }

    /// Set the held value of a signal, in its own unit (a controller's output, RFC 012 rule 11).
    pub fn hold(&mut self, signal: usize, value: f64) {
        self.plant.signals[signal].held = value;
    }

    /// The formula of the next step and whether it is tested (rules 7 and 10): after a cut two
    /// backward Euler steps without a test, a third with the order-1 test, then the profile's
    /// method with the order-2 test; under `fixed` no test and the method from the third step.
    fn formula(&self) -> (Formula, bool) {
        let fixed = self.plant.profile.control == Control::Fixed;
        match self.history.since_cut {
            0 | 1 => (Formula::BackwardEuler, false),
            2 if !fixed => (Formula::BackwardEuler, true),
            _ => (Formula::of(self.plant.profile.method), !fixed),
        }
    }

    /// The start of the first attempt of the next step, before it is shaped to a limit.
    fn first_try(&self, formula: Formula) -> u64 {
        let p = &self.plant.profile;
        if p.control == Control::Fixed {
            return p.step;
        }
        let n = self.proposal;
        match (formula, self.history.gaps.last()) {
            // Gear 2 never lets the ratio of two steps exceed 2 (rule 12).
            (Formula::Gear2, Some(last)) => n.min(2 * last).max(p.min_step),
            _ => n,
        }
    }

    fn junctions(&self, x: &[f64]) -> Memory {
        self.plant
            .devices
            .iter()
            .map(|d| match &d.element {
                Element::Diode { a, k, .. } => a.map_or(0.0, |i| x[i]) - k.map_or(0.0, |i| x[i]),
                _ => 0.0,
            })
            .collect()
    }

    /// One attempt of a step of `n` ticks from the last accepted point.
    fn attempt(&self, n: u64, formula: Formula, tested: bool) -> Tried {
        let p = &self.plant.profile;
        let tick = self.plant.tick;
        let h = n as f64 * tick;
        let gaps = &self.history.gaps;
        let h_prev = gaps.last().map_or(h, |g| *g as f64 * tick);
        let points = &self.history.points;
        let last = &points[points.len() - 1];
        let before = if points.len() >= 2 {
            &points[points.len() - 2]
        } else {
            last
        };
        let k = self.dynamics.len();
        let history: Vec<f64> = (0..k)
            .map(|i| {
                integrate::history(
                    formula,
                    h,
                    h_prev,
                    last[i],
                    before[i],
                    self.history.derivative[i],
                )
            })
            .collect();
        let alpha = integrate::alpha(formula, h, h_prev);
        let companion = Companion { alpha, history };
        let mut memory = self.junctions(&self.x);
        let solved = newton::solve_form(
            &self.plant,
            Form::Step(&companion),
            self.x.clone(),
            p.gmin,
            &mut memory,
        );
        let c = match solved {
            Ok(c) => c,
            Err(f) => return Tried::Failed(f),
        };
        let values: Vec<f64> = self.dynamics.iter().map(|d| d.value(&c.x)).collect();
        let derivative: Vec<f64> = values
            .iter()
            .zip(&companion.history)
            .map(|(v, hist)| alpha * v - hist)
            .collect();
        let (ratio, worst) = if tested {
            let (r, w) = self.ratio(n, h, h_prev, formula, &values);
            (Some(r), w)
        } else {
            (None, 0)
        };
        Tried::Done(Attempt {
            x: c.x,
            iterations: c.iterations,
            values,
            derivative,
            ratio,
            worst,
        })
    }

    /// The error ratio of a candidate (rule 11): the largest over the dynamic state variables of
    /// the estimate over `ltol·max(|x|, floor) + tol`, with the variable that gave it. A plant with
    /// no dynamic element has none (the ratio is 0).
    fn ratio(&self, n: u64, h: f64, h_prev: f64, formula: Formula, values: &[f64]) -> (f64, usize) {
        let p = &self.plant.profile;
        let tick = self.plant.tick;
        let (points, gaps) = (&self.history.points, &self.history.gaps);
        let m = points.len();
        let (mut worst, mut owner) = (0.0, 0);
        for (i, dynamic) in self.dynamics.iter().enumerate() {
            let divided = if formula.order() == 1 {
                // The last two accepted points and the candidate.
                let (n1, n2) = (gaps[gaps.len() - 1], n);
                integrate::dd2(
                    points[m - 2][i],
                    points[m - 1][i],
                    values[i],
                    n1 as f64 * tick,
                    h,
                    (n1 + n2) as f64 * tick,
                )
            } else {
                // The last three accepted points and the candidate.
                let (n0, n1, n2) = (gaps[gaps.len() - 2], gaps[gaps.len() - 1], n);
                integrate::dd3(
                    [
                        points[m - 3][i],
                        points[m - 2][i],
                        points[m - 1][i],
                        values[i],
                    ],
                    [n0 as f64 * tick, n1 as f64 * tick, h],
                    (n0 + n1) as f64 * tick,
                    (n1 + n2) as f64 * tick,
                    (n0 + n1 + n2) as f64 * tick,
                )
            };
            let estimate = integrate::estimate(formula, h, h_prev, divided);
            let tol = match dynamic.kind {
                UnknownKind::Current => p.abstol,
                UnknownKind::Voltage | UnknownKind::State => p.vntol,
            };
            let scale = p.ltol * fmax(values[i].abs(), tol) + tol;
            let share = estimate / scale;
            if share > worst {
                (worst, owner) = (share, i);
            }
        }
        (worst, owner)
    }

    /// Advance the plant from `now` to the limit `until` (a breakpoint or the advance's tick) by as
    /// many steps as the controller takes (rules 10 to 13). A limit not after `now` is a no-op.
    pub fn advance(&mut self, until: u64) -> Outcome {
        let mut report = Advance::default();
        while self.now < until {
            if let Err(stuck) = self.step(until, &mut report) {
                return Outcome {
                    report,
                    stuck: Some(stuck),
                };
            }
        }
        Outcome {
            report,
            stuck: None,
        }
    }

    /// One accepted step: attempts until one is accepted, or the solver gives up.
    fn step(&mut self, until: u64, report: &mut Advance) -> Result<(), Stuck> {
        let p = self.plant.profile.clone();
        let (formula, tested) = self.formula();
        let d = until - self.now;
        let mut tried = self.first_try(formula);
        let mut rejections = 0u32;
        let mut attempts = 0u32;
        loop {
            // A fixed step is `min(step, d)` and is never lengthened (rule 13, amended); every
            // other step is shaped by rule 13.
            let n = if p.control == Control::Fixed {
                tried.min(d)
            } else {
                shape(tried, d, p.min_step)
            };
            let at_minimum = n <= p.min_step;
            attempts += 1;
            report.attempts += 1;
            match self.attempt(n, formula, tested) {
                Tried::Failed(f) => {
                    report.iterations += f.iterations;
                    let stuck = |why| Stuck {
                        tick: self.now,
                        why,
                        iterations: f.iterations,
                        unknown: Some(f.unknown),
                        owner: None,
                    };
                    if f.singular {
                        return Err(stuck(Why::Singular));
                    }
                    if p.control == Control::Fixed {
                        return Err(stuck(Why::Fixed));
                    }
                    if at_minimum {
                        return Err(stuck(Why::AtMinimumStep));
                    }
                    rejections += 1;
                    report.rejected += 1;
                    report.first_cause.get_or_insert(Cause::Convergence);
                    if rejections > p.max_rejections {
                        return Err(stuck(Why::TooManyRejections));
                    }
                    tried = quarter(n, p.min_step);
                }
                Tried::Done(a) => {
                    report.iterations += a.iterations;
                    let order = formula.order();
                    if let Some(r) = a.ratio
                        && (r.is_nan() || r > 1.0)
                    {
                        if at_minimum {
                            // Nothing smaller is allowed: accepted anyway, and notified.
                            report.minimum.push(Minimum {
                                tick: self.now,
                                step: n,
                            });
                        } else {
                            rejections += 1;
                            report.rejected += 1;
                            report.first_cause.get_or_insert(Cause::Error);
                            if rejections > p.max_rejections {
                                return Err(Stuck {
                                    tick: self.now,
                                    why: Why::TooManyRejections,
                                    iterations: a.iterations,
                                    unknown: None,
                                    owner: Some(self.dynamics[a.worst].device),
                                });
                            }
                            tried = shrink(n, r, order, p.min_step);
                            continue;
                        }
                    }
                    let shortened = n < tried;
                    self.accept(a, n, until, formula, attempts, shortened, report);
                    return Ok(());
                }
            }
        }
    }

    /// Take the accepted attempt: the state, the history and the proposal (rule 12).
    #[allow(clippy::too_many_arguments)]
    fn accept(
        &mut self,
        a: Attempt,
        n: u64,
        limit: u64,
        formula: Formula,
        attempts: u32,
        shortened: bool,
        report: &mut Advance,
    ) {
        let p = &self.plant.profile;
        let start = self.now;
        if let Some(trace) = &mut self.trace {
            trace.push(StepRecord {
                start,
                ticks: n,
                limit,
                formula,
                ratio: a.ratio,
                attempts,
            });
        }
        // An untested step leaves the proposal as it was (the two backward Euler steps of a start,
        // and `fixed`). A plant with no dynamic element has the ratio 0 and its step grows to
        // `max-step` (rule 11).
        if let Some(r) = a.ratio {
            let proposed = grow(n, r, formula.order(), p.min_step, p.max_step);
            // A step shortened to end at a limit does not shrink the proposal (rule 10).
            self.proposal = if shortened {
                proposed.max(self.proposal)
            } else {
                proposed
            };
        }
        let History { points, gaps, .. } = &mut self.history;
        points.push(a.values);
        gaps.push(n);
        if points.len() > 3 {
            points.remove(0);
        }
        if gaps.len() > 2 {
            gaps.remove(0);
        }
        self.history.derivative = a.derivative;
        self.history.since_cut += 1;
        self.x = a.x;
        self.now += n;
        report.steps += 1;
    }
}

#[cfg(test)]
mod tests;
