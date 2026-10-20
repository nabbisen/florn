//! Stage 3, the estimate (RFC 003 rules 2, 16 to 20; RFC 002 rules 15 and 17): what the scenario
//! needs from every module-backed actor, against the limits of its class and the Host's capacity,
//! before anything is instantiated.

use std::collections::BTreeMap;

use florn_model::manifest::{ActorKind, Manifest};
use florn_model::{HostMaxima, Limits};

use crate::error::{Error, Errors, Kind};
use crate::manifest::ModuleManifest;
use crate::validate::PAGE;

/// The limits of one actor class, resolved (RFC 003 rule 16).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClassLimits {
    pub budget: u64,
    pub stack: u64,
    pub memory_bytes: u64,
    pub effects_per_activation: u64,
    pub payload_bytes: u64,
    pub pending_timers: u64,
    pub telemetry_per_activation_bytes: u64,
}

impl ClassLimits {
    /// The defaults of RFC 003 rule 19, as measured in Phase 1: the budget covers the
    /// compute-style reference activation (10.15 million units) ten times over.
    pub fn defaults() -> Self {
        Self {
            budget: 100_000_000,
            stack: 1_024,
            memory_bytes: 4 * 1024 * 1024,
            effects_per_activation: 1_000,
            payload_bytes: 64 * 1024,
            pending_timers: 64,
            telemetry_per_activation_bytes: 16 * 1024,
        }
    }

    /// A scenario's class over the defaults, capped at the Host maxima (RFC 004 rule 15 already
    /// refused a value above the maximum; the cap is a second check).
    pub fn resolve(limits: &Limits, defaults: &ClassLimits, host: &HostMaxima) -> Self {
        Self {
            budget: limits.budget.unwrap_or(defaults.budget).min(host.budget),
            stack: limits.stack.unwrap_or(defaults.stack).min(host.stack),
            memory_bytes: limits
                .memory_bytes
                .unwrap_or(defaults.memory_bytes)
                .min(host.memory_bytes),
            effects_per_activation: limits
                .effects_per_activation
                .unwrap_or(defaults.effects_per_activation)
                .min(host.effects_per_activation),
            payload_bytes: limits
                .payload_bytes
                .unwrap_or(defaults.payload_bytes)
                .min(host.payload_bytes),
            pending_timers: limits
                .pending_timers
                .unwrap_or(defaults.pending_timers)
                .min(host.pending_timers),
            telemetry_per_activation_bytes: limits
                .telemetry_per_activation_bytes
                .unwrap_or(defaults.telemetry_per_activation_bytes)
                .min(host.telemetry_per_activation_bytes),
        }
    }

    /// The memory limit in whole pages, rounded down; at least one page.
    pub fn memory_pages(&self) -> u64 {
        (self.memory_bytes / PAGE).max(1)
    }
}

/// What the Host can hold, from the probe at start-up (RFC 003 rule 20).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capacity {
    pub instances: u64,
    pub memory_bytes: u64,
}

/// What stage 3 needs to know about one module of the scenario, by its `[modules]` name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleFacts {
    pub source_sha256: [u8; 32],
    pub manifest: ModuleManifest,
    pub memory_initial_pages: u64,
}

/// The estimate: one line per module-backed actor, and the totals against capacity.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Estimate {
    /// (actor identity, module name, resolved limits), in actor order.
    pub actors: Vec<(u32, String, ClassLimits)>,
    pub instances: u64,
    pub memory_bytes: u64,
}

fn below_need(
    errors: &mut Errors,
    actor: u32,
    path: &str,
    limit: &'static str,
    need: Option<u64>,
    value: u64,
) {
    if let Some(need) = need
        && need > value
    {
        errors.push(Error::at(
            Kind::LimitBelowNeed {
                actor,
                limit,
                need,
                value,
            },
            path,
        ));
    }
}

