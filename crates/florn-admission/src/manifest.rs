//! Stage 2, the module manifest (RFC 002 §3): both custom sections, the schema of rule 12, the
//! parameter types of rule 13, port kinds, channels, needs.

use std::collections::BTreeSet;

use florn_model::{DomainValue, Quantity};
use toml::Value;

use crate::error::{Error, Errors, Kind};
use crate::world::{WorldInterfaces, WorldSpec, supported_worlds};

/// The name of the florn manifest section; the contract identity's name starts with `component-type`.
pub const MANIFEST_SECTION: &str = "florn.manifest";
pub const CONTRACT_SECTION_PREFIX: &str = "component-type";
/// The manifest schema version this Host reads.
pub const MANIFEST_SCHEMA: i64 = 1;
/// Maxima on the manifest (RFC 003 rule 4).
pub const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
pub const MAX_PORT_KINDS: usize = 256;
pub const MAX_PARAMS: usize = 1_024;
pub const MAX_CHANNELS: usize = 1_024;
pub const MAX_FIELDS: usize = 256;

/// The parameter types of RFC 002 rule 13 (`enum(parameter-type)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamType {
    Bool,
    U64,
    S64,
    F64,
    Text,
    Bytes,
    Ticks,
    /// `s64` in the declared delivery unit, a quantity such as `0.01USD`.
    Integer,
    /// `f64` in the SI unit of the dimension of the declared unit.
    Real,
}

