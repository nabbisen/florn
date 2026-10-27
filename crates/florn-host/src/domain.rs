//! The domain interface (RFC 008 §2): what a domain crate implements, how the Host composes the
//! domains a scenario declares into one world, and how it routes to them. The kernel knows no
//! domain (ARCH-006); a domain reaches no adapter (RFC 005 rule 3).
//!
//! A domain is a value the Host builds for one run from a factory in [`Domains`]. The Host calls
//! its operations as RFC 008 rule 1 lists them: the types and codes at validation, the units at
//! config encoding, the classes and the world's signatures at composition, `deliver` per
//! committed send, `change` and `intervene` at their instants, `event` when the kernel hands out
//! an event of its classes, `answer` inside an activation, `builtin` for its built-in actors,
//! `export` and `restore` at checkpoints.
//!
//! Implemented by PR-15 of `rfcs/handoffs/008-network-domain/task-breakdown-pr-plan.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use florn_abi::core_world::{self, Signature};
use florn_abi::{Ty, Val};
use florn_admission::config::Units;
use florn_admission::manifest::ModuleManifest;
use florn_admission::world::{FuncSig, ImportInterface, WorldInterfaces, WorldSpec};
use florn_control::display::Record as DisplayRecord;
use florn_control::{Ends, Notification};
use florn_kernel::{Delivery, EventKey, Identity, Instant, Sink, Tick, World};
use florn_model::manifest::{ActorKind, DomainMap, Port, domain_of};
use florn_model::{Decimal, Manifest, Param};

use crate::world::{CLASS_MESSAGE, CLASS_TIMER, CoreWorld};

/// A refusal a domain reports (RFC 004 §7; RFC 008 rule 18): a code of RFC 007 §4 with its
/// parameters, at a field of the manifest.
pub fn refusal(
    code: &str,
    field: impl Into<String>,
    params: Vec<(&'static str, Param)>,
) -> Notification {
    Notification::from_error(code, params, Some(field.into()), None, Ends::Command)
}

/// The world a domain adds to the core's (RFC 008 rule 6), in the type subset of RFC 002 rule
/// 7: the Host derives the flat signatures admission checks and the adapters bind from it.
#[derive(Clone, Debug, PartialEq)]
pub struct DomainWorld {
    /// `florn:network/router@0.1.0`
    pub name: String,
    /// The interface ids the world imports and exports beyond the core's, as the contract
    /// identity records them (RFC 002 rule 11).
    pub imports_ids: Vec<String>,
    pub exports_ids: Vec<String>,
    /// Imported interfaces: the module name and its functions.
    pub imports: Vec<(String, Vec<Signature>)>,
    /// Exported interfaces: the interface id and its functions; an export's canonical name is
    /// `<id>#<function>`.
    pub exports: Vec<(String, Vec<Signature>)>,
}

fn flat_sig(name: String, s: &Signature) -> FuncSig {
    let (params, results) = Ty::flat_signature(&s.param_types(), s.result.as_ref());
    FuncSig {
        name,
        params,
        results,
    }
}

impl DomainWorld {
    /// The admission specification: the core's imports and exports plus this world's.
    pub fn spec(&self) -> WorldSpec {
        let core = WorldSpec::core();
        let mut imports: Vec<&str> = core.interfaces.imports.iter().map(String::as_str).collect();
        imports.extend(self.imports_ids.iter().map(String::as_str));
        let mut exports: Vec<&str> = core.interfaces.exports.iter().map(String::as_str).collect();
        exports.extend(self.exports_ids.iter().map(String::as_str));
        let mut spec = WorldSpec {
            name: self.name.clone(),
            interfaces: WorldInterfaces::new(&imports, &exports),
            imports: core.imports,
            exports: core.exports,
        };
        for (module, functions) in &self.imports {
            spec.imports.push(ImportInterface {
                module: module.clone(),
                functions: functions
                    .iter()
                    .map(|s| flat_sig(s.name.to_string(), s))
                    .collect(),
            });
        }
        for (id, functions) in &self.exports {
            for s in functions {
                spec.exports.push(flat_sig(format!("{id}#{}", s.name), s));
            }
        }
        spec
    }
}

/// An event of a domain's class as the kernel hands it out (RFC 008 rule 1, `event`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainEvent {
    pub key: EventKey,
    pub target: Identity,
    pub payload: Vec<u8>,
}

