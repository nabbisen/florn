//! The run loop (RFC 001 §2.1; RFC 005 rule 15) and the end of a run, a replay and the comparison with
//! the record (RFC 001 rules 49 and 50; RFC 005 rule 26), the `run.toml` of a run (RFC 005 rule 29).

use std::collections::BTreeMap;

use florn_admission::{ADMISSION_VERSION, Admission};
use florn_control::{Ends, Notification, Scope, Severity, Value};
use florn_engine::Engine;
use florn_kernel::trace::Comparison;
use florn_kernel::{Advance, Instant};
use florn_model::Manifest;
use florn_platform::Platform;

use crate::domain::Domains;
use crate::identity::{EndReason, RunToml};

use super::{HostError, Options, Run, at, cost_table};

impl<E: Engine> Run<E> {
    /// Advance by one unit (RFC 001 §2.1; RFC 005 rule 15). Returns the events handled. Pacing is
    /// the caller's: this computes, it never waits.
    pub fn advance(&mut self, unit: Advance) -> u64 {
        if self.ended.is_some() {
            return 0;
        }
        let start = self.kernel.now();
        let mut handled = 0u64;
        loop {
            if self.ended.is_some() {
                break;
            }
            self.checkpoint_if_due();
            let ev = self
                .kernel
                .next_event_if(|i| unit.includes(start, i, handled));
            let Some(ev) = ev else {
                if let Some(f) = self.kernel.run_fault().cloned() {
                    let (what, limit, origins, instant) = match f {
                        florn_kernel::RunFault::MicrostepLimit {
                            tick,
                            limit,
                            origins,
                        } => (
                            "microsteps-per-tick",
                            limit as u64,
                            origins,
                            Instant::new(tick, limit),
                        ),
                        florn_kernel::RunFault::EventsPerInstantLimit {
                            instant,
                            limit,
                            origins,
                        } => ("events-per-instant", limit, origins, instant),
                    };
                    let n = Notification::new(
                        "run.limit-exceeded",
                        Severity::Error,
                        Scope::run(),
                        Ends::Run,
                    )
                    .with("what", Value::Enum(what.into()))
                    .with("limit", Value::Count(limit))
                    .with(
                        "origins",
                        Value::Text(
                            origins
                                .iter()
                                .map(|o| o.to_string())
                                .collect::<Vec<_>>()
                                .join(","),
                        ),
                    )
                    .at(at(instant));
                    self.notify(n);
                    self.end(EndReason::HostFault);
                } else if !self.kernel.has_next() {
                    self.end(EndReason::EndReached);
                }
                break;
            };
            handled += 1;
            self.handle(ev);
        }
        let now = self.kernel.now().tick;
        let end = self.scenario.time.end;
        self.reached = match (self.ended, unit) {
            (Some(EndReason::EndReached), _) => end,
            (Some(_), _) => now,
            (None, Advance::ToTick(t)) => t.min(end).max(now),
            (None, Advance::OneTick) => now.saturating_add(1).min(end),
            (None, _) => now,
        }
        .max(self.reached);
        handled
    }

    /// Run to the end (`end` reached, the queue empty, or a fault that stops the run).
    pub fn run_to_end(&mut self) -> EndReason {
        while self.ended.is_none() {
            self.advance(Advance::ToTick(u64::MAX));
        }
        self.ended.expect("ended")
    }

    pub(super) fn end(&mut self, reason: EndReason) {
        if self.ended.is_some() {
            return;
        }
        self.ended = Some(reason);
        self.inputs.interventions = self.kernel.applied_interventions().to_vec();
        let n = Notification::new("run.ended", Severity::Info, Scope::run(), Ends::Run)
            .with("reason", Value::Enum(reason.as_str().into()))
            .at(at(self.kernel.now()));
        self.notify(n);
        if let Some(stored) = self.replay_against.take() {
            let c = florn_kernel::trace::compare_reference(&self.kernel.trace.stored(), &stored);
            let n = match c {
                Comparison::Matched { points } => Notification::new(
                    "replay.matched",
                    Severity::Info,
                    Scope::run(),
                    Ends::Nothing,
                )
                .with("points", Value::Count(points)),
                Comparison::Diverged { from, to } => Notification::new(
                    "replay.diverged",
                    Severity::Error,
                    Scope::run(),
                    Ends::Nothing,
                )
                .with("from", Value::Count(from))
                .with("to", Value::Count(to)),
                Comparison::Incomparable => Notification::new(
                    "replay.diverged",
                    Severity::Error,
                    Scope::run(),
                    Ends::Nothing,
                )
                .with("from", Value::Count(0))
                .with("to", Value::Count(0)),
            };
            self.notify(n);
            self.replay_against = Some(stored);
        }
    }

