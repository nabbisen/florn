//! The solver's part of the domain's checkpoint state (RFC 012 rule 29): `solved`, the solution
//! vector, the integration history since the last cut with the count of steps, the step
//! controller's unconstrained proposal, and the inputs in force as the last step used them. Every
//! `f64` is its canonical bits (RFC 001 rule 59).

use florn_model::canonical::{Decoder, Encoder};

use crate::plant::{Element, Plant};

use super::{History, Transient};

fn put_f64(e: &mut Encoder, x: f64) {
    e.u64(crate::math::canonical(x).to_bits());
}

fn get_f64(d: &mut Decoder<'_>) -> Option<f64> {
    d.u64().map(f64::from_bits)
}

fn put_list(e: &mut Encoder, xs: &[f64]) {
    e.list(xs, |e, x| put_f64(e, *x));
}

fn get_list(d: &mut Decoder<'_>) -> Option<Vec<f64>> {
    d.list(get_f64)
}

impl Transient {
    /// Write the state.
    pub fn encode(&self, e: &mut Encoder) {
        e.u64(self.now);
        put_list(e, &self.x);
        e.u64(self.proposal);
        e.u32(self.history.since_cut);
        e.list(&self.history.points, |e, p| put_list(e, p));
        e.list(&self.history.gaps, |e, g| e.u64(*g));
        put_list(e, &self.history.derivative);
        // The inputs in force: each settable value and each held value as the plant reads them.
        let sources: Vec<f64> = self
            .plant
            .devices
            .iter()
            .filter_map(|d| self.source_value(d.actor))
            .collect();
        put_list(e, &sources);
        let held: Vec<f64> = self.plant.signals.iter().map(|s| s.held).collect();
        put_list(e, &held);
    }

    /// Read the state back over `plant`, which says what the unknowns, the dynamic variables, the
    /// sources and the signals are; `None` when the bytes do not fit it.
    pub fn decode(plant: &Plant, d: &mut Decoder<'_>) -> Option<Self> {
        let now = d.u64()?;
        let x = get_list(d)?;
        let proposal = d.u64()?;
        let since_cut = d.u32()?;
        let points = d.list(get_list)?;
        let gaps = d.list(|d| d.u64())?;
        let derivative = get_list(d)?;
        let sources = get_list(d)?;
        let held = get_list(d)?;
        let mut plant = plant.clone();
        let mut sources = sources.into_iter();
        for dev in &mut plant.devices {
            match &mut dev.element {
                Element::VoltageSource { v, .. } => *v = sources.next()?,
                Element::CurrentSource { i, .. } => *i = sources.next()?,
                Element::Comparator { threshold, .. } => *threshold = sources.next()?,
                _ => {}
            }
        }
        if sources.next().is_some() || held.len() != plant.signals.len() {
            return None;
        }
        for (s, h) in plant.signals.iter_mut().zip(held) {
            s.held = h;
        }
        let dynamics = crate::integrate::dynamics(&plant);
        let fits = x.len() == plant.transient_unknowns
            && derivative.len() == dynamics.len()
            && !points.is_empty()
            && points.len() <= 3
            && gaps.len() <= 2
            && points.iter().all(|p| p.len() == dynamics.len());
        fits.then_some(Self {
            plant,
            dynamics,
            x,
            now,
            history: History {
                points,
                gaps,
                derivative,
                since_cut,
            },
            proposal,
            trace: None,
        })
    }
}
