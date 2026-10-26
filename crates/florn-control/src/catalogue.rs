//! The catalogue of codes (RFC 007 §4): every code with its parameters typed, its severity, its
//! scope, and what it ends; the closed enumerations of §4.5; the fault codes of rule 2. A test
//! keeps this file equal to the RFC's tables. No text: the front end's catalogue renders.

use crate::notification::{Ends, Notification, Severity, Value};

/// A parameter's type (RFC 007 rule 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamType {
    Identity,
    Name,
    Count,
    Bytes,
    Quantity,
    Tick,
    Instant,
    Code,
    Text,
    /// A closed set of §4.5, by name.
    Enum(&'static str),
}

/// Where a code applies, as the catalogue lists it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopeKind {
    File,
    Field,
    Run,
    Actor,
    Command,
    None,
}

/// One row of the catalogue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub code: Code,
    pub name: &'static str,
    pub params: &'static [(&'static str, ParamType)],
    pub severity: Severity,
    pub scope: ScopeKind,
    pub ends: Ends,
}

/// Every code of RFC 007 §4, one variant each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Code {
    ManifestTooLarge,
    ManifestInvalid,
    ManifestSchemaVersion,
    ManifestType,
    ManifestUnknownKey,
    ManifestMissingField,
    IdentityDuplicate,
    IdentityReserved,
    IdentityPortDuplicate,
    NameDuplicate,
    ReferenceUnresolved,
    DomainUndeclared,
    ModuleHashMismatch,
    ModuleContractMismatch,
    ConnectionTooFewPorts,
    QuantityMissingUnit,
    QuantityNotWholeTicks,
    QuantityNotWholeUnit,
    QuantityUnknownUnit,
    QuantityOutOfRange,
    LimitAboveMaximum,
    TimeEndBeforeChange,
    ConfigMissingParameter,
    ConfigUnknownParameter,
    ConfigType,
    ConnectionMixedTypes,
    DomainUnknown,
    ModuleInvalid,
    ModuleTooLarge,
    ModuleFeatureRefused,
    ModuleMemory,
    ModuleStartSection,
    ModuleTableMutation,
    ModuleImportRefused,
    ModuleExportMissing,
    ModuleExportReserved,
    ModuleInitialMemoryAboveLimit,
    ModuleManifestMissing,
    ModuleManifestSchema,
    ModuleContractUnsupported,
    ModulePortTypeUnknown,
    ModulePortCount,
    LimitBelowNeed,
    AdmissionTransformFailed,
    AdmissionResult,
    ModuleExportUnsupported,
    HostCapacityExceeded,
    HostCapacityWarning,
    HostCapacity,
    HostInstanceFailed,
    HostEngineRefused,
    HostNotReproducible,
    HostSwapping,
    HostLimitedBy,
    ActorFaulted,
    ActorText,
    RunEnded,
    RunLimitExceeded,
    InterventionApplied,
    TelemetryDropped,
    TelemetryTransport,
    DomainEvent,
    ControlVersion,
    ControlInvalid,
    ControlNotNow,
    PackagePartial,
    PackageEntryMismatch,
    FileTooLarge,
    FileUnreadable,
    ExportPreview,
    ReplayMatched,
    ReplayDiverged,
    CheckpointMismatch,
    StorageCache,
    MessageUnknown,
}

impl Code {
    pub fn as_str(self) -> &'static str {
        self.entry().name
    }
    pub fn entry(self) -> &'static Entry {
        &CATALOGUE[self as usize]
    }
    pub fn parse(name: &str) -> Option<Code> {
        CATALOGUE.iter().find(|e| e.name == name).map(|e| e.code)
    }
    /// A notification of this code with the given values, in the catalogue's order.
    pub fn notification(self, scope: crate::Scope, values: Vec<Value>) -> Notification {
        let e = self.entry();
        let mut n = Notification::new(e.name, e.severity, scope, e.ends);
        for ((name, _), v) in e.params.iter().zip(values) {
            n = n.with(name, v);
        }
        n
    }
}

