//! Export and restore of actor state (RFC 003 §6), checkpoints and the kept ones (RFC 009 rules 1
//! to 9), going back, the trace window, a branch (rule 12).

use std::collections::BTreeMap;

use florn_admission::{ADMISSION_VERSION, Admission, STACK_EXPORT};
use florn_control::{Ends, Notification, Scope, Severity, Value};
use florn_engine::{Engine, EngineError, GlobalValue};
use florn_kernel::{Advance, AppliedIntervention, Identity, Kernel};
use florn_model::Manifest;
use florn_platform::Platform;

use crate::domain::Domains;
use crate::identity::EndReason;
use crate::log::DiagnosticLog;
use crate::record::Contents;
use crate::snapshot::{ActorState, Checkpoint, CheckpointIdentity, SparseBytes};

use super::{HostError, Kept, Options, Run, at, cost_table, size_of_checkpoint};

impl<E: Engine> Run<E> {
    /// Export an actor's state between activations (RFC 003 rule 21): from its instance, or,
    /// while it is not resident, the state as it was exported (RFC 009 rule 15) with the
    /// kernel's part as it is now.
    pub fn export_actor(&mut self, id: Identity) -> Result<ActorState, HostError> {
        let slot = self.actors.get(&id).ok_or(HostError::NoInstance(id))?;
        if slot.instance.is_none()
            && let Some(s) = &slot.swapped
        {
            let mut state = s.clone();
            state.kernel = self
                .kernel
                .actor(id)
                .cloned()
                .ok_or(HostError::NoInstance(id))?;
            return Ok(state);
        }
        self.export_instance(id)
    }

    /// The instance's memory, globals and tables, with the kernel's part of the actor's state.
    pub(super) fn export_instance(&mut self, id: Identity) -> Result<ActorState, HostError> {
        let slot = self.actors.get_mut(&id).ok_or(HostError::NoInstance(id))?;
        let inst = slot.instance.as_mut().ok_or(HostError::NoInstance(id))?;
        let size = self.engine.memory_size(inst);
        let mut memory = Vec::new();
        let mut err = None;
        self.engine
            .with_memory(inst, &mut |m| match m.read(0, size) {
                Ok(b) => memory = b,
                Err(e) => err = Some(e),
            })?;
        if let Some(e) = err {
            return Err(HostError::Engine(EngineError::Memory(e.to_string())));
        }
        let mut globals = Vec::new();
        for (name, kind) in &slot.admitted.layout.globals {
            let v = self
                .engine
                .global_get(inst, None, name)
                .ok_or_else(|| EngineError::Global(name.clone()))?;
            let bits = match v {
                GlobalValue::I32(x) => x as u32 as u64,
                GlobalValue::I64(x) => x as u64,
                GlobalValue::F32(b) => b as u64,
                GlobalValue::F64(b) => b,
            };
            globals.push((name.clone(), *kind, bits));
        }
        let mut tables = Vec::new();
        for (name, size) in &slot.admitted.layout.tables {
            let found = self
                .engine
                .table_size(inst, name)
                .ok_or_else(|| EngineError::Export(name.clone()))?;
            if u64::from(found) != *size {
                return Err(HostError::Malformed("table size"));
            }
            tables.push((name.clone(), *size));
        }
        let kernel = self
            .kernel
            .actor(id)
            .cloned()
            .ok_or(HostError::NoInstance(id))?;
        Ok(ActorState {
            memory: SparseBytes::from_dense(&memory),
            globals,
            tables,
            kernel,
        })
    }

    /// Restore an actor into a fresh instance (RFC 003 rule 22): grow the memory, write the bytes,
    /// set the globals, verify the tables, restore the kernel's state. Host-owned globals reset.
    /// In a swapping run the actor is left non-resident with the state to bring in at its next
    /// activation (RFC 009 rule 15).
    pub fn restore_actor(&mut self, id: Identity, state: &ActorState) -> Result<(), HostError> {
        let limit = self
            .actors
            .get(&id)
            .ok_or(HostError::NoInstance(id))?
            .limits
            .memory_bytes;
        // RFC 006 section 7: a checkpoint's memory is at most the actor's memory limit.
        if state.memory.len() as u64 > limit {
            return Err(HostError::Malformed("memory above the actor's limit"));
        }
        self.drop_instance(id);
        if self.pool.is_some() {
            if let Some(s) = self.actors.get_mut(&id) {
                s.swapped = Some(state.clone());
            }
        } else {
            self.instantiate(id)?;
            self.write_state(id, state)?;
        }
        self.kernel.restore_actor(id, state.kernel.clone());
        Ok(())
    }

