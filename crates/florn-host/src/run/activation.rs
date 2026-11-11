//! One activation and its outcome (RFC 003 rules 11 to 13 and 17): the import handler, the budget in
//! place before lowering, classification by globals, the fault record, the built-in actors.

use std::collections::{BTreeMap, VecDeque};

use florn_abi::core_world;
use florn_abi::{
    AbiError, Flat, Memory, Ty, Val, lift_flat_params, lower_flat_params, lower_result, store,
};
use florn_admission::{
    BUDGET_IMPORT_MODULE, BUDGET_IMPORT_NAME, ClassLimits, FaultClass, STACK_EXPORT, classify,
};
use florn_control::{Ends, Notification, Scope, Severity, Value};
use florn_engine::{Engine, GlobalValue, ImportHandler, InstanceId, Refusal, TrapKind};
use florn_kernel::{
    Activation, Digest, Event, FaultKind, Identity, Instant, Kernel, Sink, SinkRefusal,
};

use crate::domain::{BuiltinEvent, Composite, WorldSigs};
use crate::identity::EndReason;

use super::{FaultPolicy, Run, Slot, Telemetry, at};

/// What an activation ended with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    /// Committed, with the channel 0 texts of the activation for the diagnostic log (RFC 002 rule 33).
    Committed { texts: Vec<String> },
    Faulted {
        kind: FaultKind,
        parameter: u32,
        metered: u64,
    },
    /// A Host fault (RFC 001 rule 65): the run ends, not reproducible.
    HostFault { reason: &'static str, text: String },
}

/// The Host's side of the imports for one activation (RFC 003 rule 17; RFC 002 §5): the core's
/// five, and a domain's answered from the activation's starting state (RFC 002 rule 47).
pub(super) struct Handler<'a> {
    pub(super) kernel: &'a mut Kernel,
    pub(super) sink: &'a mut Sink,
    pub(super) world: &'a mut Composite,
    pub(super) actor: Identity,
    pub(super) sigs: &'a WorldSigs,
    /// A guest of a world with no queue presence (RFC 012 rule 19: a device): its `send` and
    /// timers are contract violations.
    pub(super) queueless: bool,
    /// A lifting failure: the contract fault of RFC 002 rule 8, kept for classification.
    pub(super) contract: Option<AbiError>,
}

impl ImportHandler for Handler<'_> {
    fn call(
        &mut self,
        _: InstanceId,
        function: &str,
        args: &[Flat],
        memory: &mut dyn Memory,
    ) -> Result<Vec<Flat>, Refusal> {
        let Some((_, module, tys, result)) = self.sigs.imports.iter().find(|(n, ..)| n == function)
        else {
            return Err(Refusal);
        };
        // A result beyond one flat value comes back through a return pointer the guest passes
        // as the last argument (RFC 002 §2).
        let retptr = result
            .as_ref()
            .is_some_and(|t| t.flat_count() > florn_abi::MAX_FLAT_RESULTS);
        let (flats, ret) = match (retptr, args) {
            (true, [rest @ .., Flat::I32(p)]) => (rest, Some(*p as u32)),
            (true, _) => return Err(Refusal),
            (false, _) => (args, None),
        };
        let vals = match lift_flat_params(memory, tys, flats) {
            Ok(v) => v,
            Err(e) => {
                self.contract = Some(e);
                return Err(Refusal);
            }
        };
        if module != core_world::HOST_MODULE {
            let Some(d) = self.world.of_module(module) else {
                return Err(Refusal);
            };
            let answer = d.answer(self.actor, function, &vals);
            return match (result, answer, ret) {
                (None, None, None) => Ok(Vec::new()),
                (Some(t), Some(v), Some(ptr)) => {
                    store(memory, t, &v, ptr).map(|()| Vec::new()).map_err(|e| {
                        self.contract = Some(e);
                        Refusal
                    })
                }
                (Some(t), Some(v), None) => lower_result(memory, Some(t), Some(&v))
                    .map(|f| f.into_iter().collect())
                    .map_err(|e| {
                        self.contract = Some(e);
                        Refusal
                    }),
                _ => Err(Refusal),
            };
        }
        if self.queueless && matches!(function, "send" | "set-timer" | "cancel-timer") {
            self.sink.violate(
                "RFC 012 rule 19",
                format!("a guest of this world has no queue presence: {function}"),
            );
            return Err(Refusal);
        }
        let r = match (function, vals.as_slice()) {
            ("send", [Val::U32(port), payload]) => self
                .sink
                .send(*port, payload.as_bytes().unwrap_or_default())
                .map(|()| Vec::new()),
            ("set-timer", [Val::U32(timer), Val::U64(delay)]) => {
                self.sink.set_timer(*timer, *delay).map(|()| Vec::new())
            }
            ("cancel-timer", [Val::U32(timer)]) => {
                self.sink.cancel_timer(*timer).map(|()| Vec::new())
            }
            ("random", []) => Ok(vec![Flat::I64(self.kernel.draw(self.actor) as i64)]),
            ("report", [Val::U32(channel), payload]) => self
                .sink
                .report(*channel, payload.as_bytes().unwrap_or_default())
                .map(|()| Vec::new()),
            _ => return Err(Refusal),
        };
        r.map_err(|_| Refusal)
    }
}

