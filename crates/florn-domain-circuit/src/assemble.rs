//! The equations of the plant at a trial point (RFC 013 rules 3 to 5 and 9): Kirchhoff's current
//! law at every non-ground net, the branch equation of every current unknown, the state equation of
//! every extra state, and a conductance `gmin` from every net to ground. The devices are visited
//! in ascending actor identity and add their stamps in a fixed order, which fixes the float sums
//! (rule 5). This is the system of the initial solve: a capacitor is open (or a voltage source
//! with an `ic`), an inductor short (or a current source with an `ic`), the motor in its steady
//! state (rule 9).

use crate::device::Stamp;
use crate::diode::Junction;
use crate::lu::Matrix;
use crate::plant::{Element, GuestPins, Node, Plant};

/// The system at a trial point: the Jacobian, the residual, and for every net the sum of the
/// magnitudes of the branch currents at it (the scale of the residual test, rule 8).
pub struct System {
    pub j: Matrix,
    pub f: Vec<f64>,
    pub scale: Vec<f64>,
    /// Whether a device limited a junction in this evaluation.
    pub limited: bool,
}

/// The coefficients of one step (RFC 013 rule 6): `x' = α·x − history[k]` for the `k`-th dynamic
/// state variable, in the order of [`crate::integrate::dynamics`] (the devices in ascending
/// identity: a capacitor's voltage, an inductor's current, a motor's current and then its speed).
#[derive(Clone, Debug, PartialEq)]
pub struct Companion {
    pub alpha: f64,
    pub history: Vec<f64>,
}

/// Which system is assembled.
#[derive(Clone, Copy, Debug)]
pub enum Form<'a> {
    /// The initial solve, with the independent sources scaled by `lambda` (source stepping).
    Dc { lambda: f64 },
    /// A step of the transient, over the unknowns of rule 3 only.
    Step(&'a Companion),
}

/// The junction voltage each diode used at the previous iteration of the solve, by device index
/// (rule 8); zero at the start of a chain of solves.
pub type Memory = Vec<f64>;

pub fn memory(plant: &Plant) -> Memory {
    vec![0.0; plant.devices.len()]
}

fn volt(x: &[f64], n: Node) -> f64 {
    n.map_or(0.0, |k| x[k])
}

impl System {
    fn new(n: usize) -> Self {
        Self {
            j: Matrix::new(n),
            f: vec![0.0; n],
            scale: vec![0.0; n],
            limited: false,
        }
    }
    /// A current `i` leaving net `n` through a branch.
    fn leaves(&mut self, n: Node, i: f64) {
        if let Some(k) = n {
            self.f[k] += i;
            self.scale[k] += i.abs();
        }
    }
    fn jac(&mut self, row: Node, col: Node, g: f64) {
        if let (Some(r), Some(c)) = (row, col) {
            self.j.add(r, c, g);
        }
    }
    /// A two-terminal element whose current from `a` to `b` is `i`, with the slope `g` in the
    /// voltage across it.
    fn between(&mut self, a: Node, b: Node, i: f64, g: f64) {
        self.leaves(a, i);
        self.leaves(b, -i);
        self.jac(a, a, g);
        self.jac(a, b, -g);
        self.jac(b, a, -g);
        self.jac(b, b, g);
    }
    /// The branch unknown `m` is the current from `p` to `n` through an element.
    fn branch(&mut self, p: Node, n: Node, m: usize, current: f64) {
        self.leaves(p, current);
        self.leaves(n, -current);
        if let Some(k) = p {
            self.j.add(k, m, 1.0);
        }
        if let Some(k) = n {
            self.j.add(k, m, -1.0);
        }
    }
    /// The row of a voltage constraint `v_p − v_n − rhs = 0` for the unknown `m`.
    fn constrain(&mut self, row: usize, p: Node, n: Node, x: &[f64], rhs: f64) {
        self.f[row] += volt(x, p) - volt(x, n) - rhs;
        if let Some(k) = p {
            self.j.add(row, k, 1.0);
        }
        if let Some(k) = n {
            self.j.add(row, k, -1.0);
        }
    }
}

/// The value a signal holds, in SI, for a consumer that reads a dimensioned signal.
fn held_si(plant: &Plant, s: Option<usize>) -> f64 {
    s.map_or(0.0, |i| plant.signals[i].held * plant.signals[i].factor)
}

/// The raw held value for a dimensionless consumer (`cmd`, `ctl`).
fn held_raw(plant: &Plant, s: Option<usize>) -> f64 {
    s.map_or(0.0, |i| plant.signals[i].held)
}

/// A guest device's stamps added into the system at its place in the order of the evaluation (RFC
/// 013 rule 5): the current into each pin leaves its net, and the Jacobian is added row by row.
fn stamp_guest(s: &mut System, g: &GuestPins, st: &Stamp) {
    let n = g.nodes.len();
    for (k, node) in g.nodes.iter().enumerate() {
        s.leaves(*node, st.currents[k]);
    }
    for (k, row) in g.nodes.iter().enumerate() {
        for (j, col) in g.nodes.iter().enumerate() {
            s.jac(*row, *col, st.jacobian[k * n + j]);
        }
    }
    s.limited |= st.limited;
}

