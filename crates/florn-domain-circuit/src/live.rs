//! What the plant does between the events of the run (RFC 012 rules 7 to 11 and 13; RFC 013 rule
//! 17): the held values of the signals, the breakpoints, the sensors' due ticks, the one pending
//! `advance`, the plant's status, the observation of the last advance. The domain drives the solver
//! (`transient`) from here: the `advance` event takes the plant from `solved` to its tick by as many
//! steps as the controller takes, split at every breakpoint; a sensor's sample reads the state at
//! `solved`; a committed output or a change records a breakpoint and advances nothing.

use florn_control::{Code, Scope, Value};
use florn_host::compound::CallIn;
use florn_host::domain::{Action, EndRequest, Limit};
use florn_kernel::Identity;

use crate::controllers::Sampler;
use crate::dc::Solution;
use crate::device::Calls;
use crate::inputs::{Breakpoint, Input, Pending};
use crate::plant::{Element, Plant};
use crate::profile::OnNonConvergence;
use crate::read::note;
use crate::transient::{Advance, Cause, Transient};
use crate::{CLASS_ADVANCE, CLASS_SAMPLE};

/// A signal's held value in its own unit, and whether it is frozen (RFC 012 rule 15).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Held {
    pub value: f64,
    pub frozen: bool,
}

/// A sampling built-in: its device, its period and its next due tick (RFC 012 rule 10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sensor {
    pub device: usize,
    pub actor: Identity,
    pub period: u64,
    pub next_due: u64,
}

/// Running, or held at the last accepted step (RFC 013 rule 17, `on-non-convergence = "hold"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Running,
    Held,
}

/// What the last advance did, and the rejections so far: the world inspection (RFC 012 rule 31).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub steps: u32,
    pub iterations: u32,
    pub rejected: u32,
    pub rejections: u64,
    /// The steps the error test failed at the minimum, accepted anyway (the notification is once
    /// per advance, RFC 013 rule 12; this is the count).
    pub at_minimum: u32,
}

#[derive(Clone, Debug)]
pub struct Live {
    pub held: Vec<Held>,
    /// Whether a signal drives a built-in of the plant (`cmd`, `ctl`, `load`).
    pub drives: Vec<bool>,
    pub pending: Pending,
    pub sensors: Vec<Sensor>,
    /// The sampled controllers (rule 10): their period and next due tick and index.
    pub samplers: Vec<Sampler>,
    pub detector: bool,
    pub advance_at: Option<u64>,
    pub last_advance: u64,
    pub status: Status,
    /// The once-only mark of the non-convergence notification (RFC 013 rule 17).
    pub notified: bool,
    /// The guest device was lost (it faulted, or its result was no stamp) and the plant has none
    /// from then on (RFC 012 rule 17).
    pub guest_lost: bool,
    pub summary: Summary,
    pub transient: Option<Transient>,
    pub limit: Option<Limit>,
    pub end_request: Option<EndRequest>,
}

fn period_phase(e: &Element) -> Option<(u64, u64)> {
    match e {
        Element::Voltmeter { period, phase, .. }
        | Element::Ammeter { period, phase, .. }
        | Element::Motor { period, phase, .. } => Some((*period, *phase)),
        _ => None,
    }
}

impl Live {
    /// The state of a composed scenario before the start: every signal at its held value, every
    /// sensor due at its phase.
    pub fn new(plant: &Plant) -> Self {
        let mut drives = vec![false; plant.signals.len()];
        let mut detector = false;
        for d in &plant.devices {
            match &d.element {
                Element::VoltageSource { cmd, .. } | Element::CurrentSource { cmd, .. } => {
                    if let Some(s) = cmd {
                        drives[*s] = true;
                    }
                }
                Element::Switch { ctl: Some(s), .. } | Element::Motor { load: Some(s), .. } => {
                    drives[*s] = true;
                }
                Element::Comparator { .. } => detector = true,
                _ => {}
            }
        }
        let sensors = plant
            .devices
            .iter()
            .enumerate()
            .filter_map(|(i, d)| {
                period_phase(&d.element).map(|(period, phase)| Sensor {
                    device: i,
                    actor: d.actor,
                    period,
                    next_due: phase,
                })
            })
            .collect();
        Self {
            held: plant
                .signals
                .iter()
                .map(|s| Held {
                    value: s.held,
                    frozen: false,
                })
                .collect(),
            drives,
            pending: Pending::default(),
            sensors,
            samplers: Vec::new(),
            detector,
            advance_at: None,
            last_advance: 0,
            status: Status::Running,
            notified: false,
            guest_lost: false,
            summary: Summary::default(),
            transient: None,
            limit: None,
            end_request: None,
        }
    }