/// The parameter of a fault record (RFC 005 rule 20): for `quota`, the quota's index in RFC 003
/// rule 16's table; for `contract`, the number of the rule violated: a rule of RFC 002 keeps its
/// number, a rule of another RFC is that RFC's number times 1,000 plus the rule's (RFC 012 rule
/// 20: a device's send is 12019); otherwise zero.
pub(super) fn fault_parameter(r: &SinkRefusal) -> u32 {
    match r {
        SinkRefusal::Quota { quota, .. } => match *quota {
            "effects-per-activation" => 4,
            "payload" => 5,
            "pending-timers" => 6,
            "telemetry-per-activation" => 7,
            _ => 0,
        },
        SinkRefusal::Contract { rule, .. } => contract_parameter(rule),
    }
}

/// `RFC <n> rule <m>` as the fault parameter: `m` for RFC 2, `n * 1000 + m` for any other. Text
/// of another form gives its last word as a number, or zero (as it always did).
pub(super) fn contract_parameter(rule: &str) -> u32 {
    let words: Vec<&str> = rule.split(' ').collect();
    if let ["RFC", n, "rule", m] = words.as_slice()
        && let (Ok(n), Ok(m)) = (n.parse::<u32>(), m.parse::<u32>())
    {
        return if n == 2 { m } else { n * 1000 + m };
    }
    words.last().and_then(|w| w.parse().ok()).unwrap_or(0)
}

pub(crate) fn fault_code(kind: FaultKind) -> &'static str {
    match kind {
        FaultKind::Trap => "fault.trap",
        FaultKind::Budget => "fault.budget",
        FaultKind::Stack => "fault.stack",
        FaultKind::Memory => "fault.memory",
        FaultKind::Quota => "fault.quota",
        FaultKind::Contract => "fault.contract",
    }
}

/// The budget and the stack height after a call (RFC 003 rule 13).
pub(super) fn read_globals<E: Engine>(engine: &mut E, inst: &mut E::Instance) -> (i64, i32) {
    let remaining = match engine.global_get(inst, Some(BUDGET_IMPORT_MODULE), BUDGET_IMPORT_NAME) {
        Some(GlobalValue::I64(r)) => r,
        _ => 0,
    };
    let height = match engine.global_get(inst, None, STACK_EXPORT) {
        Some(GlobalValue::I32(h)) => h,
        _ => 0,
    };
    (remaining, height)
}

/// The work an activation used: the budget it was given less what remains (all of it when the
/// counter went negative).
pub(super) fn metered_of(budget: u64, remaining: i64) -> u64 {
    if remaining < 0 {
        budget
    } else {
        budget.saturating_sub(remaining as u64)
    }
}

/// RFC 003 rule 11: the budget in place before any guest code, the lowering's realloc included.
pub(super) fn set_budget<E: Engine>(
    engine: &mut E,
    inst: &mut E::Instance,
    budget: u64,
) -> Result<(), florn_engine::EngineError> {
    engine.global_set(
        inst,
        Some(BUDGET_IMPORT_MODULE),
        BUDGET_IMPORT_NAME,
        GlobalValue::I64(budget as i64),
    )
}

