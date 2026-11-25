//! What a committed report draws as a series (RFC 014 rules 2 and 3; RFC 012 finding F-6): each
//! `f64` field of a report, with its **index among the actor's declared `f64` fields, counted in
//! channel order and then field order**, so that a front end computes it from the channel summaries
//! it holds. A built-in sensor declares one channel, `probe`, whose one `f64` is index 0; a detector
//! one channel, `condition`, whose first field, the output, is its only `f64` (the second is a
//! tick).

use florn_kernel::Identity;

use crate::plant::Plant;
use crate::{CHANNEL_CONDITION, CHANNEL_PROBE, kinds};

/// The `f64` fields of the layout of a channel, in field order, by their byte offset: `probe`'s
/// `value`; `condition`'s `output` (then a `u64` the plot does not draw).
fn f64_fields(channel: u32) -> &'static [usize] {
    match channel {
        CHANNEL_PROBE | CHANNEL_CONDITION => &[0],
        _ => &[],
    }
}

pub fn display_series(
    plant: Option<&Plant>,
    actor: Identity,
    channel: u32,
    payload: &[u8],
) -> Vec<(u8, f64)> {
    let Some(device) = plant.and_then(|p| p.devices.iter().find(|d| d.actor == actor)) else {
        return Vec::new();
    };
    let declared = kinds::by_builtin(&format!("circuit/{}", device.kind))
        .map(|k| k.channels())
        .unwrap_or_default();
    // The index of this channel's first `f64` among the declared ones, in channel order.
    let mut before = 0usize;
    for c in declared {
        if c == channel {
            return f64_fields(channel)
                .iter()
                .enumerate()
                .filter_map(|(n, offset)| {
                    let bytes = payload.get(*offset..*offset + 8)?;
                    let value = f64::from_le_bytes(bytes.try_into().ok()?);
                    u8::try_from(before + n).ok().map(|i| (i, value))
                })
                .collect();
        }
        before += f64_fields(c).len();
    }
    Vec::new()
}
