//! The domain's checkpoint state (RFC 012 rule 29; RFC 009 rule 1): everything the plant is
//! between events, in the canonical encoding with a version byte first: the solver's state
//! (`transient::snapshot`), the pending breakpoints in the order recorded, every signal's held
//! value and frozen flag, each sensor's next due tick, the tick of the pending advance and the last
//! one, the plant's status with the once-only mark of the non-convergence notification, the
//! operating point, and last the inspection summary. A checkpoint written by one Host restores in
//! the other.

use florn_model::canonical::{Decoder, Encoder};

use crate::dc::{Solution, Stage};
use crate::inputs::{Breakpoint, Input, Pending};
use crate::live::{Held, Live, Status, Summary};
use crate::plant::Plant;
use crate::transient::Transient;

/// The version of this encoding.
pub const VERSION: u8 = 4;

fn stage_code(s: Stage) -> u8 {
    match s {
        Stage::Plain => 0,
        Stage::GminStepping => 1,
        Stage::SourceStepping => 2,
    }
}

fn stage_of(c: u8) -> Option<Stage> {
    Some(match c {
        0 => Stage::Plain,
        1 => Stage::GminStepping,
        2 => Stage::SourceStepping,
        _ => return None,
    })
}

fn put_input(e: &mut Encoder, i: Input) {
    match i {
        Input::Source(a) => {
            e.u8(0);
            e.u32(a);
        }
        Input::Signal(s) => {
            e.u8(1);
            e.u32(s as u32);
        }
    }
}

fn get_input(d: &mut Decoder<'_>) -> Option<Input> {
    let (kind, n) = (d.u8()?, d.u32()?);
    Some(match kind {
        0 => Input::Source(n),
        1 => Input::Signal(n as usize),
        _ => return None,
    })
}

/// The domain's state, for a checkpoint.
pub fn encode(live: &Live, solution: Option<&Solution>) -> Vec<u8> {
    let mut e = Encoder::default();
    e.u8(VERSION);
    e.u64(live.last_advance);
    e.option(live.advance_at, |e, t| e.u64(t));
    e.u8(match live.status {
        Status::Running => 0,
        Status::Held => 1,
    });
    e.u8(u8::from(live.notified));
    e.u8(u8::from(live.guest_lost));
    e.list(&live.sensors, |e, s| e.u64(s.next_due));
    e.list(&live.samplers, |e, c| {
        e.u32(c.actor);
        e.u64(c.period);
        e.u64(c.next_due);
        e.u64(c.index);
    });
    e.list(&live.held, |e, h| {
        e.u64(crate::math::canonical(h.value).to_bits());
        e.u8(u8::from(h.frozen));
    });
    e.list(live.pending.items(), |e, b| {
        e.u64(b.tick);
        put_input(e, b.input);
        e.u64(crate::math::canonical(b.value).to_bits());
        e.u32(b.origin);
    });
    e.option(solution, |e, s| {
        e.u8(stage_code(s.stage));
        e.u32(s.iterations);
        e.list(&s.x, |e, x| e.u64(crate::math::canonical(*x).to_bits()));
    });
    e.option(live.transient.as_ref(), |e, t| t.encode(e));
    // The inspection summary comes last: observation, deterministic like the rest.
    e.u32(live.summary.steps);
    e.u32(live.summary.iterations);
    e.u32(live.summary.rejected);
    e.u64(live.summary.rejections);
    e.u32(live.summary.at_minimum);
    e.finish()
}

/// The state back over the composed plant; `None` when the bytes are not this domain's, or do not
/// fit the plant.
pub fn decode(plant: &Plant, bytes: &[u8]) -> Option<(Live, Option<Solution>)> {
    let mut d = Decoder::new(bytes);
    if d.u8()? != VERSION {
        return None;
    }
    let mut live = Live::new(plant);
    live.last_advance = d.u64()?;
    live.advance_at = d.option(|d| d.u64())?;
    live.status = match d.u8()? {
        0 => Status::Running,
        1 => Status::Held,
        _ => return None,
    };
    live.notified = d.u8()? != 0;
    live.guest_lost = d.u8()? != 0;
    let due = d.list(|d| d.u64())?;
    if due.len() != live.sensors.len() {
        return None;
    }
    for (s, t) in live.sensors.iter_mut().zip(due) {
        s.next_due = t;
    }
    live.samplers = d.list(|d| {
        Some(crate::controllers::Sampler {
            actor: d.u32()?,
            period: d.u64()?,
            next_due: d.u64()?,
            index: d.u64()?,
        })
    })?;
    let held = d.list(|d| {
        Some(Held {
            value: f64::from_bits(d.u64()?),
            frozen: d.u8()? != 0,
        })
    })?;
    if held.len() != live.held.len() {
        return None;
    }
    live.held = held;
    let pending = d.list(|d| {
        Some(Breakpoint {
            tick: d.u64()?,
            input: get_input(d)?,
            value: f64::from_bits(d.u64()?),
            origin: d.u32()?,
        })
    })?;
    live.pending = Pending::from_items(pending);
    let solution = d.option(|d| {
        Some(Solution {
            stage: stage_of(d.u8()?)?,
            iterations: d.u32()?,
            x: d.list(|d| d.u64().map(f64::from_bits))?,
        })
    })?;
    live.transient = d.option(|d| Transient::decode(plant, d))?;
    live.summary = Summary {
        steps: d.u32()?,
        iterations: d.u32()?,
        rejected: d.u32()?,
        rejections: d.u64()?,
        at_minimum: d.u32()?,
    };
    d.done().then_some((live, solution))
}

#[cfg(test)]
mod tests;
