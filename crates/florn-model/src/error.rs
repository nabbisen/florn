//! Validation errors: a stable code with typed parameters (RFC 004 rule 38, RFC 007 §4.1).
//! No text: the front end renders the code from its catalogue; `florn-control` carries these
//! into the notification type of RFC 007 rule 3.

use std::fmt;

/// A typed parameter of a code (RFC 007 rule 6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Param {
    Identity(u32),
    Name(String),
    Count(u64),
    Bytes(u64),
    /// A quantity as written: the number text and the unit.
    Quantity(String, String),
    Tick(u64),
    /// A closed enumeration value, by its name.
    Enum(&'static str),
    /// Opaque text: a field path, a file name, a parser's reason.
    Text(String),
}

impl fmt::Display for Param {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Param::Identity(i) => write!(f, "{i}"),
            Param::Name(n) | Param::Text(n) => write!(f, "{n}"),
            Param::Count(c) => write!(f, "{c}"),
            Param::Bytes(b) => write!(f, "{b}B"),
            Param::Quantity(n, u) => write!(f, "{n}{u}"),
            Param::Tick(t) => write!(f, "tick {t}"),
            Param::Enum(e) => write!(f, "{e}"),
        }
    }
}

/// The codes this crate produces (RFC 007 §4.1 and §4.4). Each variant's fields are its parameters,
/// in the catalogue's order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `manifest.too-large`: the text exceeds the maximum, or a list exceeds its maximum.
    ManifestTooLarge {
        what: &'static str,
        found: u64,
        maximum: u64,
    },
    /// `manifest.invalid`: the text is not TOML.
    ManifestInvalid {
        reason: String,
    },
    ManifestSchemaVersion {
        found: u64,
        supported: u64,
    },
    ManifestUnknownKey {
        key: String,
    },
    ManifestMissingField {
        field: String,
    },
    /// `manifest.type`: a field has the wrong type.
    ManifestType {
        field: String,
        expected: &'static str,
    },
    IdentityDuplicate {
        identity: u32,
        first: String,
        second: String,
    },
    IdentityReserved {
        identity: u32,
    },
    IdentityPortDuplicate {
        actor: u32,
        port: u32,
    },
    NameDuplicate {
        name: String,
        kind: &'static str,
    },
    ReferenceUnresolved {
        reference: String,
        kind: &'static str,
    },
    DomainUndeclared {
        domain: String,
        where_: String,
    },
    ModuleHashMismatch {
        module: String,
        declared: String,
        actual: String,
    },
    ConnectionTooFewPorts {
        connection: u32,
        ports: u64,
    },
    /// `connection.mixed-types`: ports or a type of two domains on one connection (RFC 008 rule 2).
    ConnectionMixedTypes {
        connection: String,
    },
    QuantityMissingUnit {
        field: String,
        dimension: &'static str,
    },
    QuantityNotWholeTicks {
        field: String,
        value: (String, String),
        tick: (String, String),
        smallest: (String, String),
    },
    QuantityNotWholeUnit {
        field: String,
        value: (String, String),
        unit: String,
    },
    QuantityUnknownUnit {
        field: String,
        unit: String,
    },
    /// `quantity.out-of-range`: the value does not fit the field's range (for example an `end` beyond the largest tick).
    QuantityOutOfRange {
        field: String,
        value: (String, String),
    },
    LimitAboveMaximum {
        limit: String,
        value: u64,
        maximum: u64,
    },
    TimeEndBeforeChange {
        change: String,
        at: u64,
        end: u64,
    },
    PackagePartial {
        missing: u64,
    },
    PackageEntryMismatch {
        entry: String,
        declared: String,
        actual: String,
    },
    FileUnreadable {
        kind: &'static str,
        reason: String,
    },
}

/// One validation error: its code, its parameters, and the manifest field it concerns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub kind: Kind,
    /// The field path in the manifest (`actors[2].ports[0].type`), when the error has one.
    pub field: Option<String>,
}