/// What a domain asks the Host to do (RFC 008 rule 1: `change`, `event`).
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Activate an actor's export of the domain's world (`on-link`), with the values the
    /// canonical ABI lowers; nothing happens for an actor that is not alive.
    Call {
        actor: Identity,
        export: String,
        values: Vec<Val>,
    },
    /// Schedule an event of one of the domain's classes, `delay` ticks from now (zero is the
    /// next microstep), from `origin` (a connection or the world), to `target`.
    Schedule {
        origin: Identity,
        delay: Tick,
        class: u8,
        target: Identity,
        payload: Vec<u8>,
    },
    /// Something for the diagnostic log and the timeline (RFC 008 rule 3): never a guest input.
    Notify(Notification),
}

/// An activation of a built-in actor (RFC 008 rule 19).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuiltinEvent<'a> {
    Init {
        ports: &'a [Port],
        config: &'a DomainMap,
    },
    Message {
        port: u32,
        payload: &'a [u8],
    },
    Timer {
        timer: u32,
    },
    Domain(&'a DomainEvent),
}

/// A domain of the Host core (RFC 008 rule 1). Every operation has the default of a domain
/// without that feature; `name`, the types and `deliver` are the least a domain gives.
pub trait Domain {
    /// The prefix of its types: `network`.
    fn name(&self) -> &'static str;
    /// The connection types it owns, each `<domain>/<name>`.
    fn connection_types(&self) -> Vec<&'static str>;
    /// The port types it owns.
    fn port_types(&self) -> Vec<&'static str>;
    /// The built-in kinds it runs.
    fn builtin_kinds(&self) -> Vec<&'static str> {
        Vec::new()
    }
    /// The telemetry channels a built-in kind reports on (RFC 002 rule 32 for a guest).
    fn builtin_channels(&self, _kind: &str) -> Vec<u32> {
        Vec::new()
    }
    /// The intervention kinds it decodes (RFC 008 rule 4), beside the core's `message`.
    fn intervention_kinds(&self) -> Vec<&'static str> {
        Vec::new()
    }
    /// Its event classes with their names (RFC 001 rule 15); 0 to 2 are the core's.
    fn classes(&self) -> Vec<(u8, &'static str)> {
        Vec::new()
    }
    /// The units it registers, each with its factor to the SI unit of its dimension (RFC 004
    /// rules 23 to 27).
    fn units(&self) -> Vec<(&'static str, Decimal)> {
        Vec::new()
    }
    /// The world it adds (RFC 008 rule 6), if its guests need more than the core.
    fn world(&self) -> Option<DomainWorld> {
        None
    }
    /// Check every connection of its types, every actor of its port types, its top-level
    /// table and every built-in's ports and config; report every error in one pass.
    fn validate(&self, scenario: &Manifest) -> Vec<Notification>;
    /// Check an admitted module's manifest for what concerns the domain: its table (RFC 008
    /// rule 16) and the layouts of the channels the domain's views read (rule 17). A warning
    /// reaches the log; an error refuses the run.
    fn validate_module(&self, _module: &str, _manifest: &ModuleManifest) -> Vec<Notification> {
        Vec::new()
    }
    /// Build the run's state from the validated scenario: the connections' attributes and the
    /// streams derived for them (RFC 001 rules 28 to 31; `florn_kernel::StreamKey::derive`
    /// with the connection's identity and the purpose label).
    fn compose(&mut self, scenario: &Manifest, root_seed: u64);
    /// The deliveries of a committed `send` on a port of its types (RFC 008 §4), in the
    /// kernel's terms: target, class, delay and kind; `dead` holds the actors that faulted.
    fn deliver(
        &mut self,
        now: Instant,
        origin: Identity,
        seq: u64,
        port: u32,
        payload: &[u8],
        dead: &BTreeSet<Identity>,
    ) -> Vec<Delivery>;
    /// A scheduled change addressed to one of its connections or to its world (RFC 004 rule
    /// 20), at its instant.
    fn change(&mut self, now: Instant, target: Identity, payload: &DomainMap) -> Vec<Action>;
    /// Whether an intervention of one of its kinds is well formed, at request time (RFC 005
    /// rule 26): a refusal names the field.
    fn check_intervention(
        &self,
        target: Identity,
        kind: &str,
        content: &DomainMap,
    ) -> Result<(), Box<Notification>>;
    /// An intervention of one of its kinds, addressed to `target` (one of its connections or
    /// its world), at its instant (RFC 001 rule 35).
    fn intervene(
        &mut self,
        now: Instant,
        target: Identity,
        kind: &str,
        content: &DomainMap,
    ) -> Vec<Action>;
    /// An event of its classes: `alive` says whether the target actor can be activated.
    fn event(&mut self, event: &DomainEvent, alive: bool) -> Vec<Action>;
    /// An event of its classes that carried a message to an actor that faulted meanwhile:
    /// the frame is dropped at delivery (RFC 008 rule 5).
    fn undelivered(&mut self, _event: &DomainEvent) -> Vec<Action> {
        Vec::new()
    }
    /// An import of its world called by a guest, answered from the activation's starting state
    /// (RFC 002 rule 47). `None` refuses the call: the activation faults.
    fn answer(&mut self, actor: Identity, function: &str, args: &[Val]) -> Option<Val>;
    /// A built-in actor's activation: effects through the sink, as a guest's (RFC 008 rule 19).
    fn builtin(&mut self, actor: Identity, kind: &str, event: &BuiltinEvent<'_>, sink: &mut Sink);
    /// A payload's fields for the inspector, by the protocol a module declares (OBS-006).
    fn decode(&self, _protocol: &str, _payload: &[u8]) -> Option<Vec<(String, String)>> {
        None
    }
    /// The channel names and layouts its views read (RFC 008 §7): channel, then the fields
    /// with their types.
    fn telemetry(&self) -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
        Vec::new()
    }
    /// Its own state for a checkpoint (RFC 003 §6); empty when it has none.
    fn export(&self) -> Vec<u8> {
        Vec::new()
    }
    /// Its state from a checkpoint; `false` when the bytes are not its own.
    fn restore(&mut self, bytes: &[u8]) -> bool {
        bytes.is_empty()
    }
}

