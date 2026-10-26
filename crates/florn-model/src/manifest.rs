//! The parsed manifest (RFC 004 §1 to §3): the fields the core reads, typed; the domain fields
//! carried opaquely as [`DomainValue`] trees.

use std::collections::BTreeMap;

use crate::quantity::TickLength;

/// A domain field, opaque to the core (RFC 004 rule 32): an integer, a decimal in shortest text, a
/// text, a boolean, a list, or a map. Dates and quantities arrive as text; the domain parses them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DomainValue {
    Integer(i64),
    /// A decimal number, as its shortest round-trip text (RFC 001 rule 63).
    Decimal(String),
    Text(String),
    Boolean(bool),
    List(Vec<DomainValue>),
    Map(BTreeMap<String, DomainValue>),
}

pub type DomainMap = BTreeMap<String, DomainValue>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TiePolicy {
    ByIdentity,
    BySeed,
}

impl TiePolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ByIdentity => "by-identity",
            Self::BySeed => "by-seed",
        }
    }
}

/// `[time]` (RFC 004 §3.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Time {
    pub tick: TickLength,
    /// `end`, in ticks. The first event at or after it is not handled.
    pub end: u64,
}

/// `[order]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Order {
    pub tie: TiePolicy,
    pub microsteps_per_tick: Option<u64>,
    pub events_per_instant: Option<u64>,
}

/// One class of `[limits.<class>]` (RFC 004 §3.4). Every field optional: absent means the Host's default.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Limits {
    pub budget: Option<u64>,
    pub stack: Option<u64>,
    pub memory_bytes: Option<u64>,
    pub effects_per_activation: Option<u64>,
    pub payload_bytes: Option<u64>,
    pub pending_timers: Option<u64>,
    pub telemetry_per_activation_bytes: Option<u64>,
}

/// `[modules.<name>]` (RFC 004 §3.5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    /// 32 bytes, from the 64 hex digits.
    pub sha256: [u8; 32],
    /// `florn:network/router@0.1.0`
    pub contract: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActorKind {
    /// `kind = "module"`: the key of `[modules]`.
    Module(String),
    /// `kind = "builtin"`: `<domain>/<name>`.
    Builtin(String),
}

/// A port of an actor (RFC 004 rule 9).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Port {
    pub id: u32,
    pub name: String,
    /// `<domain>/<name>`
    pub r#type: String,
}

/// `[[actors]]` (RFC 004 §3.6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Actor {
    pub id: u32,
    pub name: String,
    pub kind: ActorKind,
    /// A key of `[limits]`; `default` when absent.
    pub class: String,
    pub ports: Vec<Port>,
    /// `[actors.config]`, a domain field.
    pub config: DomainMap,
}

/// A port reference of a connection, resolved: the actor's identity and the port's identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PortRef {
    pub actor: u32,
    pub port: u32,
}

/// The domain prefix of a type written `<domain>/<name>` (RFC 004 rule 5).
pub fn domain_of(ty: &str) -> &str {
    ty.split_once('/').map_or(ty, |(d, _)| d)
}

/// `[[connections]]` (RFC 004 §3.7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    pub id: u32,
    pub name: String,
    /// `<domain>/<name>`, read by the core and routed to the domain (RFC 004 rule 19 as RFC
    /// 008 rule 8 amends it); `None` when the manifest gives none. The connection's domain is
    /// its ports' either way (rule 2 of RFC 008: one domain per connection).
    pub r#type: Option<String>,
    /// In the order written; two or more.
    pub ports: Vec<PortRef>,
    /// `[connections.attrs]`, without `per-port`.
    pub attrs: DomainMap,
    /// `[connections.attrs.per-port]`, keyed by the resolved port reference.
    pub per_port: BTreeMap<PortRef, DomainMap>,
}

/// The target of a scheduled change (RFC 004 §3.8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeTarget {
    Actor(u32),
    Connection(u32),
    /// `world:<domain>`, as the index of the domain in `domains` (its world identity is `0xF000_0010 + n`, rule 8).
    World(u32),
}

