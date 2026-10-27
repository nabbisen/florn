//! Decoders for the inspector (RFC 008 rule 16; OBS-006): a frame's fields by the protocol a
//! module declares in its `[network]` table. The ladder's protocols; a decoder as a sandboxed
//! module is 0.3.

/// The protocols with a decoder.
pub const PROTOCOLS: &[&str] = &["net-echo", "distance-vector"];

/// A frame's fields as `(name, value)` text pairs, or `None` when the protocol is unknown or
/// the frame does not parse.
pub fn decode(protocol: &str, payload: &[u8]) -> Option<Vec<(String, String)>> {
    match protocol {
        "net-echo" => {
            // The ordinal of the send in the first eight bytes, then a pattern.
            let ordinal = u64::from_le_bytes(payload.get(0..8)?.try_into().ok()?);
            Some(vec![
                ("ordinal".into(), ordinal.to_string()),
                ("length".into(), payload.len().to_string()),
            ])
        }
        "distance-vector" => {
            // Version 1: the sender's id, the entry count, then (destination, metric) pairs
            // (RFC 008 rule 20; the guest's frame layout).
            if payload.first() != Some(&1) {
                return None;
            }
            let sender = u64::from_le_bytes(payload.get(1..9)?.try_into().ok()?);
            let n = u16::from_le_bytes(payload.get(9..11)?.try_into().ok()?) as usize;
            let mut routes = Vec::with_capacity(n);
            for i in 0..n {
                let at = 11 + i * 10;
                let dest = u64::from_le_bytes(payload.get(at..at + 8)?.try_into().ok()?);
                let metric = u16::from_le_bytes(payload.get(at + 8..at + 10)?.try_into().ok()?);
                routes.push(format!("{dest}:{metric}"));
            }
            if payload.len() != 11 + n * 10 {
                return None;
            }
            Some(vec![
                ("sender".into(), sender.to_string()),
                ("count".into(), n.to_string()),
                ("routes".into(), routes.join(" ")),
            ])
        }
        _ => None,
    }
}
