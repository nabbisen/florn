//! A notification (RFC 007 rule 3): what the Host sends to a front end for a person and writes to
//! the diagnostic log. A code, typed parameters, a severity, a scope, what it ends, and the instant
//! it concerns. The Host formats nothing; the front end's catalogue renders it.

use std::fmt::Write as _;

use florn_model::Param;
use florn_model::canonical::Encoder;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// What a notification ends (RFC 007 rule 11).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ends {
    Nothing,
    Command,
    Activation,
    Run,
}

impl Ends {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Nothing => "nothing",
            Self::Command => "command",
            Self::Activation => "activation",
            Self::Run => "run",
        }
    }
}

/// An instant of the logical world: a tick and a microstep.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct At {
    pub tick: u64,
    pub microstep: u32,
}

/// Where a notification applies (RFC 007 rule 3): every reference optional.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scope {
    pub run: bool,
    pub actor: Option<u32>,
    pub connection: Option<u32>,
    pub port: Option<u32>,
    /// A path in a manifest.
    pub field: Option<String>,
    pub file: Option<String>,
    pub command: Option<String>,
}

impl Scope {
    pub fn run() -> Self {
        Self {
            run: true,
            ..Default::default()
        }
    }
    pub fn actor(id: u32) -> Self {
        Self {
            run: true,
            actor: Some(id),
            ..Default::default()
        }
    }
    pub fn field(path: impl Into<String>) -> Self {
        Self {
            field: Some(path.into()),
            ..Default::default()
        }
    }
    pub fn file(name: impl Into<String>) -> Self {
        Self {
            file: Some(name.into()),
            ..Default::default()
        }
    }
    pub fn command(name: impl Into<String>) -> Self {
        Self {
            command: Some(name.into()),
            ..Default::default()
        }
    }
}

/// A typed parameter value (RFC 007 rule 6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Identity(u32),
    Name(String),
    Count(u64),
    Bytes(u64),
    /// Number text and unit text.
    Quantity(String, String),
    Tick(u64),
    Instant(At),
    /// Another code.
    Code(String),
    Text(String),
    /// A value of a closed set the code declares.
    Enum(String),
}

impl From<Param> for Value {
    fn from(p: Param) -> Self {
        match p {
            Param::Identity(i) => Value::Identity(i),
            Param::Name(n) => Value::Name(n),
            Param::Count(c) => Value::Count(c),
            Param::Bytes(b) => Value::Bytes(b),
            Param::Quantity(n, u) => Value::Quantity(n, u),
            Param::Tick(t) => Value::Tick(t),
            Param::Enum(e) => Value::Enum(e.to_string()),
            Param::Text(t) => Value::Text(t),
        }
    }
}

impl Value {
    /// The type's name, as rule 6 lists them.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Identity(_) => "identity",
            Value::Name(_) => "name",
            Value::Count(_) => "count",
            Value::Bytes(_) => "bytes",
            Value::Quantity(..) => "quantity",
            Value::Tick(_) => "tick",
            Value::Instant(_) => "instant",
            Value::Code(_) => "code",
            Value::Text(_) => "text",
            Value::Enum(_) => "enum",
        }
    }
}

/// The notification of RFC 007 rule 3.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification {
    pub code: String,
    pub severity: Severity,
    pub scope: Scope,
    /// In the catalogue's order.
    pub params: Vec<(String, Value)>,
    pub ends: Ends,
    pub at: Option<At>,
}

impl Notification {
    pub fn new(code: &str, severity: Severity, scope: Scope, ends: Ends) -> Self {
        Self {
            code: code.to_string(),
            severity,
            scope,
            params: Vec::new(),
            ends,
            at: None,
        }
    }
    pub fn with(mut self, name: &str, value: Value) -> Self {
        self.params.push((name.to_string(), value));
        self
    }
    pub fn at(mut self, at: At) -> Self {
        self.at = Some(at);
        self
    }
    /// A notification from a coded error of the model or admission: `error` severity, scope by the
    /// field when there is one, otherwise the file.
    pub fn from_error(
        code: &str,
        params: Vec<(&'static str, Param)>,
        field: Option<String>,
        file: Option<&str>,
        ends: Ends,
    ) -> Self {
        let scope = match (field, file) {
            (Some(f), _) => Scope::field(f),
            (None, Some(f)) => Scope::file(f),
            (None, None) => Scope::default(),
        };
        Self {
            code: code.to_string(),
            severity: Severity::Error,
            scope,
            params: params
                .into_iter()
                .map(|(n, p)| (n.to_string(), Value::from(p)))
                .collect(),
            ends,
            at: None,
        }
    }

    /// The canonical encoding (RFC 004 §5 applied to the fields of rule 3, in order; parameters as
    /// a map sorted by name, each value tagged by its type).
    pub fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::default();
        e.text(&self.code);
        e.text(self.severity.as_str());
        e.u8(self.scope.run as u8);
        e.option(self.scope.actor, Encoder::u32);
        e.option(self.scope.connection, Encoder::u32);
        e.option(self.scope.port, Encoder::u32);
        e.option(self.scope.field.as_deref(), Encoder::text);
        e.option(self.scope.file.as_deref(), Encoder::text);
        e.option(self.scope.command.as_deref(), Encoder::text);
        let mut params: Vec<&(String, Value)> = self.params.iter().collect();
        params.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        e.u32(params.len() as u32);
        for (name, v) in params {
            e.text(name);
            e.text(v.type_name());
            match v {
                Value::Identity(i) => e.u32(*i),
                Value::Name(s) | Value::Code(s) | Value::Text(s) | Value::Enum(s) => e.text(s),
                Value::Count(c) | Value::Bytes(c) | Value::Tick(c) => e.u64(*c),
                Value::Quantity(n, u) => e.quantity(n, u),
                Value::Instant(at) => {
                    e.u64(at.tick);
                    e.u32(at.microstep);
                }
            }
        }
        e.text(self.ends.as_str());
        e.option(self.at, |e, at| {
            e.u64(at.tick);
            e.u32(at.microstep);
        });
        e.finish()
    }

