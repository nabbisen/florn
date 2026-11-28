//! The results file (RFC 013 rule 21; AUTO-004): flat text, the first line `florn-results 1`,
//! then one line per trace record carrying everything the comparison of rule 20 reads, so that the
//! browser scripts and the native comparison read the same text without a TOML parser. Its syntax
//! is frozen by the kit's format test.
//!
//! ```text
//! florn-results 1
//! h <key> <target> <work> <draws> <effect>…
//! f <key> <actor> <kind> <parameter> <work>
//! i <key> <digest> <len>
//! ```
//!
//! A `<key>` is `tick.microstep.class.tie.origin.seq` in decimal. An `<effect>` is one token:
//! `s:<seq>:<port>:<len>:<payload>` (a send), `t:<seq>:<timer>:<delay>` (a timer),
//! `c:<seq>:<timer>` (a cancel) or `r:<seq>:<channel>:<len>:<payload>` (a report). A `<payload>`
//! is `d=<64 hex digits>` (its digest), or `n=<rest>` followed by `+<field>` for each numeric
//! field: `<rest>` is the digest of what is not a numeric field, a `<field>` is
//! `<name>|<unit>|<factor>|<value>` with the factor's and the value's `f64` bits as 16 hex digits.
//! A name or a unit writes every byte but a letter, a digit and `_ . / -` as `%` and two hex
//! digits.

use std::fmt::Write;

use florn_kernel::{Digest, EventKey};

use super::{Effect, Number, Payload, Row};

/// The first line of every results file.
pub const FIRST_LINE: &str = "florn-results 1";

fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

fn escape(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'/' | b'-') {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02x}");
        }
    }
    out
}

