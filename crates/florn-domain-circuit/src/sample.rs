//! A sensor's sample (RFC 012 rules 10 and 23): the value of a `voltmeter`, an `ammeter` or a
//! motor's `speed` at the last accepted state, in the unit of its signal (one division by the
//! unit's factor, rule 5), and the port it is sent on.

use florn_kernel::Identity;

use crate::live::Live;
use crate::plant::{Element, Plant};

/// The value in SI of what a sensor measures, from the unknowns.
pub fn measure(element: &Element, x: &[f64]) -> Option<f64> {
    let volt = |n: &Option<usize>| n.map_or(0.0, |k| x[k]);
    match element {
        Element::Voltmeter { p, n, .. } => Some(volt(p) - volt(n)),
        Element::Ammeter { branch, .. } => Some(x[*branch]),
        Element::Motor { state, .. } => Some(x[*state]),
        _ => None,
    }
}

/// The sample of `actor` now, in its signal's unit, and the port it is sent on. `None` for an actor
/// that samples nothing or before the start.
pub fn read(plant: &Plant, live: &Live, actor: Identity) -> Option<(f64, Option<u32>)> {
    let transient = live.transient.as_ref()?;
    let device = plant.devices.iter().find(|d| d.actor == actor)?;
    let (out, port) = match &device.element {
        Element::Voltmeter { out, out_port, .. }
        | Element::Ammeter { out, out_port, .. }
        | Element::Motor { out, out_port, .. } => (*out, *out_port),
        _ => return None,
    };
    let si = measure(&device.element, transient.x())?;
    let value = match out.map(|s| &plant.signals[s]) {
        Some(s) if s.unit.is_some() => si / s.factor,
        _ => si,
    };
    Some((value, port))
}