/// The engine could not do what the activation needs (set the budget, reach the memory): a Host
/// fault, not the guest's.
pub(super) fn engine_failed(
    kernel: &mut Kernel,
    act: Activation,
    e: &florn_engine::EngineError,
) -> Outcome {
    kernel.fault(act, FaultKind::Trap, 0, 0);
    Outcome::HostFault {
        reason: "engine-stack",
        text: e.to_string(),
    }
}

/// Lowering the arguments failed: the guest's allocator ran under the budget, so classify by the
/// globals (rule 13). The Fault record is written.
pub(super) fn lowering_outcome(
    kernel: &mut Kernel,
    act: Activation,
    e: &AbiError,
    globals: (i64, i32),
    limits: &ClassLimits,
) -> Outcome {
    let (remaining, height) = globals;
    let metered = metered_of(limits.budget, remaining);
    let kind = match (classify(remaining, height, limits.stack), e) {
        (FaultClass::Budget, _) => FaultKind::Budget,
        (FaultClass::Stack, _) => FaultKind::Stack,
        (FaultClass::Trap, AbiError::Realloc { .. }) => FaultKind::Memory,
        (FaultClass::Trap, _) => FaultKind::Contract,
    };
    kernel.fault(act, kind, 0, metered);
    Outcome::Faulted {
        kind,
        parameter: 0,
        metered,
    }
}

/// Lifting failed inside an import: the contract fault of RFC 002 rule 8. The Fault is written.
pub(super) fn contract_outcome(kernel: &mut Kernel, act: Activation, metered: u64) -> Outcome {
    kernel.fault(act, FaultKind::Contract, 8, metered);
    Outcome::Faulted {
        kind: FaultKind::Contract,
        parameter: 8,
        metered,
    }
}

/// A call into the guest ended in a trap: a refusal by the Host's own import handler is the
/// sink's (a quota, a contract), a guest trap is classified by the globals (rule 13), the engine's
/// own limits are Host faults (RFC 001 rule 65). The Fault is written.
pub(super) fn trap_outcome(
    kernel: &mut Kernel,
    act: Activation,
    refusal: Option<&SinkRefusal>,
    trap: florn_engine::Trap,
    globals: (i64, i32),
    limits: &ClassLimits,
) -> Outcome {
    let (remaining, height) = globals;
    let metered = metered_of(limits.budget, remaining);
    match trap.kind {
        TrapKind::HostRefused => {
            let (kind, parameter) = match refusal {
                Some(r) => (FaultKind::from_refusal(r), fault_parameter(r)),
                None => (FaultKind::Contract, 0),
            };
            kernel.fault(act, kind, parameter, metered);
            Outcome::Faulted {
                kind,
                parameter,
                metered,
            }
        }
        TrapKind::Guest => {
            let kind = match classify(remaining, height, limits.stack) {
                FaultClass::Budget => FaultKind::Budget,
                FaultClass::Stack => FaultKind::Stack,
                FaultClass::Trap => FaultKind::Trap,
            };
            kernel.fault(act, kind, 0, metered);
            Outcome::Faulted {
                kind,
                parameter: 0,
                metered,
            }
        }
        TrapKind::EngineStack | TrapKind::Watchdog | TrapKind::Other => {
            kernel.fault(act, FaultKind::Trap, 0, metered);
            Outcome::HostFault {
                reason: match trap.kind {
                    TrapKind::EngineStack => "engine-stack",
                    _ => "watchdog",
                },
                text: trap.message,
            }
        }
    }
}

/// Where a committed activation's reports and payloads go: the run's telemetry queue and payload
/// store.
pub(super) struct Records<'a> {
    pub keep_payloads: bool,
    pub payloads: &'a mut BTreeMap<Digest, Vec<u8>>,
    pub telemetry: &'a mut VecDeque<Telemetry>,
    pub telemetry_dropped: &'a mut u64,
    pub telemetry_capacity: usize,
    pub instant: Instant,
}

