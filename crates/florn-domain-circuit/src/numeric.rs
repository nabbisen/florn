//! The numeric fields of a payload an actor of the plant committed, for the comparison of Tier 2
//! (RFC 013 rule 20, item 2): a sensor's `probe` value and the eight bytes of a `send` on a
//! `signal-out` port, each in the unit of its signal, converted to SI by the unit's factor.

use florn_host::domain::{Numeric, NumericField, Site};
use florn_kernel::Identity;

use crate::guests::{GuestInfo, Guests, f64_offsets};
use crate::plant::{Element, Plant};
use crate::units::CircuitUnits;
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
    guests: &Guests,
    actor: Identity,
    site: Site,
    payload: &[u8],
) -> Option<Numeric> {
    if let (Some(info), Site::Report { channel }) = (guests.get(actor), site) {
        return guest_report(info, channel, payload);
    }
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
        Site::Send { port } => match plant.port_signal.get(&(actor, port)) {
            Some(&signal) => {
                let (unit, factor) = unit_of(plant, Some(signal), "");
                ("output", unit, factor)
            }
            // A sensor's own output port that is on no signal still sends its sample (rule 11),
            // in the sensor's SI unit, as its report does: a `signal-out` port is numeric
            // whether or not anything is connected to it (RFC 013 rule 20, item 2).
            None => {
                let device = plant.devices.iter().find(|d| d.actor == actor)?;
                let base = match &device.element {
                    Element::Voltmeter { out_port, .. } if *out_port == Some(port) => "V",
                    Element::Ammeter { out_port, .. } if *out_port == Some(port) => "A",
                    Element::Motor { out_port, .. } if *out_port == Some(port) => "rad/s",
                    _ => return None,
                };
                let (unit, factor) = unit_of(plant, None, base);
                ("output", unit, factor)
            }
        },
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

/// A report of a guest controller on a channel its module declares (RFC 013 rule 20, item 2): every
/// `f64` field is numeric, in the unit its module gives it (`control-units`, for the `control`
/// channel) and dimensionless otherwise; the rest of the payload is compared exactly. A layout with
/// a field of variable size before the end of its numbers has no fixed offsets: the payload is then
/// compared by digest.
fn guest_report(info: &GuestInfo, channel: u32, payload: &[u8]) -> Option<Numeric> {
    use florn_admission::config::Units;
    let declared = info.channel(channel)?;
    let offsets = f64_offsets(declared);
    if offsets.is_empty() {
        return None;
    }
    let mut fields = Vec::new();
    let mut taken = vec![false; payload.len()];
    for (name, at) in &offsets {
        let bytes: [u8; 8] = payload.get(*at..*at + 8)?.try_into().ok()?;
        taken[*at..*at + 8].iter_mut().for_each(|t| *t = true);
        let (unit, factor) = match info.units.get(name).filter(|_| declared.name == "control") {
            Some(symbol) => {
                let u = CircuitUnits.unit(symbol)?;
                (symbol.clone(), crate::analyse::factor_of(&u))
            }
            None => (String::new(), 1.0),
        };
        fields.push(NumericField {
            name: name.clone(),
            unit,
            factor,
            value: f64::from_le_bytes(bytes),
        });
    }
    let exact = payload
        .iter()
        .zip(&taken)
        .filter(|(_, t)| !**t)
        .map(|(b, _)| *b)
        .collect();
    Some(Numeric { fields, exact })
}

#[cfg(test)]
mod tests;
