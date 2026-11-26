//! The plant's inputs and their breakpoints (RFC 012 rules 9 and 11, 15). An input changes when a
//! held signal that drives a built-in changes, a settable value is set, or a signal is frozen or
//! released. At the instant the domain handles the change it records a **breakpoint** `(tick,
//! input, value)` in the order of handling, without advancing the plant, which is why it can
//! happen inside a commit. A change that sets an input to the value it already holds (equal bits)
//! records nothing and cuts nothing, and a breakpoint replaces a pending one for the same input at
//! the same tick. The plant sees a breakpoint from the step that **starts** at its tick.

use florn_kernel::Identity;

use crate::transient::Transient;

/// What changes: a built-in's settable value (a source's `v` or `i`, a comparator's `threshold`)
/// or a signal's held value (the plant reads it through the built-ins it drives). A signal frozen
/// or released changes no value in force and so records no breakpoint (RFC 012 rule 9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Source(Identity),
    Signal(usize),
}

/// A pending breakpoint: the tick it acts from, the input, its value, and the actor whose output
/// or target it is (for the report of a bound exceeded).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Breakpoint {
    pub tick: u64,
    pub input: Input,
    pub value: f64,
    pub origin: Identity,
}

/// The pending breakpoints, in the order recorded.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pending {
    items: Vec<Breakpoint>,
}

impl Pending {
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn items(&self) -> &[Breakpoint] {
        &self.items
    }

    pub fn from_items(items: Vec<Breakpoint>) -> Self {
        Self { items }
    }

    /// The value the input will have when every pending breakpoint has acted: the last pending
    /// one for it, if any.
    pub fn last_value(&self, input: Input) -> Option<f64> {
        self.items
            .iter()
            .rev()
            .find(|b| b.input == input)
            .map(|b| b.value)
    }

    /// Record a breakpoint: it replaces a pending one for the same input at the same tick. The
    /// caller has checked that the value differs from the one the input holds.
    pub fn record(&mut self, b: Breakpoint) {
        self.items
            .retain(|p| !(p.input == b.input && p.tick == b.tick));
        self.items.push(b);
    }

    /// Drop every pending breakpoint (the plant is held, RFC 013 rule 17).
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// The actors whose outputs or targets hold the pending breakpoints, ascending and without
    /// repeats.
    pub fn origins(&self) -> Vec<Identity> {
        let mut o: Vec<Identity> = self.items.iter().map(|b| b.origin).collect();
        o.sort_unstable();
        o.dedup();
        o
    }

    /// The tick of the earliest pending breakpoint later than `solved`, if any.
    pub fn next_after(&self, solved: u64) -> Option<u64> {
        self.items
            .iter()
            .map(|b| b.tick)
            .filter(|t| *t > solved)
            .min()
    }

    /// Take the breakpoints with a tick not later than `solved`, in the order recorded, and apply
    /// each to the plant: the input is set and the history is cut (RFC 013 rule 7).
    pub fn apply_due(&mut self, solved: u64, plant: &mut Transient) -> usize {
        let (due, later): (Vec<Breakpoint>, Vec<Breakpoint>) =
            self.items.iter().copied().partition(|b| b.tick <= solved);
        self.items = later;
        for b in &due {
            match b.input {
                Input::Source(actor) => {
                    plant.set(actor, b.value);
                }
                Input::Signal(s) => plant.hold(s, b.value),
            }
            plant.cut();
        }
        due.len()
    }
}

#[cfg(test)]
mod tests;