/// The guest returned normally and its imports were lawful: commit the activation's effects with
/// `world` (RFC 001 rule 23) and, if the kernel took them, keep what the display path and the
/// payload store need. A refusal at commit is a Fault (the kernel wrote it).
pub(super) fn committed_outcome(
    kernel: &mut Kernel,
    world: &mut dyn florn_kernel::World,
    sink: &mut Sink,
    actor: Identity,
    act: Activation,
    metered: u64,
    out: Records<'_>,
) -> Outcome {
    // Collect what the display path and the payload store need before the kernel takes the
    // effects; they are kept only if the commit succeeds (RFC 002 rule 34).
    let mut reports = Vec::new();
    let mut kept = Vec::new();
    for e in sink.effects() {
        match e {
            florn_kernel::Effect::Report { channel, payload } => {
                reports.push((*channel, payload.clone()));
                if out.keep_payloads {
                    kept.push(payload.clone());
                }
            }
            florn_kernel::Effect::Send { payload, .. } if out.keep_payloads => {
                kept.push(payload.clone());
            }
            _ => {}
        }
    }
    match kernel.commit(act, sink, metered, world) {
        Ok(()) => {
            let texts = reports
                .iter()
                .filter(|(channel, _)| *channel == 0)
                .map(|(_, payload)| String::from_utf8_lossy(payload).into_owned())
                .collect();
            for (channel, payload) in reports {
                if out.telemetry.len() >= out.telemetry_capacity {
                    out.telemetry.pop_front();
                    *out.telemetry_dropped += 1;
                }
                out.telemetry.push_back(Telemetry {
                    at: out.instant,
                    actor,
                    channel,
                    payload,
                });
            }
            for p in kept {
                out.payloads
                    .entry(*blake3::hash(&p).as_bytes())
                    .or_insert(p);
            }
            Outcome::Committed { texts }
        }
        Err(refusal) => {
            let (kind, parameter) = match &refusal {
                florn_kernel::CommitRefusal::DelayBeyondLargestTick { timer, .. } => {
                    (FaultKind::Contract, *timer)
                }
                florn_kernel::CommitRefusal::PendingTimers { .. } => (FaultKind::Quota, 6),
                florn_kernel::CommitRefusal::Refused(r) => {
                    (FaultKind::from_refusal(r), fault_parameter(r))
                }
            };
            Outcome::Faulted {
                kind,
                parameter,
                metered,
            }
        }
    }
}

/// The activation itself, over disjoint borrows of the run.
#[allow(clippy::too_many_arguments)]
fn activate_slot<E: Engine>(
    engine: &mut E,
    kernel: &mut Kernel,
    world: &mut Composite,
    slot: &mut Slot<E>,
    act: Activation,
    which: usize,
    vals: &[Val],
    keep_payloads: bool,
    payloads: &mut BTreeMap<Digest, Vec<u8>>,
    telemetry: &mut VecDeque<Telemetry>,
    telemetry_dropped: &mut u64,
    telemetry_capacity: usize,
    instant: Instant,
) -> Outcome {
    let actor = act.actor;
    let budget = slot.limits.budget;
    let sigs = slot.world.clone();
    let tys: &[Ty] = &sigs.exports[which].1;
    let inst = slot.instance.as_mut().expect("checked by the caller");
    slot.sink.reset();
    if let Err(e) = set_budget(engine, inst, budget) {
        return engine_failed(kernel, act, &e);
    }
    let mut lowered: Result<Vec<Flat>, AbiError> = Ok(Vec::new());
    if let Err(e) = engine.with_memory(inst, &mut |m| lowered = lower_flat_params(m, tys, vals)) {
        return engine_failed(kernel, act, &e);
    }
    let flats = match lowered {
        Ok(f) => f,
        Err(e) => {
            let globals = read_globals(engine, inst);
            return lowering_outcome(kernel, act, &e, globals, &slot.limits);
        }
    };
    let func = slot.funcs[which].clone();
    let result = {
        let mut handler = Handler {
            kernel,
            sink: &mut slot.sink,
            world,
            actor,
            sigs: &sigs,
            queueless: false,
            contract: None,
        };
        let r = engine.call(inst, &func, &flats, &mut handler);
        (r, handler.contract)
    };
    let globals = read_globals(engine, inst);
    let metered = metered_of(budget, globals.0);
    match result {
        (Ok(_), None) => committed_outcome(
            kernel,
            world,
            &mut slot.sink,
            actor,
            act,
            metered,
            Records {
                keep_payloads,
                payloads,
                telemetry,
                telemetry_dropped,
                telemetry_capacity,
                instant,
            },
        ),
        // Lifting failed inside an import: the contract fault of RFC 002 rule 8.
        (Ok(_), Some(_)) | (Err(_), Some(_)) => contract_outcome(kernel, act, metered),
        (Err(trap), None) => trap_outcome(
            kernel,
            act,
            slot.sink.refusal(),
            trap,
            globals,
            &slot.limits,
        ),
    }
}