impl ParamType {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "bool" => Self::Bool,
            "u64" => Self::U64,
            "s64" => Self::S64,
            "f64" => Self::F64,
            "text" => Self::Text,
            "bytes" => Self::Bytes,
            "ticks" => Self::Ticks,
            "integer" => Self::Integer,
            "real" => Self::Real,
            _ => return None,
        })
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::U64 => "u64",
            Self::S64 => "s64",
            Self::F64 => "f64",
            Self::Text => "text",
            Self::Bytes => "bytes",
            Self::Ticks => "ticks",
            Self::Integer => "integer",
            Self::Real => "real",
        }
    }
    /// The telemetry field types are the first six (rule 16).
    pub fn is_field_type(self) -> bool {
        matches!(
            self,
            Self::Bool | Self::U64 | Self::S64 | Self::F64 | Self::Text | Self::Bytes
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortKind {
    pub kind: String,
    /// `network/ethernet`
    pub ty: String,
    pub min: u64,
    pub max: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub ty: ParamType,
    pub required: bool,
    /// As a domain value; validated against `ty` when the manifest is read.
    pub default: Option<DomainValue>,
    /// `integer`: the delivery unit, a quantity (`0.01USD`); `real`: the unit symbol (`ohm`).
    pub unit: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Channel {
    pub channel: u32,
    pub name: String,
    pub fields: Vec<(String, ParamType)>,
}

/// `[resources]`: what the module needs (RFC 002 rule 17). Named as the limits of RFC 003 rule 16.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resources {
    pub budget: Option<u64>,
    pub stack: Option<u64>,
    pub memory_bytes: Option<u64>,
    pub effects_per_activation: Option<u64>,
    pub payload_bytes: Option<u64>,
    pub pending_timers: Option<u64>,
    pub telemetry_per_activation_bytes: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleManifest {
    pub world: String,
    pub kind: String,
    pub description: Option<String>,
    pub sdk: Option<String>,
    pub ports: Vec<PortKind>,
    pub params: Vec<Param>,
    pub telemetry: Vec<Channel>,
    pub resources: Resources,
}

impl ModuleManifest {
    pub fn port_kind_of_type(&self, ty: &str) -> Option<&PortKind> {
        self.ports.iter().find(|p| p.ty == ty)
    }
    pub fn param(&self, name: &str) -> Option<&Param> {
        self.params.iter().find(|p| p.name == name)
    }
    pub fn channel(&self, channel: u32) -> Option<&Channel> {
        self.telemetry.iter().find(|c| c.channel == channel)
    }
}

/// Both custom sections, read from a source module that passed stage 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sections {
    /// The contract identity: the interfaces the encoded world imports and exports. The encoding
    /// keeps the interfaces, not the world's own name; the manifest names the world (rule 11).
    pub contract: WorldInterfaces,
    pub manifest_text: String,
}

fn schema(reason: impl Into<String>) -> Error {
    Error::new(Kind::ManifestSchema {
        reason: reason.into(),
    })
}

/// Find both sections and decode the contract identity.
pub fn read_sections(
    bytes: &[u8],
    custom_sections: &[(String, std::ops::Range<usize>)],
) -> Result<Sections, Errors> {
    let mut errors = Errors::default();
    let manifest = custom_sections
        .iter()
        .filter(|(n, _)| n == MANIFEST_SECTION)
        .collect::<Vec<_>>();
    let has_contract = custom_sections
        .iter()
        .any(|(n, _)| n.starts_with(CONTRACT_SECTION_PREFIX));
    if !has_contract {
        errors.push(Error::new(Kind::ManifestMissing {
            section: "contract-identity",
        }));
    }
    let manifest_text = match manifest.as_slice() {
        [] => {
            errors.push(Error::new(Kind::ManifestMissing {
                section: "florn-manifest",
            }));
            None
        }
        [(_, range)] => {
            let data = &bytes[range.clone()];
            if data.len() > MAX_MANIFEST_BYTES {
                errors.push(schema(format!(
                    "the manifest is {} bytes; the maximum is {MAX_MANIFEST_BYTES}",
                    data.len()
                )));
                None
            } else {
                match std::str::from_utf8(data) {
                    Ok(t) => Some(t.to_string()),
                    Err(e) => {
                        errors.push(schema(format!("the manifest is not UTF-8: {e}")));
                        None
                    }
                }
            }
        }
        _ => {
            errors.push(schema("more than one florn.manifest section"));
            None
        }
    };
    let contract = if has_contract {
        match wit_component::metadata::decode(bytes) {
            Ok((_, bindgen)) => {
                let resolve = &bindgen.resolve;
                let world = &resolve.worlds[bindgen.world];
                let name = |key: &wit_parser::WorldKey| match key {
                    wit_parser::WorldKey::Interface(id) => {
                        resolve.id_of(*id).unwrap_or_else(|| "<anonymous>".into())
                    }
                    wit_parser::WorldKey::Name(n) => n.clone(),
                };
                let imports: Vec<String> = world.imports.keys().map(name).collect();
                let exports: Vec<String> = world.exports.keys().map(name).collect();
                Some(WorldInterfaces::new(
                    &imports.iter().map(String::as_str).collect::<Vec<_>>(),
                    &exports.iter().map(String::as_str).collect::<Vec<_>>(),
                ))
            }
            Err(e) => {
                errors.push(schema(format!("contract identity: {e:#}")));
                None
            }
        }
    } else {
        None
    };
    match (contract, manifest_text) {
        (Some(contract), Some(manifest_text)) if errors.is_empty() => Ok(Sections {
            contract,
            manifest_text,
        }),
        _ => Err(errors),
    }
}

/// Parse the manifest text (rule 12) and check its world against the contract identity (rule 11)
/// and against the supported worlds (RFC 003 rule 2, stage 2). Returns the manifest and the spec.
pub fn read_manifest(
    module: &str,
    sections: &Sections,
) -> Result<(ModuleManifest, WorldSpec), Errors> {
    let manifest = parse_manifest(&sections.manifest_text)?;
    let supported = supported_worlds();
    let Some(world) = supported.iter().find(|w| w.name == manifest.world) else {
        return Err(Error::at(
            Kind::ContractUnsupported {
                declared: manifest.world.clone(),
                supported: supported
                    .iter()
                    .map(|w| w.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            },
            "world",
        )
        .into());
    };
    if world.interfaces != sections.contract {
        return Err(Error::at(
            Kind::ContractMismatch {
                module: module.to_string(),
                declared: manifest.world.clone(),
                found: sections.contract.to_string(),
            },
            "world",
        )
        .into());
    }
    Ok((manifest, world.clone()))
}

struct Reader {
    errors: Errors,
}

impl Reader {
    fn table<'a>(&mut self, v: &'a Value, path: &str) -> Option<&'a toml::Table> {
        match v.as_table() {
            Some(t) => Some(t),
            None => {
                self.errors.push(Error::at(
                    Kind::ManifestSchema {
                        reason: "expected a table".into(),
                    },
                    path,
                ));
                None
            }
        }
    }
    fn known(&mut self, t: &toml::Table, path: &str, keys: &[&str]) {
        for k in t.keys() {
            if !keys.contains(&k.as_str()) {
                let at = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                self.errors.push(Error::at(
                    Kind::ManifestSchema {
                        reason: format!("unknown key `{k}`"),
                    },
                    at,
                ));
            }
        }
    }
    fn string(&mut self, t: &toml::Table, path: &str, key: &str, required: bool) -> Option<String> {
        match t.get(key) {
            Some(Value::String(s)) => Some(s.clone()),
            Some(_) => {
                self.errors.push(Error::at(
                    Kind::ManifestSchema {
                        reason: "expected a string".into(),
                    },
                    format!("{path}{key}"),
                ));
                None
            }
            None => {
                if required {
                    self.errors.push(Error::at(
                        Kind::ManifestSchema {
                            reason: format!("missing `{key}`"),
                        },
                        format!("{path}{key}"),
                    ));
                }
                None
            }
        }
    }
    fn count(&mut self, t: &toml::Table, path: &str, key: &str) -> Option<u64> {
        match t.get(key) {
            Some(Value::Integer(i)) if *i >= 0 => Some(*i as u64),
            Some(_) => {
                self.errors.push(Error::at(
                    Kind::ManifestSchema {
                        reason: "expected a non-negative integer".into(),
                    },
                    format!("{path}{key}"),
                ));
                None
            }
            None => None,
        }
    }
    fn bytes(&mut self, t: &toml::Table, path: &str, key: &str) -> Option<u64> {
        let s = self.string(t, path, key, false)?;
        let q = Quantity::parse(&s).ok();
        match q.and_then(|q| q.to_bytes().ok().flatten()) {
            Some(b) => Some(b),
            None => {
                self.errors.push(Error::at(
                    Kind::ManifestSchema {
                        reason: format!("`{s}` is not a whole number of bytes with a unit"),
                    },
                    format!("{path}{key}"),
                ));
                None
            }
        }
    }
    fn array<'a>(
        &mut self,
        t: &'a toml::Table,
        key: &str,
        max: usize,
    ) -> Vec<(usize, &'a toml::Table)> {
        match t.get(key) {
            None => Vec::new(),
            Some(Value::Array(a)) => {
                if a.len() > max {
                    self.errors.push(Error::at(
                        Kind::ManifestSchema {
                            reason: format!("{} entries; the maximum is {max}", a.len()),
                        },
                        key,
                    ));
                    return Vec::new();
                }
                a.iter()
                    .enumerate()
                    .filter_map(|(i, v)| match v.as_table() {
                        Some(t) => Some((i, t)),
                        None => {
                            self.errors.push(Error::at(
                                Kind::ManifestSchema {
                                    reason: "expected a table".into(),
                                },
                                format!("{key}[{i}]"),
                            ));
                            None
                        }
                    })
                    .collect()
            }
            Some(_) => {
                self.errors.push(Error::at(
                    Kind::ManifestSchema {
                        reason: "expected an array of tables".into(),
                    },
                    key,
                ));
                Vec::new()
            }
        }
    }
}

