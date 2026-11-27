//! The circuit domain (RFC 012 and RFC 013) over the domain interface of `florn-host`: nets and
//! signals between the pins and the signal ports of the plant's built-ins, the units and the
//! dimensions of RFC 012 rule 5, the solver profile, the codes of rule 28, and the static plant:
//! modified nodal analysis with a dense LU, Newton iteration with the limiter inside the diode, and
//! the initial solve in the start hook (RFC 012 rule 30; RFC 013 rule 9), and the transient solver
//! that carries the plant on from it.
//!
//! The kernel knows nothing of this crate (ARCH-006). The arithmetic is IEEE `f64` with the
//! `libm` crate for every function IEEE does not round (RFC 013 rule 19): `clippy.toml` lists the
//! `std` methods the crate may not call, and `ci/check-deps.py` lists its dependencies.
//!
//! The static plant is PR-44's and the transient integration PR-45's (`transient`: the
//! trapezoidal rule, variable-step Gear 2 and backward Euler for starting, the error estimate and
//! the step controller, the limits of a step in ticks, the restarts); the advance on demand, the
//! controller and the device model are the units after it, in
//! `rfcs/handoffs/012-circuit-domain/task-breakdown-pr-plan.md`.
#![forbid(unsafe_code)]

use florn_admission::manifest::ModuleManifest;
use florn_control::message::InspectField;
use florn_control::{Code, Notification, Scope, Value};
use florn_host::domain::{Action, BuiltinEvent, Domain, DomainEvent, Guest, Site, Tolerance};
use florn_host::{CallIn, FaultPolicy, UnitDef};
use florn_kernel::{Delivery, Identity, Instant, Sink};
use florn_model::Manifest;
use florn_model::manifest::{DomainMap, DomainValue};

pub mod analyse;
mod assemble;
pub mod bits;
mod build;
pub mod changes;
pub mod control;
pub mod controllers;
pub mod dc;
pub mod diode;
pub mod guests;
pub mod inputs;
mod inspect;
pub mod integrate;
pub mod kinds;
pub mod live;
pub mod lu;
pub mod math;
pub mod newton;
mod numeric;
mod params;
pub mod plant;
pub mod profile;
mod read;
mod sample;
mod series;
pub mod state;
pub mod transient;
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
/// The most guest device models in a scenario (RFC 012 rule 19, finding F-8 deferred).
pub const MAX_DEVICES: usize = 1;

/// The circuit domain's state for one run.
#[derive(Clone, Debug)]
pub struct CircuitDomain {
    limits: Limits,
    plant: Option<plant::Plant>,
    /// The operating point at tick 0, from the start hook.
    solution: Option<dc::Solution>,
    /// The scenario's name, for the notes the start hook makes at file scope.
    scenario: String,
    /// The guest actors the start hook read (RFC 012 finding F-18).
    guests: guests::Guests,
    /// The module actors' names and `config` tables as the scenario gives them, and its tick: what
    /// the start hook reads a controller's period and phase from (rule 10).
    module_configs: std::collections::BTreeMap<Identity, (String, DomainMap)>,
    tick: florn_model::TickLength,
    /// What the plant does between events: held values, breakpoints, the sensors' due ticks, the
    /// pending advance, the solver from tick 0 (built by the start hook).
    live: Option<live::Live>,
}