impl<E: Engine> Run<E> {
    /// A built-in actor's activation (RFC 008 rule 19): the domain's code over a sink, committed
    /// like a guest's with `metered = 0`.
    pub(super) fn activate_builtin(&mut self, id: Identity, event: Event, be: &BuiltinEvent<'_>) {
        if !self.alive(id) {
            return;
        }
        let instant = self.kernel.now();
        let act = self.kernel.begin(id, event);
        let Some(b) = self.builtins.get_mut(&id) else {
            self.kernel.fault(act, FaultKind::Trap, 0, 0);
            self.note_activity(id, true, instant);
            return;
        };
        b.sink.reset();
        let Some((d, kind)) = self.world.builtin(id) else {
            self.kernel.fault(act, FaultKind::Trap, 0, 0);
            self.note_activity(id, true, instant);
            return;
        };
        d.builtin(id, &kind, be, &mut b.sink);
        let reports: Vec<(u32, Vec<u8>)> = b
            .sink
            .effects()
            .iter()
            .filter_map(|e| match e {
                florn_kernel::Effect::Report { channel, payload } => {
                    Some((*channel, payload.clone()))
                }
                _ => None,
            })
            .collect();
        let kept: Vec<Vec<u8>> = if self.options.keep_payloads {
            b.sink
                .effects()
                .iter()
                .filter_map(|e| match e {
                    florn_kernel::Effect::Send { payload, .. }
                    | florn_kernel::Effect::Report { payload, .. } => Some(payload.clone()),
                    _ => None,
                })
                .collect()
        } else {
            Vec::new()
        };
        match self.kernel.commit(act, &mut b.sink, 0, &mut self.world) {
            Ok(()) => {
                self.note_activity(id, false, instant);
                for (channel, payload) in reports {
                    if channel == 0 {
                        let n = Notification::new(
                            "actor.text",
                            Severity::Info,
                            Scope::actor(id),
                            Ends::Nothing,
                        )
                        .with("actor", Value::Identity(id))
                        .with(
                            "text",
                            Value::Text(String::from_utf8_lossy(&payload).into_owned()),
                        )
                        .at(at(instant));
                        self.notify(n);
                    }
                    if self.telemetry.len() >= self.options.telemetry_capacity {
                        self.telemetry.pop_front();
                        self.telemetry_dropped += 1;
                    }
                    self.telemetry.push_back(Telemetry {
                        at: instant,
                        actor: id,
                        channel,
                        payload,
                    });
                }
                for p in kept {
                    self.payloads
                        .entry(*blake3::hash(&p).as_bytes())
                        .or_insert(p);
                }
            }
            Err(refusal) => {
                let (kind, parameter) = match &refusal {
                    florn_kernel::CommitRefusal::DelayBeyondLargestTick { timer, .. } => {
                        (FaultKind::Contract, *timer)
                    }
                    florn_kernel::CommitRefusal::PendingTimers { .. } => (FaultKind::Quota, 6),
                    florn_kernel::CommitRefusal::Refused(r) => {
                        (FaultKind::from_refusal(r), fault_parameter(r))
                    }
                };
                self.note_activity(id, true, instant);
                self.faulted(id, kind, parameter, 0, instant);
            }
        }
    }

    /// Make the actor ready for an activation (RFC 009 rule 13: its instance is in the pool) and
    /// count the activation for the pool's policy. `false` when there is nothing to activate: the
    /// actor is dead, is no module, or the run ended because the instance could not be held
    /// (`host.instance-failed`).
    pub(super) fn prepare_activation(&mut self, id: Identity) -> bool {
        if !self.alive(id) || !self.actors.contains_key(&id) {
            return false;
        }
        let instant = self.kernel.now();
        // Swapping (RFC 009 rule 13): the actor's instance before anything of the activation.
        if let Err(e) = self.ensure_resident(id) {
            let n = Notification::new(
                "host.instance-failed",
                Severity::Error,
                Scope::actor(id),
                Ends::Run,
            )
            .with("actor", Value::Identity(id))
            .with("text", Value::Text(e.to_string()))
            .at(at(instant));
            self.notify(n);
            self.end(EndReason::HostFault);
            return false;
        }
        if !self.actors.get(&id).is_some_and(|s| s.instance.is_some()) {
            return false;
        }
        self.activation_seq += 1;
        let seq = self.activation_seq;
        if let Some(s) = self.actors.get_mut(&id) {
            s.last_activated = seq;
        }
        true
    }