    /// The plant with the held values the signals have now: what the initial solve and the first
    /// step read (RFC 012 rule 30: the outputs the controllers' `init` set are in force).
    pub fn plant_in_force(&self, plant: &Plant) -> Plant {
        let mut p = plant.clone();
        for (s, h) in p.signals.iter_mut().zip(&self.held) {
            s.held = h.value;
        }
        p
    }

    /// The start hook after the initial solve (rule 30): the solver at tick 0, the breakpoints the
    /// `init`s recorded consumed, the first advance scheduled (rule 8).
    pub fn start(
        &mut self,
        plant: &Plant,
        solution: &Solution,
        samplers: Vec<Sampler>,
    ) -> Vec<Action> {
        self.pending.clear();
        self.samplers = samplers;
        let in_force = self.plant_in_force(plant);
        self.transient = Some(Transient::new(&in_force, solution));
        self.last_advance = 0;
        // The first advance (rule 8) and each controller's first sample at its phase (rule 10);
        // what is scheduled with delay zero lands one microstep later (rule 10).
        let mut actions = self.schedule(plant, 0);
        for c in &self.samplers {
            if c.next_due < plant.end {
                actions.push(sample_event(c.actor, c.next_due));
            }
        }
        actions
    }

    /// A controller's sample event at `tick` (rule 10): its `on-sample` activation, then its next
    /// sample `period` later, unless that is at or after the end. A faulted controller (`alive`
    /// false) is not sampled again: the event is dropped (rule 17).
    pub fn controller_sample(
        &mut self,
        plant: &Plant,
        actor: Identity,
        tick: u64,
        alive: bool,
    ) -> Vec<Action> {
        let Some(c) = self.samplers.iter_mut().find(|c| c.actor == actor) else {
            return Vec::new();
        };
        if !alive {
            return Vec::new();
        }
        let index = c.index;
        c.index += 1;
        c.next_due = tick + c.period;
        let mut actions = vec![Action::Call {
            actor,
            export: crate::controllers::ON_SAMPLE.to_string(),
            values: vec![florn_abi::Val::U64(tick), florn_abi::Val::U64(index)],
        }];
        if c.next_due < plant.end {
            actions.push(Action::Schedule {
                origin: actor,
                delay: c.period,
                class: CLASS_SAMPLE,
                target: actor,
                payload: Vec::new(),
            });
        }
        actions
    }

    /// The tick of the next advance (rule 8): the earliest tick a sensor is due, or the last
    /// advance's tick plus `detect-interval` when the scenario has a detector, and only when it is
    /// before the scenario's end.
    fn next_advance(&self, plant: &Plant, after: u64) -> Option<u64> {
        let mut next = self.sensors.iter().map(|s| s.next_due).min();
        if self.detector {
            let wake = after.saturating_add(plant.profile.detect_interval);
            next = Some(next.map_or(wake, |n| n.min(wake)));
        }
        next.filter(|t| *t < plant.end)
    }

    /// The pending advance, as the action that schedules it, counting from `from`.
    fn schedule(&mut self, plant: &Plant, from: u64) -> Vec<Action> {
        self.advance_at = self.next_advance(plant, self.last_advance);
        match self.advance_at {
            Some(t) => vec![Action::Schedule {
                origin: plant.world,
                delay: t.saturating_sub(from),
                class: CLASS_ADVANCE,
                target: plant.world,
                payload: Vec::new(),
            }],
            None => Vec::new(),
        }
    }

