//! The circuit domain (RFC 012 and RFC 013) over the domain interface of `florn-host`: nets and
//! signals between the pins and the signal ports of the plant's built-ins, the units and the
//! dimensions of RFC 012 rule 5, the solver profile, the codes of rule 28, and the static plant:
//! modified nodal analysis with a dense LU, Newton iteration with the limiter inside the diode, and
//! the initial solve in the start hook (RFC 012 rule 30; RFC 013 rule 9).
//!
//! The kernel knows nothing of this crate (ARCH-006). The arithmetic is IEEE `f64` with the
//! `libm` crate for every function IEEE does not round (RFC 013 rule 19): `clippy.toml` lists the
//! `std` methods the crate may not call, and `ci/check-deps.py` lists its dependencies.
//!
//! Implemented by PR-44 of `rfcs/handoffs/012-circuit-domain/task-breakdown-pr-plan.md`; the
//! transient integration, the advance on demand, the controller and the device model are the units
//! after it.
#![forbid(unsafe_code)]

use florn_admission::manifest::ModuleManifest;
use florn_control::message::InspectField;
use florn_control::{Code, Notification, Scope, Severity, Value};
use florn_host::domain::{Action, BuiltinEvent, Domain, DomainEvent, Site, Tolerance};
use florn_host::{CallIn, FaultPolicy, UnitDef};
use florn_kernel::{Delivery, Identity, Instant, Sink};
use florn_model::Manifest;
use florn_model::manifest::{DomainMap, DomainValue};

pub mod analyse;
mod assemble;
mod build;
pub mod dc;
pub mod diode;
mod inspect;
pub mod kinds;
pub mod lu;
pub mod math;
pub mod newton;
mod params;
pub mod plant;
pub mod profile;
mod read;
mod series;
pub mod units;

pub use profile::{Limits, Profile};

pub const NAME: &str = "circuit";
pub const NET: &str = "circuit/net";
pub const SIGNAL: &str = "circuit/signal";
pub const PIN: &str = "circuit/pin";
pub const SIGNAL_OUT: &str = "circuit/signal-out";
pub const SIGNAL_IN: &str = "circuit/signal-in";

/// The channels of the built-ins (RFC 012 rule 24), frozen by a format test of the kit: `probe` is
/// channel 1 and `condition` is channel 2; channel 0 is the log (RFC 002).
pub const CHANNEL_PROBE: u32 = 1;
pub const CHANNEL_CONDITION: u32 = 2;
/// The event classes (RFC 012 rules 8, 10 and 13).
pub const CLASS_ADVANCE: u8 = 6;
pub const CLASS_SAMPLE: u8 = 7;
pub const CLASS_CONDITION: u8 = 8;

/// The circuit domain's state for one run.
#[derive(Clone, Debug)]
pub struct CircuitDomain {
    limits: Limits,
    plant: Option<plant::Plant>,
    /// The operating point at tick 0, from the start hook.
    solution: Option<dc::Solution>,
    /// The warnings and notes of the reading (`circuit.floating-node`, `circuit.signal-unconnected`).
    /// The Host refuses a run for any notification `validate` returns, so a warning cannot come
    /// that way: it reaches the log from the start hook as a notification of the domain.
    warnings: Vec<Notification>,
}

impl CircuitDomain {
    /// The domain with the Host's maxima (RFC 013 rule 18).
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            plant: None,
            solution: None,
            warnings: Vec::new(),
        }
    }
    /// The plant and its solution, for the inspectors and the kit's tests.
    pub fn plant(&self) -> Option<&plant::Plant> {
        self.plant.as_ref()
    }
    pub fn solution(&self) -> Option<&dc::Solution> {
        self.solution.as_ref()
    }
}

impl Default for CircuitDomain {
    fn default() -> Self {
        Self::new(Limits::default())
    }
}

fn is_sensor_or_detector(kind: &str) -> bool {
    matches!(kind, "voltmeter" | "ammeter" | "dc-motor" | "comparator")
}