    /// One JSON object, for the exported diagnostic log (RFC 007 rule 12).
    pub fn to_json(&self) -> String {
        let mut s = String::from("{");
        let _ = write!(s, "\"code\":{}", json_str(&self.code));
        let _ = write!(s, ",\"severity\":\"{}\"", self.severity.as_str());
        s.push_str(",\"scope\":{");
        let mut first = true;
        let mut put = |s: &mut String, k: &str, v: String| {
            if !first {
                s.push(',');
            }
            first = false;
            let _ = write!(s, "\"{k}\":{v}");
        };
        if self.scope.run {
            put(&mut s, "run", "true".into());
        }
        if let Some(a) = self.scope.actor {
            put(&mut s, "actor", a.to_string());
        }
        if let Some(c) = self.scope.connection {
            put(&mut s, "connection", c.to_string());
        }
        if let Some(p) = self.scope.port {
            put(&mut s, "port", p.to_string());
        }
        if let Some(f) = &self.scope.field {
            put(&mut s, "field", json_str(f));
        }
        if let Some(f) = &self.scope.file {
            put(&mut s, "file", json_str(f));
        }
        if let Some(c) = &self.scope.command {
            put(&mut s, "command", json_str(c));
        }
        s.push_str("},\"params\":{");
        for (i, (name, v)) in self.params.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            let value = match v {
                Value::Identity(i) => i.to_string(),
                Value::Count(c) | Value::Bytes(c) | Value::Tick(c) => c.to_string(),
                Value::Name(t) | Value::Code(t) | Value::Text(t) | Value::Enum(t) => json_str(t),
                Value::Quantity(n, u) => {
                    format!("{{\"number\":{},\"unit\":{}}}", json_str(n), json_str(u))
                }
                Value::Instant(at) => {
                    format!("{{\"tick\":{},\"microstep\":{}}}", at.tick, at.microstep)
                }
            };
            let _ = write!(
                s,
                "{}:{{\"type\":\"{}\",\"value\":{}}}",
                json_str(name),
                v.type_name(),
                value
            );
        }
        let _ = write!(s, "}},\"ends\":\"{}\"", self.ends.as_str());
        if let Some(at) = self.at {
            let _ = write!(
                s,
                ",\"at\":{{\"tick\":{},\"microstep\":{}}}",
                at.tick, at.microstep
            );
        }
        s.push('}');
        s
    }
}

/// A JSON string literal.
pub fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_is_canonical_and_json_is_one_line() {
        let a = Notification::new(
            "actor.faulted",
            Severity::Error,
            Scope::actor(3),
            Ends::Activation,
        )
        .with("actor", Value::Identity(3))
        .with("fault", Value::Code("fault.budget".into()))
        .with("metered", Value::Count(10_000_000))
        .with("policy", Value::Enum("drop".into()))
        .at(At {
            tick: 12,
            microstep: 1,
        });
        let mut b = a.clone();
        b.params.reverse();
        assert_eq!(
            a.encode(),
            b.encode(),
            "parameter order does not change the encoding"
        );
        let j = a.to_json();
        assert!(!j.contains('\n'));
        assert!(j.starts_with("{\"code\":\"actor.faulted\",\"severity\":\"error\",\"scope\":{\"run\":true,\"actor\":3},\"params\":{\"actor\":{\"type\":\"identity\",\"value\":3}"));
        assert!(j.ends_with("\"ends\":\"activation\",\"at\":{\"tick\":12,\"microstep\":1}}"));
        assert_eq!(json_str("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
    }
}
