//! The sampled controller (RFC 012 rules 10, 11 and 18): the world `florn:circuit/controller`, a
//! guest that runs at its sample instants and sees the plant through held signals. The Host binds
//! the world (`read-input` answered from the activation's starting state, `set-output` recorded as
//! a `send` on the signal port); the domain schedules the samples and delivers each as an
//! `on-sample` activation.

use std::collections::BTreeMap;

use florn_abi::Ty;
use florn_abi::core_world::Signature;
use florn_admission::manifest::ModuleManifest;
use florn_control::{Code, Notification, Scope, Value};
use florn_host::domain::{DomainWorld, Guest, RecordedImport};
use florn_kernel::Identity;
use florn_model::TickLength;
use florn_model::manifest::{DomainMap, DomainValue};

use crate::NAME;
use crate::read::{note, time_ticks};
use crate::units::CircuitUnits;

pub const WORLD: &str = "florn:circuit/controller@0.1.0";
pub const SIGNALS: &str = "florn:circuit/signals@0.1.0";
pub const EVENTS: &str = "florn:circuit/controller-events@0.1.0";
/// The canonical name of the export the Host calls at a sample instant.
pub const ON_SAMPLE: &str = "florn:circuit/controller-events@0.1.0#on-sample";

/// The world of RFC 012 rule 18: `read-input` and `set-output` imported, `on-sample` exported.
/// `set-output` records a `send` of the value's eight canonical bytes on the signal port (rule 11),
/// and a value that is not finite violates rule 18.
pub fn world() -> DomainWorld {
    DomainWorld {
        name: WORLD.to_string(),
        imports_ids: vec![SIGNALS.to_string()],
        exports_ids: vec![EVENTS.to_string()],
        imports: vec![(
            SIGNALS.to_string(),
            vec![
                Signature {
                    name: "read-input",
                    params: vec![("port", Ty::U32)],
                    result: Some(Ty::option(Ty::F64)),
                },
                Signature {
                    name: "set-output",
                    params: vec![("port", Ty::U32), ("value", Ty::F64)],
                    result: None,
                },
            ],
        )],
        exports: vec![(
            EVENTS.to_string(),
            vec![Signature {
                name: "on-sample",
                params: vec![("now", Ty::U64), ("index", Ty::U64)],
                result: None,
            }],
        )],
        compounds: vec![],
        records: vec![RecordedImport {
            function: "set-output",
            port: 0,
            value: 1,
            non_finite: Some("RFC 012 rule 18"),
        }],
    }
}

/// A controller's sampling: its identity, its period, the tick of its next sample and the index of
/// the next (RFC 012 rule 10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sampler {
    pub actor: Identity,
    pub period: u64,
    pub next_due: u64,
    pub index: u64,
}

/// The value of a parameter a module's `[circuit]` table names: what the actor's `config` says, or
/// the module's default.
fn param<'a>(
    manifest: &'a ModuleManifest,
    config: Option<&'a DomainMap>,
    name: &str,
) -> Option<&'a DomainValue> {
    config
        .and_then(|c| c.get(name))
        .or_else(|| manifest.param(name).and_then(|p| p.default.as_ref()))
}

/// The samplers of the controllers among `guests` (rule 10): each module's `[circuit]` table names
/// its `period` parameter and optionally its `phase`; their values are the actor's config or the
/// module's defaults, read as whole ticks. A controller without a period is `circuit.no-period`, one
/// of zero ticks `circuit.period-zero`; the first sample is at the phase.
pub fn samplers(
    guests: &[Guest],
    controllers: &[Identity],
    configs: &BTreeMap<Identity, (String, DomainMap)>,
    tick: TickLength,
) -> Result<Vec<Sampler>, Box<Notification>> {
    let named: Vec<(Identity, &str, &ModuleManifest)> = guests
        .iter()
        .map(|g| (g.actor, g.module.as_str(), g.manifest()))
        .collect();
    samplers_of(&named, controllers, configs, tick)
}

/// [`samplers`] over the actors' identities, module names and manifests.
pub fn samplers_of(
    guests: &[(Identity, &str, &ModuleManifest)],
    controllers: &[Identity],
    configs: &BTreeMap<Identity, (String, DomainMap)>,
    tick: TickLength,
) -> Result<Vec<Sampler>, Box<Notification>> {
    let mut out = Vec::new();
    for (actor, module, manifest) in guests.iter().filter(|g| controllers.contains(&g.0)) {
        let actor = *actor;
        let name = configs
            .get(&actor)
            .map_or_else(|| module.to_string(), |(n, _)| n.clone());
        let config = configs.get(&actor).map(|(_, c)| c);
        let table = manifest.domain_tables.get(NAME);
        let named = |key: &str| match table.and_then(|t| t.get(key)) {
            Some(DomainValue::Text(t)) => Some(t.as_str()),
            _ => None,
        };
        let no_period = || {
            Box::new(note(
                Code::CircuitNoPeriod,
                Scope::actor(actor),
                None,
                vec![Value::Name(name.clone())],
            ))
        };
        let field = |p: &str| format!("actors.{name}.config.{p}");
        let period = match named("period").and_then(|p| param(manifest, config, p).map(|v| (p, v)))
        {
            Some((p, v)) => time_ticks(&CircuitUnits, &field(p), v, tick)?,
            None => return Err(no_period()),
        };
        if period == 0 {
            return Err(Box::new(note(
                Code::CircuitPeriodZero,
                Scope::actor(actor),
                None,
                vec![Value::Name(name)],
            )));
        }
        let phase = match named("phase").and_then(|p| param(manifest, config, p).map(|v| (p, v))) {
            Some((p, v)) => time_ticks(&CircuitUnits, &field(p), v, tick)?,
            None => 0,
        };
        out.push(Sampler {
            actor,
            period,
            next_due: phase,
            index: 0,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