    /// Write exported state into the actor's fresh instance: the memory, the globals, the
    /// tables verified; the Host-owned stack height reset.
    pub(super) fn write_state(
        &mut self,
        id: Identity,
        state: &ActorState,
    ) -> Result<(), HostError> {
        let slot = self.actors.get_mut(&id).expect("checked");
        let inst = slot.instance.as_mut().expect("instantiated");
        let size = u32::try_from(state.memory.len()).map_err(|_| HostError::Malformed("memory"))?;
        self.engine.memory_grow_to(inst, size)?;
        let mut err = None;
        self.engine.with_memory(inst, &mut |m| {
            err = m.write(0, &state.memory.to_dense()).err()
        })?;
        if let Some(e) = err {
            return Err(HostError::Engine(EngineError::Memory(e.to_string())));
        }
        for (name, kind, bits) in &state.globals {
            let v = match kind {
                florn_admission::GlobalKind::I32 => GlobalValue::I32(*bits as u32 as i32),
                florn_admission::GlobalKind::I64 => GlobalValue::I64(*bits as i64),
                florn_admission::GlobalKind::F32 => GlobalValue::F32(*bits as u32),
                florn_admission::GlobalKind::F64 => GlobalValue::F64(*bits),
            };
            self.engine.global_set(inst, None, name, v)?;
        }
        for (name, size) in &state.tables {
            let found = self
                .engine
                .table_size(inst, name)
                .ok_or_else(|| EngineError::Export(name.clone()))?;
            if u64::from(found) != *size {
                return Err(HostError::Malformed("table size"));
            }
        }
        self.engine
            .global_set(inst, None, STACK_EXPORT, GlobalValue::I32(0))?;
        Ok(())
    }

    /// Ask for a checkpoint (RFC 009 rule 2): taken now when the kernel stands at a boundary,
    /// else at the next one the run reaches. Returns whether it was taken now.
    pub fn request_checkpoint(&mut self) -> bool {
        self.checkpoint_requested = true;
        self.checkpoint_if_due()
    }

    /// A checkpoint when one is due at this boundary: requested, or the interval of rule 2
    /// since the Host's last. Returns whether one was taken.
    pub(super) fn checkpoint_if_due(&mut self) -> bool {
        if self.detail {
            return false;
        }
        let interval = self.options.checkpoint_interval;
        let due_by_interval = interval > 0
            && self
                .kernel
                .handled()
                .saturating_sub(self.last_checkpoint_handled)
                >= interval;
        if !(self.checkpoint_requested || due_by_interval) || !self.kernel.at_boundary() {
            return false;
        }
        match self.checkpoint() {
            Ok(c) => {
                self.checkpoint_requested = false;
                self.last_checkpoint_handled = self.kernel.handled();
                self.keep(c);
                // The records before this checkpoint leave (DATA-008): a window of them is
                // produced by replay from it or an earlier one.
                if self.options.records_since_checkpoint {
                    self.kernel.trace.set_kept(Vec::new());
                }
                true
            }
            Err(e) => {
                // An actor whose state cannot be read is a Host fault in the log; the run
                // goes on, and the request stands for the next boundary.
                let n = Notification::new(
                    "host.instance-failed",
                    Severity::Error,
                    Scope::run(),
                    Ends::Nothing,
                )
                .with("actor", Value::Identity(0))
                .with("text", Value::Text(format!("checkpoint: {e}")))
                .at(at(self.kernel.now()));
                self.notify(n);
                false
            }
        }
    }