impl Domain for CircuitDomain {
    fn name(&self) -> &'static str {
        NAME
    }
    fn connection_types(&self) -> Vec<&'static str> {
        vec![NET, SIGNAL]
    }
    fn port_types(&self) -> Vec<&'static str> {
        vec![PIN, SIGNAL_OUT, SIGNAL_IN]
    }
    fn builtin_kinds(&self) -> Vec<&'static str> {
        kinds::KINDS.iter().map(|k| k.full_name()).collect()
    }
    fn builtin_channels(&self, kind: &str) -> Vec<u32> {
        kinds::by_builtin(kind)
            .map(|k| k.channels())
            .unwrap_or_default()
    }
    fn classes(&self) -> Vec<(u8, &'static str)> {
        vec![
            (CLASS_ADVANCE, "advance"),
            (CLASS_SAMPLE, "sample"),
            (CLASS_CONDITION, "condition"),
        ]
    }
    fn units(&self) -> Vec<UnitDef> {
        units::all()
    }

    /// The errors of the reading, every one in one pass; its warnings are the start hook's.
    fn validate(&self, scenario: &Manifest) -> Vec<Notification> {
        analyse::analyse(scenario, self.limits)
            .notes
            .into_iter()
            .filter(|n| n.severity == Severity::Error)
            .collect()
    }

    fn validate_module(&self, module: &str, manifest: &ModuleManifest) -> Vec<Notification> {
        let Some(table) = manifest.domain_tables.get(NAME) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let kind = match table.get("kind") {
            Some(DomainValue::Text(k)) => k.clone(),
            _ => String::new(),
        };
        if kind != "controller" && kind != "device" {
            out.push(read::note(
                Code::CircuitKindUnknown,
                Scope::file(module),
                None,
                vec![Value::Name(module.to_string()), Value::Text(kind.clone())],
            ));
        }
        if kind == "device" {
            let pins: Vec<_> = manifest.ports.iter().filter(|p| p.ty == PIN).collect();
            let fits = pins.len() == 1
                && (2..=16).contains(&pins[0].min)
                && (2..=16).contains(&pins[0].max);
            if !fits {
                out.push(read::note(
                    Code::CircuitDevicePorts,
                    Scope::file(module),
                    None,
                    vec![
                        Value::Name(module.to_string()),
                        Value::Count(pins.len() as u64),
                    ],
                ));
            }
        }
        out
    }

    fn compose(&mut self, scenario: &Manifest, _root_seed: u64) {
        let analysis = analyse::analyse(scenario, self.limits);
        self.plant = analysis.plant;
        self.warnings = analysis
            .notes
            .into_iter()
            .filter(|n| n.severity != Severity::Error)
            .collect();
        self.solution = None;
    }

    fn deliver(
        &mut self,
        _now: Instant,
        _origin: Identity,
        _seq: u64,
        _port: u32,
        _payload: &[u8],
        _dead: &std::collections::BTreeSet<Identity>,
    ) -> Vec<Delivery> {
        Vec::new()
    }

    fn change(&mut self, _now: Instant, _target: Identity, _payload: &DomainMap) -> Vec<Action> {
        Vec::new()
    }

    fn check_intervention(
        &self,
        _target: Identity,
        _kind: &str,
        _content: &DomainMap,
    ) -> Result<(), Box<Notification>> {
        Ok(())
    }

    fn intervene(
        &mut self,
        _now: Instant,
        _target: Identity,
        _kind: &str,
        _content: &DomainMap,
    ) -> Vec<Action> {
        Vec::new()
    }

    fn event(&mut self, _event: &DomainEvent, _alive: bool) -> Vec<Action> {
        Vec::new()
    }

    fn faulted_policy(&self) -> FaultPolicy {
        self.plant
            .as_ref()
            .map_or(FaultPolicy::Drop, |p| p.profile.faulted_policy)
    }

    fn tolerance(&self) -> Option<Tolerance> {
        let p = self.plant.as_ref().map(|p| &p.profile)?;
        Some(Tolerance {
            atol: p.atol,
            rtol: p.rtol,
        })
    }

    fn continuous_actor(&self, actor: Identity) -> bool {
        self.plant
            .as_ref()
            .and_then(|p| p.devices.iter().find(|d| d.actor == actor))
            .is_some_and(|d| is_sensor_or_detector(d.kind))
    }

    fn numeric_fields(
        &self,
        _actor: Identity,
        _site: Site,
        _payload: &[u8],
    ) -> Option<florn_host::domain::Numeric> {
        None
    }

    fn display_series(&self, actor: Identity, channel: u32, payload: &[u8]) -> Vec<(u8, f64)> {
        series::display_series(self.plant.as_ref(), actor, channel, payload)
    }

    fn inspect(&self, target: Identity) -> Option<Vec<InspectField>> {
        inspect::inspect(self.plant.as_ref(), self.solution.as_ref(), target)
    }

    /// The start hook (RFC 012 rule 30): the held values are set when the plant is built (a
    /// `constant`'s value, a comparator's 0, the signals' `initial`), and the initial solve of
    /// RFC 013 rule 9 refuses the run when it finds no operating point.
    fn start_calling(&mut self, _calls: &mut dyn CallIn) -> Result<Vec<Action>, Box<Notification>> {
        let Some(plant) = &self.plant else {
            return Ok(Vec::new());
        };
        match dc::initial_solve(plant) {
            Ok(solution) => {
                self.solution = Some(solution);
                Ok(self.warnings.iter().cloned().map(Action::Notify).collect())
            }
            Err(e) => Err(Box::new(read::note(
                Code::CircuitNoOperatingPoint,
                Scope::run(),
                None,
                vec![
                    Value::Name(e.unknown),
                    Value::Enum(e.kind.as_str().to_string()),
                    Value::Count(u64::from(e.iterations)),
                ],
            ))),
        }
    }

    fn answer(
        &mut self,
        _actor: Identity,
        _function: &str,
        _args: &[florn_abi::Val],
    ) -> Option<florn_abi::Val> {
        None
    }

    fn builtin(
        &mut self,
        _actor: Identity,
        _kind: &str,
        _event: &BuiltinEvent<'_>,
        _sink: &mut Sink,
    ) {
    }

    fn telemetry(&self) -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
        vec![
            ("probe", vec![("value", "f64")]),
            (
                "control",
                vec![("input", "f64"), ("output", "f64"), ("error", "f64")],
            ),
            ("condition", vec![("output", "f64"), ("located", "u64")]),
        ]
    }
}

#[cfg(test)]
mod tests;
