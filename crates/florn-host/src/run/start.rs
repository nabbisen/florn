//! The start of a run (RFC 005 rule 26, `new-run`) and the instance pool (RFC 009 rule 13):
//! admission and the estimate before anything exists, the kernel and the log's first entries,
//! the instances, the first activation of every actor in ascending identity order.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use florn_abi::Val;
use florn_admission::config::encode_config;
use florn_admission::estimate::{Capacity as EstimateCapacity, ModuleFacts, estimate};
use florn_admission::{
    ADMISSION_VERSION, Admission, Admitted, BUDGET_IMPORT_MODULE, BUDGET_IMPORT_NAME, ClassLimits,
    Errors,
};
use florn_control::{Ends, Notification, Scope, Severity, Value};
use florn_engine::{Engine, EngineError, HostGlobal, ImportFunction, ImportSpec};
use florn_kernel::{Config, Event, EventKey, EventKind, Identity, Kernel, Quotas, Sink, TiePolicy};
use florn_model::Manifest;
use florn_model::manifest::ActorKind;
use florn_platform::Platform;

use crate::domain::{self, BuiltinEvent, Composite, DomainUnits, Domains, WorldSigs};
use crate::identity::{Environment, ModuleInput, RunInputs};
use crate::log::DiagnosticLog;
use crate::world::CLASS_TIMER;

use super::{Builtin, HostError, INIT, Options, Residency, Run, Seed, Slot, cost_table};