    /// End the run on the user's word.
    pub fn end_run(&mut self) {
        self.end(EndReason::User);
    }

    /// A replay (RFC 001 rules 49 and 50; RFC 005 rule 26): a new run from the record's inputs,
    /// the recorded interventions applied at their instants, compared at the end with the stored
    /// chain. The scenario and modules must be the record's (`checkpoint.mismatch` otherwise).
    // The Host's parts and the run's inputs, each named: a struct would only move the list.
    #[allow(clippy::too_many_arguments)]
    pub fn replay(
        engine: E,
        admission: Admission,
        platform: &mut dyn Platform,
        domains: &Domains,
        scenario: Manifest,
        modules: &BTreeMap<String, Vec<u8>>,
        record: &RunToml,
        mut options: Options,
    ) -> Result<Self, HostError> {
        options.chain_interval = record.chain_interval;
        let seed = record.inputs.root_seed;
        let wanted = record.inputs.cost_model_version;
        let known = cost_table(wanted).is_some();
        if known {
            options.cost_model = wanted;
        }
        let mut run = Self::prepare(
            engine, admission, platform, domains, scenario, modules, seed, options,
        )
        .map_err(|parts| parts.0)?;
        if !known {
            return Err(run.mismatch("cost-model", wanted.to_string(), "none".to_string()));
        }
        let hex = florn_model::canonical::hex;
        let i = &record.inputs;
        if i.scenario_sha256 != run.inputs.scenario_sha256 {
            return Err(run.mismatch(
                "scenario-hash",
                hex(&i.scenario_sha256),
                hex(&run.inputs.scenario_sha256),
            ));
        }
        for m in &i.modules {
            let found = run.inputs.modules.iter().find(|x| x.name == m.name);
            match found {
                Some(x) if x.source_sha256 == m.source_sha256 => {}
                Some(x) => {
                    return Err(run.mismatch(
                        "module-hash",
                        hex(&m.source_sha256),
                        hex(&x.source_sha256),
                    ));
                }
                None => {
                    return Err(run.mismatch("module-hash", hex(&m.source_sha256), String::new()));
                }
            }
        }
        if i.admission_version != ADMISSION_VERSION {
            return Err(run.mismatch(
                "admission-version",
                i.admission_version.to_string(),
                ADMISSION_VERSION.to_string(),
            ));
        }
        if i.cost_model_version != run.inputs.cost_model_version {
            return Err(run.mismatch(
                "cost-model",
                i.cost_model_version.to_string(),
                run.inputs.cost_model_version.to_string(),
            ));
        }
        run.kernel.set_replay(i.interventions.clone());
        // The record's points are stored by this run too, so that a record exported before
        // its run ended compares at every point it holds (RFC 001 rule 50).
        run.kernel
            .trace
            .watch(record.chain.iter().map(|(n, _)| *n).collect());
        run.replay_against = Some(record.chain.clone());
        // A branch's provenance travels with its replay (RFC 009 rule 12: compared by nothing).
        run.parent = record.parent.clone();
        run.instantiate_all()?;
        run.say_swapping(platform)?;
        run.keep_start();
        run.init_all();
        Ok(run)
    }

    /// The comparison of a replay with its record, once the run has ended: at every point the
    /// record holds (RFC 001 rule 50; a record exported before its run ended, or a branch's
    /// parent, holds fewer than the scenario's end), as `replay.matched` says it.
    pub fn comparison(&self) -> Option<Comparison> {
        let stored = self.replay_against.as_ref()?;
        self.ended?;
        Some(florn_kernel::trace::compare_reference(
            &self.kernel.trace.stored(),
            stored,
        ))
    }

    /// `run.toml` of the run record (RFC 005 rule 29).
    pub fn run_toml(&self) -> RunToml {
        let mut inputs = self.inputs.clone();
        inputs.interventions = self.kernel.applied_interventions().to_vec();
        RunToml {
            environment: self.environment.clone(),
            inputs,
            parent: self.parent.clone(),
            chain: self.kernel.trace.stored(),
            chain_interval: self.kernel.trace.interval(),
            records: self.kernel.trace.records(),
            ended: self.ended,
            reproducible: self.reproducible,
            run_id: self.run_id.clone(),
            created_ms: self.created_ms,
        }
    }
}
