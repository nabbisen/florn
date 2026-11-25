//! Loading a scenario, a module or a checkpoint, and starting, replaying or resuming a run from them
//! (RFC 005 rules 10 and 26).

use super::*;

impl<E: Engine> Controller<E> {
    pub(super) fn load_scenario(&mut self, bytes: &[u8], out: &mut Vec<Message>) {
        // The ended run gives its engine and admission cache back: the next run needs them.
        if let Some(run) = self.run.take() {
            let (e, a) = run.into_parts();
            self.engine = Some(e);
            self.admission = Some(a);
        }
        self.scenario = None;
        self.state = RunState::NoScenario;
        let host = self.options.host;
        let is_zip = bytes.starts_with(b"PK\x03\x04");
        let loaded = if is_zip {
            Package::read(bytes, &host).map(|p| {
                for (h, b) in p.modules {
                    self.modules.insert(h, b);
                }
                if !p.missing.is_empty() {
                    out.push(Message::Notification(Code::PackagePartial.notification(
                        Scope::file("package"),
                        vec![Value::Count(p.missing.len() as u64)],
                    )));
                }
                Loaded {
                    text: p.manifest_text,
                    manifest: p.manifest,
                }
            })
        } else {
            match std::str::from_utf8(bytes) {
                Ok(text) => Manifest::parse(text, &host).map(|manifest| Loaded {
                    text: text.to_string(),
                    manifest,
                }),
                Err(e) => Err(florn_model::Errors(vec![florn_model::Error::new(
                    florn_model::Kind::FileUnreadable {
                        kind: "manifest",
                        reason: e.to_string(),
                    },
                )])),
            }
        };
        // The domains' validation (RFC 008 rule 1) follows the core's, at load.
        let loaded = loaded.and_then(|l| {
            let notes = match self.domains.instantiate(&l.manifest) {
                Ok(d) => domain::validate(&d, &l.manifest),
                Err(n) => vec![*n],
            };
            // The validation returns every note; only an error refuses (RFC 012 finding F-18).
            let refused = notes
                .iter()
                .any(|n| n.severity == florn_control::Severity::Error);
            out.extend(notes.into_iter().map(Message::Notification));
            if refused {
                self.state = RunState::Invalid;
                Err(florn_model::Errors::default())
            } else {
                Ok(l)
            }
        });
        match loaded {
            Ok(l) => {
                self.scenario = Some(l);
                self.refresh_ready();
                out.push(Message::Answer(Answer::Done));
            }
            Err(errors) => {
                self.state = RunState::Invalid;
                for e in errors.0 {
                    out.push(Message::Notification(
                        florn_control::Notification::from_error(
                            e.code(),
                            e.params(),
                            e.field.clone(),
                            Some(if is_zip { "package" } else { "manifest" }),
                            florn_control::Ends::Command,
                        ),
                    ));
                }
            }
        }
    }

    /// The modules the scenario references and the Host holds, by `[modules]` name; the missing ones.
    pub(super) fn modules_for(
        &self,
        manifest: &Manifest,
    ) -> (BTreeMap<String, Vec<u8>>, Vec<String>) {
        let mut found = BTreeMap::new();
        let mut missing = Vec::new();
        for (name, m) in &manifest.modules {
            let h = hex(&m.sha256);
            match self.modules.get(&h) {
                Some(b) => {
                    found.insert(name.clone(), b.clone());
                }
                None => missing.push(h),
            }
        }
        (found, missing)
    }

    pub(super) fn refresh_ready(&mut self) {
        if self.scenario.is_some() && matches!(self.state, RunState::NoScenario | RunState::Ready) {
            self.state = RunState::Ready;
        }
    }

    /// A run from a checkpoint file (RFC 009 rule 6): the loaded scenario and its modules, the
    /// checkpoint's inputs and instant; refused with the field named on a mismatch.
    pub(super) fn load_checkpoint(&mut self, checkpoint: Checkpoint, out: &mut Vec<Message>) {
        let Some(loaded) = &self.scenario else {
            return self.notify(out, self.not_now("load-checkpoint"));
        };
        let (modules, missing) = self.modules_for(&loaded.manifest);
        if let Some(h) = missing.first() {
            return self.notify(
                out,
                control_invalid("load-checkpoint", "missing-field", &format!("modules.{h}")),
            );
        }
        if let Some(run) = self.run.take() {
            let (e, a) = run.into_parts();
            self.engine = Some(e);
            self.admission = Some(a);
        }
        let (Some(engine), Some(admission)) = (self.engine.take(), self.admission.take()) else {
            return self.notify(out, self.not_now("load-checkpoint"));
        };
        let manifest = loaded.manifest.clone();
        let mut options = self.options.clone();
        options.display = self.subs.display;
        let result = Run::from_checkpoint(
            engine,
            admission,
            self.platform.as_mut(),
            &self.domains,
            manifest,
            &modules,
            &checkpoint,
            options,
        );
        self.started(result, out);
    }