    /// Keep a checkpoint within the budget (RFC 009 rule 3): when the new one does not fit, every
    /// second checkpoint among those older than the newest eight is dropped, oldest first, until
    /// it does; the newest eight are never dropped by the budget.
    fn keep(&mut self, checkpoint: Checkpoint) {
        let bytes = size_of_checkpoint(&checkpoint);
        let k = Kept {
            instant: checkpoint.kernel.now,
            records: checkpoint.kernel.trace.records,
            handled: checkpoint.kernel.handled,
            bytes,
            checkpoint,
        };
        self.kept.push(k);
        let budget = self.options.checkpoint_memory;
        let total = |kept: &[Kept]| kept.iter().map(|k| k.bytes).sum::<u64>();
        let mut dropped = 0u64;
        while total(&self.kept) > budget {
            let old = self
                .kept
                .len()
                .saturating_sub(Options::CHECKPOINTS_ALWAYS_KEPT);
            if old <= 1 {
                break;
            }
            // The even positions of the old part but the start, oldest first: half of it in one
            // pass. The start (rule 2) is kept whatever the budget: moving back always has it.
            let mut i = 0;
            let mut keep = Vec::with_capacity(self.kept.len());
            for (n, k) in self.kept.drain(..).enumerate() {
                if n > 0 && n < old && n % 2 == 0 {
                    dropped += 1;
                    i += 1;
                } else {
                    keep.push(k);
                }
            }
            self.kept = keep;
            if i == 0 {
                break;
            }
        }
        let n = Notification::new(
            "checkpoint.taken",
            Severity::Info,
            Scope::run(),
            Ends::Nothing,
        )
        .with("instant", Value::Tick(self.kernel.now().tick))
        .with("bytes", Value::Bytes(bytes))
        .with("kept", Value::Count(self.kept.len() as u64))
        .at(at(self.kernel.now()));
        self.notify(n);
        if dropped > 0 {
            let n = Notification::new(
                "checkpoint.budget",
                Severity::Info,
                Scope::run(),
                Ends::Nothing,
            )
            .with("kept", Value::Count(self.kept.len() as u64))
            .with("dropped", Value::Count(dropped))
            .with("bytes", Value::Bytes(total(&self.kept)))
            .at(at(self.kernel.now()));
            self.notify(n);
        }
    }

    /// The kept checkpoints, oldest first; the first is the start (rule 2).
    pub fn kept(&self) -> &[Kept] {
        &self.kept
    }

    /// The start as a kept checkpoint (rule 2): the instances before `init`, so that a restore of
    /// it runs `init` again and the run begins as a fresh one, records and all; what moving back
    /// to the beginning restores. Never dropped by the budget; not said in the log.
    pub(super) fn keep_start(&mut self) {
        if let Ok(c) = self.checkpoint() {
            let bytes = size_of_checkpoint(&c);
            self.kept.insert(
                0,
                Kept {
                    instant: c.kernel.now,
                    records: c.kernel.trace.records,
                    handled: c.kernel.handled,
                    bytes,
                    checkpoint: c,
                },
            );
            self.last_checkpoint_handled = 0;
        }
    }

    /// Move to an earlier tick (RFC 001 rule 53; RFC 009 rule 7): restore the nearest kept
    /// checkpoint at or before it, then execute forward to it. The interventions after it leave
    /// the inputs and are said (`run.rewound`); a replay re-applies them at their instants.
    pub fn go_back(&mut self, to: u64) -> Result<(), HostError> {
        let index = self
            .kept
            .iter()
            .rposition(|k| k.instant.tick <= to)
            .ok_or(HostError::Malformed("no checkpoint at or before the tick"))?;
        self.restore_kept(index)?;
        if self.kernel.now().tick < to {
            self.advance(Advance::ToTick(to));
        }
        Ok(())
    }