    /// The advance event at `tick` (rule 8): the plant made current, the sensors due scheduled on
    /// the next microstep, the next advance scheduled. An advance calls no actor.
    pub fn on_advance(&mut self, plant: &Plant, tick: u64, calls: &mut dyn CallIn) -> Vec<Action> {
        let mut actions = Vec::new();
        self.last_advance = tick;
        self.advance_at = None;
        let mut report = Advance::default();
        let mut stuck = None;
        let mut ended = false;
        // The guest device, if the plant has one still: it is called inside every attempted step
        // (RFC 012 rules 19 to 21).
        let guest = self
            .transient
            .as_ref()
            .and_then(|t| t.plant().guest.clone());
        if self.status == Status::Running
            && let Some(t) = self.transient.as_mut()
        {
            self.pending.apply_due(t.now(), t);
            while t.now() < tick {
                let limit = self
                    .pending
                    .next_after(t.now())
                    .map_or(tick, |b| b.min(tick));
                let out = match t.plant().guest.as_ref() {
                    Some(g) => {
                        t.advance_with(limit, &mut Calls::new(&mut *calls, g.actor, g.nodes.len()))
                    }
                    None => t.advance(limit),
                };
                merge(&mut report, out.report);
                if out.stuck.is_some() {
                    stuck = out.stuck;
                    break;
                }
                if out.ended {
                    ended = true;
                    break;
                }
                if t.now() < tick {
                    self.pending.apply_due(t.now(), t);
                }
            }
        }
        self.summary = Summary {
            steps: report.steps,
            iterations: report.iterations,
            rejected: report.rejected,
            rejections: self.summary.rejections + u64::from(report.rejected),
            at_minimum: report.minimum.len() as u32,
        };
        // The device lost in this advance: `circuit.stamp-invalid` for a result that was no stamp,
        // and `circuit.device-faulted`, once (rule 17), for any loss.
        if let (Some(lost), Some(g)) = (&report.lost, &guest) {
            self.guest_lost = true;
            actions.extend(crate::device::lost_notes(lost, g));
        }
        // The run ended in a call of the device (a fault under `stop-run`): the advance is
        // abandoned where it stands and the plant keeps its last accepted step (rule 17).
        if ended {
            return actions;
        }
        if report.rejected > 0 {
            let cause = match report.first_cause {
                Some(Cause::Convergence) => "convergence",
                _ => "error",
            };
            actions.push(Action::Notify(note(
                Code::CircuitStepRejected,
                Scope::run(),
                None,
                vec![
                    Value::Tick(tick),
                    Value::Count(u64::from(report.rejected)),
                    Value::Enum(cause.to_string()),
                ],
            )));
        }
        // Once per advance, for the first step at the minimum (rule 28's row).
        for m in report.minimum.iter().take(1) {
            actions.push(Action::Notify(note(
                Code::CircuitStepMinimum,
                Scope::run(),
                None,
                vec![Value::Tick(m.tick), Value::Count(m.step)],
            )));
        }
        if let Some(s) = stuck {
            let device = s
                .owner
                .or_else(|| s.unknown.map(|u| owner_of_unknown(plant, u)));
            let builtin = device.and_then(|d| plant.devices.get(d));
            // The device with the largest residual (rule 17), the guest device included: it is named
            // when it has a pin on the net and a lower identity than the built-in found.
            let name = match (&guest, s.unknown) {
                (Some(g), Some(u))
                    if self
                        .transient
                        .as_ref()
                        .is_some_and(|t| t.plant().guest.is_some())
                        && g.nodes.contains(&Some(u))
                        && builtin.is_none_or(|b| g.actor < b.actor) =>
                {
                    g.name.clone()
                }
                _ => builtin.map_or_else(String::new, |d| d.name.clone()),
            };
            let never_said = !self.notified;
            self.notified = true;
            if never_said {
                actions.push(Action::Notify(note(
                    Code::CircuitNonConvergence,
                    Scope::run(),
                    None,
                    vec![
                        Value::Name(name),
                        Value::Tick(s.tick),
                        Value::Count(u64::from(s.iterations)),
                    ],
                )));
            }
            match plant.profile.on_non_convergence {
                OnNonConvergence::StopRun => {
                    actions.push(Action::EndRun(EndRequest::NonConvergence));
                    return actions;
                }
                OnNonConvergence::Hold => {
                    // Frozen at the last accepted step; the pending breakpoints are dropped.
                    self.status = Status::Held;
                    self.pending.clear();
                }
            }
        }
        // The sensors due now sample on the next microstep, in ascending identity (rule 10).
        for s in &mut self.sensors {
            if s.next_due <= tick {
                actions.push(Action::Schedule {
                    origin: s.actor,
                    delay: 0,
                    class: CLASS_SAMPLE,
                    target: s.actor,
                    payload: Vec::new(),
                });
                s.next_due += s.period;
            }
        }
        actions.extend(self.schedule(plant, tick));
        actions
    }