type Factory = Arc<dyn Fn() -> Box<dyn Domain>>;

/// The domains a Host provides: a factory per domain, run by name when a scenario declares it.
#[derive(Clone, Default)]
pub struct Domains {
    factories: Vec<(&'static str, Factory)>,
}

impl fmt::Debug for Domains {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.factories.iter().map(|(n, _)| n))
            .finish()
    }
}

impl Domains {
    /// The core alone: every 0.1 scenario.
    pub fn core() -> Self {
        Self::default()
    }

    /// With a domain. A second factory under the same name replaces the first.
    pub fn with<D: Domain + 'static>(mut self, make: impl Fn() -> D + 'static) -> Self {
        let name = make().name();
        self.factories.retain(|(n, _)| *n != name);
        self.factories
            .push((name, Arc::new(move || Box::new(make()) as Box<dyn Domain>)));
        self
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.factories.iter().map(|(n, _)| *n).collect()
    }

    /// The worlds every domain adds, for admission (RFC 008 rule 6).
    pub fn worlds(&self) -> Vec<WorldSpec> {
        self.factories
            .iter()
            .filter_map(|(_, make)| make().world().map(|w| w.spec()))
            .collect()
    }

    /// Fresh domain values for a scenario, in the scenario's declaration order. A declared
    /// domain this Host does not provide is `domain.unknown`.
    pub fn instantiate(
        &self,
        scenario: &Manifest,
    ) -> Result<Vec<Box<dyn Domain>>, Box<Notification>> {
        let mut out = Vec::new();
        for name in &scenario.domains {
            if name == "core" {
                continue;
            }
            match self.factories.iter().find(|(n, _)| n == name) {
                Some((_, make)) => out.push(make()),
                None => {
                    return Err(Box::new(refusal(
                        "domain.unknown",
                        "domains",
                        vec![("domain", Param::Name(name.clone()))],
                    )));
                }
            }
        }
        Ok(out)
    }
}

/// The units of every composed domain, for config encoding (RFC 004 rule 27).
pub struct DomainUnits<'a>(pub &'a [Box<dyn Domain>]);

