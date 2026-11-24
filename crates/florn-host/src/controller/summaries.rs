//! What the queries and the commands summarise for a front end: an actor, a module, a limit, a trace
//! entry, an instant, and the file kinds a Host writes.

use super::*;

pub(super) fn at(i: florn_kernel::Instant) -> At {
    At {
        tick: i.tick,
        microstep: i.microstep,
    }
}

pub(super) fn actor_summary(a: &florn_model::Actor) -> ActorSummary {
    ActorSummary {
        id: a.id,
        name: a.name.clone(),
        module: match &a.kind {
            florn_model::ActorKind::Module(m) => Some(m.clone()),
            _ => None,
        },
        class: a.class.clone(),
        ports: a
            .ports
            .iter()
            .map(|p| PortSummary {
                id: p.id,
                name: p.name.clone(),
                ty: p.r#type.clone(),
            })
            .collect(),
        config: domain_map_text(&a.config),
    }
}

pub(super) fn module_summary(name: &str, m: &ModuleManifest, source: &[u8; 32]) -> ModuleSummary {
    let needs = [
        ("budget", m.resources.budget),
        ("stack", m.resources.stack),
        ("memory", m.resources.memory_bytes),
        ("effects-per-activation", m.resources.effects_per_activation),
        ("payload", m.resources.payload_bytes),
        ("pending-timers", m.resources.pending_timers),
        (
            "telemetry-per-activation",
            m.resources.telemetry_per_activation_bytes,
        ),
    ]
    .into_iter()
    .filter_map(|(n, v)| v.map(|v| (n.to_string(), v)))
    .collect();
    ModuleSummary {
        name: name.to_string(),
        source_sha256: hex(source),
        contract: m.world.clone(),
        kind: m.kind.clone(),
        description: m.description.clone(),
        port_kinds: m
            .ports
            .iter()
            .map(|p| (p.kind.clone(), p.ty.clone(), p.min, p.max))
            .collect(),
        params: m
            .params
            .iter()
            .map(|p| ParamSummary {
                name: p.name.clone(),
                ty: p.ty.name().to_string(),
                required: p.required,
                default: p.default.as_ref().map(domain_value_text),
                unit: p.unit.clone(),
            })
            .collect(),
        channels: m
            .telemetry
            .iter()
            .map(|c| ChannelSummary {
                channel: c.channel,
                name: c.name.clone(),
                fields: c
                    .fields
                    .iter()
                    .map(|(n, t)| (n.clone(), t.name().to_string()))
                    .collect(),
            })
            .collect(),
        needs,
        tables: m
            .domain_tables
            .iter()
            .map(|(n, t)| (n.clone(), domain_map_text(t)))
            .collect(),
    }
}

/// A 64-hex digest's bytes.
pub(super) fn hex_digest(text: &str) -> Option<[u8; 32]> {
    crate::world::unhex(text)?.try_into().ok()
}

/// `[limits.<class>]` as text: counts bare, bytes with the unit `B`.
pub(super) fn limit_fields(l: &florn_model::manifest::Limits) -> Vec<(String, String)> {
    let mut f = Vec::new();
    let count = |f: &mut Vec<(String, String)>, k: &str, v: Option<u64>| {
        if let Some(v) = v {
            f.push((k.to_string(), v.to_string()));
        }
    };
    let bytes = |f: &mut Vec<(String, String)>, k: &str, v: Option<u64>| {
        if let Some(v) = v {
            f.push((k.to_string(), format!("{v}B")));
        }
    };
    count(&mut f, "budget", l.budget);
    count(&mut f, "stack", l.stack);
    bytes(&mut f, "memory", l.memory_bytes);
    count(&mut f, "effects-per-activation", l.effects_per_activation);
    bytes(&mut f, "payload", l.payload_bytes);
    count(&mut f, "pending-timers", l.pending_timers);
    bytes(
        &mut f,
        "telemetry-per-activation",
        l.telemetry_per_activation_bytes,
    );
    f
}

pub(super) fn domain_map_text(m: &florn_model::manifest::DomainMap) -> Vec<(String, String)> {
    m.iter()
        .map(|(k, v)| (k.clone(), domain_value_text(v)))
        .collect()
}

fn domain_value_text(v: &florn_model::DomainValue) -> String {
    match v {
        florn_model::DomainValue::Integer(i) => i.to_string(),
        florn_model::DomainValue::Decimal(d) | florn_model::DomainValue::Text(d) => d.clone(),
        florn_model::DomainValue::Boolean(b) => b.to_string(),
        other => format!("{other:?}"),
    }
}

pub(super) fn trace_entry(r: &Record) -> TraceEntry {
    match r {
        Record::Handled {
            key,
            target,
            metered,
            draws,
            effects,
        } => TraceEntry::Handled {
            at: at(key.instant()),
            class: key.class,
            origin: key.origin,
            seq: key.seq,
            target: *target,
            metered: *metered,
            draws: *draws,
            effects: effects.len() as u32,
            sends: effects
                .iter()
                .filter_map(|e| match e {
                    florn_kernel::trace::CommittedEffect::Send {
                        seq,
                        port,
                        len,
                        digest,
                    } => Some(SendSummary {
                        seq: *seq,
                        port: *port,
                        len: *len,
                        digest: hex(digest),
                    }),
                    _ => None,
                })
                .collect(),
        },
        Record::Fault {
            key,
            actor,
            kind,
            parameter,
            metered,
        } => TraceEntry::Fault {
            at: at(key.instant()),
            class: key.class,
            origin: key.origin,
            seq: key.seq,
            actor: *actor,
            code: crate::run::fault_code(*kind).to_string(),
            parameter: *parameter,
            metered: *metered,
        },
        Record::Intervention {
            key, content_len, ..
        } => TraceEntry::Intervention {
            at: at(key.instant()),
            seq: key.seq,
            len: *content_len,
        },
    }
}

/// The file kinds a Host writes, for the front end's file dialogs.
pub fn export_kinds() -> [FileKind; 4] {
    [
        FileKind::RunRecord,
        FileKind::DiagnosticLog,
        FileKind::Package,
        FileKind::Checkpoint,
    ]
}