/// A TOML value as a domain value (a default), or `None` for a shape no parameter takes.
fn domain_value(v: &Value) -> Option<DomainValue> {
    Some(match v {
        Value::Integer(i) => DomainValue::Integer(*i),
        Value::Float(f) => DomainValue::Decimal(format!("{f}")),
        Value::String(s) => DomainValue::Text(s.clone()),
        Value::Boolean(b) => DomainValue::Boolean(*b),
        _ => return None,
    })
}

/// Whether a value has the shape rule 13 writes for the type (the scenario's form).
pub fn value_fits(ty: ParamType, v: &DomainValue) -> bool {
    match (ty, v) {
        (ParamType::Bool, DomainValue::Boolean(_)) => true,
        (ParamType::U64, DomainValue::Integer(i)) => *i >= 0,
        (ParamType::S64, DomainValue::Integer(_)) => true,
        (ParamType::F64, DomainValue::Integer(_) | DomainValue::Decimal(_)) => true,
        (ParamType::Text, DomainValue::Text(_)) => true,
        (ParamType::Bytes, DomainValue::Text(s)) => {
            s.len() % 2 == 0 && s.bytes().all(|b| b.is_ascii_hexdigit())
        }
        (ParamType::Ticks | ParamType::Integer | ParamType::Real, DomainValue::Text(s)) => {
            Quantity::parse(s).is_ok()
        }
        _ => false,
    }
}