/// The catalogue, in the RFC's order (the `Code` discriminant indexes it).
pub const CATALOGUE: &[Entry] = &[
    Entry {
        code: Code::ManifestTooLarge,
        name: "manifest.too-large",
        params: &[
            ("what", ParamType::Enum("manifest-count")),
            ("found", ParamType::Count),
            ("maximum", ParamType::Count),
        ],
        severity: Severity::Error,
        scope: ScopeKind::File,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ManifestInvalid,
        name: "manifest.invalid",
        params: &[("reason", ParamType::Text)],
        severity: Severity::Error,
        scope: ScopeKind::File,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ManifestSchemaVersion,
        name: "manifest.schema-version",
        params: &[("found", ParamType::Count), ("supported", ParamType::Count)],
        severity: Severity::Error,
        scope: ScopeKind::File,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ManifestType,
        name: "manifest.type",
        params: &[
            ("field", ParamType::Text),
            ("expected", ParamType::Enum("field-type")),
        ],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ManifestUnknownKey,
        name: "manifest.unknown-key",
        params: &[("key", ParamType::Text)],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ManifestMissingField,
        name: "manifest.missing-field",
        params: &[("field", ParamType::Text)],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::IdentityDuplicate,
        name: "identity.duplicate",
        params: &[
            ("identity", ParamType::Identity),
            ("first", ParamType::Name),
            ("second", ParamType::Name),
        ],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::IdentityReserved,
        name: "identity.reserved",
        params: &[("identity", ParamType::Identity)],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::IdentityPortDuplicate,
        name: "identity.port-duplicate",
        params: &[
            ("actor", ParamType::Identity),
            ("port", ParamType::Identity),
        ],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::NameDuplicate,
        name: "name.duplicate",
        params: &[
            ("name", ParamType::Name),
            ("kind", ParamType::Enum("element")),
        ],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ReferenceUnresolved,
        name: "reference.unresolved",
        params: &[
            ("reference", ParamType::Text),
            ("kind", ParamType::Enum("element")),
        ],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::DomainUndeclared,
        name: "domain.undeclared",
        params: &[("domain", ParamType::Name), ("where", ParamType::Text)],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ModuleHashMismatch,
        name: "module.hash-mismatch",
        params: &[
            ("module", ParamType::Name),
            ("declared", ParamType::Text),
            ("actual", ParamType::Text),
        ],
        severity: Severity::Error,
        scope: ScopeKind::File,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ModuleContractMismatch,
        name: "module.contract-mismatch",
        params: &[
            ("module", ParamType::Name),
            ("declared", ParamType::Text),
            ("found", ParamType::Text),
        ],
        severity: Severity::Error,
        scope: ScopeKind::File,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ConnectionTooFewPorts,
        name: "connection.too-few-ports",
        params: &[
            ("connection", ParamType::Identity),
            ("ports", ParamType::Count),
        ],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::QuantityMissingUnit,
        name: "quantity.missing-unit",
        params: &[
            ("field", ParamType::Text),
            ("dimension", ParamType::Enum("dimension")),
        ],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::QuantityNotWholeTicks,
        name: "quantity.not-whole-ticks",
        params: &[
            ("field", ParamType::Text),
            ("value", ParamType::Quantity),
            ("tick", ParamType::Quantity),
            ("smallest", ParamType::Quantity),
        ],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::QuantityNotWholeUnit,
        name: "quantity.not-whole-unit",
        params: &[
            ("field", ParamType::Text),
            ("value", ParamType::Quantity),
            ("unit", ParamType::Text),
        ],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::QuantityUnknownUnit,
        name: "quantity.unknown-unit",
        params: &[("field", ParamType::Text), ("unit", ParamType::Text)],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::QuantityOutOfRange,
        name: "quantity.out-of-range",
        params: &[("field", ParamType::Text), ("value", ParamType::Quantity)],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::LimitAboveMaximum,
        name: "limit.above-maximum",
        params: &[
            ("limit", ParamType::Name),
            ("value", ParamType::Count),
            ("maximum", ParamType::Count),
        ],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::TimeEndBeforeChange,
        name: "time.end-before-change",
        params: &[
            ("change", ParamType::Text),
            ("at", ParamType::Instant),
            ("end", ParamType::Instant),
        ],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ConfigMissingParameter,
        name: "config.missing-parameter",
        params: &[
            ("actor", ParamType::Identity),
            ("parameter", ParamType::Name),
        ],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ConfigUnknownParameter,
        name: "config.unknown-parameter",
        params: &[
            ("actor", ParamType::Identity),
            ("parameter", ParamType::Name),
        ],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ConfigType,
        name: "config.type",
        params: &[
            ("actor", ParamType::Identity),
            ("parameter", ParamType::Name),
            ("expected", ParamType::Enum("parameter-type")),
        ],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ConnectionMixedTypes,
        name: "connection.mixed-types",
        params: &[("connection", ParamType::Name)],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::DomainUnknown,
        name: "domain.unknown",
        params: &[("domain", ParamType::Name)],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ModuleInvalid,
        name: "module.invalid",
        params: &[("reason", ParamType::Text)],
        severity: Severity::Error,
        scope: ScopeKind::File,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ModuleTooLarge,
        name: "module.too-large",
        params: &[
            ("what", ParamType::Enum("module-count")),
            ("found", ParamType::Count),
            ("maximum", ParamType::Count),
        ],
        severity: Severity::Error,
        scope: ScopeKind::File,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ModuleFeatureRefused,
        name: "module.feature-refused",
        params: &[("feature", ParamType::Name)],
        severity: Severity::Error,
        scope: ScopeKind::File,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ModuleMemory,
        name: "module.memory",
        params: &[("reason", ParamType::Enum("memory-rule"))],
        severity: Severity::Error,
        scope: ScopeKind::File,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ModuleStartSection,
        name: "module.start-section",
        params: &[],
        severity: Severity::Error,
        scope: ScopeKind::File,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ModuleTableMutation,
        name: "module.table-mutation",
        params: &[("instruction", ParamType::Name)],
        severity: Severity::Error,
        scope: ScopeKind::File,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ModuleImportRefused,
        name: "module.import-refused",
        params: &[("module", ParamType::Name), ("name", ParamType::Name)],
        severity: Severity::Error,
        scope: ScopeKind::File,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ModuleExportMissing,
        name: "module.export-missing",
        params: &[("name", ParamType::Name)],
        severity: Severity::Error,
        scope: ScopeKind::File,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ModuleExportReserved,
        name: "module.export-reserved",
        params: &[("name", ParamType::Name)],
        severity: Severity::Error,
        scope: ScopeKind::File,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ModuleInitialMemoryAboveLimit,
        name: "module.initial-memory-above-limit",
        params: &[("pages", ParamType::Count), ("limit", ParamType::Count)],
        severity: Severity::Error,
        scope: ScopeKind::File,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ModuleManifestMissing,
        name: "module.manifest-missing",
        params: &[("section", ParamType::Enum("manifest-section"))],
        severity: Severity::Error,
        scope: ScopeKind::File,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ModuleManifestSchema,
        name: "module.manifest-schema",
        params: &[("reason", ParamType::Text)],
        severity: Severity::Error,
        scope: ScopeKind::File,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ModuleContractUnsupported,
        name: "module.contract-unsupported",
        params: &[
            ("declared", ParamType::Text),
            ("supported", ParamType::Text),
        ],
        severity: Severity::Error,
        scope: ScopeKind::File,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ModulePortTypeUnknown,
        name: "module.port-type-unknown",
        params: &[
            ("actor", ParamType::Identity),
            ("port", ParamType::Name),
            ("type", ParamType::Text),
        ],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ModulePortCount,
        name: "module.port-count",
        params: &[
            ("actor", ParamType::Identity),
            ("kind", ParamType::Name),
            ("found", ParamType::Count),
            ("min", ParamType::Count),
            ("max", ParamType::Count),
        ],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::LimitBelowNeed,
        name: "limit.below-need",
        params: &[
            ("actor", ParamType::Identity),
            ("limit", ParamType::Name),
            ("need", ParamType::Count),
            ("value", ParamType::Count),
        ],
        severity: Severity::Error,
        scope: ScopeKind::Field,
        ends: Ends::Command,
    },
    Entry {
        code: Code::AdmissionTransformFailed,
        name: "admission.transform-failed",
        params: &[("reason", ParamType::Text)],
        severity: Severity::Error,
        scope: ScopeKind::File,
        ends: Ends::Command,
    },
    Entry {
        code: Code::AdmissionResult,
        name: "admission.result",
        params: &[
            ("module", ParamType::Name),
            ("hash", ParamType::Text),
            ("admitted", ParamType::Enum("trust-state")),
        ],
        severity: Severity::Info,
        scope: ScopeKind::File,
        ends: Ends::Nothing,
    },
    Entry {
        code: Code::ModuleExportUnsupported,
        name: "module.export-unsupported",
        params: &[("name", ParamType::Name)],
        severity: Severity::Error,
        scope: ScopeKind::File,
        ends: Ends::Command,
    },
    Entry {
        code: Code::HostCapacityExceeded,
        name: "host.capacity-exceeded",
        params: &[
            ("need", ParamType::Count),
            ("capacity", ParamType::Count),
            ("what", ParamType::Enum("capacity-kind")),
        ],
        severity: Severity::Error,
        scope: ScopeKind::Run,
        ends: Ends::Command,
    },
    Entry {
        code: Code::HostCapacityWarning,
        name: "host.capacity-warning",
        params: &[
            ("used", ParamType::Count),
            ("capacity", ParamType::Count),
            ("what", ParamType::Enum("capacity-kind")),
        ],
        severity: Severity::Warning,
        scope: ScopeKind::Run,
        ends: Ends::Nothing,
    },
    Entry {
        code: Code::HostCapacity,
        name: "host.capacity",
        params: &[
            ("instances", ParamType::Count),
            ("memory", ParamType::Bytes),
        ],
        severity: Severity::Info,
        scope: ScopeKind::Run,
        ends: Ends::Nothing,
    },
    Entry {
        code: Code::HostInstanceFailed,
        name: "host.instance-failed",
        params: &[("actor", ParamType::Identity), ("text", ParamType::Text)],
        severity: Severity::Error,
        scope: ScopeKind::Run,
        ends: Ends::Run,
    },
    Entry {
        code: Code::HostEngineRefused,
        name: "host.engine-refused",
        params: &[("module", ParamType::Name), ("text", ParamType::Text)],
        severity: Severity::Error,
        scope: ScopeKind::Run,
        ends: Ends::Run,
    },
    Entry {
        code: Code::HostNotReproducible,
        name: "host.not-reproducible",
        params: &[
            ("reason", ParamType::Enum("machine-condition")),
            ("text", ParamType::Text),
        ],
        severity: Severity::Error,
        scope: ScopeKind::Run,
        ends: Ends::Run,
    },
    Entry {
        code: Code::HostSwapping,
        name: "host.swapping",
        params: &[
            ("resident", ParamType::Count),
            ("actors", ParamType::Count),
            ("cost", ParamType::Count),
        ],
        severity: Severity::Info,
        scope: ScopeKind::Run,
        ends: Ends::Nothing,
    },
    Entry {
        code: Code::HostLimitedBy,
        name: "host.limited-by",
        params: &[("what", ParamType::Enum("limiting-factor"))],
        severity: Severity::Info,
        scope: ScopeKind::Run,
        ends: Ends::Nothing,
    },
    Entry {
        code: Code::ActorFaulted,
        name: "actor.faulted",
        params: &[
            ("actor", ParamType::Identity),
            ("fault", ParamType::Code),
            ("metered", ParamType::Count),
            ("policy", ParamType::Enum("faulted-policy")),
        ],
        severity: Severity::Error,
        scope: ScopeKind::Actor,
        ends: Ends::Activation,
    },
    Entry {
        code: Code::ActorText,
        name: "actor.text",
        params: &[("actor", ParamType::Identity), ("text", ParamType::Text)],
        severity: Severity::Info,
        scope: ScopeKind::Actor,
        ends: Ends::Nothing,
    },
    Entry {
        code: Code::RunEnded,
        name: "run.ended",
        params: &[("reason", ParamType::Enum("end-reason"))],
        severity: Severity::Info,
        scope: ScopeKind::Run,
        ends: Ends::Run,
    },
    Entry {
        code: Code::RunLimitExceeded,
        name: "run.limit-exceeded",
        params: &[
            ("what", ParamType::Enum("run-limit")),
            ("limit", ParamType::Count),
            ("origins", ParamType::Text),
        ],
        severity: Severity::Error,
        scope: ScopeKind::Run,
        ends: Ends::Run,
    },
    Entry {
        code: Code::InterventionApplied,
        name: "intervention.applied",
        params: &[("at", ParamType::Instant), ("kind", ParamType::Name)],
        severity: Severity::Info,
        scope: ScopeKind::Run,
        ends: Ends::Nothing,
    },
    Entry {
        code: Code::TelemetryDropped,
        name: "telemetry.dropped",
        params: &[
            ("records", ParamType::Count),
            ("path", ParamType::Enum("display-path")),
        ],
        severity: Severity::Info,
        scope: ScopeKind::Run,
        ends: Ends::Nothing,
    },
    Entry {
        code: Code::TelemetryTransport,
        name: "telemetry.transport",
        params: &[("transport", ParamType::Enum("transport"))],
        severity: Severity::Info,
        scope: ScopeKind::Run,
        ends: Ends::Nothing,
    },
    Entry {
        code: Code::DomainEvent,
        name: "domain.event",
        params: &[
            ("domain", ParamType::Name),
            ("event", ParamType::Name),
            ("target", ParamType::Identity),
            ("detail", ParamType::Text),
        ],
        severity: Severity::Info,
        scope: ScopeKind::Run,
        ends: Ends::Nothing,
    },
    Entry {
        code: Code::ControlVersion,
        name: "control.version",
        params: &[("front-end", ParamType::Text), ("host", ParamType::Text)],
        severity: Severity::Error,
        scope: ScopeKind::Command,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ControlInvalid,
        name: "control.invalid",
        params: &[
            ("command", ParamType::Name),
            ("reason", ParamType::Enum("invalid-reason")),
            ("field", ParamType::Text),
        ],
        severity: Severity::Error,
        scope: ScopeKind::Command,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ControlNotNow,
        name: "control.not-now",
        params: &[
            ("command", ParamType::Name),
            ("state", ParamType::Enum("run-state")),
        ],
        severity: Severity::Error,
        scope: ScopeKind::Command,
        ends: Ends::Command,
    },
    Entry {
        code: Code::PackagePartial,
        name: "package.partial",
        params: &[("missing", ParamType::Count)],
        severity: Severity::Warning,
        scope: ScopeKind::File,
        ends: Ends::Nothing,
    },
    Entry {
        code: Code::PackageEntryMismatch,
        name: "package.entry-mismatch",
        params: &[
            ("entry", ParamType::Text),
            ("declared", ParamType::Text),
            ("actual", ParamType::Text),
        ],
        severity: Severity::Error,
        scope: ScopeKind::File,
        ends: Ends::Command,
    },
    Entry {
        code: Code::FileTooLarge,
        name: "file.too-large",
        params: &[
            ("kind", ParamType::Enum("file-kind")),
            ("size", ParamType::Bytes),
            ("maximum", ParamType::Bytes),
        ],
        severity: Severity::Error,
        scope: ScopeKind::File,
        ends: Ends::Command,
    },
    Entry {
        code: Code::FileUnreadable,
        name: "file.unreadable",
        params: &[
            ("kind", ParamType::Enum("file-kind")),
            ("reason", ParamType::Text),
        ],
        severity: Severity::Error,
        scope: ScopeKind::File,
        ends: Ends::Command,
    },
    Entry {
        code: Code::ExportPreview,
        name: "export.preview",
        params: &[
            ("kind", ParamType::Enum("file-kind")),
            ("size", ParamType::Bytes),
            ("payloads", ParamType::Enum("yes-no")),
        ],
        severity: Severity::Info,
        scope: ScopeKind::File,
        ends: Ends::Nothing,
    },
    Entry {
        code: Code::ReplayMatched,
        name: "replay.matched",
        params: &[("points", ParamType::Count)],
        severity: Severity::Info,
        scope: ScopeKind::Run,
        ends: Ends::Nothing,
    },
    Entry {
        code: Code::ReplayDiverged,
        name: "replay.diverged",
        params: &[("from", ParamType::Count), ("to", ParamType::Count)],
        severity: Severity::Error,
        scope: ScopeKind::Run,
        ends: Ends::Nothing,
    },
    Entry {
        code: Code::CheckpointMismatch,
        name: "checkpoint.mismatch",
        params: &[
            ("what", ParamType::Enum("checkpoint-field")),
            ("expected", ParamType::Text),
            ("found", ParamType::Text),
        ],
        severity: Severity::Error,
        scope: ScopeKind::File,
        ends: Ends::Command,
    },
    Entry {
        code: Code::StorageCache,
        name: "storage.cache",
        params: &[("items", ParamType::Count)],
        severity: Severity::Info,
        scope: ScopeKind::File,
        ends: Ends::Nothing,
    },
    Entry {
        code: Code::MessageUnknown,
        name: "message.unknown",
        params: &[("code", ParamType::Text), ("params", ParamType::Text)],
        severity: Severity::Warning,
        scope: ScopeKind::None,
        ends: Ends::Nothing,
    },
];