impl<E: Engine> Run<E> {
    /// A new run (RFC 005 rule 26, `new-run`): compose the domains the scenario declares and
    /// validate through them (RFC 008 rule 1), admit every module, estimate, instantiate the
    /// pool, initialise every actor in ascending identity order (RFC 002 rule 36). Nothing is
    /// instantiated before the estimate passes (RFC 003 rule 4).
    // The Host's parts and the run's inputs, each named: a struct would only move the list.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        engine: E,
        admission: Admission,
        platform: &mut dyn Platform,
        domains: &Domains,
        scenario: Manifest,
        modules: &BTreeMap<String, Vec<u8>>,
        seed: Seed,
        options: Options,
    ) -> Result<Self, HostError> {
        Self::try_new(
            engine, admission, platform, domains, scenario, modules, seed, options,
        )
        .map_err(|parts| parts.0)
    }

    /// As [`Self::new`], but a failure gives the engine and the admission cache back (boxed, as
    /// the engine is large), so that a front end can start another run (RFC 005 rule 10: the
    /// run ends, the Host does not).
    // The Host's parts and the run's inputs, each named: a struct would only move the list.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        engine: E,
        admission: Admission,
        platform: &mut dyn Platform,
        domains: &Domains,
        scenario: Manifest,
        modules: &BTreeMap<String, Vec<u8>>,
        seed: Seed,
        options: Options,
    ) -> Result<Self, Box<(HostError, E, Admission)>> {
        let seed = match seed {
            Seed::Given(s) => s,
            Seed::Choose => platform.new_seed(),
        };
        let mut run = Self::prepare(
            engine, admission, platform, domains, scenario, modules, seed, options,
        )?;
        if let Err(e) = run.instantiate_all() {
            let (engine, admission) = run.into_parts();
            return Err(Box::new((e, engine, admission)));
        }
        if let Err(e) = run.say_swapping(platform) {
            let (engine, admission) = run.into_parts();
            return Err(Box::new((e, engine, admission)));
        }
        run.keep_start();
        run.init_all();
        Ok(run)
    }

    /// Everything before instances exist: admission, the estimate, the kernel, the log's first
    /// entries.
    // The Host's parts and the run's inputs, each named: a struct would only move the list.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare(
        engine: E,
        mut admission: Admission,
        platform: &mut dyn Platform,
        domains: &Domains,
        scenario: Manifest,
        modules: &BTreeMap<String, Vec<u8>>,
        seed: u64,
        options: Options,
    ) -> Result<Self, Box<(HostError, E, Admission)>> {
        let capacity = platform.capacity();
        let created_ms = platform.wall_clock_ms();
        let run_id = format!("{created_ms:x}-{seed:x}");
        let mut log = DiagnosticLog::default();
        let mut push = |n: Notification| log.push(created_ms, &run_id, n);
        push(
            Notification::new("host.capacity", Severity::Info, Scope::run(), Ends::Nothing)
                .with("instances", Value::Count(capacity.instances))
                .with("memory", Value::Bytes(capacity.memory_bytes)),
        );

        // The domains (RFC 008 rule 1): a declared one this Host lacks, then their validation,
        // every refusal reported together.
        let domain_values = match domains.instantiate(&scenario) {
            Ok(d) => d,
            Err(n) => {
                push((*n).clone());
                return Err(Box::new((HostError::Domain(vec![*n]), engine, admission)));
            }
        };
        let refusals = domain::validate(&domain_values, &scenario);
        if !refusals.is_empty() {
            for n in &refusals {
                push(n.clone());
            }
            return Err(Box::new((HostError::Domain(refusals), engine, admission)));
        }
        for d in &domain_values {
            if let Some(w) = d.world() {
                admission.add_world(w.spec());
            }
        }
        // Stages 1, 2, 4 per module and class; every refusal reported together.
        let mut errors = Errors::default();
        let mut admitted: BTreeMap<(String, ClassLimits), Arc<Admitted>> = BTreeMap::new();
        let mut facts: BTreeMap<String, ModuleFacts> = BTreeMap::new();
        let mut inputs_modules: Vec<ModuleInput> = Vec::new();
        let mut contracts: Vec<String> = Vec::new();
        for actor in &scenario.actors {
            let ActorKind::Module(name) = &actor.kind else {
                continue;
            };
            let limits = ClassLimits::resolve(
                &scenario.limits_for(&actor.class),
                &options.class_defaults,
                &options.host,
            );
            let key = (name.clone(), limits);
            if admitted.contains_key(&key) {
                continue;
            }
            let Some(bytes) = modules.get(name) else {
                errors.push(florn_admission::Error::at(
                    florn_admission::Kind::Model(Box::new(
                        florn_model::Kind::ReferenceUnresolved {
                            reference: name.clone(),
                            kind: "module",
                        },
                    )),
                    format!("modules.{name}"),
                ));
                continue;
            };
            match admission.admit_under(name, bytes, &limits, options.cost_model) {
                Ok(a) => {
                    if !facts.contains_key(name) {
                        facts.insert(
                            name.clone(),
                            ModuleFacts {
                                source_sha256: a.source_sha256,
                                manifest: a.manifest.clone(),
                                memory_initial_pages: a.memory_initial_pages,
                            },
                        );
                        inputs_modules.push(ModuleInput {
                            name: name.clone(),
                            source_sha256: a.source_sha256,
                            admitted_sha256: a.admitted_sha256,
                        });
                        if !contracts.contains(&a.world.name) {
                            contracts.push(a.world.name.clone());
                        }
                        push(
                            Notification::new(
                                "admission.result",
                                Severity::Info,
                                Scope::file(name),
                                Ends::Nothing,
                            )
                            .with("module", Value::Name(name.clone()))
                            .with("hash", Value::Text(a.admitted_hex()))
                            .with("admitted", Value::Enum("validated".into())),
                        );
                    }
                    admitted.insert(key, a);
                }
                Err(e) => {
                    for err in &e.0 {
                        push(Notification::from_error(
                            err.code(),
                            err.params(),
                            err.field.clone(),
                            Some(name),
                            Ends::Command,
                        ));
                    }
                    errors.0.extend(e.0);
                }
            }
        }
        if !errors.is_empty() {
            return Err(Box::new((errors.into(), engine, admission)));
        }
        // The domains' view of each module's manifest (RFC 008 rule 1, `telemetry`; rule 16).
        let mut refusals = Vec::new();
        for (name, f) in &facts {
            for d in &domain_values {
                for n in d.validate_module(name, &f.manifest) {
                    push(n.clone());
                    if n.severity == Severity::Error {
                        refusals.push(n);
                    }
                }
            }
        }
        if !refusals.is_empty() {
            return Err(Box::new((HostError::Domain(refusals), engine, admission)));
        }
        // Stage 3: the estimate against limits and capacity (RFC 003 rule 20). Swapping (RFC
        // 009 rule 14): the check applies to the resident set, so the capacity is not the
        // estimate's to check; the pool is sized below.
        let swapping = options.residency == Residency::Swap;
        let est = estimate(
            &scenario,
            &facts,
            &options.class_defaults,
            &options.host,
            if swapping {
                EstimateCapacity {
                    instances: u64::MAX,
                    memory_bytes: u64::MAX,
                }
            } else {
                EstimateCapacity {
                    instances: capacity.instances,
                    memory_bytes: capacity.memory_bytes,
                }
            },
        );
        let refuse = |e: &Errors, push: &mut dyn FnMut(Notification)| {
            for err in &e.0 {
                push(Notification::from_error(
                    err.code(),
                    err.params(),
                    err.field.clone(),
                    None,
                    Ends::Command,
                ));
            }
        };
        let est = match est {
            Ok(e) => e,
            Err(e) => {
                refuse(&e, &mut push);
                return Err(Box::new((e.into(), engine, admission)));
            }
        };
        // The resident set (RFC 009 rule 14): the probed capacity less the Host's reserve, and
        // what the memory holds of the largest actor; refused only when one actor's limit
        // exceeds what can be reserved, or when nothing can be resident at all.
        let pool = if swapping {
            let largest = est
                .actors
                .iter()
                .map(|(_, _, l)| l.memory_bytes)
                .max()
                .unwrap_or(0);
            let by_instances = capacity.instances.saturating_sub(Options::RESIDENT_RESERVE);
            let by_memory = capacity
                .memory_bytes
                .checked_div(largest)
                .unwrap_or(u64::MAX);
            let mut pool = by_instances.min(by_memory);
            if let Some(limit) = options.resident_limit {
                pool = pool.min(limit);
            }
            let exceeded = if largest > capacity.memory_bytes {
                Some((largest, capacity.memory_bytes, "memory"))
            } else if pool == 0 && est.instances > 0 {
                Some((1, by_instances, "instances"))
            } else {
                None
            };
            if let Some((need, cap, what)) = exceeded {
                let mut e = Errors::default();
                e.push(florn_admission::Error::new(
                    florn_admission::Kind::CapacityExceeded {
                        need,
                        capacity: cap,
                        what,
                    },
                ));
                refuse(&e, &mut push);
                return Err(Box::new((e.into(), engine, admission)));
            }
            Some(pool)
        } else {
            None
        };
        for (what, used, cap) in [
            ("instances", est.instances, capacity.instances),
            ("memory", est.memory_bytes, capacity.memory_bytes),
        ] {
            if !swapping && used.saturating_mul(10) >= cap.saturating_mul(9) {
                push(
                    Notification::new(
                        "host.capacity-warning",
                        Severity::Warning,
                        Scope::run(),
                        Ends::Nothing,
                    )
                    .with("used", Value::Count(used))
                    .with("capacity", Value::Count(cap))
                    .with("what", Value::Enum(what.into())),
                );
            }
        }
        let config = Config {
            root_seed: seed,
            tie: match scenario.order.tie {
                florn_model::TiePolicy::ByIdentity => TiePolicy::ByIdentity,
                florn_model::TiePolicy::BySeed => TiePolicy::BySeed,
            },
            end: scenario.time.end,
            microsteps_per_tick: scenario
                .order
                .microsteps_per_tick
                .and_then(|m| u32::try_from(m).ok())
                .unwrap_or(Config::default().microsteps_per_tick),
            events_per_instant: scenario
                .order
                .events_per_instant
                .unwrap_or(Config::default().events_per_instant),
            timer_class: CLASS_TIMER,
            chain_interval: options.chain_interval,
            keep_records: options.keep_records,
        };
        let kernel = Kernel::new(config);
        let mut world = match Composite::new(&scenario, domain_values, seed) {
            Ok(w) => w,
            Err(what) => return Err(Box::new((HostError::Malformed(what), engine, admission))),
        };
        world.set_display(options.display);
        let mut worlds: BTreeMap<String, Arc<WorldSigs>> = BTreeMap::new();
        let core_sigs = WorldSigs::core();
        worlds.insert(core_sigs.name.clone(), Arc::new(core_sigs));
        for d in world.domains() {
            if let Some(w) = d.world() {
                let sigs = WorldSigs::of(&w);
                worlds.insert(sigs.name.clone(), Arc::new(sigs));
            }
        }
        let info = engine.info();
        let environment = Environment {
            florn_version: env!("CARGO_PKG_VERSION").to_string(),
            host_build: options.host_build.clone(),
            engine: info.name,
            engine_version: info.version,
            arch: std::env::consts::ARCH.to_string(),
        };
        let inputs = RunInputs {
            scenario_sha256: scenario.scenario_hash(),
            modules: inputs_modules,
            admission_version: ADMISSION_VERSION,
            cost_model_version: options.cost_model,
            cost_table: cost_table(options.cost_model).unwrap_or("").to_string(),
            contracts,
            root_seed: seed,
            interventions: Vec::new(),
            generator: florn_kernel::GENERATOR_NAME.to_string(),
            derivation: florn_kernel::DERIVATION_NAME.to_string(),
            digest: florn_kernel::DIGEST_NAME.to_string(),
        };
        let mut run = Self {
            engine,
            admission,
            scenario,
            kernel,
            world,
            actors: BTreeMap::new(),
            builtins: BTreeMap::new(),
            inputs,
            environment,
            run_id,
            created_ms,
            wall_ms: created_ms,
            log,
            telemetry: VecDeque::new(),
            telemetry_dropped: 0,
            activity: options.display.then(BTreeMap::new),
            reached: 0,
            payloads: BTreeMap::new(),
            options,
            ended: None,
            reproducible: true,
            replay_against: None,
            worlds,
            capacity,
            kept: Vec::new(),
            last_checkpoint_handled: 0,
            checkpoint_requested: false,
            detail: false,
            parent: None,
            pool,
            activation_seq: 0,
            swaps: 0,
        };
        // The slots, without instances yet.
        let tick = run.scenario.time.tick;
        let mut errors = Errors::default();
        for (id, name, limits) in &est.actors {
            let actor = run.scenario.actor(*id).expect("estimated actor exists");
            let a = admitted
                .get(&(name.clone(), *limits))
                .expect("admitted above")
                .clone();
            let config = match encode_config(
                *id,
                &a.manifest,
                &actor.config,
                tick,
                &DomainUnits(run.world.domains()),
            ) {
                Ok(c) => c,
                Err(e) => {
                    errors.0.extend(e.0);
                    continue;
                }
            };
            let Some(world) = run.worlds.get(&a.world.name).cloned() else {
                errors.push(florn_admission::Error::at(
                    florn_admission::Kind::ContractUnsupported {
                        declared: a.world.name.clone(),
                        supported: run.worlds.keys().cloned().collect::<Vec<_>>().join(", "),
                    },
                    format!("modules.{name}"),
                ));
                continue;
            };
            let ports = Val::List(
                actor
                    .ports
                    .iter()
                    .map(|p| {
                        let kind = a
                            .manifest
                            .ports
                            .iter()
                            .position(|k| k.ty == p.r#type)
                            .unwrap_or(0) as u32;
                        Val::Record(vec![Val::U32(p.id), Val::U32(kind), Val::str(&p.name)])
                    })
                    .collect(),
            );
            let quotas = Quotas {
                effects_per_activation: limits.effects_per_activation as usize,
                payload_bytes: limits.payload_bytes as usize,
                telemetry_per_activation_bytes: limits.telemetry_per_activation_bytes as usize,
                pending_timers: limits.pending_timers as usize,
            };
            let sink = Sink::new(
                quotas,
                actor.ports.iter().map(|p| p.id).collect(),
                a.manifest.telemetry.iter().map(|c| c.channel).collect(),
            );
            let spec = ImportSpec {
                functions: a
                    .world
                    .imports
                    .iter()
                    .flat_map(|i| i.functions.iter().map(move |f| (i.module.clone(), f)))
                    .map(|(module, f)| ImportFunction {
                        module,
                        name: f.name.clone(),
                        params: f.params.clone(),
                        results: f.results.clone(),
                    })
                    .collect(),
                globals: vec![HostGlobal {
                    module: BUDGET_IMPORT_MODULE.into(),
                    name: BUDGET_IMPORT_NAME.into(),
                    initial: 0,
                }],
            };
            run.kernel.add_actor(*id);
            run.actors.insert(
                *id,
                Slot {
                    instance: None,
                    swapped: None,
                    last_activated: 0,
                    funcs: Vec::new(),
                    posts: Vec::new(),
                    sink,
                    admitted: a,
                    module_name: name.clone(),
                    limits: *limits,
                    ports,
                    config,
                    spec,
                    world,
                },
            );
        }
        // The built-in actors (RFC 008 rule 19): no instance, no limits; a sink of their own.
        for actor in &run.scenario.actors {
            let ActorKind::Builtin(kind) = &actor.kind else {
                continue;
            };
            let channels = run
                .world
                .builtin(actor.id)
                .map(|(d, kind)| d.builtin_channels(&kind))
                .unwrap_or_default();
            run.kernel.add_actor(actor.id);
            let _ = kind;
            run.builtins.insert(
                actor.id,
                Builtin {
                    sink: Sink::new(
                        Quotas::default(),
                        actor.ports.iter().map(|p| p.id).collect(),
                        channels,
                    ),
                    ports: actor.ports.clone(),
                    config: actor.config.clone(),
                },
            );
        }
        if !errors.is_empty() {
            for err in &errors.0 {
                run.notify(Notification::from_error(
                    err.code(),
                    err.params(),
                    err.field.clone(),
                    None,
                    Ends::Command,
                ));
            }
            let (engine, admission) = run.into_parts();
            return Err(Box::new((errors.into(), engine, admission)));
        }
        for c in &run.scenario.changes {
            let mut e = florn_model::canonical::Encoder::default();
            florn_model::canonical::domain_map(&mut e, &c.payload);
            run.kernel
                .schedule_change(c.at, c.target.identity(), e.finish());
        }
        Ok(run)
    }

    pub(super) fn instantiate_all(&mut self) -> Result<(), HostError> {
        let ids: Vec<Identity> = self.actors.keys().copied().collect();
        for id in ids {
            // Swapping (RFC 009 rule 13): a fresh instance beyond the pool is exported at once
            // and holds its initial state until its first activation brings it in.
            self.ensure_resident(id)?;
        }
        Ok(())
    }

    /// The resident instances now.
    pub(super) fn resident_count(&self) -> u64 {
        self.actors
            .values()
            .filter(|s| s.instance.is_some())
            .count() as u64
    }

    /// An instance for the actor before its activation (RFC 009 rule 13): the one it has, or a
    /// fresh one from the compiled cache with its swapped state written in, after the least
    /// recently activated resident actor was exported and dropped when the pool was full.
    pub(super) fn ensure_resident(&mut self, id: Identity) -> Result<(), HostError> {
        let slot = self.actors.get(&id).ok_or(HostError::NoInstance(id))?;
        if slot.instance.is_some() {
            return Ok(());
        }
        if let Some(pool) = self.pool {
            while self.resident_count() >= pool {
                let victim = self
                    .actors
                    .iter()
                    .filter(|(_, s)| s.instance.is_some())
                    .min_by_key(|(_, s)| s.last_activated)
                    .map(|(id, _)| *id)
                    .ok_or(HostError::NoInstance(id))?;
                let state = self.export_instance(victim)?;
                self.drop_instance(victim);
                if let Some(s) = self.actors.get_mut(&victim) {
                    s.swapped = Some(state);
                }
            }
        }
        self.instantiate(id)?;
        let swapped = self.actors.get_mut(&id).and_then(|s| s.swapped.take());
        if let Some(state) = swapped {
            self.write_state(id, &state)?;
            self.swaps += 1;
        }
        Ok(())
    }

    /// One swap out and in of the first guest-backed actor, timed (RFC 009 rule 14: the cost per
    /// kilobyte the notice says), then `host.swapping`. Nothing of the trace is touched.
    pub(super) fn say_swapping(&mut self, platform: &mut dyn Platform) -> Result<(), HostError> {
        let Some(pool) = self.pool else {
            return Ok(());
        };
        let actors = self.actors.len() as u64;
        let mut cost = 0;
        if let Some(&id) = self.actors.keys().next() {
            self.ensure_resident(id)?;
            let t0 = platform.monotonic_us();
            let state = self.export_instance(id)?;
            self.drop_instance(id);
            self.instantiate(id)?;
            self.write_state(id, &state)?;
            let elapsed = platform.monotonic_us().saturating_sub(t0);
            cost = elapsed / (state.memory.len() as u64 / 1024).max(1);
        }
        let n = Notification::new("host.swapping", Severity::Info, Scope::run(), Ends::Nothing)
            .with("resident", Value::Count(pool.min(actors)))
            .with("actors", Value::Count(actors))
            .with("cost", Value::Count(cost));
        self.notify(n);
        Ok(())
    }

    /// A fresh instance for an actor from its cached module (RFC 005 rule 9: never compiles
    /// twice); exports resolved once (rule 7).
    pub(super) fn instantiate(&mut self, id: Identity) -> Result<(), HostError> {
        let slot = self.actors.get_mut(&id).ok_or(HostError::NoInstance(id))?;
        let module = self.engine.compile(&slot.admitted.bytes)?;
        let mut inst = match self.engine.instantiate(&module, &slot.spec) {
            Ok(i) => i,
            Err(e) => {
                // RFC 005 rule 10: a failure to hold the instance names the actor and the
                // engine's text; a refusal of the module names the module (RFC 003 rule 26).
                let n = match &e {
                    EngineError::Resource(text) => {
                        let n = Notification::new(
                            "host.instance-failed",
                            Severity::Error,
                            Scope::actor(id),
                            Ends::Run,
                        )
                        .with("actor", Value::Identity(id))
                        .with("text", Value::Text(text.clone()));
                        self.log.push(self.wall_ms, &self.run_id, n);
                        return Err(HostError::InstanceFailed {
                            actor: id,
                            text: text.clone(),
                        });
                    }
                    other => Notification::new(
                        "host.engine-refused",
                        Severity::Error,
                        Scope::actor(id),
                        Ends::Run,
                    )
                    .with("module", Value::Name(slot.module_name.clone()))
                    .with("text", Value::Text(other.to_string())),
                };
                self.log.push(self.wall_ms, &self.run_id, n);
                return Err(e.into());
            }
        };
        let mut funcs = Vec::with_capacity(slot.world.exports.len());
        for (n, _, shape) in &slot.world.exports {
            funcs.push(
                self.engine
                    .func(&mut inst, n, *shape)
                    .ok_or_else(|| EngineError::Export(n.to_string()))?,
            );
        }
        // The post-return of an export that returns a list or a string (RFC 002 rule 2);
        // admission required it (RFC 003 rule 5).
        let mut posts = Vec::with_capacity(funcs.len());
        for ((n, _, _), result) in slot.world.exports.iter().zip(&slot.world.results) {
            posts.push(
                if result
                    .as_ref()
                    .is_some_and(florn_abi::Ty::needs_post_return)
                {
                    let post = florn_abi::core_world::post_return_name(n);
                    Some(
                        self.engine
                            .func(&mut inst, &post, florn_abi::Shape::POST_RETURN)
                            .ok_or(EngineError::Export(post))?,
                    )
                } else {
                    None
                },
            );
        }
        slot.instance = Some(inst);
        slot.funcs = funcs;
        slot.posts = posts;
        Ok(())
    }

    /// `init` for every actor, module-backed or built-in, in ascending identity order (RFC 002
    /// rule 36).
    pub(super) fn init_all(&mut self) {
        let mut ids: Vec<Identity> = self.actors.keys().copied().collect();
        ids.extend(self.builtins.keys().copied());
        ids.sort_unstable();
        for id in ids {
            if self.ended.is_some() {
                break;
            }
            let event = Event {
                key: EventKey {
                    tick: 0,
                    microstep: 0,
                    class: florn_kernel::CLASS_CONTROL,
                    tie: 0,
                    origin: id,
                    seq: 0,
                },
                target: id,
                kind: EventKind::Domain {
                    payload: Vec::new(),
                },
            };
            if let Some(b) = self.builtins.get(&id) {
                let (ports, config) = (b.ports.clone(), b.config.clone());
                self.activate_builtin(
                    id,
                    event,
                    &BuiltinEvent::Init {
                        ports: &ports,
                        config: &config,
                    },
                );
                continue;
            }
            let slot = &self.actors[&id];
            let vals = vec![
                Val::U64(0),
                Val::U32(id),
                slot.ports.clone(),
                Val::bytes(&slot.config),
            ];
            self.activate(id, event, INIT, &vals);
        }
    }
}