/// Parse the manifest text alone (rule 12).
pub fn parse_manifest(text: &str) -> Result<ModuleManifest, Errors> {
    let table: toml::Table = match text.parse() {
        Ok(t) => t,
        Err(e) => return Err(schema(format!("not TOML: {e}")).into()),
    };
    let mut r = Reader {
        errors: Errors::default(),
    };
    r.known(
        &table,
        "",
        &[
            "florn-module",
            "world",
            "kind",
            "description",
            "sdk",
            "ports",
            "params",
            "telemetry",
            "resources",
        ],
    );
    match table.get("florn-module") {
        Some(Value::Integer(v)) if *v == MANIFEST_SCHEMA => {}
        Some(Value::Integer(v)) => r.errors.push(Error::at(
            Kind::ManifestSchema {
                reason: format!("schema version {v}; this Host reads {MANIFEST_SCHEMA}"),
            },
            "florn-module",
        )),
        _ => r.errors.push(Error::at(
            Kind::ManifestSchema {
                reason: "missing `florn-module = 1`".into(),
            },
            "florn-module",
        )),
    }
    let world = r.string(&table, "", "world", true);
    let kind = r.string(&table, "", "kind", true);
    let description = r.string(&table, "", "description", false);
    let sdk = r.string(&table, "", "sdk", false);

    let mut ports = Vec::new();
    let mut port_types = BTreeSet::new();
    for (i, t) in r.array(&table, "ports", MAX_PORT_KINDS) {
        let path = format!("ports[{i}].");
        r.known(t, &path[..path.len() - 1], &["kind", "type", "min", "max"]);
        let kind = r.string(t, &path, "kind", true);
        let ty = r.string(t, &path, "type", true);
        let min = r.count(t, &path, "min").unwrap_or(0);
        let max = r.count(t, &path, "max").unwrap_or(u64::MAX);
        if min > max {
            r.errors.push(Error::at(
                Kind::ManifestSchema {
                    reason: "min above max".into(),
                },
                format!("{path}min"),
            ));
        }
        if let (Some(kind), Some(ty)) = (kind, ty) {
            if !port_types.insert(ty.clone()) {
                r.errors.push(Error::at(
                    Kind::ManifestSchema {
                        reason: format!("a second port kind of type `{ty}`"),
                    },
                    format!("{path}type"),
                ));
            }
            ports.push(PortKind { kind, ty, min, max });
        }
    }

    let mut params = Vec::new();
    let mut names = BTreeSet::new();
    for (i, t) in r.array(&table, "params", MAX_PARAMS) {
        let path = format!("params[{i}].");
        r.known(
            t,
            &path[..path.len() - 1],
            &["name", "type", "required", "default", "unit"],
        );
        let name = r.string(t, &path, "name", true);
        let ty = r
            .string(t, &path, "type", true)
            .and_then(|s| match ParamType::parse(&s) {
                Some(t) => Some(t),
                None => {
                    r.errors.push(Error::at(
                        Kind::ManifestSchema {
                            reason: format!("unknown parameter type `{s}`"),
                        },
                        format!("{path}type"),
                    ));
                    None
                }
            });
        let required = match t.get("required") {
            None => false,
            Some(Value::Boolean(b)) => *b,
            Some(_) => {
                r.errors.push(Error::at(
                    Kind::ManifestSchema {
                        reason: "expected a boolean".into(),
                    },
                    format!("{path}required"),
                ));
                false
            }
        };
        let unit = r.string(t, &path, "unit", false);
        let default = t.get("default").map(|v| (v, domain_value(v)));
        if let (Some(name), Some(ty)) = (name, ty) {
            if !names.insert(name.clone()) {
                r.errors.push(Error::at(
                    Kind::ManifestSchema {
                        reason: format!("a second parameter named `{name}`"),
                    },
                    format!("{path}name"),
                ));
            }
            let needs_unit = matches!(ty, ParamType::Integer | ParamType::Real);
            match (&unit, needs_unit) {
                (None, true) => r.errors.push(Error::at(
                    Kind::ManifestSchema {
                        reason: format!("type `{}` needs a unit", ty.name()),
                    },
                    format!("{path}unit"),
                )),
                (Some(_), false) => r.errors.push(Error::at(
                    Kind::ManifestSchema {
                        reason: format!("type `{}` takes no unit", ty.name()),
                    },
                    format!("{path}unit"),
                )),
                (Some(u), true) => {
                    let ok = match ty {
                        ParamType::Integer => Quantity::parse(u).is_ok(),
                        _ => {
                            !u.is_empty()
                                && u.chars().all(|c| {
                                    c.is_ascii_alphanumeric()
                                        || c == '/'
                                        || c == '%'
                                        || c == '°'
                                        || c == 'µ'
                                })
                        }
                    };
                    if !ok {
                        r.errors.push(Error::at(
                            Kind::ManifestSchema {
                                reason: format!("`{u}` is not a unit for type `{}`", ty.name()),
                            },
                            format!("{path}unit"),
                        ));
                    }
                }
                (None, false) => {}
            }
            let default = match default {
                None => None,
                Some((_, Some(v))) if value_fits(ty, &v) => Some(v),
                Some(_) => {
                    r.errors.push(Error::at(
                        Kind::ManifestSchema {
                            reason: format!("the default is not a `{}`", ty.name()),
                        },
                        format!("{path}default"),
                    ));
                    None
                }
            };
            if required && default.is_some() {
                r.errors.push(Error::at(
                    Kind::ManifestSchema {
                        reason: "a required parameter has a default".into(),
                    },
                    format!("{path}default"),
                ));
            }
            params.push(Param {
                name,
                ty,
                required,
                default,
                unit,
            });
        }
    }

    let mut telemetry = Vec::new();
    let mut channels = BTreeSet::new();
    for (i, t) in r.array(&table, "telemetry", MAX_CHANNELS) {
        let path = format!("telemetry[{i}].");
        r.known(t, &path[..path.len() - 1], &["channel", "name", "fields"]);
        let channel = r.count(t, &path, "channel");
        let name = r.string(t, &path, "name", true);
        let mut fields = Vec::new();
        match t.get("fields") {
            Some(Value::Array(a)) if a.len() <= MAX_FIELDS => {
                for (j, f) in a.iter().enumerate() {
                    let fpath = format!("{path}fields[{j}].");
                    let Some(ft) = r.table(f, &fpath[..fpath.len() - 1]) else {
                        continue;
                    };
                    r.known(ft, &fpath[..fpath.len() - 1], &["name", "type"]);
                    let fname = r.string(ft, &fpath, "name", true);
                    let fty =
                        r.string(ft, &fpath, "type", true).and_then(|s| {
                            match ParamType::parse(&s) {
                                Some(t) if t.is_field_type() => Some(t),
                                _ => {
                                    r.errors.push(Error::at(
                                        Kind::ManifestSchema {
                                            reason: format!("unknown field type `{s}`"),
                                        },
                                        format!("{fpath}type"),
                                    ));
                                    None
                                }
                            }
                        });
                    if let (Some(n), Some(t)) = (fname, fty) {
                        fields.push((n, t));
                    }
                }
            }
            Some(Value::Array(a)) => r.errors.push(Error::at(
                Kind::ManifestSchema {
                    reason: format!("{} fields; the maximum is {MAX_FIELDS}", a.len()),
                },
                format!("{path}fields"),
            )),
            Some(_) => r.errors.push(Error::at(
                Kind::ManifestSchema {
                    reason: "expected an array of tables".into(),
                },
                format!("{path}fields"),
            )),
            None => r.errors.push(Error::at(
                Kind::ManifestSchema {
                    reason: "missing `fields`".into(),
                },
                format!("{path}fields"),
            )),
        }
        match (channel, name) {
            (Some(0), _) => r.errors.push(Error::at(
                Kind::ManifestSchema {
                    reason: "channel 0 is reserved for text".into(),
                },
                format!("{path}channel"),
            )),
            (Some(c), Some(name)) if c <= u32::MAX as u64 => {
                if !channels.insert(c) {
                    r.errors.push(Error::at(
                        Kind::ManifestSchema {
                            reason: format!("a second declaration of channel {c}"),
                        },
                        format!("{path}channel"),
                    ));
                }
                telemetry.push(Channel {
                    channel: c as u32,
                    name,
                    fields,
                });
            }
            (Some(_), _) => r.errors.push(Error::at(
                Kind::ManifestSchema {
                    reason: "channel above 32 bits".into(),
                },
                format!("{path}channel"),
            )),
            (None, _) => r.errors.push(Error::at(
                Kind::ManifestSchema {
                    reason: "missing `channel`".into(),
                },
                format!("{path}channel"),
            )),
        }
    }

    let mut resources = Resources::default();
    if let Some(v) = table.get("resources")
        && let Some(t) = r.table(v, "resources")
    {
        {
            r.known(
                t,
                "resources",
                &[
                    "budget",
                    "stack",
                    "memory",
                    "effects-per-activation",
                    "payload",
                    "pending-timers",
                    "telemetry-per-activation",
                ],
            );
            let p = "resources.";
            resources = Resources {
                budget: r.count(t, p, "budget"),
                stack: r.count(t, p, "stack"),
                memory_bytes: r.bytes(t, p, "memory"),
                effects_per_activation: r.count(t, p, "effects-per-activation"),
                payload_bytes: r.bytes(t, p, "payload"),
                pending_timers: r.count(t, p, "pending-timers"),
                telemetry_per_activation_bytes: r.bytes(t, p, "telemetry-per-activation"),
            };
        }
    }

    match (world, kind) {
        (Some(world), Some(kind)) if r.errors.is_empty() => Ok(ModuleManifest {
            world,
            kind,
            description,
            sdk,
            ports,
            params,
            telemetry,
            resources,
        }),
        _ => Err(r.errors),
    }
}