impl Units for DomainUnits<'_> {
    fn to_si(&self, unit: &str) -> Option<Decimal> {
        self.0
            .iter()
            .flat_map(|d| d.units())
            .find(|(u, _)| *u == unit)
            .map(|(_, f)| f)
    }
}

/// Validate what the core routes to the domains (RFC 008 rule 1): a type of a declared domain
/// must be one the domain owns, then each domain checks its own.
pub fn validate(domains: &[Box<dyn Domain>], scenario: &Manifest) -> Vec<Notification> {
    let mut out = Vec::new();
    let owner = |ty: &str| domains.iter().find(|d| d.name() == domain_of(ty));
    for (i, c) in scenario.connections.iter().enumerate() {
        let Some(t) = &c.r#type else {
            continue;
        };
        if domain_of(t) == "core" {
            continue;
        }
        if !owner(t).is_some_and(|d| d.connection_types().contains(&t.as_str())) {
            out.push(refusal(
                "reference.unresolved",
                format!("connections[{i}].type"),
                vec![
                    ("reference", Param::Text(t.clone())),
                    ("kind", Param::Enum("connection")),
                ],
            ));
        }
    }
    for (i, a) in scenario.actors.iter().enumerate() {
        for (j, p) in a.ports.iter().enumerate() {
            if domain_of(&p.r#type) == "core" {
                continue;
            }
            if !owner(&p.r#type).is_some_and(|d| d.port_types().contains(&p.r#type.as_str())) {
                out.push(refusal(
                    "reference.unresolved",
                    format!("actors[{i}].ports[{j}].type"),
                    vec![
                        ("reference", Param::Text(p.r#type.clone())),
                        ("kind", Param::Enum("port")),
                    ],
                ));
            }
        }
        if let ActorKind::Builtin(kind) = &a.kind
            && !owner(kind).is_some_and(|d| d.builtin_kinds().contains(&kind.as_str()))
        {
            out.push(refusal(
                "reference.unresolved",
                format!("actors[{i}].builtin"),
                vec![
                    ("reference", Param::Text(kind.clone())),
                    ("kind", Param::Enum("actor")),
                ],
            ));
        }
    }
    for d in domains {
        out.extend(d.validate(scenario));
    }
    out
}

/// The signatures of one world as the Host calls and answers them: exports in the order the
/// slots resolve them (the core's three first), imports by name.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldSigs {
    pub name: String,
    /// Canonical export names, their parameter types and their flat shapes.
    pub exports: Vec<(String, Vec<Ty>, florn_abi::Shape)>,
    /// Import name, module, parameter types, result type.
    pub imports: Vec<(String, String, Vec<Ty>, Option<Ty>)>,
}

impl WorldSigs {
    pub fn core() -> Self {
        let mut w = Self {
            name: florn_admission::world::CORE_WORLD.to_string(),
            exports: Vec::new(),
            imports: Vec::new(),
        };
        for s in core_world::actor_functions() {
            w.push_export(format!("{}{}", core_world::ACTOR_EXPORT_PREFIX, s.name), &s);
        }
        for s in core_world::host_functions() {
            w.imports.push((
                s.name.to_string(),
                core_world::HOST_MODULE.to_string(),
                s.param_types(),
                s.result.clone(),
            ));
        }
        w
    }

    fn push_export(&mut self, name: String, s: &Signature) {
        let (params, results) = Ty::flat_signature(&s.param_types(), s.result.as_ref());
        let shape = florn_abi::Shape::of(&params, &results).expect("admission checked the shape");
        self.exports.push((name, s.param_types(), shape));
    }

    /// The core's signatures followed by a domain world's.
    pub fn of(world: &DomainWorld) -> Self {
        let mut w = Self::core();
        w.name = world.name.clone();
        for (id, functions) in &world.exports {
            for s in functions {
                w.push_export(format!("{id}#{}", s.name), s);
            }
        }
        for (module, functions) in &world.imports {
            for s in functions {
                w.imports.push((
                    s.name.to_string(),
                    module.clone(),
                    s.param_types(),
                    s.result.clone(),
                ));
            }
        }
        w
    }

    pub fn export_index(&self, name: &str) -> Option<usize> {
        self.exports.iter().position(|(n, _, _)| n == name)
    }
}