fn unescape(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let h = text.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(h, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn key_text(k: &EventKey) -> String {
    format!(
        "{}.{}.{}.{}.{}.{}",
        k.tick, k.microstep, k.class, k.tie, k.origin, k.seq
    )
}

fn payload_text(p: &Payload) -> String {
    match p {
        Payload::Digest(d) => format!("d={}", hex(d)),
        Payload::Numbers { fields, rest } => {
            let mut s = format!("n={}", hex(rest));
            for f in fields {
                let _ = write!(
                    s,
                    "+{}|{}|{:016x}|{:016x}",
                    escape(&f.name),
                    escape(&f.unit),
                    f.factor,
                    f.value
                );
            }
            s
        }
    }
}

/// The results file of a trace's rows.
pub fn write(rows: &[Row]) -> String {
    let mut out = String::from(FIRST_LINE);
    out.push('\n');
    for row in rows {
        match row {
            Row::Handled {
                key,
                target,
                work,
                draws,
                effects,
            } => {
                let _ = write!(out, "h {} {target} {work} {draws}", key_text(key));
                for e in effects {
                    out.push(' ');
                    match e {
                        Effect::Send {
                            seq,
                            port,
                            len,
                            payload,
                        } => {
                            let _ = write!(out, "s:{seq}:{port}:{len}:{}", payload_text(payload));
                        }
                        Effect::SetTimer { seq, timer, delay } => {
                            let _ = write!(out, "t:{seq}:{timer}:{delay}");
                        }
                        Effect::CancelTimer { seq, timer } => {
                            let _ = write!(out, "c:{seq}:{timer}");
                        }
                        Effect::Report {
                            seq,
                            channel,
                            len,
                            payload,
                        } => {
                            let _ =
                                write!(out, "r:{seq}:{channel}:{len}:{}", payload_text(payload));
                        }
                    }
                }
            }
            Row::Fault {
                key,
                actor,
                kind,
                parameter,
                work,
            } => {
                let _ = write!(out, "f {} {actor} {kind} {parameter} {work}", key_text(key));
            }
            Row::Intervention { key, digest, len } => {
                let _ = write!(out, "i {} {} {len}", key_text(key), hex(digest));
            }
        }
        out.push('\n');
    }
    out
}

fn key_of(text: &str) -> Option<EventKey> {
    let mut parts = text.split('.');
    let key = EventKey {
        tick: parts.next()?.parse().ok()?,
        microstep: parts.next()?.parse().ok()?,
        class: parts.next()?.parse().ok()?,
        tie: parts.next()?.parse().ok()?,
        origin: parts.next()?.parse().ok()?,
        seq: parts.next()?.parse().ok()?,
    };
    parts.next().is_none().then_some(key)
}

fn digest_of(text: &str) -> Option<Digest> {
    unhex(text)?.try_into().ok()
}

fn payload_of(text: &str) -> Option<Payload> {
    if let Some(d) = text.strip_prefix("d=") {
        return Some(Payload::Digest(digest_of(d)?));
    }
    let body = text.strip_prefix("n=")?;
    let mut parts = body.split('+');
    let rest = digest_of(parts.next()?)?;
    let mut fields = Vec::new();
    for f in parts {
        let mut p = f.split('|');
        let name = unescape(p.next()?)?;
        let unit = unescape(p.next()?)?;
        let factor = u64::from_str_radix(p.next()?, 16).ok()?;
        let value = u64::from_str_radix(p.next()?, 16).ok()?;
        if p.next().is_some() {
            return None;
        }
        fields.push(Number {
            name,
            unit,
            factor,
            value,
        });
    }
    Some(Payload::Numbers { fields, rest })
}

fn effect_of(token: &str) -> Option<Effect> {
    let mut p = token.splitn(5, ':');
    let kind = p.next()?;
    let seq: u64 = p.next()?.parse().ok()?;
    match kind {
        "s" | "r" => {
            let id: u32 = p.next()?.parse().ok()?;
            let len: u32 = p.next()?.parse().ok()?;
            let payload = payload_of(p.next()?)?;
            Some(if kind == "s" {
                Effect::Send {
                    seq,
                    port: id,
                    len,
                    payload,
                }
            } else {
                Effect::Report {
                    seq,
                    channel: id,
                    len,
                    payload,
                }
            })
        }
        "t" => {
            let timer = p.next()?.parse().ok()?;
            let delay = p.next()?.parse().ok()?;
            Some(Effect::SetTimer { seq, timer, delay })
        }
        "c" => {
            let timer = p.next()?.parse().ok()?;
            Some(Effect::CancelTimer { seq, timer })
        }
        _ => None,
    }
}

/// The rows of a results file, or the line that is not one. The syntax is frozen on both sides
/// (RFC 013 rule 21): a text the writer would not write for the rows it holds is refused, so a
/// carriage return, an upper-case hexadecimal digit, a sign or a leading zero on a number, or an
/// empty line at the end is not read as the file it resembles.
pub fn parse(text: &str) -> Result<Vec<Row>, String> {
    let rows = parse_lines(text)?;
    let written = write(&rows);
    if written != text {
        let (a, b) = (text.split('\n'), written.split('\n'));
        let n = a
            .clone()
            .zip(b.clone())
            .position(|(x, y)| x != y)
            .unwrap_or_else(|| a.count().min(b.count()));
        return Err(format!(
            "line {}: not the text the writer writes for it (a carriage return, an upper-case digit, a sign, a leading zero, or an empty line)",
            n + 1
        ));
    }
    Ok(rows)
}

fn parse_lines(text: &str) -> Result<Vec<Row>, String> {
    let mut lines = text.lines();
    if lines.next() != Some(FIRST_LINE) {
        return Err(format!("the first line is not `{FIRST_LINE}`"));
    }
    let mut rows = Vec::new();
    for (n, line) in lines.enumerate() {
        let bad = || format!("line {}: not a results line", n + 2);
        let mut words = line.split(' ');
        let row = match words.next() {
            Some("h") => {
                let key = key_of(words.next().ok_or_else(bad)?).ok_or_else(bad)?;
                let target = words.next().and_then(|w| w.parse().ok()).ok_or_else(bad)?;
                let work = words.next().and_then(|w| w.parse().ok()).ok_or_else(bad)?;
                let draws = words.next().and_then(|w| w.parse().ok()).ok_or_else(bad)?;
                let effects = words
                    .map(|w| effect_of(w).ok_or_else(bad))
                    .collect::<Result<Vec<_>, _>>()?;
                Row::Handled {
                    key,
                    target,
                    work,
                    draws,
                    effects,
                }
            }
            Some("f") => {
                let key = key_of(words.next().ok_or_else(bad)?).ok_or_else(bad)?;
                let mut number = || words.next().and_then(|w| w.parse::<u64>().ok());
                let (actor, kind, parameter, work) = (number(), number(), number(), number());
                match (actor, kind, parameter, work) {
                    (Some(a), Some(k), Some(p), Some(w)) => Row::Fault {
                        key,
                        actor: u32::try_from(a).map_err(|_| bad())?,
                        kind: u8::try_from(k).map_err(|_| bad())?,
                        parameter: u32::try_from(p).map_err(|_| bad())?,
                        work: w,
                    },
                    _ => return Err(bad()),
                }
            }
            Some("i") => {
                let key = key_of(words.next().ok_or_else(bad)?).ok_or_else(bad)?;
                let digest = digest_of(words.next().ok_or_else(bad)?).ok_or_else(bad)?;
                let len = words.next().and_then(|w| w.parse().ok()).ok_or_else(bad)?;
                Row::Intervention { key, digest, len }
            }
            _ => return Err(bad()),
        };
        rows.push(row);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests;