    /// The records of a window (RFC 001 rule 55; RFC 009 rule 9): from the kept records when
    /// they hold it, else by re-execution through this run's own instances: the present is
    /// checkpointed, the nearest kept checkpoint at or before the window is restored with
    /// recording on, the run executes to the window's end, and the present is restored. The
    /// run's instant, inputs, trace and kept set are as before; nothing of the way is logged,
    /// kept or displayed. Needs a boundary (RFC 001 rule 51); mid-instant, the kept records
    /// alone answer.
    pub fn trace_window(&mut self, from: u64, count: u32) -> Vec<florn_kernel::Record> {
        let end = from.saturating_add(u64::from(count));
        let from_kept =
            |records: &[florn_kernel::Record], first: u64| -> Vec<florn_kernel::Record> {
                let lo = from.saturating_sub(first) as usize;
                let hi = end.saturating_sub(first) as usize;
                records
                    .get(lo.min(records.len())..hi.min(records.len()))
                    .map(|r| r.to_vec())
                    .unwrap_or_default()
            };
        // The kept records cover the window: the Host keeps them from the start, or from the
        // last checkpoint (the browser), so the first kept record's index is known.
        let total = self.kernel.trace.records();
        if let Some(records) = self.kernel.trace.kept_records() {
            let first = total.saturating_sub(records.len() as u64);
            if from >= first {
                return from_kept(records, first);
            }
        }
        if !self.kernel.at_boundary() {
            return self
                .kernel
                .trace
                .kept_records()
                .map(|r| from_kept(r, total.saturating_sub(r.len() as u64)))
                .unwrap_or_default();
        }
        let Some(index) = self.kept.iter().rposition(|k| k.records <= from) else {
            return Vec::new();
        };
        let Ok(present) = self.checkpoint() else {
            return Vec::new();
        };
        let own_records = self.kernel.trace.kept_records().map(|r| r.to_vec());
        // The present's own pending replay list, as it was: the detail run's leftover is not it.
        let own_replay = self.kernel.replay_pending().to_vec();
        let ended = self.ended;
        let reached = self.reached;
        self.detail = true;
        let start = self.kept[index].checkpoint.clone();
        let mut out = Vec::new();
        if self.restore_quietly(&start).is_ok() {
            while self.ended.is_none() && self.kernel.trace.records() < end {
                if self.advance(Advance::OneInstant) == 0 && !self.kernel.has_next() {
                    break;
                }
            }
            let first = self.kept[index].records;
            if let Some(records) = self.kernel.trace.kept_records() {
                out = from_kept(records, first);
            }
        }
        let _ = self.restore_quietly(&present);
        self.kernel.set_replay(own_replay);
        // The Host's own kept records, as they were; none when it kept none.
        match own_records {
            Some(records) => self.kernel.trace.set_kept(records),
            None => self.kernel.trace.stop_recording(),
        }
        self.ended = ended;
        self.reached = reached;
        self.detail = false;
        out
    }

    /// A restore for detail by replay (rule 9): the undone interventions are re-applied at their
    /// instants whatever the run, nothing is said, and the kept set is untouched.
    fn restore_quietly(&mut self, checkpoint: &Checkpoint) -> Result<(), HostError> {
        self.restore_with(checkpoint, true)
    }

    /// Restore a kept checkpoint in this run (RFC 009 rules 5 and 7).
    pub fn restore_kept(&mut self, index: usize) -> Result<(), HostError> {
        if index >= self.kept.len() {
            return Err(HostError::Malformed("no such checkpoint"));
        }
        let k = self.kept.remove(index);
        let result = self.restore_in_place(&k.checkpoint);
        // The restored one is kept again at its place; those after it belong to a future that is
        // executed again (rule 7).
        self.kept.truncate(index);
        self.kept.push(k);
        result
    }

    /// Continue this run from a checkpoint (RFC 009 rule 7): the identity is checked first
    /// (RFC 003 rule 22), then the kernel, the domains and every actor are restored, the trace
    /// and everything after the checkpoint's instant discarded, and the interventions applied
    /// after it leave the inputs (rule 8; a replay re-applies them at their instants).
    pub fn restore_in_place(&mut self, checkpoint: &Checkpoint) -> Result<(), HostError> {
        self.restore_with(checkpoint, false)
    }

