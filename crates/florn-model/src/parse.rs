//! Parsing and validation of the manifest text (RFC 004 §3, §4, §7). Every error found is
//! reported, in one pass, with its code and typed parameters; nothing proportional to a count is
//! allocated before the count is checked (rule 39): the text size is checked first, and every list
//! is checked against its maximum before its elements are converted.

use std::collections::{BTreeMap, BTreeSet};

use toml::Value;

use crate::error::{Error, Errors, Kind};
use crate::manifest::*;
use crate::quantity::{Quantity, QuantityError, TickLength};
use crate::{HostMaxima, maxima};

impl Manifest {
    /// Parse and validate a manifest. `Err` carries every error found.
    pub fn parse(text: &str, host: &HostMaxima) -> Result<Manifest, Errors> {
        let mut errors = Errors::default();
        if text.len() > maxima::MANIFEST_BYTES {
            errors.push(Error::new(Kind::ManifestTooLarge {
                what: "bytes",
                found: text.len() as u64,
                maximum: maxima::MANIFEST_BYTES as u64,
            }));
            return Err(errors);
        }
        let table: toml::Table = match text.parse() {
            Ok(t) => t,
            Err(e) => {
                errors.push(Error::new(Kind::ManifestInvalid {
                    reason: e.message().to_string(),
                }));
                return Err(errors);
            }
        };
        let mut p = Parser {
            errors,
            host: *host,
        };
        let m = p.manifest(&table);
        p.errors.into_result(m)
    }
}

struct Parser {
    errors: Errors,
    host: HostMaxima,
}

/// The core sections and their keys (rule 12): everything else at the top level is a domain table
/// or an unknown key.
const TOP_KEYS: &[&str] = &[
    "florn",
    "name",
    "domains",
    "description",
    "next-identity",
    "time",
    "order",
    "seed",
    "limits",
    "modules",
    "actors",
    "connections",
    "changes",
    "views",
];
const TIME_KEYS: &[&str] = &["tick", "end"];
const ORDER_KEYS: &[&str] = &["tie", "microsteps-per-tick", "events-per-instant"];
const SEED_KEYS: &[&str] = &["root"];
const LIMIT_KEYS: &[&str] = &[
    "budget",
    "stack",
    "memory",
    "effects-per-activation",
    "payload",
    "pending-timers",
    "telemetry-per-activation",
];
const MODULE_KEYS: &[&str] = &["sha256", "contract"];
const ACTOR_KEYS: &[&str] = &[
    "id", "name", "kind", "module", "builtin", "class", "ports", "config",
];
const PORT_KEYS: &[&str] = &["id", "name", "type"];
const CONNECTION_KEYS: &[&str] = &["id", "name", "type", "ports", "attrs"];
const CHANGE_KEYS: &[&str] = &["at", "target", "payload"];