/// Run stage 3 for a scenario. Every module referenced by an actor must have facts.
pub fn estimate(
    scenario: &Manifest,
    modules: &BTreeMap<String, ModuleFacts>,
    defaults: &ClassLimits,
    host: &HostMaxima,
    capacity: Capacity,
) -> Result<Estimate, Errors> {
    let mut errors = Errors::default();
    let mut out = Estimate::default();
    // The scenario's `[modules]` against the facts: hash and contract (RFC 004 rule 16).
    for (name, m) in &scenario.modules {
        let Some(facts) = modules.get(name) else {
            errors.push(Error::at(
                Kind::Model(Box::new(florn_model::Kind::ReferenceUnresolved {
                    reference: name.clone(),
                    kind: "module",
                })),
                format!("modules.{name}"),
            ));
            continue;
        };
        if facts.source_sha256 != m.sha256 {
            errors.push(Error::at(
                Kind::Model(Box::new(florn_model::Kind::ModuleHashMismatch {
                    module: name.clone(),
                    declared: florn_model::canonical::hex(&m.sha256),
                    actual: florn_model::canonical::hex(&facts.source_sha256),
                })),
                format!("modules.{name}.sha256"),
            ));
        }
        if facts.manifest.world != m.contract {
            errors.push(Error::at(
                Kind::ContractMismatch {
                    module: name.clone(),
                    declared: m.contract.clone(),
                    found: facts.manifest.world.clone(),
                },
                format!("modules.{name}.contract"),
            ));
        }
    }
    for (i, actor) in scenario.actors.iter().enumerate() {
        let ActorKind::Module(name) = &actor.kind else {
            continue;
        };
        let Some(facts) = modules.get(name) else {
            continue;
        };
        let path = format!("actors[{i}]");
        let limits = ClassLimits::resolve(&scenario.limits_for(&actor.class), defaults, host);
        let need = &facts.manifest.resources;
        below_need(
            &mut errors,
            actor.id,
            &path,
            "budget",
            need.budget,
            limits.budget,
        );
        below_need(
            &mut errors,
            actor.id,
            &path,
            "stack",
            need.stack,
            limits.stack,
        );
        below_need(
            &mut errors,
            actor.id,
            &path,
            "memory",
            need.memory_bytes,
            limits.memory_bytes,
        );
        below_need(
            &mut errors,
            actor.id,
            &path,
            "effects-per-activation",
            need.effects_per_activation,
            limits.effects_per_activation,
        );
        below_need(
            &mut errors,
            actor.id,
            &path,
            "payload",
            need.payload_bytes,
            limits.payload_bytes,
        );
        below_need(
            &mut errors,
            actor.id,
            &path,
            "pending-timers",
            need.pending_timers,
            limits.pending_timers,
        );
        below_need(
            &mut errors,
            actor.id,
            &path,
            "telemetry-per-activation",
            need.telemetry_per_activation_bytes,
            limits.telemetry_per_activation_bytes,
        );
        if facts.memory_initial_pages > limits.memory_pages() {
            errors.push(Error::at(
                Kind::InitialMemoryAboveLimit {
                    pages: facts.memory_initial_pages,
                    limit: limits.memory_pages(),
                },
                &path,
            ));
        }
        // Port kinds (RFC 002 rule 15).
        let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
        for (j, port) in actor.ports.iter().enumerate() {
            match facts.manifest.port_kind_of_type(&port.r#type) {
                Some(kind) => *counts.entry(kind.kind.as_str()).or_default() += 1,
                None => errors.push(Error::at(
                    Kind::PortTypeUnknown {
                        actor: actor.id,
                        port: port.name.clone(),
                        ty: port.r#type.clone(),
                    },
                    format!("{path}.ports[{j}].type"),
                )),
            }
        }
        for kind in &facts.manifest.ports {
            let found = counts.get(kind.kind.as_str()).copied().unwrap_or(0);
            if found < kind.min || found > kind.max {
                errors.push(Error::at(
                    Kind::PortCount {
                        actor: actor.id,
                        kind: kind.kind.clone(),
                        found,
                        min: kind.min,
                        max: kind.max,
                    },
                    format!("{path}.ports"),
                ));
            }
        }
        out.instances += 1;
        out.memory_bytes = out.memory_bytes.saturating_add(limits.memory_bytes);
        out.actors.push((actor.id, name.clone(), limits));
    }
    if out.instances > capacity.instances {
        errors.push(Error::new(Kind::CapacityExceeded {
            need: out.instances,
            capacity: capacity.instances,
            what: "instances",
        }));
    }
    if out.memory_bytes > capacity.memory_bytes {
        errors.push(Error::new(Kind::CapacityExceeded {
            need: out.memory_bytes,
            capacity: capacity.memory_bytes,
            what: "memory",
        }));
    }
    errors.into_result(out)
}
