//! The refusals of admission, as codes with typed parameters (RFC 007 §4.1 to §4.3).

use std::fmt;

pub use florn_model::Param;

/// What was refused and why. Every variant is one code of RFC 007 §4.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `module.invalid`: not a well-formed module under the admitted feature set.
    Invalid { reason: String },
    /// `module.too-large`: a count above the Host maximum; `what` is an `enum(module-count)`.
    TooLarge {
        what: &'static str,
        found: u64,
        maximum: u64,
    },
    /// `module.feature-refused`: a proposal outside RFC 003 rule 5.
    FeatureRefused { feature: &'static str },
    /// `module.memory`: `reason` is an `enum(memory-rule)`: count, imported, shared, 64-bit, not-exported.
    Memory { reason: &'static str },
    /// `module.start-section`.
    StartSection,
    /// `module.table-mutation`.
    TableMutation { instruction: &'static str },
    /// `module.import-refused`: an import outside the world and the Host's interfaces.
    ImportRefused { module: String, name: String },
    /// `module.export-missing`: a required export absent or of the wrong type.
    ExportMissing { name: String },
    /// `module.export-reserved`: an export under `florn.` (RFC 003 rule 8).
    ExportReserved { name: String },
    /// `module.export-unsupported`: a function of the world whose flat shape no Host can call or
    /// bind (RFC 008 rule 6).
    ExportUnsupported { name: String },
    /// `module.initial-memory-above-limit`.
    InitialMemoryAboveLimit { pages: u64, limit: u64 },
    /// `module.manifest-missing`: `section` is an `enum(manifest-section)`: contract-identity, florn-manifest.
    ManifestMissing { section: &'static str },
    /// `module.manifest-schema`.
    ManifestSchema { reason: String },
    /// `module.contract-unsupported`: the world is not one this Host supports.
    ContractUnsupported { declared: String, supported: String },
    /// `module.contract-mismatch`: the manifest, the contract identity, or the scenario name different worlds.
    ContractMismatch {
        module: String,
        declared: String,
        found: String,
    },
    /// `module.port-type-unknown`.
    PortTypeUnknown {
        actor: u32,
        port: String,
        ty: String,
    },
    /// `module.port-count`.
    PortCount {
        actor: u32,
        kind: String,
        found: u64,
        min: u64,
        max: u64,
    },
    /// `limit.below-need`.
    LimitBelowNeed {
        actor: u32,
        limit: &'static str,
        need: u64,
        value: u64,
    },
    /// `admission.transform-failed`: a defect of florn, never an expected outcome.
    TransformFailed { reason: String },
    /// `module.segment-too-long` (RFC 010 rule 3): a function whose longest check-free path
    /// exceeds the class's budget, so that an activation could run past the budget by more
    /// than one path.
    SegmentTooLong {
        function: u32,
        cost: u64,
        budget: u64,
    },
    /// `host.capacity-exceeded`: `what` is an `enum(capacity-kind)`: instances, memory.
    CapacityExceeded {
        need: u64,
        capacity: u64,
        what: &'static str,
    },
    /// `config.missing-parameter`.
    ConfigMissingParameter { actor: u32, parameter: String },
    /// `config.unknown-parameter`.
    ConfigUnknownParameter { actor: u32, parameter: String },
    /// `config.type`: `expected` is an `enum(parameter-type)`.
    ConfigType {
        actor: u32,
        parameter: String,
        expected: &'static str,
    },
    /// A code of the core model (quantities, references, hashes).
    Model(Box<florn_model::Kind>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub kind: Kind,
    /// The field path, when the error has one: `actors[2].ports[0].type`, `params[1].default`.
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
            Invalid { .. } => "module.invalid",
            TooLarge { .. } => "module.too-large",
            FeatureRefused { .. } => "module.feature-refused",
            Memory { .. } => "module.memory",
            StartSection => "module.start-section",
            TableMutation { .. } => "module.table-mutation",
            ImportRefused { .. } => "module.import-refused",
            ExportMissing { .. } => "module.export-missing",
            ExportReserved { .. } => "module.export-reserved",
            ExportUnsupported { .. } => "module.export-unsupported",
            InitialMemoryAboveLimit { .. } => "module.initial-memory-above-limit",
            ManifestMissing { .. } => "module.manifest-missing",
            ManifestSchema { .. } => "module.manifest-schema",
            ContractUnsupported { .. } => "module.contract-unsupported",
            ContractMismatch { .. } => "module.contract-mismatch",
            PortTypeUnknown { .. } => "module.port-type-unknown",
            PortCount { .. } => "module.port-count",
            LimitBelowNeed { .. } => "limit.below-need",
            TransformFailed { .. } => "admission.transform-failed",
            SegmentTooLong { .. } => "module.segment-too-long",
            CapacityExceeded { .. } => "host.capacity-exceeded",
            ConfigMissingParameter { .. } => "config.missing-parameter",
            ConfigUnknownParameter { .. } => "config.unknown-parameter",
            ConfigType { .. } => "config.type",
            Model(k) => florn_model::Error::new((**k).clone()).code(),
        }
    }

    /// The typed parameters, in the order of the catalogue.
    pub fn params(&self) -> Vec<(&'static str, Param)> {
        use Kind::*;
        use Param::*;
        match &self.kind {
            Invalid { reason } | ManifestSchema { reason } | TransformFailed { reason } => {
                vec![("reason", Text(reason.clone()))]
            }
            SegmentTooLong {
                function,
                cost,
                budget,
            } => vec![
                ("function", Text(function.to_string())),
                ("cost", Count(*cost)),
                ("budget", Count(*budget)),
            ],
            TooLarge {
                what,
                found,
                maximum,
            } => vec![
                ("what", Enum(what)),
                ("found", Count(*found)),
                ("maximum", Count(*maximum)),
            ],
            FeatureRefused { feature } => vec![("feature", Name(feature.to_string()))],
            Memory { reason } => vec![("reason", Enum(reason))],
            StartSection => vec![],
            TableMutation { instruction } => vec![("instruction", Name(instruction.to_string()))],
            ImportRefused { module, name } => vec![
                ("module", Name(module.clone())),
                ("name", Name(name.clone())),
            ],
            ExportMissing { name } | ExportReserved { name } | ExportUnsupported { name } => {
                vec![("name", Name(name.clone()))]
            }
            InitialMemoryAboveLimit { pages, limit } => {
                vec![("pages", Count(*pages)), ("limit", Count(*limit))]
            }
            ManifestMissing { section } => vec![("section", Enum(section))],
            ContractUnsupported {
                declared,
                supported,
            } => vec![
                ("declared", Text(declared.clone())),
                ("supported", Text(supported.clone())),
            ],
            ContractMismatch {
                module,
                declared,
                found,
            } => vec![
                ("module", Name(module.clone())),
                ("declared", Text(declared.clone())),
                ("found", Text(found.clone())),
            ],
            PortTypeUnknown { actor, port, ty } => vec![
                ("actor", Identity(*actor)),
                ("port", Name(port.clone())),
                ("type", Text(ty.clone())),
            ],
            PortCount {
                actor,
                kind,
                found,
                min,
                max,
            } => vec![
                ("actor", Identity(*actor)),
                ("kind", Name(kind.clone())),
                ("found", Count(*found)),
                ("min", Count(*min)),
                ("max", Count(*max)),
            ],
            LimitBelowNeed {
                actor,
                limit,
                need,
                value,
            } => vec![
                ("actor", Identity(*actor)),
                ("limit", Name(limit.to_string())),
                ("need", Count(*need)),
                ("value", Count(*value)),
            ],
            CapacityExceeded {
                need,
                capacity,
                what,
            } => vec![
                ("need", Count(*need)),
                ("capacity", Count(*capacity)),
                ("what", Enum(what)),
            ],
            ConfigMissingParameter { actor, parameter }
            | ConfigUnknownParameter { actor, parameter } => vec![
                ("actor", Identity(*actor)),
                ("parameter", Name(parameter.clone())),
            ],
            ConfigType {
                actor,
                parameter,
                expected,
            } => vec![
                ("actor", Identity(*actor)),
                ("parameter", Name(parameter.clone())),
                ("expected", Enum(expected)),
            ],
            Model(k) => florn_model::Error::new((**k).clone()).params(),
        }
    }
}

impl From<florn_model::Error> for Error {
    fn from(e: florn_model::Error) -> Self {
        Self {
            kind: Kind::Model(Box::new(e.kind)),
            field: e.field,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code())?;
        for (k, v) in self.params() {
            write!(f, " {k}={v:?}")?;
        }
        if let Some(field) = &self.field {
            write!(f, " at {field}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {}

/// Every error of one stage, reported together (RFC 003 rule 2).
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

impl From<Error> for Errors {
    fn from(e: Error) -> Self {
        Errors(vec![e])
    }
}

impl From<florn_model::Errors> for Errors {
    fn from(e: florn_model::Errors) -> Self {
        Errors(e.0.into_iter().map(Error::from).collect())
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