    fn restore_with(&mut self, checkpoint: &Checkpoint, detail: bool) -> Result<(), HostError> {
        self.check_identity(&checkpoint.identity)?;
        let scenario_actors = self.actors.len() + self.builtins.len();
        if checkpoint.actors.len() > scenario_actors
            || checkpoint
                .actors
                .keys()
                .any(|id| !self.actors.contains_key(id))
        {
            return Err(self.mismatch(
                "actors",
                checkpoint.actors.len().to_string(),
                scenario_actors.to_string(),
            ));
        }
        let applied_before = self.kernel.applied_interventions().to_vec();
        let undone: Vec<AppliedIntervention> = applied_before
            .get(checkpoint.kernel.applied_interventions.len()..)
            .map(|s| s.to_vec())
            .unwrap_or_default();
        let replay_pending = self.kernel.replay_pending().to_vec();
        let watched = self.kernel.trace.watched().to_vec();
        // The kept records before the checkpoint's instant stay; the kept list may begin after
        // the start (DATA-008: the records since the last checkpoint), so its offset counts.
        let total_before = self.kernel.trace.records();
        let kept_records = self
            .kernel
            .trace
            .kept_records()
            .map(|r| {
                let first = total_before.saturating_sub(r.len() as u64);
                let keep = checkpoint.kernel.trace.records.saturating_sub(first) as usize;
                r[..keep.min(r.len())].to_vec()
            })
            .unwrap_or_default();
        let mut config = self.kernel.config().clone();
        // Detail by replay records from the restore on, the start's `init` included.
        config.keep_records |= detail;
        self.kernel = Kernel::restore(config, checkpoint.kernel.clone());
        self.kernel.trace.set_kept(kept_records);
        self.kernel.trace.watch(watched);
        if !self.world.restore(&checkpoint.domains) {
            return Err(HostError::Malformed("domain state"));
        }
        self.world.restore_origin_seqs(&checkpoint.origins);
        self.world.dead.clear();
        for (id, a) in &checkpoint.kernel.actors {
            if !a.alive {
                self.world.dead.insert(*id);
            }
        }
        for (id, state) in &checkpoint.actors {
            self.restore_actor(*id, state)?;
        }
        if self.replay_against.is_some() || detail {
            // A replay, or detail by replay: what was undone is still to come at its instant
            // (RFC 001 rule 36).
            let mut again = undone;
            again.extend(replay_pending);
            self.kernel.set_replay(again);
        } else {
            // An intervention the checkpoint holds as scheduled for an instant after its own
            // (requested before, applied after) is undone with the rest (rule 8).
            let mut undone = undone;
            undone.extend(
                self.kernel
                    .unschedule_interventions_after(checkpoint.kernel.now),
            );
            // Said at every move back (rule 8: how many were undone, none included), so that a
            // front end knows the run went back rather than began again.
            let n = Notification::new("run.rewound", Severity::Info, Scope::run(), Ends::Nothing)
                .with("to", Value::Tick(checkpoint.kernel.now.tick))
                .with("undone", Value::Count(undone.len() as u64))
                .at(at(self.kernel.now()));
            self.notify(n);
        }
        self.ended = None;
        self.reached = checkpoint.kernel.now.tick;
        // The start (rule 2) is the run before `init`: initialised again, as a fresh run is.
        if checkpoint.kernel.handled == 0 && checkpoint.kernel.trace.records == 0 {
            self.init_all();
        }
        self.telemetry.clear();
        if let Some(a) = &mut self.activity {
            a.clear();
        }
        self.world.take_display();
        self.last_checkpoint_handled = checkpoint.kernel.handled;
        self.checkpoint_requested = false;
        Ok(())
    }

    /// The identity check of RFC 003 rule 22 (DATA-005), field by field.
    fn check_identity(&mut self, id: &CheckpointIdentity) -> Result<(), HostError> {
        let hex = florn_model::canonical::hex;
        if id.scenario_sha256 != self.inputs.scenario_sha256 {
            return Err(self.mismatch(
                "scenario-hash",
                hex(&id.scenario_sha256),
                hex(&self.inputs.scenario_sha256),
            ));
        }
        for (name, h) in &id.modules {
            let found = self.inputs.modules.iter().find(|m| &m.name == name);
            match found {
                Some(m) if m.source_sha256 == *h => {}
                Some(m) => {
                    return Err(self.mismatch("module-hash", hex(h), hex(&m.source_sha256)));
                }
                None => return Err(self.mismatch("module-hash", hex(h), String::new())),
            }
        }
        if id.admission_version != ADMISSION_VERSION {
            return Err(self.mismatch(
                "admission-version",
                id.admission_version.to_string(),
                ADMISSION_VERSION.to_string(),
            ));
        }
        if id.cost_model_version != self.inputs.cost_model_version {
            return Err(self.mismatch(
                "cost-model",
                id.cost_model_version.to_string(),
                self.inputs.cost_model_version.to_string(),
            ));
        }
        Ok(())
    }

