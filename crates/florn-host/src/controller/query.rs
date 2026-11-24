//! The queries of the control interface (RFC 005 rule 27): the Host's answers about the scenario, the
//! run, an actor, the trace.

use super::*;

impl<E: Engine> Controller<E> {
    pub(super) fn query(&mut self, q: &Query) -> Answer {
        match q {
            Query::Scenario => {
                let Some(l) = &self.scenario else {
                    return Answer::Scenario(None);
                };
                let manifest = l.manifest.clone();
                let (found, missing) = self.modules_for(&manifest);
                let mut classes: Vec<(u8, String)> = vec![
                    (0, "control".into()),
                    (crate::world::CLASS_TIMER, "timer".into()),
                    (crate::world::CLASS_MESSAGE, "message".into()),
                ];
                if let Ok(domains) = self.domains.instantiate(&manifest) {
                    for d in &domains {
                        classes.extend(d.classes().into_iter().map(|(c, n)| (c, n.to_string())));
                    }
                }
                classes.sort();
                let had_admission = self.admission.is_some();
                let mut admission = self.admission.take().unwrap_or_default();
                // The summary's admitted hashes are those of the Host's cost-model setting.
                let cost_model = self.options.cost_model;
                let modules = manifest
                    .modules
                    .iter()
                    .map(|(name, m)| {
                        match found.get(name).and_then(|b| {
                            admission
                                .admit_under(name, b, &ClassLimits::defaults(), cost_model)
                                .ok()
                        }) {
                            Some(a) => module_summary(name, &a.manifest, &m.sha256),
                            None => ModuleSummary {
                                name: name.clone(),
                                source_sha256: hex(&m.sha256),
                                contract: m.contract.clone(),
                                kind: String::new(),
                                description: None,
                                port_kinds: Vec::new(),
                                params: Vec::new(),
                                channels: Vec::new(),
                                needs: Vec::new(),
                                tables: Vec::new(),
                            },
                        }
                    })
                    .collect();
                if had_admission {
                    self.admission = Some(admission);
                }
                Answer::Scenario(Some(ScenarioSummary {
                    name: manifest.name.clone(),
                    sha256: manifest.scenario_hash_hex(),
                    tick: manifest.time.tick.as_str().to_string(),
                    end: manifest.time.end,
                    tie: manifest.order.tie.as_str().to_string(),
                    seed: manifest.seed,
                    actors: manifest.actors.iter().map(actor_summary).collect(),
                    connections: manifest
                        .connections
                        .iter()
                        .map(|c| ConnectionSummary {
                            id: c.id,
                            name: c.name.clone(),
                            ports: c.ports.iter().map(|p| (p.actor, p.port)).collect(),
                            ty: c.r#type.clone(),
                            attrs: domain_map_text(&c.attrs),
                            per_port: c
                                .per_port
                                .iter()
                                .map(|(p, m)| ((p.actor, p.port), domain_map_text(m)))
                                .collect(),
                        })
                        .collect(),
                    modules,
                    missing_modules: missing,
                    domains: manifest.domains.clone(),
                    changes: manifest
                        .changes
                        .iter()
                        .map(|c| ChangeSummary {
                            at: c.at,
                            target: c.target.identity(),
                            payload: domain_map_text(&c.payload),
                        })
                        .collect(),
                    classes,
                    limits: manifest
                        .limits
                        .iter()
                        .map(|(class, l)| (class.clone(), limit_fields(l)))
                        .collect(),
                }))
            }
            Query::Decode { protocol, payload } => {
                // The scenario's domains, as composed for a run would be (RFC 008 rule 1).
                let domains = self
                    .scenario
                    .as_ref()
                    .and_then(|l| self.domains.instantiate(&l.manifest).ok())
                    .unwrap_or_default();
                Answer::Decoded(domains.iter().find_map(|d| d.decode(protocol, payload)))
            }
            Query::Payload { digest } => {
                let bytes = hex_digest(digest).and_then(|d| {
                    self.run
                        .as_ref()
                        .and_then(|r| r.payload(&d).map(<[u8]>::to_vec))
                });
                Answer::Payload(bytes)
            }
            Query::RunState => Answer::RunState(RunStateInfo {
                state: self.state,
                now: self.run.as_ref().map(|r| at(r.kernel().now())),
                handled: self.run.as_ref().map_or(0, |r| r.kernel().handled()),
                records: self.run.as_ref().map_or(0, |r| r.kernel().trace.records()),
                chain: self.run.as_ref().map(|r| r.chain_hex()),
                seed: self.run.as_ref().map(|r| r.inputs().root_seed),
                pacing: self.pacing,
                ended: self
                    .run
                    .as_ref()
                    .and_then(|r| r.ended())
                    .map(|e| e.as_str().to_string()),
                reproducible: self.run.as_ref().is_none_or(|r| r.reproducible()),
                run_id: self.run.as_ref().map(|r| r.run_id().to_string()),
            }),
            Query::Actor { id } => Answer::Actor(
                self.scenario
                    .as_ref()
                    .and_then(|l| l.manifest.actor(*id))
                    .map(|a| {
                        let summary = actor_summary(a);
                        let (alive, faults, recent) = match &self.run {
                            Some(run) => {
                                let faults = run
                                    .log()
                                    .entries()
                                    .iter()
                                    .filter(|e| {
                                        e.notification.code == "actor.faulted"
                                            && e.notification.scope.actor == Some(*id)
                                    })
                                    .map(|e| {
                                        let n = &e.notification;
                                        let get = |name: &str| {
                                            n.params
                                                .iter()
                                                .find(|(k, _)| k == name)
                                                .map(|(_, v)| v.clone())
                                        };
                                        FaultInfo {
                                            at: n.at.unwrap_or_default(),
                                            code: match get("fault") {
                                                Some(Value::Code(c)) => c,
                                                _ => String::new(),
                                            },
                                            metered: match get("metered") {
                                                Some(Value::Count(c)) => c,
                                                _ => 0,
                                            },
                                            parameter: match get("parameter") {
                                                Some(Value::Count(c)) => c as u32,
                                                _ => 0,
                                            },
                                        }
                                    })
                                    .collect();
                                (run.alive(*id), faults, Vec::new())
                            }
                            None => (false, Vec::new(), Vec::new()),
                        };
                        ActorInfo {
                            summary,
                            alive,
                            faults,
                            recent_telemetry: recent,
                        }
                    }),
            ),
            // The window from the kept records, or by re-execution when they do not hold it
            // (RFC 009 rule 9); nothing of the way is logged or displayed.
            Query::TraceWindow { from, count } => Answer::TraceWindow(
                self.run
                    .as_mut()
                    .map(|r| {
                        r.trace_window(*from, *count)
                            .iter()
                            .map(trace_entry)
                            .collect()
                    })
                    .unwrap_or_default(),
            ),
            Query::Capacity => {
                let c = self.platform.capacity();
                Answer::Capacity(CapacityInfo {
                    instances: c.instances,
                    memory_bytes: c.memory_bytes,
                    swap_cost_us_per_kb: c.swap_cost_us_per_kb,
                })
            }
            Query::Checkpoints => Answer::Checkpoints(
                self.run
                    .as_ref()
                    .map(|r| {
                        r.kept()
                            .iter()
                            .enumerate()
                            .map(|(i, k)| CheckpointSummary {
                                index: i as u32,
                                instant: at(k.instant),
                                records: k.records,
                                handled: k.handled,
                                bytes: k.bytes,
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            ),
            Query::Support => {
                let info = match (&self.engine, &self.run) {
                    (Some(e), _) => e.info(),
                    (None, Some(r)) => r.engine().info(),
                    (None, None) => florn_engine::EngineInfo {
                        name: String::new(),
                        version: String::new(),
                    },
                };
                Answer::Support(SupportInfo {
                    control_version: CONTROL_VERSION,
                    florn_version: env!("CARGO_PKG_VERSION").to_string(),
                    host_build: self.options.host_build.clone(),
                    engine: info.name,
                    engine_version: info.version,
                    arch: std::env::consts::ARCH.to_string(),
                    features: vec![
                        "core".into(),
                        "replay".into(),
                        "export-state".into(),
                        "checkpoint".into(),
                        "cost-model-2".into(),
                        "branch".into(),
                        "swap".into(),
                    ],
                })
            }
        }
    }
}
