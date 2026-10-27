//! The inspectors (external design §3.6; OBS-005, OBS-006): what the selected element shows.
//! An actor's configuration as declared, its status and faults, the latest report on each of
//! its channels decoded by the layout its module declares; a connection's attributes and what
//! is in flight on it; a message's bytes and the fields a domain's decoder gives; the world's
//! events. Nothing here reads a guest or calls one (DES-012): every value came through the
//! control interface.
use florn_control::message::{
    ActorSummary, ConnectionSummary, FaultInfo, ModuleSummary, SendSummary,
};
use florn_control::{At, Value};

use crate::fields::{Field, Kind};

/// The latest report on one channel, decoded by the module's layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportView {
    pub channel: u32,
    pub name: String,
    pub at: At,
    pub fields: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorInspector {
    pub summary: ActorSummary,
    pub module: Option<ModuleSummary>,
    pub alive: bool,
    pub down: bool,
    pub faults: Vec<FaultInfo>,
    /// `[actors.config]`, one field per key the manifest sets or the module declares.
    pub config: Vec<Field>,
    pub reports: Vec<ReportView>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionInspector {
    pub summary: ConnectionSummary,
    pub up: bool,
    pub attrs: Vec<Field>,
    pub in_flight: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageInspector {
    /// The timeline entry and the send within it.
    pub entry: usize,
    pub send: SendSummary,
    pub origin: u32,
    pub at: At,
    /// The protocol the origin's module declares, when its domain decodes one.
    pub protocol: Option<String>,
    /// The raw bytes, once the Host answered; `None` when the run keeps no payloads.
    pub payload: Option<Vec<u8>>,
    pub decoded: Option<Vec<(String, String)>>,
    /// An answer is still to come.
    pub pending: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorldInspector {
    pub domains: Vec<String>,
    pub classes: Vec<(u8, String)>,
    /// The domains' events so far: (at, target, event, detail).
    pub events: Vec<(At, u32, String, String)>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Inspector {
    #[default]
    None,
    /// Boxed: an actor's inspector is the large one.
    Actor(Box<ActorInspector>),
    Connection(ConnectionInspector),
    Message(MessageInspector),
    World(WorldInspector),
}

/// The fields of an actor's configuration: the module's parameters first, in their order,
/// each with the manifest's value or the declared default, then any key the manifest sets
/// beyond them.
pub fn config_fields(summary: &ActorSummary, module: Option<&ModuleSummary>) -> Vec<Field> {
    let mut fields = Vec::new();
    if let Some(m) = module {
        for p in &m.params {
            let value = summary
                .config
                .iter()
                .find(|(k, _)| *k == p.name)
                .map(|(_, v)| v.clone())
                .or_else(|| p.default.clone())
                .unwrap_or_default();
            let kind = kind_of(&p.ty);
            fields.push(Field::new(&p.name, kind, &value).with_unit(p.unit.clone()));
        }
    }
    for (k, v) in &summary.config {
        if !fields.iter().any(|f| f.name == *k) {
            fields.push(Field::new(k, kind_of_text(v), v));
        }
    }
    fields
}

/// The fields of a connection's attributes, as the manifest sets them.
pub fn attr_fields(summary: &ConnectionSummary) -> Vec<Field> {
    summary
        .attrs
        .iter()
        .map(|(k, v)| Field::new(k, kind_of_text(v), v))
        .collect()
}

/// The field kind of a declared parameter type (RFC 002 rule 13).
pub fn kind_of(ty: &str) -> Kind {
    match ty {
        "bool" => Kind::Boolean,
        "u64" | "s64" => Kind::Integer,
        "f64" => Kind::Decimal,
        "ticks" | "integer" | "real" => Kind::Quantity,
        _ => Kind::Text,
    }
}

/// The field kind a value's text suggests, for keys without a declaration.
pub fn kind_of_text(v: &str) -> Kind {
    if v == "true" || v == "false" {
        Kind::Boolean
    } else if v.parse::<i64>().is_ok() {
        Kind::Integer
    } else if v.parse::<f64>().is_ok()
        && v.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '.')
    {
        Kind::Decimal
    } else if v.starts_with(|c: char| c.is_ascii_digit()) {
        Kind::Quantity
    } else {
        Kind::Text
    }
}

/// Decode a report by its channel's layout (RFC 002 rule 16; the field types of rule 13 as the
/// SDK encodes them): `bool` one byte, `u64` and `s64` eight bytes little-endian, `f64` its
/// bits, `text` and `bytes` a length then the bytes. A payload shorter than the layout gives
/// the fields it holds.
pub fn decode_report(layout: &[(String, String)], payload: &[u8]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut at = 0usize;
    let take = |at: &mut usize, n: usize| -> Option<&[u8]> {
        let s = payload.get(*at..*at + n)?;
        *at += n;
        Some(s)
    };
    for (name, ty) in layout {
        let text = match ty.as_str() {
            "bool" => take(&mut at, 1).map(|b| (b[0] != 0).to_string()),
            "u64" => take(&mut at, 8)
                .map(|b| u64::from_le_bytes(b.try_into().expect("eight bytes")).to_string()),
            "s64" => take(&mut at, 8)
                .map(|b| i64::from_le_bytes(b.try_into().expect("eight bytes")).to_string()),
            "f64" => take(&mut at, 8).map(|b| {
                f64::from_bits(u64::from_le_bytes(b.try_into().expect("eight bytes"))).to_string()
            }),
            "text" | "bytes" => take(&mut at, 4).and_then(|l| {
                let n = u32::from_le_bytes(l.try_into().expect("four bytes")) as usize;
                let b = take(&mut at, n)?;
                Some(if ty == "text" {
                    String::from_utf8_lossy(b).into_owned()
                } else {
                    b.iter().map(|x| format!("{x:02x}")).collect()
                })
            }),
            _ => None,
        };
        match text {
            Some(t) => out.push((name.clone(), t)),
            None => break,
        }
    }
    out
}

/// A parameter of a fault notification as text, for the inspector's fault list.
pub fn param_text(v: &Value) -> String {
    crate::timeline::value_text(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_route_report_decodes_by_its_layout() {
        let layout = vec![
            ("prefix".to_string(), "text".to_string()),
            ("next-hop".to_string(), "u64".to_string()),
            ("metric".to_string(), "u64".to_string()),
            ("action".to_string(), "text".to_string()),
        ];
        let mut p = Vec::new();
        p.extend_from_slice(&2u32.to_le_bytes());
        p.extend_from_slice(b"r3");
        p.extend_from_slice(&2u64.to_le_bytes());
        p.extend_from_slice(&1u64.to_le_bytes());
        p.extend_from_slice(&5u32.to_le_bytes());
        p.extend_from_slice(b"added");
        assert_eq!(
            decode_report(&layout, &p),
            vec![
                ("prefix".into(), "r3".into()),
                ("next-hop".into(), "2".into()),
                ("metric".into(), "1".into()),
                ("action".into(), "added".into()),
            ]
        );
        // Short: what it holds.
        assert_eq!(decode_report(&layout, &p[..14]).len(), 2);
    }

    #[test]
    fn config_fields_follow_the_declaration() {
        use florn_control::message::ParamSummary;
        let summary = ActorSummary {
            id: 1,
            name: "r1".into(),
            module: Some("dv".into()),
            class: "default".into(),
            ports: Vec::new(),
            config: vec![
                ("period".into(), "100ns".into()),
                ("note".into(), "x".into()),
            ],
        };
        let module = ModuleSummary {
            name: "dv".into(),
            source_sha256: String::new(),
            contract: String::new(),
            kind: "router".into(),
            description: None,
            port_kinds: Vec::new(),
            params: vec![
                ParamSummary {
                    name: "period".into(),
                    ty: "ticks".into(),
                    required: false,
                    default: Some("30s".into()),
                    unit: Some("s".into()),
                },
                ParamSummary {
                    name: "infinity".into(),
                    ty: "u64".into(),
                    required: false,
                    default: Some("16".into()),
                    unit: None,
                },
            ],
            channels: Vec::new(),
            needs: Vec::new(),
            tables: Vec::new(),
        };
        let f = config_fields(&summary, Some(&module));
        let names: Vec<&str> = f.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["period", "infinity", "note"]);
        assert_eq!(f[0].committed, "100ns");
        assert_eq!(f[0].kind, Kind::Quantity);
        assert_eq!(f[1].committed, "16");
        assert_eq!(f[1].kind, Kind::Integer);
        assert_eq!(f[2].kind, Kind::Text);
    }
}