    /// A checkpoint at the boundary (RFC 001 rule 51): the kernel and every actor.
    pub fn checkpoint(&mut self) -> Result<Checkpoint, HostError> {
        let ids: Vec<Identity> = self.actors.keys().copied().collect();
        let mut actors = BTreeMap::new();
        for id in ids {
            // A non-resident actor's state as it is (RFC 009 rule 15): nothing is restored to be
            // exported again.
            let s = &self.actors[&id];
            if s.instance.is_some() || s.swapped.is_some() {
                actors.insert(id, self.export_actor(id)?);
            }
        }
        Ok(Checkpoint {
            identity: CheckpointIdentity {
                scenario_sha256: self.inputs.scenario_sha256,
                modules: self
                    .inputs
                    .modules
                    .iter()
                    .map(|m| (m.name.clone(), m.source_sha256))
                    .collect(),
                admission_version: ADMISSION_VERSION,
                cost_model_version: self.inputs.cost_model_version,
                root_seed: self.inputs.root_seed,
            },
            kernel: self.kernel.snapshot(),
            actors,
            origins: self.world.origin_seqs(),
            domains: self.world.export(),
        })
    }

    /// A run continuing from a checkpoint (RFC 001 rule 52; RFC 003 rule 22): the inputs are
    /// checked first (DATA-005), then every actor is restored and the kernel continues.
    // The Host's parts and the run's inputs, each named: a struct would only move the list.
    #[allow(clippy::too_many_arguments)]
    pub fn from_checkpoint(
        engine: E,
        admission: Admission,
        platform: &mut dyn Platform,
        domains: &Domains,
        scenario: Manifest,
        modules: &BTreeMap<String, Vec<u8>>,
        checkpoint: &Checkpoint,
        options: Options,
    ) -> Result<Self, HostError> {
        // The root seed and the cost-model version are inputs the checkpoint carries (RFC 001
        // rule 40, items 3 and 5): the run admits under them, or names the version it lacks.
        let seed = checkpoint.identity.root_seed;
        let wanted = checkpoint.identity.cost_model_version;
        let mut options = options;
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
        run.check_identity(&checkpoint.identity)?;
        if checkpoint
            .actors
            .keys()
            .any(|id| !run.actors.contains_key(id))
        {
            return Err(run.mismatch(
                "actors",
                checkpoint.actors.len().to_string(),
                run.actors.len().to_string(),
            ));
        }
        let config = run.kernel.config().clone();
        run.kernel = Kernel::restore(config, checkpoint.kernel.clone());
        if !run.world.restore(&checkpoint.domains) {
            return Err(HostError::Malformed("domain state"));
        }
        run.world.restore_origin_seqs(&checkpoint.origins);
        for (id, state) in &checkpoint.actors {
            run.restore_actor(*id, state)?;
        }
        run.say_swapping(platform)?;
        // The start (RFC 009 rule 2) is the run before `init`: initialised, as a fresh run is.
        if checkpoint.kernel.handled == 0 && checkpoint.kernel.trace.records == 0 {
            run.init_all();
        }
        // The built-in actors' kernel state is in the kernel's snapshot; the dead ones too.
        for (id, a) in &checkpoint.kernel.actors {
            if !a.alive {
                run.world.dead.insert(*id);
            }
        }
        Ok(run)
    }

