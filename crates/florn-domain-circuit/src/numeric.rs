//! The numeric fields of a payload an actor of the plant committed, for the comparison of Tier 2
//! (RFC 013 rule 20, item 2): a sensor's `probe` value and the eight bytes of a `send` on a
//! `signal-out` port, each in the unit of its signal, converted to SI by the unit's factor.

use florn_host::domain::{Numeric, NumericField, Site};
use florn_kernel::Identity;

use crate::plant::{Element, Plant};
use crate::{CHANNEL_PROBE, units};

/// The unit symbol and the factor to SI of a signal, or of the sensor's own SI unit when the
/// sensor is on no signal.
fn unit_of(plant: &Plant, signal: Option<usize>, fallback: &'static str) -> (String, f64) {
    match signal.map(|s| &plant.signals[s]) {
        Some(s) => (s.unit.clone().unwrap_or_default(), s.factor),
        None => (fallback.to_string(), 1.0),
    }
}

pub fn numeric_fields(
    plant: &Plant,
    actor: Identity,
    site: Site,
    payload: &[u8],
) -> Option<Numeric> {
    let bytes: [u8; 8] = payload.try_into().ok()?;
    let value = f64::from_le_bytes(bytes);
    let (name, unit, factor) = match site {
        Site::Report { channel } if channel == CHANNEL_PROBE => {
            let device = plant.devices.iter().find(|d| d.actor == actor)?;
            let (signal, base) = match &device.element {
                Element::Voltmeter { out, .. } => (*out, "V"),
                Element::Ammeter { out, .. } => (*out, "A"),
                Element::Motor { out, .. } => (*out, "rad/s"),
                _ => return None,
            };
            let (unit, factor) = unit_of(plant, signal, base);
            ("value", unit, factor)
        }
        Site::Send { port } => {
            let signal = *plant.port_signal.get(&(actor, port))?;
            let (unit, factor) = unit_of(plant, Some(signal), "");
            ("output", unit, factor)
        }
        _ => return None,
    };
    debug_assert!(unit.is_empty() || units::all().iter().any(|u| u.symbol == unit));
    Some(Numeric {
        fields: vec![NumericField {
            name: name.to_string(),
            unit,
            factor,
            value,
        }],
        exact: Vec::new(),
    })
}

#[cfg(test)]
mod tests;