/// The composed world (RFC 008 rule 2): the core's connections and every domain's, dispatched
/// by the port's type; the routing tables for changes, interventions, events and built-ins.
pub struct Composite {
    core: CoreWorld,
    domains: Vec<Box<dyn Domain>>,
    by_port: BTreeMap<(Identity, u32), usize>,
    by_class: BTreeMap<u8, usize>,
    by_target: BTreeMap<Identity, usize>,
    by_kind: BTreeMap<String, usize>,
    by_module: BTreeMap<String, usize>,
    by_builtin: BTreeMap<Identity, (usize, String)>,
    class_names: BTreeMap<u8, &'static str>,
    /// Effects committed per domain origin (RFC 001 rule 17): connections and worlds.
    seq: BTreeMap<Identity, u64>,
    /// The actors that faulted, for `deliver`.
    pub dead: BTreeSet<Identity>,
    /// Each port's place for the display stream (RFC 005 rule 14): the connection's index in
    /// id order and the port's index in the connection, for the ports both fit.
    display_index: BTreeMap<(Identity, u32), (u16, u8)>,
    /// The flights of the batch in progress, when a front end subscribed to the display stream.
    display: Option<Vec<DisplayRecord>>,
}

impl Composite {
    /// Compose for a scenario: the classes must not collide (a Host defect, never a scenario's).
    pub fn new(
        scenario: &Manifest,
        mut domains: Vec<Box<dyn Domain>>,
        root_seed: u64,
    ) -> Result<Self, &'static str> {
        let mut class_names: BTreeMap<u8, &'static str> = [
            (0, "control"),
            (CLASS_TIMER, "timer"),
            (CLASS_MESSAGE, "message"),
        ]
        .into_iter()
        .collect();
        let mut by_class = BTreeMap::new();
        let mut by_kind = BTreeMap::new();
        let mut by_module = BTreeMap::new();
        for (i, d) in domains.iter().enumerate() {
            if let Some(w) = d.world() {
                for (module, _) in &w.imports {
                    if by_module.insert(module.clone(), i).is_some() {
                        return Err("import interface assigned twice");
                    }
                }
            }
            for (class, name) in d.classes() {
                if class_names.insert(class, name).is_some() {
                    return Err("event class assigned twice");
                }
                by_class.insert(class, i);
            }
            for kind in d.intervention_kinds() {
                if kind == "message" || by_kind.insert(kind.to_string(), i).is_some() {
                    return Err("intervention kind assigned twice");
                }
            }
        }
        let index = |ty: &str| domains.iter().position(|d| d.name() == domain_of(ty));
        let mut by_port = BTreeMap::new();
        let mut by_builtin = BTreeMap::new();
        for a in &scenario.actors {
            for p in &a.ports {
                if let Some(i) = index(&p.r#type) {
                    by_port.insert((a.id, p.id), i);
                }
            }
            if let ActorKind::Builtin(kind) = &a.kind
                && let Some(i) = index(kind)
            {
                by_builtin.insert(a.id, (i, kind.clone()));
            }
        }
        let mut by_target = BTreeMap::new();
        for c in &scenario.connections {
            if let Some(i) = index(scenario.connection_domain(c)) {
                by_target.insert(c.id, i);
            }
        }
        for (n, name) in scenario.domains.iter().enumerate() {
            if let Some(i) = domains.iter().position(|d| d.name() == name) {
                by_target.insert(florn_model::manifest::WORLD_IDENTITY_BASE + n as u32, i);
            }
        }
        for d in &mut domains {
            d.compose(scenario, root_seed);
        }
        let mut display_index = BTreeMap::new();
        for (i, c) in scenario.connections.iter().enumerate() {
            let Ok(connection) = u16::try_from(i) else {
                break;
            };
            for (j, p) in c.ports.iter().enumerate() {
                if let Ok(port) = u8::try_from(j) {
                    display_index.insert((p.actor, p.port), (connection, port));
                }
            }
        }
        Ok(Self {
            core: CoreWorld::from_scenario(scenario),
            domains,
            by_port,
            by_class,
            by_target,
            by_kind,
            by_module,
            by_builtin,
            class_names,
            seq: BTreeMap::new(),
            dead: BTreeSet::new(),
            display_index,
            display: None,
        })
    }

    /// Write the display stream's flights from now on, or stop.
    pub fn set_display(&mut self, on: bool) {
        self.display = on.then(Vec::new);
    }
    /// The flights since the last take, in order; empty when the display is off.
    pub fn take_display(&mut self) -> Vec<DisplayRecord> {
        self.display
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }

    pub fn core(&self) -> &CoreWorld {
        &self.core
    }
    pub fn domains(&self) -> &[Box<dyn Domain>] {
        &self.domains
    }
    pub fn domains_mut(&mut self) -> &mut [Box<dyn Domain>] {
        &mut self.domains
    }
    /// The class names of the composed world (RFC 001 rule 15).
    pub fn class_names(&self) -> &BTreeMap<u8, &'static str> {
        &self.class_names
    }
    /// The domain of an event class, if a domain owns it.
    pub fn of_class(&mut self, class: u8) -> Option<&mut dyn Domain> {
        let i = *self.by_class.get(&class)?;
        Some(self.domains[i].as_mut())
    }
    /// The domain of a change target (a connection or a world of its).
    pub fn of_target(&mut self, target: Identity) -> Option<&mut dyn Domain> {
        let i = *self.by_target.get(&target)?;
        Some(self.domains[i].as_mut())
    }
    pub fn of_kind(&self, kind: &str) -> Option<&dyn Domain> {
        let i = *self.by_kind.get(kind)?;
        Some(self.domains[i].as_ref())
    }
    /// The domain whose world imports `module` (RFC 008 rule 1, `answer`).
    pub fn of_module(&mut self, module: &str) -> Option<&mut dyn Domain> {
        let i = *self.by_module.get(module)?;
        Some(self.domains[i].as_mut())
    }
    pub fn of_kind_mut(&mut self, kind: &str) -> Option<&mut dyn Domain> {
        let i = *self.by_kind.get(kind)?;
        Some(self.domains[i].as_mut())
    }
    /// A built-in actor's domain and kind.
    pub fn builtin(&mut self, actor: Identity) -> Option<(&mut dyn Domain, String)> {
        let (i, kind) = self.by_builtin.get(&actor)?.clone();
        Some((self.domains[i].as_mut(), kind))
    }
    pub fn is_builtin(&self, actor: Identity) -> bool {
        self.by_builtin.contains_key(&actor)
    }
    /// The next sequence number of a domain origin (RFC 001 rule 17).
    pub fn next_seq(&mut self, origin: Identity) -> u64 {
        let n = self.seq.entry(origin).or_insert(0);
        *n += 1;
        *n
    }
    /// Every domain's state, by name, for a checkpoint; domains without state are left out.
    pub fn export(&self) -> BTreeMap<String, Vec<u8>> {
        self.domains
            .iter()
            .filter_map(|d| {
                let bytes = d.export();
                (!bytes.is_empty()).then(|| (d.name().to_string(), bytes))
            })
            .collect()
    }
    /// Restore every domain's state; `false` names a domain that refused its bytes.
    pub fn restore(&mut self, states: &BTreeMap<String, Vec<u8>>) -> bool {
        self.domains.iter_mut().all(|d| {
            let bytes = states.get(d.name()).map(Vec::as_slice).unwrap_or(&[]);
            d.restore(bytes)
        })
    }
}

impl World for Composite {
    fn deliver(
        &mut self,
        now: Instant,
        origin: Identity,
        seq: u64,
        port: u32,
        payload: &[u8],
    ) -> Vec<Delivery> {
        let deliveries = match self.by_port.get(&(origin, port)) {
            Some(&i) => self.domains[i].deliver(now, origin, seq, port, payload, &self.dead),
            None => self.core.deliver(now, origin, seq, port, payload),
        };
        // A delivery with a delay is in flight for that long (RFC 005 rule 14, `flight`).
        if let Some(display) = &mut self.display
            && let Some(&(connection, from)) = self.display_index.get(&(origin, port))
        {
            display.extend(deliveries.iter().filter(|d| d.delay > 0).map(|d| {
                DisplayRecord::Flight {
                    connection,
                    port: from,
                    delay: d.delay as f32,
                    sent: now.tick,
                }
            }));
        }
        deliveries
    }
}