impl Parser {
    fn err(&mut self, kind: Kind, field: &str) {
        self.errors.push(Error::at(kind, field));
    }
    fn unknown_keys(&mut self, table: &toml::Table, allowed: &[&str], path: &str) {
        for key in table.keys() {
            if !allowed.contains(&key.as_str()) {
                let field = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                self.err(Kind::ManifestUnknownKey { key: field.clone() }, &field);
            }
        }
    }
    fn missing(&mut self, field: &str) {
        self.err(
            Kind::ManifestMissingField {
                field: field.to_string(),
            },
            field,
        );
    }
    fn table<'a>(
        &mut self,
        v: Option<&'a Value>,
        field: &str,
        required: bool,
    ) -> Option<&'a toml::Table> {
        match v {
            Some(Value::Table(t)) => Some(t),
            Some(_) => {
                self.err(
                    Kind::ManifestType {
                        field: field.into(),
                        expected: "table",
                    },
                    field,
                );
                None
            }
            None => {
                if required {
                    self.missing(field);
                }
                None
            }
        }
    }
    fn string(&mut self, v: Option<&Value>, field: &str, required: bool) -> Option<String> {
        match v {
            Some(Value::String(s)) => {
                if s.len() > maxima::NAME_BYTES {
                    self.err(
                        Kind::ManifestTooLarge {
                            what: "text-bytes",
                            found: s.len() as u64,
                            maximum: maxima::NAME_BYTES as u64,
                        },
                        field,
                    );
                    return None;
                }
                Some(s.clone())
            }
            Some(_) => {
                self.err(
                    Kind::ManifestType {
                        field: field.into(),
                        expected: "text",
                    },
                    field,
                );
                None
            }
            None => {
                if required {
                    self.missing(field);
                }
                None
            }
        }
    }
    fn integer(&mut self, v: Option<&Value>, field: &str, required: bool) -> Option<u64> {
        match v {
            Some(Value::Integer(i)) if *i >= 0 => Some(*i as u64),
            Some(_) => {
                self.err(
                    Kind::ManifestType {
                        field: field.into(),
                        expected: "unsigned-integer",
                    },
                    field,
                );
                None
            }
            None => {
                if required {
                    self.missing(field);
                }
                None
            }
        }
    }
    fn identity(&mut self, v: Option<&Value>, field: &str) -> Option<u32> {
        let i = self.integer(v, field, true)?;
        match u32::try_from(i) {
            Ok(id) if id != IDENTITY_NONE && id < IDENTITY_RESERVED_FROM => Some(id),
            Ok(id) => {
                self.err(Kind::IdentityReserved { identity: id }, field);
                None
            }
            Err(_) => {
                self.err(
                    Kind::ManifestType {
                        field: field.into(),
                        expected: "identity",
                    },
                    field,
                );
                None
            }
        }
    }
    fn quantity(
        &mut self,
        v: Option<&Value>,
        field: &str,
        dimension: &'static str,
        required: bool,
    ) -> Option<Quantity> {
        match v {
            Some(Value::String(s)) => match Quantity::parse(s) {
                Ok(q) => Some(q),
                Err(QuantityError::MissingUnit) => {
                    self.err(
                        Kind::QuantityMissingUnit {
                            field: field.into(),
                            dimension,
                        },
                        field,
                    );
                    None
                }
                Err(QuantityError::Malformed) => {
                    self.err(
                        Kind::ManifestType {
                            field: field.into(),
                            expected: "quantity",
                        },
                        field,
                    );
                    None
                }
            },
            Some(Value::Integer(_)) | Some(Value::Float(_)) => {
                self.err(
                    Kind::QuantityMissingUnit {
                        field: field.into(),
                        dimension,
                    },
                    field,
                );
                None
            }
            Some(_) => {
                self.err(
                    Kind::ManifestType {
                        field: field.into(),
                        expected: "quantity",
                    },
                    field,
                );
                None
            }
            None => {
                if required {
                    self.missing(field);
                }
                None
            }
        }
    }
    /// A duration field in ticks (rule 25).
    fn ticks(
        &mut self,
        v: Option<&Value>,
        field: &str,
        tick: TickLength,
        required: bool,
    ) -> Option<u64> {
        let q = self.quantity(v, field, "time", required)?;
        match q.to_ticks(tick) {
            Ok(Some(t)) => Some(t),
            Ok(None) => {
                self.err(
                    Kind::QuantityOutOfRange {
                        field: field.into(),
                        value: q.as_pair(),
                    },
                    field,
                );
                None
            }
            Err(None) => {
                self.err(
                    Kind::QuantityUnknownUnit {
                        field: field.into(),
                        unit: q.unit.clone(),
                    },
                    field,
                );
                None
            }
            Err(Some(smallest)) => {
                self.err(
                    Kind::QuantityNotWholeTicks {
                        field: field.into(),
                        value: q.as_pair(),
                        tick: tick.as_quantity(),
                        smallest: smallest.as_quantity(),
                    },
                    field,
                );
                None
            }
        }
    }
    /// A byte field (rule 26).
    fn bytes(&mut self, v: Option<&Value>, field: &str) -> Option<u64> {
        let q = self.quantity(v, field, "bytes", false)?;
        match q.to_bytes() {
            Ok(Some(b)) => Some(b),
            Ok(None) => {
                self.err(
                    Kind::QuantityOutOfRange {
                        field: field.into(),
                        value: q.as_pair(),
                    },
                    field,
                );
                None
            }
            Err(false) => {
                self.err(
                    Kind::QuantityUnknownUnit {
                        field: field.into(),
                        unit: q.unit.clone(),
                    },
                    field,
                );
                None
            }
            Err(true) => {
                self.err(
                    Kind::QuantityNotWholeUnit {
                        field: field.into(),
                        value: q.as_pair(),
                        unit: "B".into(),
                    },
                    field,
                );
                None
            }
        }
    }
    fn array<'a>(
        &mut self,
        v: Option<&'a Value>,
        field: &str,
        max: usize,
        what: &'static str,
    ) -> Option<&'a [Value]> {
        match v {
            Some(Value::Array(a)) => {
                if a.len() > max {
                    self.err(
                        Kind::ManifestTooLarge {
                            what,
                            found: a.len() as u64,
                            maximum: max as u64,
                        },
                        field,
                    );
                    return None;
                }
                Some(a)
            }
            Some(_) => {
                self.err(
                    Kind::ManifestType {
                        field: field.into(),
                        expected: "list",
                    },
                    field,
                );
                None
            }
            None => None,
        }
    }
    fn limit(&mut self, v: Option<u64>, field: &str, name: &str, maximum: u64) -> Option<u64> {
        let v = v?;
        if v > maximum {
            self.err(
                Kind::LimitAboveMaximum {
                    limit: name.into(),
                    value: v,
                    maximum,
                },
                field,
            );
            return None;
        }
        Some(v)
    }

    fn manifest(&mut self, t: &toml::Table) -> Manifest {
        self.unknown_keys(t, TOP_KEYS, "");
        // Header (§3.2)
        let schema = self
            .integer(t.get("florn"), "florn", true)
            .unwrap_or(SCHEMA_VERSION);
        if schema != SCHEMA_VERSION {
            self.err(
                Kind::ManifestSchemaVersion {
                    found: schema,
                    supported: SCHEMA_VERSION,
                },
                "florn",
            );
        }
        let name = self.string(t.get("name"), "name", true).unwrap_or_default();
        let mut domains = Vec::new();
        if let Some(list) = self.array(t.get("domains"), "domains", maxima::DOMAINS, "domains") {
            for (i, d) in list.iter().enumerate() {
                if let Some(s) = self.string(Some(d), &format!("domains[{i}]"), true) {
                    if domains.contains(&s) {
                        self.err(
                            Kind::NameDuplicate {
                                name: s.clone(),
                                kind: "domain",
                            },
                            &format!("domains[{i}]"),
                        );
                    } else {
                        domains.push(s);
                    }
                }
            }
        } else if t.get("domains").is_none() {
            self.missing("domains");
        }
        let description = self.string(t.get("description"), "description", false);
        let next_identity = self
            .integer(t.get("next-identity"), "next-identity", false)
            .and_then(|n| u32::try_from(n).ok());

        // [time] (§3.3)
        let (tick, end) = match self.table(t.get("time"), "time", true) {
            Some(tt) => {
                self.unknown_keys(tt, TIME_KEYS, "time");
                let tick = match self.string(tt.get("tick"), "time.tick", true) {
                    Some(s) => match TickLength::parse(&s) {
                        Some(tl) => tl,
                        None => {
                            self.err(
                                Kind::ManifestType {
                                    field: "time.tick".into(),
                                    expected: "tick-length",
                                },
                                "time.tick",
                            );
                            TickLength::Ns
                        }
                    },
                    None => TickLength::Ns,
                };
                let end = self
                    .ticks(tt.get("end"), "time.end", tick, true)
                    .unwrap_or(u64::MAX);
                (tick, end)
            }
            None => (TickLength::Ns, u64::MAX),
        };

        // [order]
        let order = match self.table(t.get("order"), "order", true) {
            Some(ot) => {
                self.unknown_keys(ot, ORDER_KEYS, "order");
                let tie = match self.string(ot.get("tie"), "order.tie", true).as_deref() {
                    Some("by-identity") => TiePolicy::ByIdentity,
                    Some("by-seed") => TiePolicy::BySeed,
                    Some(_) => {
                        self.err(
                            Kind::ManifestType {
                                field: "order.tie".into(),
                                expected: "tie-policy",
                            },
                            "order.tie",
                        );
                        TiePolicy::ByIdentity
                    }
                    None => TiePolicy::ByIdentity,
                };
                let m = self.integer(
                    ot.get("microsteps-per-tick"),
                    "order.microsteps-per-tick",
                    false,
                );
                let m = self.limit(
                    m,
                    "order.microsteps-per-tick",
                    "microsteps-per-tick",
                    self.host.microsteps_per_tick,
                );
                let e = self.integer(
                    ot.get("events-per-instant"),
                    "order.events-per-instant",
                    false,
                );
                let e = self.limit(
                    e,
                    "order.events-per-instant",
                    "events-per-instant",
                    self.host.events_per_instant,
                );
                Order {
                    tie,
                    microsteps_per_tick: m,
                    events_per_instant: e,
                }
            }
            None => Order {
                tie: TiePolicy::ByIdentity,
                microsteps_per_tick: None,
                events_per_instant: None,
            },
        };

        // [seed]
        let seed = match self.table(t.get("seed"), "seed", true) {
            Some(st) => {
                self.unknown_keys(st, SEED_KEYS, "seed");
                self.integer(st.get("root"), "seed.root", true).unwrap_or(0)
            }
            None => 0,
        };

        // [limits.<class>] (§3.4)
        let mut limits = BTreeMap::new();
        if let Some(lt) = self.table(t.get("limits"), "limits", false) {
            if lt.len() > maxima::LIMIT_CLASSES {
                self.err(
                    Kind::ManifestTooLarge {
                        what: "limit-classes",
                        found: lt.len() as u64,
                        maximum: maxima::LIMIT_CLASSES as u64,
                    },
                    "limits",
                );
            } else {
                for (class, v) in lt {
                    let path = format!("limits.{class}");
                    if let Some(ct) = self.table(Some(v), &path, true) {
                        self.unknown_keys(ct, LIMIT_KEYS, &path);
                        let host = self.host;
                        let f = |k: &str| format!("{path}.{k}");
                        let budget = self.integer(ct.get("budget"), &f("budget"), false);
                        let stack = self.integer(ct.get("stack"), &f("stack"), false);
                        let memory = self.bytes(ct.get("memory"), &f("memory"));
                        let effects = self.integer(
                            ct.get("effects-per-activation"),
                            &f("effects-per-activation"),
                            false,
                        );
                        let payload = self.bytes(ct.get("payload"), &f("payload"));
                        let timers =
                            self.integer(ct.get("pending-timers"), &f("pending-timers"), false);
                        let telemetry = self.bytes(
                            ct.get("telemetry-per-activation"),
                            &f("telemetry-per-activation"),
                        );
                        let l = Limits {
                            budget: self.limit(budget, &f("budget"), "budget", host.budget),
                            stack: self.limit(stack, &f("stack"), "stack", host.stack),
                            memory_bytes: self
                                .limit(memory, &f("memory"), "memory", host.memory_bytes)
                                .and_then(|b| {
                                    if b % 65_536 != 0 {
                                        self.err(
                                            Kind::QuantityNotWholeUnit {
                                                field: f("memory"),
                                                value: (b.to_string(), "B".into()),
                                                unit: "64KiB".into(),
                                            },
                                            &f("memory"),
                                        );
                                        None
                                    } else {
                                        Some(b)
                                    }
                                }),
                            effects_per_activation: self.limit(
                                effects,
                                &f("effects-per-activation"),
                                "effects-per-activation",
                                host.effects_per_activation,
                            ),
                            payload_bytes: self.limit(
                                payload,
                                &f("payload"),
                                "payload",
                                host.payload_bytes,
                            ),
                            pending_timers: self.limit(
                                timers,
                                &f("pending-timers"),
                                "pending-timers",
                                host.pending_timers,
                            ),
                            telemetry_per_activation_bytes: self.limit(
                                telemetry,
                                &f("telemetry-per-activation"),
                                "telemetry-per-activation",
                                host.telemetry_per_activation_bytes,
                            ),
                        };
                        limits.insert(class.clone(), l);
                    }
                }
            }
        }

        // [modules.<name>] (§3.5)
        let mut modules = BTreeMap::new();
        if let Some(mt) = self.table(t.get("modules"), "modules", false) {
            if mt.len() > maxima::MODULES {
                self.err(
                    Kind::ManifestTooLarge {
                        what: "modules",
                        found: mt.len() as u64,
                        maximum: maxima::MODULES as u64,
                    },
                    "modules",
                );
            } else {
                for (name, v) in mt {
                    let path = format!("modules.{name}");
                    if let Some(m) = self.table(Some(v), &path, true) {
                        self.unknown_keys(m, MODULE_KEYS, &path);
                        let sha = self.string(m.get("sha256"), &format!("{path}.sha256"), true);
                        let contract =
                            self.string(m.get("contract"), &format!("{path}.contract"), true);
                        let sha256 = sha.and_then(|s| match parse_hex32(&s) {
                            Some(b) => Some(b),
                            None => {
                                self.err(
                                    Kind::ManifestType {
                                        field: format!("{path}.sha256"),
                                        expected: "sha256-hex",
                                    },
                                    &format!("{path}.sha256"),
                                );
                                None
                            }
                        });
                        if let (Some(sha256), Some(contract)) = (sha256, contract) {
                            modules.insert(
                                name.clone(),
                                Module {
                                    name: name.clone(),
                                    sha256,
                                    contract,
                                },
                            );
                        }
                    }
                }
            }
        }

        // [[actors]] (§3.6)
        let mut actors: Vec<Actor> = Vec::new();
        let mut identities: BTreeMap<u32, String> = BTreeMap::new();
        let mut actor_names: BTreeSet<String> = BTreeSet::new();
        if let Some(list) = self.array(t.get("actors"), "actors", maxima::ACTORS, "actors") {
            for (i, v) in list.iter().enumerate() {
                let path = format!("actors[{i}]");
                let Some(at) = self.table(Some(v), &path, true) else {
                    continue;
                };
                self.unknown_keys(at, ACTOR_KEYS, &path);
                let id = self.identity(at.get("id"), &format!("{path}.id"));
                let name = self.string(at.get("name"), &format!("{path}.name"), true);
                let kind = match self
                    .string(at.get("kind"), &format!("{path}.kind"), true)
                    .as_deref()
                {
                    Some("module") => {
                        let m = self.string(at.get("module"), &format!("{path}.module"), true);
                        m.map(|m| {
                            if !modules.contains_key(&m) {
                                self.err(
                                    Kind::ReferenceUnresolved {
                                        reference: m.clone(),
                                        kind: "module",
                                    },
                                    &format!("{path}.module"),
                                );
                            }
                            ActorKind::Module(m)
                        })
                    }
                    Some("builtin") => {
                        let b = self.string(at.get("builtin"), &format!("{path}.builtin"), true);
                        b.map(|b| {
                            match b.split_once('/') {
                                Some((d, _)) if domains.iter().any(|x| x == d) => {}
                                Some((d, _)) => self.err(
                                    Kind::DomainUndeclared {
                                        domain: d.into(),
                                        where_: format!("{path}.builtin"),
                                    },
                                    &format!("{path}.builtin"),
                                ),
                                None => self.err(
                                    Kind::ManifestType {
                                        field: format!("{path}.builtin"),
                                        expected: "domain/name",
                                    },
                                    &format!("{path}.builtin"),
                                ),
                            }
                            ActorKind::Builtin(b)
                        })
                    }
                    Some(_) => {
                        self.err(
                            Kind::ManifestType {
                                field: format!("{path}.kind"),
                                expected: "actor-kind",
                            },
                            &format!("{path}.kind"),
                        );
                        None
                    }
                    None => None,
                };
                let class = self
                    .string(at.get("class"), &format!("{path}.class"), false)
                    .unwrap_or_else(|| "default".into());
                if class != "default" && !limits.contains_key(&class) {
                    self.err(
                        Kind::ReferenceUnresolved {
                            reference: class.clone(),
                            kind: "limit-class",
                        },
                        &format!("{path}.class"),
                    );
                }
                let mut ports = Vec::new();
                let mut port_ids = BTreeSet::new();
                let mut port_names = BTreeSet::new();
                if let Some(plist) = self.array(
                    at.get("ports"),
                    &format!("{path}.ports"),
                    maxima::PORTS_PER_ACTOR,
                    "ports",
                ) {
                    for (j, pv) in plist.iter().enumerate() {
                        let ppath = format!("{path}.ports[{j}]");
                        let Some(pt) = self.table(Some(pv), &ppath, true) else {
                            continue;
                        };
                        self.unknown_keys(pt, PORT_KEYS, &ppath);
                        let pid = self
                            .integer(pt.get("id"), &format!("{ppath}.id"), true)
                            .and_then(|x| u32::try_from(x).ok());
                        let pname = self.string(pt.get("name"), &format!("{ppath}.name"), true);
                        let ptype = self.string(pt.get("type"), &format!("{ppath}.type"), true);
                        if let Some(ty) = &ptype {
                            match ty.split_once('/') {
                                Some((d, _)) if domains.iter().any(|x| x == d) => {}
                                Some((d, _)) => self.err(
                                    Kind::DomainUndeclared {
                                        domain: d.into(),
                                        where_: format!("{ppath}.type"),
                                    },
                                    &format!("{ppath}.type"),
                                ),
                                None => self.err(
                                    Kind::ManifestType {
                                        field: format!("{ppath}.type"),
                                        expected: "domain/name",
                                    },
                                    &format!("{ppath}.type"),
                                ),
                            }
                        }
                        if let (Some(pid), Some(pname), Some(ptype)) = (pid, pname, ptype) {
                            if !port_ids.insert(pid) {
                                self.err(
                                    Kind::IdentityPortDuplicate {
                                        actor: id.unwrap_or(0),
                                        port: pid,
                                    },
                                    &format!("{ppath}.id"),
                                );
                            }
                            if !port_names.insert(pname.clone()) {
                                self.err(
                                    Kind::NameDuplicate {
                                        name: pname.clone(),
                                        kind: "port",
                                    },
                                    &format!("{ppath}.name"),
                                );
                            }
                            ports.push(Port {
                                id: pid,
                                name: pname,
                                r#type: ptype,
                            });
                        }
                    }
                }
                let config = match at.get("config") {
                    Some(v) => domain_map(v, &mut self.errors, &format!("{path}.config")),
                    None => BTreeMap::new(),
                };
                if let (Some(id), Some(name), Some(kind)) = (id, name, kind) {
                    // A duplicate is reported and kept, so that references to it still resolve and
                    // the report shows the cause, not its consequences. The manifest is refused anyway.
                    if let Some(first) = identities.get(&id) {
                        self.err(
                            Kind::IdentityDuplicate {
                                identity: id,
                                first: first.clone(),
                                second: name.clone(),
                            },
                            &format!("{path}.id"),
                        );
                    } else {
                        identities.insert(id, name.clone());
                    }
                    if !actor_names.insert(name.clone()) {
                        self.err(
                            Kind::NameDuplicate {
                                name: name.clone(),
                                kind: "actor",
                            },
                            &format!("{path}.name"),
                        );
                    }
                    actors.push(Actor {
                        id,
                        name,
                        kind,
                        class,
                        ports,
                        config,
                    });
                }
            }
        }
        actors.sort_by_key(|a| a.id);

        // [[connections]] (§3.7)
        let mut connections: Vec<Connection> = Vec::new();
        let mut connection_names: BTreeSet<String> = BTreeSet::new();
        let resolve_port = |reference: &str| -> Option<PortRef> {
            let (an, pn) = reference.split_once('.')?;
            let a = actors.iter().find(|a| a.name == an)?;
            let p = a.ports.iter().find(|p| p.name == pn)?;
            Some(PortRef {
                actor: a.id,
                port: p.id,
            })
        };
        if let Some(list) = self.array(
            t.get("connections"),
            "connections",
            maxima::CONNECTIONS,
            "connections",
        ) {
            for (i, v) in list.iter().enumerate() {
                let path = format!("connections[{i}]");
                let Some(ct) = self.table(Some(v), &path, true) else {
                    continue;
                };
                self.unknown_keys(ct, CONNECTION_KEYS, &path);
                let id = self.identity(ct.get("id"), &format!("{path}.id"));
                let name = self.string(ct.get("name"), &format!("{path}.name"), true);
                // RFC 008 rule 8: a type's domain must be declared.
                let ctype = self.string(ct.get("type"), &format!("{path}.type"), false);
                if let Some(t) = &ctype {
                    let d = domain_of(t);
                    if d != "core" && !domains.iter().any(|x| x == d) {
                        self.err(
                            Kind::DomainUndeclared {
                                domain: d.to_string(),
                                where_: format!("{path}.type"),
                            },
                            &format!("{path}.type"),
                        );
                    }
                }
                let mut ports = Vec::new();
                if let Some(plist) = self.array(
                    ct.get("ports"),
                    &format!("{path}.ports"),
                    maxima::PORTS_PER_CONNECTION,
                    "ports",
                ) {
                    for (j, pv) in plist.iter().enumerate() {
                        let ppath = format!("{path}.ports[{j}]");
                        if let Some(r) = self.string(Some(pv), &ppath, true) {
                            match resolve_port(&r) {
                                Some(pr) => ports.push(pr),
                                None => self.err(
                                    Kind::ReferenceUnresolved {
                                        reference: r,
                                        kind: "port",
                                    },
                                    &ppath,
                                ),
                            }
                        }
                    }
                } else if ct.get("ports").is_none() {
                    self.missing(&format!("{path}.ports"));
                }
                let (attrs, per_port) = match ct.get("attrs") {
                    Some(Value::Table(a)) => {
                        let mut a = a.clone();
                        let per = a.remove("per-port");
                        let attrs = domain_map(
                            &Value::Table(a),
                            &mut self.errors,
                            &format!("{path}.attrs"),
                        );
                        let mut per_port = BTreeMap::new();
                        if let Some(Value::Table(pp)) = per {
                            for (k, v) in &pp {
                                match resolve_port(k) {
                                    Some(pr) if ports.contains(&pr) => {
                                        per_port.insert(
                                            pr,
                                            domain_map(
                                                v,
                                                &mut self.errors,
                                                &format!("{path}.attrs.per-port.{k}"),
                                            ),
                                        );
                                    }
                                    _ => self.err(
                                        Kind::ReferenceUnresolved {
                                            reference: k.clone(),
                                            kind: "port",
                                        },
                                        &format!("{path}.attrs.per-port.{k}"),
                                    ),
                                }
                            }
                        } else if per.is_some() {
                            self.err(
                                Kind::ManifestType {
                                    field: format!("{path}.attrs.per-port"),
                                    expected: "table",
                                },
                                &format!("{path}.attrs.per-port"),
                            );
                        }
                        (attrs, per_port)
                    }
                    Some(_) => {
                        self.err(
                            Kind::ManifestType {
                                field: format!("{path}.attrs"),
                                expected: "table",
                            },
                            &format!("{path}.attrs"),
                        );
                        (BTreeMap::new(), BTreeMap::new())
                    }
                    None => (BTreeMap::new(), BTreeMap::new()),
                };
                if let (Some(id), Some(name)) = (id, name) {
                    if ports.len() < 2 {
                        self.err(
                            Kind::ConnectionTooFewPorts {
                                connection: id,
                                ports: ports.len() as u64,
                            },
                            &format!("{path}.ports"),
                        );
                    }
                    // RFC 008 rule 2: one domain per connection, the type's and every port's.
                    let mut seen: Vec<&str> = ctype.as_deref().map(domain_of).into_iter().collect();
                    for pr in &ports {
                        let d = actors
                            .iter()
                            .find(|a| a.id == pr.actor)
                            .and_then(|a| a.ports.iter().find(|p| p.id == pr.port))
                            .map(|p| domain_of(&p.r#type));
                        if let Some(d) = d
                            && !seen.contains(&d)
                        {
                            seen.push(d);
                        }
                    }
                    if seen.len() > 1 {
                        self.err(
                            Kind::ConnectionMixedTypes {
                                connection: name.clone(),
                            },
                            &format!("{path}.ports"),
                        );
                    }
                    if let Some(first) = identities.get(&id) {
                        self.err(
                            Kind::IdentityDuplicate {
                                identity: id,
                                first: first.clone(),
                                second: name.clone(),
                            },
                            &format!("{path}.id"),
                        );
                    } else {
                        identities.insert(id, name.clone());
                    }
                    if !connection_names.insert(name.clone()) {
                        self.err(
                            Kind::NameDuplicate {
                                name: name.clone(),
                                kind: "connection",
                            },
                            &format!("{path}.name"),
                        );
                    }
                    connections.push(Connection {
                        id,
                        name,
                        r#type: ctype,
                        ports,
                        attrs,
                        per_port,
                    });
                }
            }
        }
        connections.sort_by_key(|c| c.id);

        // [[changes]] (§3.8)
        let mut changes = Vec::new();
        if let Some(list) = self.array(t.get("changes"), "changes", maxima::CHANGES, "changes") {
            for (i, v) in list.iter().enumerate() {
                let path = format!("changes[{i}]");
                let Some(ct) = self.table(Some(v), &path, true) else {
                    continue;
                };
                self.unknown_keys(ct, CHANGE_KEYS, &path);
                let at = self.ticks(ct.get("at"), &format!("{path}.at"), tick, true);
                let target = self
                    .string(ct.get("target"), &format!("{path}.target"), true)
                    .and_then(|tgt| {
                        if let Some(d) = tgt.strip_prefix("world:") {
                            match domains.iter().position(|x| x == d) {
                                Some(n) => Some(ChangeTarget::World(n as u32)),
                                None => {
                                    self.err(
                                        Kind::DomainUndeclared {
                                            domain: d.into(),
                                            where_: format!("{path}.target"),
                                        },
                                        &format!("{path}.target"),
                                    );
                                    None
                                }
                            }
                        } else if let Some(a) = actors.iter().find(|a| a.name == tgt) {
                            Some(ChangeTarget::Actor(a.id))
                        } else if let Some(c) = connections.iter().find(|c| c.name == tgt) {
                            Some(ChangeTarget::Connection(c.id))
                        } else {
                            self.err(
                                Kind::ReferenceUnresolved {
                                    reference: tgt,
                                    kind: "change-target",
                                },
                                &format!("{path}.target"),
                            );
                            None
                        }
                    });
                let payload = match ct.get("payload") {
                    Some(v) => domain_map(v, &mut self.errors, &format!("{path}.payload")),
                    None => BTreeMap::new(),
                };
                if let (Some(at), Some(target)) = (at, target) {
                    if at >= end {
                        self.err(
                            Kind::TimeEndBeforeChange {
                                change: path.clone(),
                                at,
                                end,
                            },
                            &format!("{path}.at"),
                        );
                    }
                    changes.push(Change {
                        at,
                        target,
                        payload,
                    });
                }
            }
        }

        // [views] (rule 21) and the domain tables (rule 22)
        let views = match t.get("views") {
            Some(v) => domain_map(v, &mut self.errors, "views"),
            None => BTreeMap::new(),
        };
        let mut domain_tables = BTreeMap::new();
        for (key, v) in t {
            if !TOP_KEYS.contains(&key.as_str()) && domains.iter().any(|d| d == key) {
                domain_tables.insert(key.clone(), domain_map(v, &mut self.errors, key));
            }
        }
        // An unknown top-level key that is not a declared domain was already reported by unknown_keys;
        // undo that report for declared domains.
        self.errors.0.retain(|e| !matches!(&e.kind, Kind::ManifestUnknownKey { key } if domains.iter().any(|d| d == key)));

        Manifest {
            schema,
            name,
            domains,
            description,
            next_identity,
            time: Time { tick, end },
            order,
            seed,
            limits,
            modules,
            actors,
            connections,
            changes,
            views,
            domain_tables,
        }
    }
}

fn parse_hex32(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

/// A domain field: a TOML table to the opaque tree of rule 32. Dates and floats become text and
/// shortest decimals; nothing is interpreted.
fn domain_map(v: &Value, errors: &mut Errors, path: &str) -> DomainMap {
    match v {
        Value::Table(t) => t
            .iter()
            .map(|(k, v)| (k.clone(), domain_value(v)))
            .collect(),
        _ => {
            errors.push(Error::at(
                Kind::ManifestType {
                    field: path.into(),
                    expected: "table",
                },
                path,
            ));
            BTreeMap::new()
        }
    }
}

fn domain_value(v: &Value) -> DomainValue {
    match v {
        Value::String(s) => DomainValue::Text(s.clone()),
        Value::Integer(i) => DomainValue::Integer(*i),
        Value::Float(f) => DomainValue::Decimal(shortest_float(*f)),
        Value::Boolean(b) => DomainValue::Boolean(*b),
        Value::Datetime(d) => DomainValue::Text(d.to_string()),
        Value::Array(a) => DomainValue::List(a.iter().map(domain_value).collect()),
        Value::Table(t) => DomainValue::Map(
            t.iter()
                .map(|(k, v)| (k.clone(), domain_value(v)))
                .collect(),
        ),
    }
}

/// Shortest text that reads back as the same `f64` (RFC 001 rule 63); Rust's `Display` is that.
fn shortest_float(f: f64) -> String {
    if f.is_finite() {
        let s = format!("{f}");
        if s.contains('.') || s.contains('e') {
            s
        } else {
            format!("{s}.0")
        }
    } else {
        // NaN and infinities are not numbers a manifest can mean; make them one text each.
        if f.is_nan() {
            "nan".into()
        } else if f > 0.0 {
            "inf".into()
        } else {
            "-inf".into()
        }
    }
}