impl Error {
    pub fn new(kind: Kind) -> Self {
        Self { kind, field: None }
    }
    pub fn at(kind: Kind, field: impl Into<String>) -> Self {
        Self {
            kind,
            field: Some(field.into()),
        }
    }
    /// The code of RFC 007 §4.
    pub fn code(&self) -> &'static str {
        use Kind::*;
        match &self.kind {
            ManifestTooLarge { .. } => "manifest.too-large",
            ManifestInvalid { .. } => "manifest.invalid",
            ManifestSchemaVersion { .. } => "manifest.schema-version",
            ManifestUnknownKey { .. } => "manifest.unknown-key",
            ManifestMissingField { .. } => "manifest.missing-field",
            ManifestType { .. } => "manifest.type",
            IdentityDuplicate { .. } => "identity.duplicate",
            IdentityReserved { .. } => "identity.reserved",
            IdentityPortDuplicate { .. } => "identity.port-duplicate",
            NameDuplicate { .. } => "name.duplicate",
            ReferenceUnresolved { .. } => "reference.unresolved",
            DomainUndeclared { .. } => "domain.undeclared",
            ModuleHashMismatch { .. } => "module.hash-mismatch",
            ConnectionTooFewPorts { .. } => "connection.too-few-ports",
            ConnectionMixedTypes { .. } => "connection.mixed-types",
            QuantityMissingUnit { .. } => "quantity.missing-unit",
            QuantityNotWholeTicks { .. } => "quantity.not-whole-ticks",
            QuantityNotWholeUnit { .. } => "quantity.not-whole-unit",
            QuantityUnknownUnit { .. } => "quantity.unknown-unit",
            QuantityOutOfRange { .. } => "quantity.out-of-range",
            LimitAboveMaximum { .. } => "limit.above-maximum",
            TimeEndBeforeChange { .. } => "time.end-before-change",
            PackagePartial { .. } => "package.partial",
            PackageEntryMismatch { .. } => "package.entry-mismatch",
            FileUnreadable { .. } => "file.unreadable",
        }
    }
    /// The parameters in the catalogue's order (RFC 007 §4.1), as (name, value).
    pub fn params(&self) -> Vec<(&'static str, Param)> {
        use Kind::*;
        use Param::*;
        let q = |(n, u): &(String, String)| Quantity(n.clone(), u.clone());
        match &self.kind {
            ManifestTooLarge {
                what,
                found,
                maximum,
            } => vec![
                ("what", Enum(what)),
                ("found", Count(*found)),
                ("maximum", Count(*maximum)),
            ],
            ManifestInvalid { reason } => vec![("reason", Text(reason.clone()))],
            ManifestSchemaVersion { found, supported } => {
                vec![("found", Count(*found)), ("supported", Count(*supported))]
            }
            ManifestUnknownKey { key } => vec![("key", Text(key.clone()))],
            ManifestMissingField { field } => vec![("field", Text(field.clone()))],
            ManifestType { field, expected } => {
                vec![("field", Text(field.clone())), ("expected", Enum(expected))]
            }
            IdentityDuplicate {
                identity,
                first,
                second,
            } => vec![
                ("identity", Identity(*identity)),
                ("first", Name(first.clone())),
                ("second", Name(second.clone())),
            ],
            IdentityReserved { identity } => vec![("identity", Identity(*identity))],
            IdentityPortDuplicate { actor, port } => {
                vec![("actor", Identity(*actor)), ("port", Identity(*port))]
            }
            NameDuplicate { name, kind } => {
                vec![("name", Name(name.clone())), ("kind", Enum(kind))]
            }
            ReferenceUnresolved { reference, kind } => {
                vec![("reference", Text(reference.clone())), ("kind", Enum(kind))]
            }
            DomainUndeclared { domain, where_ } => vec![
                ("domain", Name(domain.clone())),
                ("where", Text(where_.clone())),
            ],
            ModuleHashMismatch {
                module,
                declared,
                actual,
            } => vec![
                ("module", Name(module.clone())),
                ("declared", Text(declared.clone())),
                ("actual", Text(actual.clone())),
            ],
            ConnectionTooFewPorts { connection, ports } => vec![
                ("connection", Identity(*connection)),
                ("ports", Count(*ports)),
            ],
            ConnectionMixedTypes { connection } => vec![("connection", Name(connection.clone()))],
            QuantityMissingUnit { field, dimension } => vec![
                ("field", Text(field.clone())),
                ("dimension", Enum(dimension)),
            ],
            QuantityNotWholeTicks {
                field,
                value,
                tick,
                smallest,
            } => vec![
                ("field", Text(field.clone())),
                ("value", q(value)),
                ("tick", q(tick)),
                ("smallest", q(smallest)),
            ],
            QuantityNotWholeUnit { field, value, unit } => vec![
                ("field", Text(field.clone())),
                ("value", q(value)),
                ("unit", Text(unit.clone())),
            ],
            QuantityUnknownUnit { field, unit } => {
                vec![("field", Text(field.clone())), ("unit", Text(unit.clone()))]
            }
            QuantityOutOfRange { field, value } => {
                vec![("field", Text(field.clone())), ("value", q(value))]
            }
            LimitAboveMaximum {
                limit,
                value,
                maximum,
            } => vec![
                ("limit", Name(limit.clone())),
                ("value", Count(*value)),
                ("maximum", Count(*maximum)),
            ],
            TimeEndBeforeChange { change, at, end } => vec![
                ("change", Text(change.clone())),
                ("at", Tick(*at)),
                ("end", Tick(*end)),
            ],
            PackagePartial { missing } => vec![("missing", Count(*missing))],
            PackageEntryMismatch {
                entry,
                declared,
                actual,
            } => vec![
                ("entry", Text(entry.clone())),
                ("declared", Text(declared.clone())),
                ("actual", Text(actual.clone())),
            ],
            FileUnreadable { kind, reason } => {
                vec![("kind", Enum(kind)), ("reason", Text(reason.clone()))]
            }
        }
    }
}

impl fmt::Display for Error {
    /// Diagnostic form only: code, field, parameters. Not for people (RFC 007 rule 8).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code())?;
        if let Some(field) = &self.field {
            write!(f, " at {field}")?;
        }
        for (name, value) in self.params() {
            write!(f, " {name}={value}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {}

/// Every error found in one pass (RFC 004 rule 38).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Errors(pub Vec<Error>);

impl Errors {
    pub fn push(&mut self, e: Error) {
        self.0.push(e);
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn codes(&self) -> Vec<&'static str> {
        self.0.iter().map(Error::code).collect()
    }
    pub fn into_result<T>(self, value: T) -> Result<T, Errors> {
        if self.is_empty() {
            Ok(value)
        } else {
            Err(self)
        }
    }
}

impl fmt::Display for Errors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, e) in self.0.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{e}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Errors {}