/// Assemble a system of either form at `x`, with the stamps the guest device answered at this trial
/// point, when there is one (RFC 013 rules 5 and 8).
pub fn assemble_with(
    plant: &Plant,
    x: &[f64],
    gmin: f64,
    form: Form<'_>,
    memory: &mut Memory,
    guest: Option<&Stamp>,
) -> System {
    let (lambda, step) = match form {
        Form::Dc { lambda } => (lambda, None),
        Form::Step(c) => (1.0, Some(c)),
    };
    let mut s = System::new(match step {
        Some(_) => plant.transient_unknowns,
        None => plant.size(),
    });
    // The index of the next dynamic state variable of a step.
    let mut slot = 0usize;
    for (k, _) in plant.nets.iter().enumerate() {
        s.f[k] += gmin * x[k];
        s.j.add(k, k, gmin);
    }
    for (d, dev) in plant.devices.iter().enumerate() {
        if let (Some(g), Some(st)) = (&plant.guest, guest)
            && g.before == d
        {
            stamp_guest(&mut s, g, st);
        }
        match &dev.element {
            Element::Resistor { a, b, g } => {
                let i = g * (volt(x, *a) - volt(x, *b));
                s.between(*a, *b, i, *g);
            }
            Element::Capacitor { a, b, c, ic, dc } => match step {
                Some(co) => {
                    // i = c·x', x the voltage across it.
                    let v = volt(x, *a) - volt(x, *b);
                    let i = c * (co.alpha * v - co.history[slot]);
                    slot += 1;
                    s.between(*a, *b, i, c * co.alpha);
                }
                None => {
                    if let (Some(ic), Some(m)) = (ic, dc) {
                        s.branch(*a, *b, *m, x[*m]);
                        s.constrain(*m, *a, *b, x, *ic);
                    }
                }
            },
            Element::Inductor {
                a,
                b,
                l,
                ic,
                branch,
            } => {
                s.branch(*a, *b, *branch, x[*branch]);
                match (step, ic) {
                    (Some(co), _) => {
                        // v = l·x', x the current through it.
                        let rhs = l * (co.alpha * x[*branch] - co.history[slot]);
                        slot += 1;
                        s.constrain(*branch, *a, *b, x, rhs);
                        s.j.add(*branch, *branch, -(l * co.alpha));
                    }
                    (None, Some(ic)) => {
                        s.f[*branch] += x[*branch] - ic;
                        s.j.add(*branch, *branch, 1.0);
                    }
                    (None, None) => s.constrain(*branch, *a, *b, x, 0.0),
                }
            }
            Element::VoltageSource {
                p,
                n,
                v,
                gain,
                cmd,
                branch,
            } => {
                let e = lambda * (v + gain * held_raw(plant, *cmd));
                s.branch(*p, *n, *branch, x[*branch]);
                s.constrain(*branch, *p, *n, x, e);
            }
            Element::CurrentSource { p, n, i, gain, cmd } => {
                let i = lambda * (i + gain * held_raw(plant, *cmd));
                s.leaves(*p, i);
                s.leaves(*n, -i);
            }
            Element::Diode { a, k, is, n, gmin } => {
                let junction = Junction::new(*is, *n, *gmin);
                let v = volt(x, *a) - volt(x, *k);
                let e = junction.evaluate(v, memory[d]);
                memory[d] = e.v_used;
                s.limited |= e.limited;
                s.between(*a, *k, e.i, e.g);
            }
            Element::Switch {
                a,
                b,
                ron,
                roff,
                threshold,
                ctl,
            } => {
                let r = if held_raw(plant, *ctl) > *threshold {
                    *ron
                } else {
                    *roff
                };
                let g = 1.0 / r;
                let i = g * (volt(x, *a) - volt(x, *b));
                s.between(*a, *b, i, g);
            }
            Element::Voltmeter { .. } | Element::Constant { .. } | Element::Comparator { .. } => {}
            Element::Ammeter { a, b, branch, .. } => {
                s.branch(*a, *b, *branch, x[*branch]);
                s.constrain(*branch, *a, *b, x, 0.0);
            }
            Element::Motor {
                p,
                n,
                r,
                l,
                kt,
                ke,
                j,
                b,
                load,
                branch,
                state,
                ..
            } => {
                let (i, w) = (x[*branch], x[*state]);
                s.branch(*p, *n, *branch, i);
                match step {
                    None => {
                        // Armature at steady state: v = r·i + ke·ω (di/dt = 0).
                        s.constrain(*branch, *p, *n, x, r * i + ke * w);
                        s.j.add(*branch, *branch, -r);
                        s.j.add(*branch, *state, -ke);
                        // Shaft at steady state: 0 = kt·i − b·ω − load (dω/dt = 0).
                        s.f[*state] += kt * i - b * w - held_si(plant, *load);
                        s.j.add(*state, *branch, *kt);
                        s.j.add(*state, *state, -b);
                    }
                    Some(co) => {
                        // Armature v = r·i + l·i' + ke·ω; shaft j·ω' = kt·i − b·ω − load.
                        let di = co.alpha * i - co.history[slot];
                        let dw = co.alpha * w - co.history[slot + 1];
                        slot += 2;
                        s.constrain(*branch, *p, *n, x, r * i + l * di + ke * w);
                        s.j.add(*branch, *branch, -(r + l * co.alpha));
                        s.j.add(*branch, *state, -ke);
                        s.f[*state] += kt * i - b * w - held_si(plant, *load) - j * dw;
                        s.j.add(*state, *branch, *kt);
                        s.j.add(*state, *state, -(b + j * co.alpha));
                    }
                }
            }
        }
    }
    // A guest device after every built-in of lower identity than its own is after all of them.
    if let (Some(g), Some(st)) = (&plant.guest, guest)
        && g.before >= plant.devices.len()
    {
        stamp_guest(&mut s, g, st);
    }
    s
}

#[cfg(test)]
mod tests;