    /// A cap on the resident set of swapping runs (the conformance kit's pools of one and two).
    pub fn set_resident_limit(&mut self, limit: Option<u64>) {
        self.options.resident_limit = limit;
    }

    pub(super) fn new_run(
        &mut self,
        seed: SeedChoice,
        residency: florn_control::Residency,
        replay: Option<record::Read_>,
        out: &mut Vec<Message>,
    ) {
        let Some(loaded) = &self.scenario else {
            return self.notify(out, self.not_now("new-run"));
        };
        let (modules, missing) = self.modules_for(&loaded.manifest);
        if let Some(h) = missing.first() {
            return self.notify(
                out,
                control_invalid("new-run", "missing-field", &format!("modules.{h}")),
            );
        }
        // The previous run gives its engine and admission cache back.
        if let Some(run) = self.run.take() {
            let (e, a) = run.into_parts();
            self.engine = Some(e);
            self.admission = Some(a);
        }
        let (Some(engine), Some(admission)) = (self.engine.take(), self.admission.take()) else {
            return self.notify(out, self.not_now("new-run"));
        };
        let manifest = loaded.manifest.clone();
        let mut options = self.options.clone();
        options.display = self.subs.display;
        // Swapping is asked for with the run (RFC 009 rule 14); a replay keeps the Host's option.
        if replay.is_none() {
            options.residency = match residency {
                florn_control::Residency::Resident => crate::run::Residency::Resident,
                florn_control::Residency::Swap => crate::run::Residency::Swap,
            };
        }
        let result = match replay {
            // The record's trace and payloads go with it: a continuous domain's replay is compared
            // with them within a tolerance across Hosts (RFC 013 rule 20).
            Some(record) => Run::replay(
                engine,
                admission,
                self.platform.as_mut(),
                &self.domains,
                manifest,
                &modules,
                &record.run,
                options,
            )
            .map(|mut run| {
                run.record_trace(record.trace, record.payloads);
                run
            }),
            None => {
                let seed = match seed {
                    SeedChoice::Given(s) => Seed::Given(s),
                    SeedChoice::Choose => Seed::Choose,
                };
                Run::try_new(
                    engine,
                    admission,
                    self.platform.as_mut(),
                    &self.domains,
                    manifest,
                    &modules,
                    seed,
                    options,
                )
                .map_err(|parts| {
                    // A failed start gives the engine back: the Host goes on (RFC 005 rule 10).
                    let (e, engine, admission) = *parts;
                    self.engine = Some(engine);
                    self.admission = Some(admission);
                    e
                })
            }
        };
        self.started(result, out);
    }

    /// A run that started, or the reason it did not (RFC 005 rule 10: the Host goes on).
    fn started(&mut self, result: Result<Run<E>, HostError>, out: &mut Vec<Message>) {
        match result {
            Ok(mut run) => {
                self.log_seen = 0;
                // The run's log says what carries its display stream (rule 14): one line.
                if let Some(t) = self.transport {
                    run.notify_now(
                        Code::TelemetryTransport
                            .notification(Scope::run(), vec![Value::Enum(t.as_str().into())]),
                    );
                }
                self.run = Some(run);
                self.parent_record = None;
                self.state = RunState::Paused;
                self.target = None;
                self.drain_log(out);
                out.push(Message::Answer(Answer::Done));
            }
            Err(e) => {
                self.state = RunState::Ready;
                match e {
                    HostError::InstanceFailed { actor, text } => {
                        out.push(Message::Notification(
                            Code::HostInstanceFailed.notification(
                                Scope::actor(actor),
                                vec![Value::Identity(actor), Value::Text(text)],
                            ),
                        ));
                    }
                    HostError::Refused(errors) => {
                        for err in errors.0 {
                            out.push(Message::Notification(
                                florn_control::Notification::from_error(
                                    err.code(),
                                    err.params(),
                                    err.field.clone(),
                                    None,
                                    florn_control::Ends::Command,
                                ),
                            ));
                        }
                    }
                    HostError::CheckpointMismatch {
                        what,
                        expected,
                        found,
                    } => {
                        out.push(Message::Notification(
                            Code::CheckpointMismatch.notification(
                                Scope::file("record"),
                                vec![
                                    Value::Enum(what.into()),
                                    Value::Text(expected),
                                    Value::Text(found),
                                ],
                            ),
                        ));
                    }
                    other => out.push(Message::Notification(
                        Code::HostInstanceFailed.notification(
                            Scope::run(),
                            vec![Value::Identity(0), Value::Text(other.to_string())],
                        ),
                    )),
                }
            }
        }
    }
}