/// The closed enumerations of §4.5. Sets a domain extends are listed with the core's values.
pub const ENUMERATIONS: &[(&str, &[&str])] = &[
    (
        "element",
        &["actor", "connection", "port", "module", "domain"],
    ),
    ("dimension", &["time", "bytes"]),
    ("parameter-type", &[]),
    (
        "module-count",
        &[
            "functions",
            "globals",
            "tables",
            "segments",
            "imports",
            "exports",
            "custom-sections",
            "bytes",
        ],
    ),
    (
        "memory-rule",
        &["count", "imported", "shared", "64-bit", "not-exported"],
    ),
    ("manifest-section", &["contract-identity", "florn-manifest"]),
    ("trust-state", &["validated", "rejected"]),
    ("capacity-kind", &["instances", "memory"]),
    ("machine-condition", &["memory-refused", "engine-stack"]),
    (
        "limiting-factor",
        &["actors", "solver", "telemetry", "display", "memory"],
    ),
    ("faulted-policy", &["drop", "stop-run"]),
    ("end-reason", &["end-reached", "user", "host-fault"]),
    ("run-limit", &["microsteps-per-tick", "events-per-instant"]),
    ("display-path", &["ring", "messages"]),
    ("transport", &["shared", "messages"]),
    (
        "invalid-reason",
        &[
            "unknown-command",
            "missing-field",
            "wrong-type",
            "out-of-range",
            "unknown-field",
        ],
    ),
    (
        "run-state",
        &[
            "no-scenario",
            "invalid",
            "ready",
            "running",
            "paused",
            "ended",
        ],
    ),
    (
        "file-kind",
        &[
            "manifest",
            "package",
            "module",
            "run-record",
            "checkpoint",
            "diagnostic-log",
        ],
    ),
    (
        "checkpoint-field",
        &[
            "module-hash",
            "admission-version",
            "cost-model",
            "scenario-hash",
        ],
    ),
    ("yes-no", &["yes", "no"]),
    (
        "manifest-count",
        &[
            "bytes",
            "text-bytes",
            "domains",
            "modules",
            "actors",
            "ports",
            "connections",
            "changes",
            "limit-classes",
            "package-bytes",
            "package-entries",
        ],
    ),
    (
        "field-type",
        &[
            "table",
            "text",
            "unsigned-integer",
            "identity",
            "quantity",
            "list",
            "tick-length",
            "tie-policy",
            "actor-kind",
            "domain/name",
            "sha256-hex",
        ],
    ),
];