impl CircuitDomain {
    /// The domain with the Host's maxima (RFC 013 rule 18).
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            plant: None,
            solution: None,
            scenario: String::new(),
            guests: guests::Guests::default(),
            module_configs: std::collections::BTreeMap::new(),
            tick: florn_model::TickLength::Ns,
            live: None,
        }
    }
    /// The plant and its solution, for the inspectors and the kit's tests.
    pub fn plant(&self) -> Option<&plant::Plant> {
        self.plant.as_ref()
    }
    pub fn solution(&self) -> Option<&dc::Solution> {
        self.solution.as_ref()
    }
    /// The transient solver, once the run has started: the plant at `solved`.
    pub fn transient(&self) -> Option<&transient::Transient> {
        self.live.as_ref()?.transient.as_ref()
    }
    pub fn transient_mut(&mut self) -> Option<&mut transient::Transient> {
        self.live.as_mut()?.transient.as_mut()
    }
    /// What the plant does between events.
    pub fn live(&self) -> Option<&live::Live> {
        self.live.as_ref()
    }
    /// The circuit guests the start hook read: their kinds and declared channels.
    pub fn guests(&self) -> &guests::Guests {
        &self.guests
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
    fn intervention_kinds(&self) -> Vec<&'static str> {
        vec!["source", "signal"]
    }
    fn worlds(&self) -> Vec<florn_host::domain::DomainWorld> {
        vec![controllers::world()]
    }

    /// Every note of the reading, in one pass: the errors refuse the run and the warnings are
    /// kept (RFC 012 finding F-18).
    fn validate(&self, scenario: &Manifest) -> Vec<Notification> {
        let analysis = analyse::analyse(scenario, self.limits);
        let mut notes = analysis.notes;
        // The scheduled changes addressed to this world (RFC 012 rule 15), read against the plant.
        if let Some(plant) = &analysis.plant {
            for (i, c) in scenario.changes.iter().enumerate() {
                let florn_model::manifest::ChangeTarget::World(n) = &c.target else {
                    continue;
                };
                if scenario.domains.get(*n as usize).map(String::as_str) != Some(NAME) {
                    continue;
                }
                let field = format!("changes[{i}].payload");
                if let Err(fault) = changes::source(plant, &c.payload, &field) {
                    notes.push(fault.at_field(&field));
                }
            }
        }
        notes
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
        self.plant = analyse::analyse(scenario, self.limits).plant;
        self.scenario = scenario.name.clone();
        self.solution = None;
        self.live = self.plant.as_ref().map(live::Live::new);
        self.guests = guests::Guests::default();
        self.tick = scenario.time.tick;
        self.module_configs = scenario
            .actors
            .iter()
            .filter(|a| matches!(a.kind, florn_model::manifest::ActorKind::Module(_)))
            .map(|a| (a.id, (a.name.clone(), a.config.clone())))
            .collect();
    }

    /// A committed send on a `signal-out` port (RFC 012 rule 11): the value the port's signal holds
    /// from now on, and a breakpoint where it drives the plant. It returns no delivery.
    fn deliver(
        &mut self,
        now: Instant,
        origin: Identity,
        _seq: u64,
        port: u32,
        payload: &[u8],
        _dead: &std::collections::BTreeSet<Identity>,
    ) -> Vec<Delivery> {
        if let (Some(plant), Some(live), Ok(bytes)) = (
            self.plant.as_ref(),
            self.live.as_mut(),
            <[u8; 8]>::try_from(payload),
        ) {
            let value = f64::from_le_bytes(bytes);
            if value.is_finite() {
                live.output(plant, now.tick, origin, port, value);
            }
        }
        Vec::new()
    }

    /// A scheduled change addressed to the world (rule 15): a breakpoint at its instant.
    fn change(&mut self, now: Instant, _target: Identity, payload: &DomainMap) -> Vec<Action> {
        if let (Some(plant), Some(live)) = (self.plant.as_ref(), self.live.as_mut())
            && let Ok(source) = changes::source(plant, payload, "payload")
        {
            live.set_source(plant, now.tick, source);
        }
        Vec::new()
    }

    fn check_intervention(
        &self,
        target: Identity,
        kind: &str,
        content: &DomainMap,
    ) -> Result<(), Box<Notification>> {
        let Some(plant) = &self.plant else {
            return Err(changes::invalid("target"));
        };
        match kind {
            "source" => {
                if target != plant.world {
                    return Err(changes::invalid("target"));
                }
                changes::source(plant, content, "content")
                    .map(|_| ())
                    .map_err(changes::Fault::as_intervention)
            }
            "signal" => changes::signal(plant, target, content).map(|_| ()),
            _ => Err(changes::invalid("kind")),
        }
    }

    fn intervene(
        &mut self,
        now: Instant,
        target: Identity,
        kind: &str,
        content: &DomainMap,
    ) -> Vec<Action> {
        let (Some(plant), Some(live)) = (self.plant.as_ref(), self.live.as_mut()) else {
            return Vec::new();
        };
        match kind {
            "source" => {
                if let Ok(source) = changes::source(plant, content, "content") {
                    live.set_source(plant, now.tick, source);
                }
            }
            "signal" => {
                if let Ok((signal, frozen)) = changes::signal(plant, target, content) {
                    live.freeze(signal, frozen);
                }
            }
            _ => {}
        }
        Vec::new()
    }

    /// The `advance` event (rule 8) and nothing else: no other event is the domain's own.
    fn event(&mut self, event: &DomainEvent, alive: bool) -> Vec<Action> {
        let (Some(plant), Some(live)) = (self.plant.as_ref(), self.live.as_mut()) else {
            return Vec::new();
        };
        match event.key.class {
            CLASS_ADVANCE => live.on_advance(plant, event.key.tick),
            // A controller's sample: a built-in sensor's is the Host's `builtin`.
            CLASS_SAMPLE => live.controller_sample(plant, event.target, event.key.tick, alive),
            _ => Vec::new(),
        }
    }

    fn limit_reached(&mut self) -> Option<florn_host::domain::Limit> {
        self.live.as_mut()?.limit.take()
    }

    fn end_requested(&mut self) -> Option<florn_host::domain::EndRequest> {
        self.live.as_mut()?.end_request.take()
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
            || self.guests.controllers().contains(&actor)
    }

    fn numeric_fields(
        &self,
        actor: Identity,
        site: Site,
        payload: &[u8],
    ) -> Option<florn_host::domain::Numeric> {
        numeric::numeric_fields(self.plant.as_ref()?, &self.guests, actor, site, payload)
    }

    fn display_series(&self, actor: Identity, channel: u32, payload: &[u8]) -> Vec<(u8, f64)> {
        series::display_series(self.plant.as_ref(), &self.guests, actor, channel, payload)
    }

    fn inspect(&self, target: Identity) -> Option<Vec<InspectField>> {
        inspect::inspect(
            self.plant.as_ref(),
            self.solution.as_ref(),
            self.live.as_ref(),
            target,
        )
    }

    /// The start hook (RFC 012 rule 30): the held values are set when the plant is built (a
    /// `constant`'s value, a comparator's 0, the signals' `initial`); the guests the Host reads
    /// to it are kept and a second device model is refused (rule 19, finding F-18); the initial
    /// solve of RFC 013 rule 9 refuses the run when it finds no operating point.
    fn start_calling(
        &mut self,
        _calls: &mut dyn CallIn,
        guests: &[Guest],
    ) -> Result<Vec<Action>, Box<Notification>> {
        self.guests = guests::Guests::read(guests);
        if self.guests.devices() > MAX_DEVICES {
            return Err(Box::new(read::note(
                Code::CircuitDeviceLimit,
                Scope::file(&self.scenario),
                None,
                vec![
                    Value::Count(self.guests.devices() as u64),
                    Value::Count(MAX_DEVICES as u64),
                ],
            )));
        }
        let (Some(plant), Some(live)) = (self.plant.as_ref(), self.live.as_mut()) else {
            return Ok(Vec::new());
        };
        // Each controller's period and phase (rule 10), refused when a period is missing or zero.
        let samplers = controllers::samplers(
            guests,
            &self.guests.controllers(),
            &self.module_configs,
            self.tick,
        )?;
        // The signals hold what the controllers' `init` set (rule 30, step 2).
        match dc::initial_solve(&live.plant_in_force(plant)) {
            Ok(solution) => {
                let actions = live.start(plant, &solution, samplers);
                self.solution = Some(solution);
                Ok(actions)
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

    /// The plant's state (rule 29), empty for a scenario with no plant.
    fn export(&self) -> Vec<u8> {
        match &self.live {
            Some(live) => state::encode(live, self.solution.as_ref()),
            None => Vec::new(),
        }
    }

    fn restore(&mut self, bytes: &[u8]) -> bool {
        if bytes.is_empty() {
            return self.live.is_none();
        }
        let Some(plant) = self.plant.as_ref() else {
            return false;
        };
        match state::decode(plant, bytes) {
            Some((live, solution)) => {
                self.live = Some(live);
                self.solution = solution;
                true
            }
            None => false,
        }
    }

    /// The guests after a restore, as the start hook has them (RFC 012 finding F-18).
    fn restored(&mut self, guests: &[Guest]) {
        self.guests = guests::Guests::read(guests);
    }

    /// `read-input` (rule 18): the value held on the signal the port is on, in the unit of that
    /// signal, as it was when the activation began; `none` for a port on no signal.
    fn answer(
        &mut self,
        actor: Identity,
        function: &str,
        args: &[florn_abi::Val],
    ) -> Option<florn_abi::Val> {
        match (function, args) {
            ("read-input", [florn_abi::Val::U32(port)]) => {
                let held = self
                    .plant
                    .as_ref()
                    .zip(self.live.as_ref())
                    .and_then(|(p, l)| {
                        p.port_signal.get(&(actor, *port)).map(|s| l.held[*s].value)
                    });
                Some(florn_abi::Val::Option(
                    held.map(|v| Box::new(florn_abi::Val::F64(math::canonical(v)))),
                ))
            }
            _ => None,
        }
    }

    /// A sensor's sample (rule 10): the state at `solved`, reported on `probe` and sent on its
    /// signal, in the signal's unit; the plant was made current by the advance of this tick.
    fn builtin(&mut self, actor: Identity, _kind: &str, event: &BuiltinEvent<'_>, sink: &mut Sink) {
        let BuiltinEvent::Domain(ev) = event else {
            return;
        };
        if ev.key.class != CLASS_SAMPLE {
            return;
        }
        if let (Some(plant), Some(live)) = (self.plant.as_ref(), self.live.as_ref())
            && let Some((value, port)) = sample::read(plant, live, actor)
        {
            let bytes = math::canonical(value).to_le_bytes().to_vec();
            let _ = sink.report(CHANNEL_PROBE, bytes.clone());
            if let Some(port) = port {
                let _ = sink.send(port, bytes);
            }
        }
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
