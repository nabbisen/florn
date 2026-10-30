//! Decoders for the inspector (RFC 008 rule 16; OBS-006): a frame's fields by the protocol a
//! module declares in its `[network]` table. The ladder's protocols; a decoder as a sandboxed
//! module is 0.3.

/// The protocols with a decoder.
pub const PROTOCOLS: &[&str] = &["net-echo", "distance-vector", "ospf", "bgp"];

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
        "bgp" => decode_bgp(payload),
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

/// Version 1 of the BGP subset's frames (RFC 011 rule 7; the guest's layout): the type, the
/// sender's AS and router id, then an open's hold time, an update's withdrawn prefixes and
/// advertised prefixes as `prefix [as,as,…] med`, or a notification's code and text; a frame
/// that is not this layout is `None`, which the inspector shows as unknown.
fn decode_bgp(payload: &[u8]) -> Option<Vec<(String, String)>> {
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
    let prefix_at = |at: usize| -> Option<(String, usize)> {
        let n = usize::from(*payload.get(at)?);
        let bytes = payload.get(at + 1..at + 1 + n)?;
        Some((String::from_utf8_lossy(bytes).into_owned(), at + 1 + n))
    };
    if payload.first() != Some(&1) {
        return None;
    }
    let kind = *payload.get(1)?;
    let sender_as = u32_at(2)?;
    let router_id = u64_at(6)?;
    let mut out = vec![
        (
            "type".to_string(),
            match kind {
                1 => "open",
                2 => "update",
                3 => "keepalive",
                4 => "notification",
                _ => return None,
            }
            .to_string(),
        ),
        ("sender-as".to_string(), sender_as.to_string()),
        ("router-id".to_string(), router_id.to_string()),
    ];
    match kind {
        1 => {
            out.push(("hold".into(), u32_at(14)?.to_string()));
            if payload.len() != 18 {
                return None;
            }
        }
        2 => {
            let nw = usize::from(u16_at(14)?);
            let mut at = 16;
            let mut withdrawn = Vec::with_capacity(nw);
            for _ in 0..nw {
                let (p, next) = prefix_at(at)?;
                withdrawn.push(p);
                at = next;
            }
            let na = usize::from(u16_at(at)?);
            at += 2;
            let mut advertised = Vec::with_capacity(na);
            for _ in 0..na {
                let (p, next) = prefix_at(at)?;
                at = next;
                let n = usize::from(u16_at(at)?);
                at += 2;
                let path: Vec<String> = (0..n)
                    .map(|i| u32_at(at + i * 4).map(|a| a.to_string()))
                    .collect::<Option<Vec<_>>>()?;
                at += n * 4;
                let med = u32_at(at)?;
                at += 4;
                advertised.push(format!("{p} [{}] {med}", path.join(",")));
            }
            if payload.len() != at {
                return None;
            }
            out.push(("withdrawn".into(), withdrawn.join(" ")));
            out.push(("advertised".into(), advertised.join(" ")));
        }
        3 => {
            if payload.len() != 14 {
                return None;
            }
        }
        _ => {
            let code = *payload.get(14)?;
            let n = usize::from(u16_at(15)?);
            let text = payload.get(17..17 + n)?;
            if payload.len() != 17 + n {
                return None;
            }
            out.push((
                "code".into(),
                match code {
                    1 => "bad-version",
                    2 => "own-as",
                    3 => "hold-time",
                    4 => "outside-the-subset",
                    _ => "unknown",
                }
                .to_string(),
            ));
            out.push(("text".into(), String::from_utf8_lossy(text).into_owned()));
        }
    }
    Some(out)
}