pub fn enumeration(set: &str) -> Option<&'static [&'static str]> {
    ENUMERATIONS
        .iter()
        .find(|(n, _)| *n == set)
        .map(|(_, v)| *v)
}

/// The fault codes of RFC 007 rule 2, with their parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaultCode {
    Trap,
    Budget,
    Stack,
    Memory,
    Quota,
    Contract,
}

impl FaultCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trap => "fault.trap",
            Self::Budget => "fault.budget",
            Self::Stack => "fault.stack",
            Self::Memory => "fault.memory",
            Self::Quota => "fault.quota",
            Self::Contract => "fault.contract",
        }
    }
    pub fn params(self) -> &'static [(&'static str, ParamType)] {
        match self {
            Self::Trap => &[("text", ParamType::Text)],
            Self::Budget => &[("budget", ParamType::Count)],
            Self::Stack => &[("limit", ParamType::Count)],
            Self::Memory => &[("limit", ParamType::Bytes)],
            Self::Quota => &[("quota", ParamType::Name), ("limit", ParamType::Count)],
            Self::Contract => &[("rule", ParamType::Text), ("detail", ParamType::Text)],
        }
    }
    pub fn parse(name: &str) -> Option<Self> {
        [
            Self::Trap,
            Self::Budget,
            Self::Stack,
            Self::Memory,
            Self::Quota,
            Self::Contract,
        ]
        .into_iter()
        .find(|f| f.as_str() == name)
    }
}