    /// One activation (RFC 005 rule 15; RFC 003 rules 11 to 13; RFC 001 rules 22 to 24).
    pub(super) fn activate(&mut self, id: Identity, event: Event, which: usize, vals: &[Val]) {
        if !self.prepare_activation(id) {
            return;
        }
        let instant = self.kernel.now();
        let act = self.kernel.begin(id, event);
        let outcome = {
            let Self {
                engine,
                kernel,
                world,
                actors,
                options,
                payloads,
                telemetry,
                telemetry_dropped,
                ..
            } = self;
            let slot = actors.get_mut(&id).expect("checked above");
            activate_slot(
                engine,
                kernel,
                world,
                slot,
                act,
                which,
                vals,
                options.keep_payloads,
                payloads,
                telemetry,
                telemetry_dropped,
                options.telemetry_capacity,
                instant,
            )
        };
        self.settle(id, outcome, instant);
    }

    /// What follows an activation's end, whether it was one call or a compound activation: the
    /// display batch's count, the channel 0 texts of a commit for the diagnostic log, the
    /// actor's death and the policy for a fault, the end of the run for a Host fault.
    pub(super) fn settle(&mut self, id: Identity, outcome: Outcome, instant: Instant) {
        self.note_activity(id, !matches!(outcome, Outcome::Committed { .. }), instant);
        match outcome {
            Outcome::Committed { texts } => {
                // Channel 0 is text for the author; it reaches the diagnostic log (RFC 002 §10).
                for text in texts {
                    let n = Notification::new(
                        "actor.text",
                        Severity::Info,
                        Scope::actor(id),
                        Ends::Nothing,
                    )
                    .with("actor", Value::Identity(id))
                    .with("text", Value::Text(text))
                    .at(at(instant));
                    self.notify(n);
                }
            }
            Outcome::Faulted {
                kind,
                parameter,
                metered,
            } => self.faulted(id, kind, parameter, metered, instant),
            Outcome::HostFault { reason, text } => {
                self.reproducible = false;
                let n = Notification::new(
                    "host.not-reproducible",
                    Severity::Error,
                    Scope::actor(id),
                    Ends::Run,
                )
                .with("reason", Value::Enum(reason.into()))
                .with("text", Value::Text(text))
                .at(at(instant));
                self.notify(n);
                self.drop_instance(id);
                self.end(EndReason::HostFault);
            }
        }
    }

    pub(super) fn drop_instance(&mut self, id: Identity) {
        if let Some(inst) = self.actors.get_mut(&id).and_then(|s| s.instance.take()) {
            self.engine.drop_instance(inst);
        }
        if let Some(s) = self.actors.get_mut(&id) {
            s.funcs.clear();
            s.posts.clear();
        }
    }

    fn faulted(
        &mut self,
        id: Identity,
        kind: FaultKind,
        parameter: u32,
        metered: u64,
        instant: Instant,
    ) {
        // RFC 003 rule 13: the instance is discarded; the actor handles nothing further.
        self.drop_instance(id);
        if let Some(s) = self.actors.get_mut(&id) {
            s.swapped = None;
        }
        self.world.dead.insert(id);
        let policy = self.options.fault_policy;
        let n = Notification::new(
            "actor.faulted",
            Severity::Error,
            Scope::actor(id),
            Ends::Activation,
        )
        .with("actor", Value::Identity(id))
        .with("fault", Value::Code(fault_code(kind).into()))
        .with("metered", Value::Count(metered))
        .with("policy", Value::Enum(policy.as_str().into()))
        .with("parameter", Value::Count(parameter as u64))
        .at(at(instant));
        self.notify(n);
        if policy == FaultPolicy::StopRun {
            self.end(EndReason::User);
        }
    }
}

#[cfg(test)]
mod tests;