impl ChangeTarget {
    /// The origin identity of the target (RFC 004 rule 8).
    pub fn identity(&self) -> u32 {
        match self {
            Self::Actor(i) | Self::Connection(i) => *i,
            Self::World(n) => WORLD_IDENTITY_BASE + n,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// In ticks.
    pub at: u64,
    pub target: ChangeTarget,
    pub payload: DomainMap,
}

/// The reserved identities of RFC 004 rule 8.
pub const IDENTITY_NONE: u32 = 0;
pub const IDENTITY_RESERVED_FROM: u32 = 0xF000_0000;
pub const IDENTITY_INTERVENTIONS: u32 = 0xF000_0000;
pub const IDENTITY_SCHEDULED_CHANGES: u32 = 0xF000_0001;
pub const WORLD_IDENTITY_BASE: u32 = 0xF000_0010;

/// The manifest schema version this crate reads (RFC 004 rule 13).
pub const SCHEMA_VERSION: u64 = 1;

/// A parsed, validated scenario manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub schema: u64,
    pub name: String,
    /// In declaration order: the n-th domain's world has identity `0xF000_0010 + n`.
    pub domains: Vec<String>,
    pub description: Option<String>,
    pub next_identity: Option<u32>,
    pub time: Time,
    pub order: Order,
    pub seed: u64,
    /// By class name; `default` applies to every class without its own table.
    pub limits: BTreeMap<String, Limits>,
    pub modules: BTreeMap<String, Module>,
    /// Sorted by identity.
    pub actors: Vec<Actor>,
    /// Sorted by identity.
    pub connections: Vec<Connection>,
    /// In the order written.
    pub changes: Vec<Change>,
    /// `[views]`: not read, not hashed (rule 21).
    pub views: DomainMap,
    /// One table per declared domain, opaque (rule 22).
    pub domain_tables: BTreeMap<String, DomainMap>,
}

impl Manifest {
    pub fn actor(&self, id: u32) -> Option<&Actor> {
        self.actors
            .binary_search_by_key(&id, |a| a.id)
            .ok()
            .map(|i| &self.actors[i])
    }
    pub fn connection(&self, id: u32) -> Option<&Connection> {
        self.connections
            .binary_search_by_key(&id, |c| c.id)
            .ok()
            .map(|i| &self.connections[i])
    }
    /// The port of an actor, by identities.
    pub fn port(&self, actor: u32, port: u32) -> Option<&Port> {
        self.actor(actor)?.ports.iter().find(|p| p.id == port)
    }
    /// The domain of a connection: its type's, or its ports' (validation keeps them one);
    /// `core` for a connection of core ports.
    pub fn connection_domain<'a>(&'a self, c: &'a Connection) -> &'a str {
        if let Some(t) = &c.r#type {
            return domain_of(t);
        }
        c.ports
            .first()
            .and_then(|pr| self.port(pr.actor, pr.port))
            .map_or("core", |p| domain_of(&p.r#type))
    }
    pub fn actor_by_name(&self, name: &str) -> Option<&Actor> {
        self.actors.iter().find(|a| a.name == name)
    }
    /// The limits that apply to a class: its own table over `default` over the Host's defaults
    /// (which the caller supplies as `None` fields).
    pub fn limits_for(&self, class: &str) -> Limits {
        let default = self.limits.get("default").cloned().unwrap_or_default();
        let own = self.limits.get(class).cloned().unwrap_or_default();
        Limits {
            budget: own.budget.or(default.budget),
            stack: own.stack.or(default.stack),
            memory_bytes: own.memory_bytes.or(default.memory_bytes),
            effects_per_activation: own
                .effects_per_activation
                .or(default.effects_per_activation),
            payload_bytes: own.payload_bytes.or(default.payload_bytes),
            pending_timers: own.pending_timers.or(default.pending_timers),
            telemetry_per_activation_bytes: own
                .telemetry_per_activation_bytes
                .or(default.telemetry_per_activation_bytes),
        }
    }
    /// The world identity of a declared domain (RFC 004 rule 8).
    pub fn world_identity(&self, domain: &str) -> Option<u32> {
        self.domains
            .iter()
            .position(|d| d == domain)
            .map(|n| WORLD_IDENTITY_BASE + n as u32)
    }
}