/// Why a notification does not match its catalogue entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CatalogueError {
    UnknownCode(String),
    /// A parameter the code does not declare, or one missing, or of the wrong type.
    Parameter {
        name: String,
        expected: Option<ParamType>,
    },
    /// An enumeration value outside its set.
    EnumValue {
        set: &'static str,
        value: String,
    },
    Severity,
    Ends,
}

impl Value {
    fn matches(&self, t: ParamType) -> Result<(), CatalogueError> {
        let ok = matches!(
            (self, t),
            (Value::Identity(_), ParamType::Identity)
                | (Value::Name(_), ParamType::Name)
                | (Value::Count(_), ParamType::Count)
                | (Value::Bytes(_), ParamType::Bytes)
                | (Value::Quantity(..), ParamType::Quantity)
                | (Value::Tick(_), ParamType::Tick)
                | (Value::Instant(_), ParamType::Instant)
                | (Value::Code(_), ParamType::Code)
                | (Value::Text(_), ParamType::Text)
                | (Value::Enum(_), ParamType::Enum(_))
        );
        if !ok {
            return Err(CatalogueError::Parameter {
                name: String::new(),
                expected: Some(t),
            });
        }
        if let (Value::Enum(v), ParamType::Enum(set)) = (self, t)
            && let Some(values) = enumeration(set)
            && !values.contains(&v.as_str())
        {
            return Err(CatalogueError::EnumValue {
                set,
                value: v.clone(),
            });
        }
        Ok(())
    }
}

impl Notification {
    /// Check a notification against the catalogue: the code exists; the parameters are exactly
    /// the declared ones, each of its type, enumerations within their sets; severity and ends
    /// as listed. Extra parameters are allowed only after the declared ones (a Host may add
    /// detail), never missing or mistyped ones.
    pub fn check(&self) -> Result<&'static Entry, CatalogueError> {
        let code = Code::parse(&self.code)
            .ok_or_else(|| CatalogueError::UnknownCode(self.code.clone()))?;
        let e = code.entry();
        for (name, t) in e.params {
            let v = self
                .params
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v)
                .ok_or_else(|| CatalogueError::Parameter {
                    name: name.to_string(),
                    expected: Some(*t),
                })?;
            v.matches(*t).map_err(|err| match err {
                CatalogueError::Parameter { expected, .. } => CatalogueError::Parameter {
                    name: name.to_string(),
                    expected,
                },
                other => other,
            })?;
        }
        if self.severity != e.severity {
            return Err(CatalogueError::Severity);
        }
        if self.ends != e.ends {
            return Err(CatalogueError::Ends);
        }
        Ok(e)
    }
}
