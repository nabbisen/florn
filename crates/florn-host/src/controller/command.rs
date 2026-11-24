//! The control interface's commands (RFC 005 rule 25): each command checked against the run's state
//! and applied.

use super::*;

impl<E: Engine> Controller<E> {
    pub(super) fn command(&mut self, c: Command, out: &mut Vec<Message>) {
        if let Err(n) = c.validate() {
            self.notify(out, *n);
            return;
        }
        let name = c.name();
        match c {
            Command::LoadScenario { bytes } => {
                if matches!(self.state, RunState::Running | RunState::Paused) {
                    return self.notify(out, self.not_now(name));
                }
                self.load_scenario(&bytes, out);
            }
            Command::LoadModule { bytes } => {
                if matches!(self.state, RunState::Running | RunState::Paused) {
                    return self.notify(out, self.not_now(name));
                }
                // The module maximum applies before anything is held (RFC 006 section 7).
                let max = florn_model::package::MAX_MODULE_BYTES;
                if bytes.len() as u64 > max {
                    return self.notify(
                        out,
                        Code::ModuleTooLarge.notification(
                            Scope::file("module"),
                            vec![
                                Value::Enum("bytes".into()),
                                Value::Count(bytes.len() as u64),
                                Value::Count(max),
                            ],
                        ),
                    );
                }
                let h = hex(&florn_admission::sha256(&bytes));
                self.modules.insert(h, bytes);
                self.refresh_ready();
                out.push(Message::Answer(Answer::Done));
            }
            Command::Admit { bytes } => {
                let Some(admission) = self.admission.as_mut() else {
                    return self.notify(out, self.not_now(name));
                };
                // Under the loaded scenario's default class when there is one, so that the hash
                // printed is the hash a run of that scenario records; else the Host's defaults.
                let limits = match &self.scenario {
                    Some(l) => ClassLimits::resolve(
                        &l.manifest.limits_for("default"),
                        &self.options.class_defaults,
                        &self.options.host,
                    ),
                    None => self.options.class_defaults,
                };
                let cost_model = self.options.cost_model;
                match admission.admit_under("module", &bytes, &limits, cost_model) {
                    Ok(a) => {
                        // A module from a file has no `[modules]` name yet: its kind names it.
                        let summary =
                            module_summary(&a.manifest.kind, &a.manifest, &a.source_sha256);
                        out.push(Message::Answer(Answer::Admitted {
                            module: summary,
                            admitted_sha256: a.admitted_hex(),
                        }));
                        out.push(Message::Notification(Code::AdmissionResult.notification(
                            Scope::file("module"),
                            vec![
                                Value::Name("module".into()),
                                Value::Text(a.admitted_hex()),
                                Value::Enum("validated".into()),
                            ],
                        )));
                    }
                    Err(e) => {
                        for err in &e.0 {
                            out.push(Message::Notification(
                                florn_control::Notification::from_error(
                                    err.code(),
                                    err.params(),
                                    err.field.clone(),
                                    Some("module"),
                                    florn_control::Ends::Command,
                                ),
                            ));
                        }
                    }
                }
            }
            Command::NewRun { seed, residency } => {
                if !matches!(self.state, RunState::Ready | RunState::Ended) {
                    return self.notify(out, self.not_now(name));
                }
                self.new_run(seed, residency, None, out);
            }
            Command::EndRun => {
                if !matches!(self.state, RunState::Running | RunState::Paused) {
                    return self.notify(out, self.not_now(name));
                }
                if let Some(run) = &mut self.run {
                    run.end_run();
                }
                self.target = None;
                self.after_advance(out, true);
                out.push(Message::Answer(Answer::Done));
            }
            Command::Advance { unit } => {
                // Backward (RFC 009 rule 7) from a run that ended too: the person goes back
                // from the end as from a pause; forward from the end is nothing.
                let backward = matches!(unit, Unit::ToTick(t)
                    if self.run.as_ref().is_some_and(|r| t < r.kernel().now().tick));
                if !(self.state == RunState::Paused || (self.state == RunState::Ended && backward))
                {
                    return self.notify(out, self.not_now(name));
                }
                let Some(run) = &mut self.run else {
                    return self.notify(out, self.not_now(name));
                };
                run.set_wall_clock(self.platform.wall_clock_ms());
                // Backward (RFC 009 rule 7): the nearest kept checkpoint at or before the tick,
                // then forward to it, at once; paced, the way forward from the checkpoint is paced.
                if let Unit::ToTick(t) = unit
                    && t < run.kernel().now().tick
                {
                    // A branch begins at its instant (RFC 009 rule 10): nothing before it is its
                    // to move to.
                    if run.kept().first().is_none_or(|k| k.instant.tick > t) {
                        return self.notify(out, control_invalid(name, "out-of-range", "tick"));
                    }
                    let paced = matches!(self.pacing, Pacing::TicksPerSecond(_));
                    let result = if paced {
                        let index = run
                            .kept()
                            .iter()
                            .rposition(|k| k.instant.tick <= t)
                            .expect("the start is kept");
                        run.restore_kept(index)
                    } else {
                        run.go_back(t)
                    };
                    if let Err(e) = result {
                        self.drain_log(out);
                        return self.notify(
                            out,
                            Code::FileUnreadable.notification(
                                Scope::file("checkpoint"),
                                vec![Value::Enum("checkpoint".into()), Value::Text(e.to_string())],
                            ),
                        );
                    }
                    // Back from the end as from a pause: the run goes on from here.
                    self.state = RunState::Paused;
                    if paced && run.kernel().now().tick < t {
                        self.target = Some(t);
                        self.state = RunState::Running;
                    }
                    self.after_advance(out, true);
                    out.push(Message::Answer(Answer::Done));
                    return;
                }
                match (unit, self.pacing) {
                    (Unit::OneEvent, _) => {
                        run.advance(Advance::OneEvent);
                    }
                    (Unit::OneInstant, _) => {
                        run.advance(Advance::OneInstant);
                    }
                    (Unit::OneTick, _) => {
                        run.advance(Advance::OneTick);
                    }
                    (Unit::ToTick(t), Pacing::Fastest) => {
                        run.advance(Advance::ToTick(t));
                    }
                    (Unit::ToTick(t), Pacing::TicksPerSecond(_)) => {
                        if t <= run.kernel().now().tick {
                            return self.notify(out, control_invalid(name, "out-of-range", "tick"));
                        }
                        self.target = Some(t);
                        self.state = RunState::Running;
                    }
                }
                self.after_advance(out, true);
                out.push(Message::Answer(Answer::Done));
            }
            Command::Pause => {
                if self.state != RunState::Running {
                    return self.notify(out, self.not_now(name));
                }
                self.state = RunState::Paused;
                self.target = None;
                out.push(Message::Answer(Answer::Done));
            }
            Command::SetPacing { pacing } => {
                self.pacing = pacing;
                out.push(Message::Answer(Answer::Done));
            }
            Command::Intervene {
                target,
                content,
                at,
            } => {
                if !matches!(self.state, RunState::Running | RunState::Paused) {
                    return self.notify(out, self.not_now(name));
                }
                let Some(run) = &mut self.run else {
                    return self.notify(out, self.not_now(name));
                };
                if let Some(t) = at
                    && (t < run.kernel().now().tick || t >= run.scenario().time.end)
                {
                    return self.notify(out, control_invalid(name, "out-of-range", "at"));
                }
                // The core's `message` and the domains' kinds alike (RFC 008 rule 4).
                if let Err(n) = run.intervene_map(at, target, &content) {
                    return self.notify(out, *n);
                }
                out.push(Message::Answer(Answer::Done));
            }
            Command::Export { kind, confirmed } => self.export(kind, confirmed, out),
            Command::Checkpoint => {
                let Some(run) = &mut self.run else {
                    return self.notify(out, self.not_now(name));
                };
                // Taken now at a boundary, else at the next one the run reaches (RFC 009 rule 2).
                run.request_checkpoint();
                self.drain_log(out);
                out.push(Message::Answer(Answer::Done));
            }
            Command::Restore { checkpoint } => {
                if !matches!(self.state, RunState::Paused | RunState::Ended) {
                    return self.notify(out, self.not_now(name));
                }
                let Some(run) = &mut self.run else {
                    return self.notify(out, self.not_now(name));
                };
                let index = checkpoint as usize;
                if index >= run.kept().len() {
                    return self.notify(out, control_invalid(name, "out-of-range", "checkpoint"));
                }
                match run.restore_kept(index) {
                    Ok(()) => {
                        self.state = RunState::Paused;
                        self.target = None;
                        self.after_advance(out, true);
                        out.push(Message::Answer(Answer::Done));
                    }
                    Err(e) => {
                        self.drain_log(out);
                        if !matches!(e, HostError::CheckpointMismatch { .. }) {
                            out.push(Message::Notification(Code::FileUnreadable.notification(
                                Scope::file("checkpoint"),
                                vec![Value::Enum("checkpoint".into()), Value::Text(e.to_string())],
                            )));
                        }
                    }
                }
            }
            Command::LoadCheckpoint { bytes } => {
                if !matches!(self.state, RunState::Ready | RunState::Ended) {
                    return self.notify(out, self.not_now(name));
                }
                match Checkpoint::decode_file(&bytes, snapshot::MAX_FILE_BYTES) {
                    Ok(checkpoint) => self.load_checkpoint(checkpoint, out),
                    Err("file.too-large") => {
                        out.push(Message::Notification(Code::FileTooLarge.notification(
                            Scope::file("checkpoint"),
                            vec![
                                Value::Enum("checkpoint".into()),
                                Value::Bytes(bytes.len() as u64),
                                Value::Bytes(snapshot::MAX_FILE_BYTES),
                            ],
                        )))
                    }
                    Err(reason) => {
                        out.push(Message::Notification(Code::FileUnreadable.notification(
                            Scope::file("checkpoint"),
                            vec![Value::Enum("checkpoint".into()), Value::Text(reason.into())],
                        )))
                    }
                }
            }
            Command::SetCostModel { version } => {
                if matches!(self.state, RunState::Running | RunState::Paused) {
                    return self.notify(out, self.not_now(name));
                }
                if florn_admission::cost_table(version).is_none() {
                    return self.notify(out, control_invalid(name, "out-of-range", "version"));
                }
                self.options.cost_model = version;
                out.push(Message::Answer(Answer::Done));
            }
            Command::Branch => {
                if !matches!(self.state, RunState::Paused | RunState::Ended) {
                    return self.notify(out, self.not_now(name));
                }
                let Some(run) = &mut self.run else {
                    return self.notify(out, self.not_now(name));
                };
                run.set_wall_clock(self.platform.wall_clock_ms());
                match run.branch(self.platform.as_mut()) {
                    Ok(parent) => {
                        // The parent's last entries, then the branch's log from its first.
                        let entries = parent.log.entries();
                        for e in &entries[self.log_seen.min(entries.len())..] {
                            out.push(Message::Notification(e.notification.clone()));
                        }
                        self.log_seen = 0;
                        self.parent_record = Some(parent);
                        self.state = RunState::Paused;
                        self.target = None;
                        self.after_advance(out, true);
                        out.push(Message::Answer(Answer::Done));
                    }
                    // Within an instant (RFC 001 rule 51): at the next boundary.
                    Err(HostError::MidInstant) => {
                        self.drain_log(out);
                        self.notify(out, self.not_now(name));
                    }
                    Err(e) => {
                        self.drain_log(out);
                        out.push(Message::Notification(
                            Code::HostInstanceFailed.notification(
                                Scope::run(),
                                vec![Value::Identity(0), Value::Text(e.to_string())],
                            ),
                        ));
                    }
                }
            }
            Command::Replay { record } => {
                if !matches!(self.state, RunState::Ready | RunState::Ended) {
                    return self.notify(out, self.not_now(name));
                }
                match record::read(&record) {
                    Ok(read) => self.new_run(
                        SeedChoice::Given(read.run.inputs.root_seed),
                        florn_control::Residency::Resident,
                        Some(read),
                        out,
                    ),
                    Err(reason) if reason == "file.too-large" => {
                        out.push(Message::Notification(Code::FileTooLarge.notification(
                            Scope::file("record"),
                            vec![
                                Value::Enum("run-record".into()),
                                Value::Bytes(record.len() as u64),
                                Value::Bytes(record::MAX_BYTES),
                            ],
                        )))
                    }
                    Err(reason) => {
                        out.push(Message::Notification(Code::FileUnreadable.notification(
                            Scope::file("record"),
                            vec![Value::Enum("run-record".into()), Value::Text(reason)],
                        )))
                    }
                }
            }
        }
    }
}