    pub(super) fn mismatch(
        &mut self,
        what: &'static str,
        expected: String,
        found: String,
    ) -> HostError {
        let n = Notification::new(
            "checkpoint.mismatch",
            Severity::Error,
            Scope::run(),
            Ends::Command,
        )
        .with("what", Value::Enum(what.into()))
        .with("expected", Value::Text(expected.clone()))
        .with("found", Value::Text(found.clone()));
        self.notify(n);
        HostError::CheckpointMismatch {
            what,
            expected,
            found,
        }
    }

    /// A branch (RFC 001 rule 54; RFC 009 rules 10 to 12): this run ends at the current instant
    /// `T`, at a boundary, and becomes a new run from the checkpoint there, with the inputs so
    /// far as its own and an identity naming the parent. The parent's record is returned for
    /// keeping and export (rule 11). The branch begins at `T`: the checkpoint there is its start,
    /// kept whatever the budget, and the parent's earlier ones leave with the parent; the point
    /// `T` is stored in the branch's own chain, so that its replay compares there too.
    pub fn branch(&mut self, platform: &mut dyn Platform) -> Result<Contents, HostError> {
        if self.detail || !self.kernel.at_boundary() {
            return Err(HostError::MidInstant);
        }
        self.kernel.trace.mark();
        let c = self.checkpoint()?;
        let start = Kept {
            instant: c.kernel.now,
            records: c.kernel.trace.records,
            handled: c.kernel.handled,
            bytes: size_of_checkpoint(&c),
            checkpoint: c,
        };
        // The parent ends here (rule 11); its record is the Host's to offer.
        self.end(EndReason::User);
        let parent_record = Contents {
            run: Some(self.run_toml()),
            trace: self.records().map(|r| r.to_vec()),
            payloads: None,
            log: self.log.clone(),
            readme: None,
        };
        let parent = crate::identity::Parent {
            chain: self.chain(),
            records: self.kernel.trace.records(),
            instant: self.kernel.now(),
        };
        let parent_id = std::mem::take(&mut self.run_id);
        // The branch: its own identity and log; the parent's inputs after `T` are nobody's
        // (rule 10), so a replay's remaining interventions and watched points leave.
        self.kernel.set_replay(Vec::new());
        self.kernel.trace.watch(Vec::new());
        self.replay_against = None;
        self.ended = None;
        self.created_ms = platform.wall_clock_ms();
        self.wall_ms = self.created_ms;
        self.run_id = format!(
            "{:x}-{:x}-{:x}",
            self.created_ms, self.inputs.root_seed, parent.records
        );
        self.kept = vec![start];
        self.last_checkpoint_handled = self.kernel.handled();
        self.checkpoint_requested = false;
        self.log = DiagnosticLog::default();
        self.notify(
            Notification::new("host.capacity", Severity::Info, Scope::run(), Ends::Nothing)
                .with("instances", Value::Count(self.capacity.instances))
                .with("memory", Value::Bytes(self.capacity.memory_bytes)),
        );
        let n = Notification::new("run.branched", Severity::Info, Scope::run(), Ends::Nothing)
            .with("instant", Value::Tick(parent.instant.tick))
            .with("parent", Value::Text(parent_id))
            .at(at(parent.instant));
        self.notify(n);
        self.parent = Some(parent);
        Ok(parent_record)
    }

    /// The kept trace records, when `keep_records` is on: from the start, or from the last
    /// checkpoint when the Host keeps the records since it only.
    pub fn records(&self) -> Option<&[florn_kernel::Record]> {
        self.kernel.trace.kept_records()
    }

    /// Every record of the run so far, for an export (RFC 005 rule 29): the kept ones when they
    /// are the whole trace, else assembled window by window, by replay where needed (rule 9).
    /// `None` when the run keeps no records.
    pub fn full_trace(&mut self) -> Option<Vec<florn_kernel::Record>> {
        let total = self.kernel.trace.records();
        let kept = self.kernel.trace.kept_records()?;
        if kept.len() as u64 == total {
            return Some(kept.to_vec());
        }
        let mut out = Vec::with_capacity(total as usize);
        let mut from = 0u64;
        while from < total {
            let count = (total - from).min(10_000) as u32;
            let window = self.trace_window(from, count);
            if window.is_empty() {
                break;
            }
            from += window.len() as u64;
            out.extend(window);
        }
        Some(out)
    }
}