    /// Record a change of an input at `tick` (rule 9), unless it sets the value the input already
    /// holds; the bound on the pending breakpoints ends the run when it is passed. While the plant
    /// is held nothing is recorded.
    pub fn record(&mut self, plant: &Plant, tick: u64, input: Input, value: f64, origin: Identity) {
        if self.status == Status::Held || self.transient.is_none() {
            return;
        }
        let in_force = match input {
            Input::Source(a) => self.transient.as_ref().and_then(|t| t.source_value(a)),
            // The plant's own view: the held value is updated before the breakpoint is recorded.
            Input::Signal(s) => self.transient.as_ref().map(|t| t.held(s)),
        };
        let holds = self.pending.last_value(input).or(in_force);
        if holds.is_some_and(|h| h.to_bits() == value.to_bits()) {
            return;
        }
        self.pending.record(Breakpoint {
            tick,
            input,
            value,
            origin,
        });
        if self.pending.len() as u64 > plant.profile.max_breakpoints && self.limit.is_none() {
            self.limit = Some(Limit {
                what: "pending-breakpoints",
                limit: plant.profile.max_breakpoints,
                origins: self.pending.origins(),
            });
        }
    }

    /// A committed output on a `signal-out` port (rule 11): the signal's held value, unless it is
    /// frozen; a breakpoint for the plant's inputs it drives when the value changes.
    pub fn output(&mut self, plant: &Plant, tick: u64, origin: Identity, port: u32, value: f64) {
        let Some(&sig) = plant.port_signal.get(&(origin, port)) else {
            return;
        };
        if self.held[sig].frozen || self.held[sig].value.to_bits() == value.to_bits() {
            return;
        }
        self.held[sig].value = value;
        if self.drives[sig] {
            self.record(plant, tick, Input::Signal(sig), value, origin);
        }
    }

    /// A settable value set at `tick` (rule 15): a source or a threshold is a breakpoint; a
    /// `constant`'s value is the held value of its signal, with a breakpoint where it drives.
    pub fn set_source(&mut self, plant: &Plant, tick: u64, s: crate::changes::Source) {
        match s.signal {
            Some(sig) => {
                if self.held[sig].value.to_bits() == s.value.to_bits() {
                    return;
                }
                self.held[sig].value = s.value;
                if self.drives[sig] {
                    self.record(plant, tick, Input::Signal(sig), s.value, s.actor);
                }
            }
            None => self.record(plant, tick, s.input, s.value, s.actor),
        }
    }

    /// Freeze or release a signal (rule 15). The value in force does not change, so no breakpoint
    /// is recorded and no history is cut (rule 9).
    pub fn freeze(&mut self, signal: usize, frozen: bool) {
        self.held[signal].frozen = frozen;
    }
}

fn merge(into: &mut Advance, from: Advance) {
    into.steps += from.steps;
    into.attempts += from.attempts;
    into.rejected += from.rejected;
    if into.first_cause.is_none() {
        into.first_cause = from.first_cause;
    }
    into.iterations += from.iterations;
    into.minimum.extend(from.minimum);
    if into.lost.is_none() {
        into.lost = from.lost;
    }
}

/// The device a notification names for an unknown: the owner of a branch current or a state, the
/// lowest-identity device with a pin on a net.
pub fn owner_of_unknown(plant: &Plant, unknown: usize) -> usize {
    let nets = plant.nets.len();
    for (i, d) in plant.devices.iter().enumerate() {
        let hit = match &d.element {
            Element::Inductor { branch, .. }
            | Element::VoltageSource { branch, .. }
            | Element::Ammeter { branch, .. } => *branch == unknown,
            Element::Motor { branch, state, .. } => *branch == unknown || *state == unknown,
            _ => false,
        };
        if hit {
            return i;
        }
    }
    if unknown < nets {
        for (i, d) in plant.devices.iter().enumerate() {
            if pins_of(&d.element).iter().flatten().any(|n| *n == unknown) {
                return i;
            }
        }
    }
    0
}

fn pins_of(e: &Element) -> [Option<usize>; 2] {
    match e {
        Element::Resistor { a, b, .. }
        | Element::Capacitor { a, b, .. }
        | Element::Inductor { a, b, .. }
        | Element::Switch { a, b, .. }
        | Element::Ammeter { a, b, .. } => [*a, *b],
        Element::VoltageSource { p, n, .. }
        | Element::CurrentSource { p, n, .. }
        | Element::Voltmeter { p, n, .. }
        | Element::Comparator { p, n, .. }
        | Element::Motor { p, n, .. } => [*p, *n],
        Element::Diode { a, k, .. } => [*a, *k],
        Element::Constant { .. } => [None, None],
    }
}

/// The event of a controller's sample at its phase: addressed to itself, from itself.
fn sample_event(actor: Identity, delay: u64) -> Action {
    Action::Schedule {
        origin: actor,
        delay,
        class: CLASS_SAMPLE,
        target: actor,
        payload: Vec::new(),
    }
}

#[cfg(test)]
mod tests;
