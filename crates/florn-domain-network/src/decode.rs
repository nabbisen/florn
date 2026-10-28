//! Decoders for the inspector (RFC 008 rule 16; OBS-006): a frame's fields by the protocol a
//! module declares in its `[network]` table. The ladder's protocols; a decoder as a sandboxed
//! module is 0.3.

/// The protocols with a decoder.
pub const PROTOCOLS: &[&str] = &["net-echo", "distance-vector", "ospf"];

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
        "ospf" => decode_ospf(payload),
        _ => None,
    }
}

/// Version 1 of the OSPF subset's packets (RFC 008 rule 21; the guest's layout): the type,
/// the sender and the area, then a hello's intervals and the ids it heard, an update's LSAs
/// (origin, sequence, links as neighbour:cost) or an acknowledgement's (origin, sequence).
fn decode_ospf(payload: &[u8]) -> Option<Vec<(String, String)>> {
    let u64_at = |at: usize| {
        payload
            .get(at..at + 8)
            .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
    };
    let u32_at = |at: usize| {
        payload
            .get(at..at + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    };
    let u16_at = |at: usize| {
        payload
            .get(at..at + 2)
            .map(|b| u16::from_le_bytes(b.try_into().unwrap()))
    };
    if payload.first() != Some(&1) {
        return None;
    }
    let kind = *payload.get(1)?;
    let sender = u64_at(2)?;
    let area = u64_at(10)?;
    let mut out = vec![
        (
            "type".to_string(),
            match kind {
                1 => "hello",
                2 => "update",
                3 => "acknowledgement",
                _ => return None,
            }
            .to_string(),
        ),
        ("sender".to_string(), sender.to_string()),
        ("area".to_string(), area.to_string()),
    ];
    match kind {
        1 => {
            out.push(("hello".into(), u32_at(18)?.to_string()));
            out.push(("dead".into(), u32_at(22)?.to_string()));
            let n = usize::from(u16_at(26)?);
            let heard: Vec<String> = (0..n)
                .map(|i| u64_at(28 + i * 8))
                .collect::<Option<Vec<_>>>()?
                .iter()
                .map(|x| x.to_string())
                .collect();
            if payload.len() != 28 + n * 8 {
                return None;
            }
            out.push(("heard".into(), heard.join(" ")));
        }
        2 => {
            let n = usize::from(u16_at(18)?);
            let mut at = 20;
            let mut lsas = Vec::with_capacity(n);
            for _ in 0..n {
                let origin = u64_at(at)?;
                let seq = u64_at(at + 8)?;
                let links = usize::from(u16_at(at + 16)?);
                at += 18;
                let mut l = Vec::with_capacity(links);
                for _ in 0..links {
                    l.push(format!("{}:{}", u64_at(at)?, u16_at(at + 8)?));
                    at += 10;
                }
                lsas.push(format!("{origin}#{seq}[{}]", l.join(",")));
            }
            if payload.len() != at {
                return None;
            }
            out.push(("count".into(), n.to_string()));
            out.push(("lsas".into(), lsas.join(" ")));
        }
        _ => {
            let n = usize::from(u16_at(18)?);
            let acks: Vec<String> = (0..n)
                .map(|i| Some(format!("{}#{}", u64_at(20 + i * 16)?, u64_at(28 + i * 16)?)))
                .collect::<Option<Vec<_>>>()?;
            if payload.len() != 20 + n * 16 {
                return None;
            }
            out.push(("count".into(), n.to_string()));
            out.push(("acks".into(), acks.join(" ")));
        }
    }
    Some(out)
}
