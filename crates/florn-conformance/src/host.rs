//! The native Host over the kit's scenarios, and what the tests read from a run.

use std::collections::BTreeMap;

use florn_admission::{Admission, ClassLimits};
use florn_control::Value;
use florn_engine_wasmtime::{Limits, WasmtimeEngine};
use florn_host::Domains;
use florn_host::{EndReason, HostError, Options, Run, Seed};
use florn_kernel::trace::CommittedEffect;
use florn_kernel::{EventKey, FaultKind, Identity, Instant, Record};
use florn_platform::FixedPlatform;

use crate::scenario::Scenario;

pub type Host = Run<WasmtimeEngine>;

/// The class limits of the kit's scenarios: the defaults of RFC 003 rule 19 with the memory of
/// `Scenario::new`. The admitted bytes depend on them (the memory maximum of RFC 003 §3, pass 1),
/// so the recorded admitted hashes are for these limits.
pub fn kit_limits() -> ClassLimits {
    ClassLimits {
        memory_bytes: 2 * 1024 * 1024,
        ..ClassLimits::defaults()
    }
}

/// The Host's domains as the shells compose them: the core and the network domain (RFC 008).
pub fn domains() -> Domains {
    Domains::core().with(florn_domain_network::NetworkDomain::new)
}

/// An admission step that admits the domains' worlds beside the core's (RFC 008 rule 6).
pub fn admission() -> Admission {
    let mut a = Admission::default();
    for w in domains().worlds() {
        a.add_world(w);
    }
    a
}

pub fn engine() -> WasmtimeEngine {
    WasmtimeEngine::new(Limits::default()).expect("the engine")
}

/// Every inspector on: records and payloads kept, a short chain interval.
pub fn options() -> Options {
    Options {
        keep_records: true,
        keep_payloads: true,
        chain_interval: 1_000,
        ..Options::default()
    }
}

pub fn try_start(s: &Scenario, options: Options) -> Result<Host, HostError> {
    try_start_with(engine(), s, options)
}

pub fn try_start_with(
    engine: WasmtimeEngine,
    s: &Scenario,
    options: Options,
) -> Result<Host, HostError> {
    Run::new(
        engine,
        Admission::default(),
        &mut FixedPlatform::default(),
        &domains(),
        s.manifest(),
        &s.modules(),
        Seed::Given(s.seed),
        options,
    )
}

pub fn start(s: &Scenario) -> Host {
    try_start(s, options()).unwrap_or_else(|e| panic!("scenario {}: {e}", s.name))
}

/// Start and run to the end.
pub fn run(s: &Scenario) -> Host {
    let mut run = start(s);
    run.run_to_end();
    run
}

/// The codes a scenario is refused with at `new-run` (empty when it starts).
pub fn refused(s: &Scenario) -> Vec<String> {
    if let Err(codes) = s.try_manifest() {
        return codes.iter().map(|c| c.to_string()).collect();
    }
    match try_start(s, options()) {
        Ok(_) => Vec::new(),
        Err(HostError::Refused(e)) => e.codes().iter().map(|c| c.to_string()).collect(),
        Err(other) => vec![other.to_string()],
    }
}

/// The fault records: (actor, kind, parameter, metered work).
pub fn faults(run: &Host) -> Vec<(Identity, FaultKind, u32, u64)> {
    run.records()
        .expect("records kept")
        .iter()
        .filter_map(|r| match r {
            Record::Fault {
                actor,
                kind,
                parameter,
                metered,
                ..
            } => Some((*actor, *kind, *parameter, *metered)),
            _ => None,
        })
        .collect()
}

/// The committed reports of an actor on a channel, with their instants, from the kept records
/// and payloads.
pub fn reports(run: &Host, actor: Identity, channel: u32) -> Vec<(Instant, Vec<u8>)> {
    let mut out = Vec::new();
    for r in run.records().expect("records kept") {
        if let Record::Handled {
            key,
            target,
            effects,
            ..
        } = r
            && *target == actor
        {
            for e in effects {
                if let CommittedEffect::Report {
                    channel: c, digest, ..
                } = e
                    && *c == channel
                {
                    out.push((
                        Instant::new(key.tick, key.microstep),
                        run.payload(digest).expect("payload kept").to_vec(),
                    ));
                }
            }
        }
    }
    out
}

/// The committed sends of an actor: (instant, port, payload).
pub fn sends(run: &Host, actor: Identity) -> Vec<(Instant, u32, Vec<u8>)> {
    let mut out = Vec::new();
    for r in run.records().expect("records kept") {
        if let Record::Handled {
            key,
            target,
            effects,
            ..
        } = r
            && *target == actor
        {
            for e in effects {
                if let CommittedEffect::Send { port, digest, .. } = e {
                    out.push((
                        Instant::new(key.tick, key.microstep),
                        *port,
                        run.payload(digest).expect("payload kept").to_vec(),
                    ));
                }
            }
        }
    }
    out
}

/// The keys of the events an actor handled.
pub fn handled(run: &Host, actor: Identity) -> Vec<EventKey> {
    run.records()
        .expect("records kept")
        .iter()
        .filter_map(|r| match r {
            Record::Handled { key, target, .. } if *target == actor => Some(*key),
            _ => None,
        })
        .collect()
}

/// The channel 0 texts in the diagnostic log, by actor (`actor.text`).
pub fn texts(run: &Host) -> Vec<(Identity, String)> {
    run.log()
        .entries()
        .iter()
        .filter(|e| e.notification.code == "actor.text")
        .map(|e| {
            let n = &e.notification;
            let actor = match n.params.iter().find(|(k, _)| k == "actor") {
                Some((_, Value::Identity(id))) => *id,
                _ => 0,
            };
            let text = match n.params.iter().find(|(k, _)| k == "text") {
                Some((_, Value::Text(t))) => t.clone(),
                _ => String::new(),
            };
            (actor, text)
        })
        .collect()
}

/// A parameter of the first log entry with the code, as text.
pub fn log_param(run: &Host, code: &str, param: &str) -> Option<String> {
    run.log()
        .entries()
        .iter()
        .find(|e| e.notification.code == code)
        .and_then(|e| e.notification.params.iter().find(|(k, _)| k == param))
        .map(|(_, v)| match v {
            Value::Text(t) => t.clone(),
            Value::Enum(t) => t.clone(),
            Value::Count(c) => c.to_string(),
            Value::Identity(i) => i.to_string(),
            other => format!("{other:?}"),
        })
}

pub fn ended(run: &Host) -> EndReason {
    run.ended().expect("the run ended")
}

/// A map of the admitted hashes by module name, from the run's inputs.
pub fn admitted_hashes(run: &Host) -> BTreeMap<String, String> {
    run.inputs()
        .modules
        .iter()
        .map(|m| {
            (
                m.name.clone(),
                florn_model::canonical::hex(&m.admitted_sha256),
            )
        })
        .collect()
}
