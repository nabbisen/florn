//! Decoders for the inspector (RFC 008 rule 16; OBS-006): a frame's fields by the protocol a
//! module declares in its `[network]` table. The ladder's protocols; a decoder as a sandboxed
//! module is 0.3.

/// The protocols with a decoder.
pub const PROTOCOLS: &[&str] = &["net-echo"];

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
        _ => None,
    }
}
