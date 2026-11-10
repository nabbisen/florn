//! The call-in handle of a run (RFC 012 finding F-3): a compound activation (RFC 002 rules 44 and
//! 45) played for a domain while it handles an event. The kernel's `begin` opens it, the budget is
//! set once at the opening call and read at the closing (RFC 003 rule 12), every call goes through
//! `florn_engine::call_lifting` (a list or string result's post-return runs once, inside the
//! activation and under its budget), and the close either commits with a world that delivers
//! nothing (a guest of such a world has no queue presence, RFC 012 rule 19) or discards: the
//! kernel's third outcome (finding F-2), which records nothing and leaves the actor alive.
//!
//! The handle holds the whole run while the domain that uses it is lent out of the run's
//! composite world (`Composite::lend_class`), so that it can bring an actor into the pool, set
//! the budget and settle a fault exactly as a plain activation does.

use florn_abi::{Flat, Val, lower_flat_params};
use florn_engine::{CallError as EngineCall, Callee, Engine, call_lifting};
use florn_kernel::{Activation, Delivery, Event, FaultKind, Identity, Instant, World};

use crate::compound::{CallError, CallIn, Compound};
use crate::domain::WorldSigs;

use super::Run;
use super::activation::{
    Handler, Outcome, Records, committed_outcome, contract_outcome, engine_failed,
    lowering_outcome, metered_of, read_globals, set_budget, trap_outcome,
};

/// The world a compound activation commits with: nothing is delivered. Its sends are refused
/// before they are recorded (the handler is queueless), so this is never asked.
struct NoSends;

impl World for NoSends {
    fn deliver(&mut self, _: Instant, _: Identity, _: u64, _: u32, _: &[u8]) -> Vec<Delivery> {
        Vec::new()
    }
}

/// The compound activation in progress.
struct Open {
    actor: Identity,
    act: Activation,
    sigs: std::sync::Arc<WorldSigs>,
    compound: Compound,
    instant: Instant,
}

/// The call-in handle for the event `event`, which the domain is handling.
pub(super) struct Calls<'a, E: Engine> {
    run: &'a mut Run<E>,
    event: Event,
    open: Option<Open>,
}

impl<'a, E: Engine> Calls<'a, E> {
    pub(super) fn new(run: &'a mut Run<E>, event: Event) -> Self {
        Self {
            run,
            event,
            open: None,
        }
    }

    /// The domain has returned: an activation it left open is discarded, never carried over (the
    /// handle does not outlive the call it was given). A domain that does so has a defect.
    pub(super) fn finish(mut self) {
        if let Some(open) = self.open.take() {
            debug_assert!(false, "a compound activation was left open by the domain");
            self.run.kernel.discard(open.act);
            if let Some(slot) = self.run.actors.get_mut(&open.actor) {
                slot.sink.reset();
            }
        }
    }

    /// End the open activation with `outcome` (a fault or a Host fault) and tell the domain.
    fn settle_failure(&mut self, outcome: Outcome) -> CallError {
        let open = self.open.take().expect("an activation is open");
        let error = match &outcome {
            Outcome::Faulted {
                kind, parameter, ..
            } => CallError::Faulted {
                kind: *kind,
                parameter: *parameter,
            },
            _ => CallError::RunEnded,
        };
        self.run.settle(open.actor, outcome, open.instant);
        error
    }

    /// One call of the open activation: lower, call through `call_lifting`, classify a failure.
    fn invoke(
        &mut self,
        export: &str,
        args: &[Val],
        list_bound: Option<u32>,
    ) -> Result<Option<Val>, CallError> {
        let (actor, act, sigs) = {
            let open = self.open.as_ref().expect("an activation is open");
            (open.actor, open.act.clone(), open.sigs.clone())
        };
        let Some(which) = sigs.export_index(export) else {
            return Err(CallError::Refused("the world has no such export"));
        };
        let tys = &sigs.exports[which].1;
        let result_ty = sigs.results[which].as_ref();
        let outcome = {
            let Run {
                engine,
                kernel,
                world,
                actors,
                ..
            } = &mut *self.run;
            let slot = actors
                .get_mut(&actor)
                .expect("an open activation has a slot");
            let limits = slot.limits;
            let inst = slot.instance.as_mut().expect("resident while open");
            let mut lowered: Result<Vec<Flat>, florn_abi::AbiError> = Ok(Vec::new());
            if let Err(e) =
                engine.with_memory(inst, &mut |m| lowered = lower_flat_params(m, tys, args))
            {
                Err(engine_failed(kernel, act, &e))
            } else {
                match lowered {
                    Err(e) => {
                        let globals = read_globals(engine, inst);
                        Err(lowering_outcome(kernel, act, &e, globals, &limits))
                    }
                    Ok(flats) => {
                        let func = slot.funcs[which].clone();
                        let post = slot.posts[which].clone();
                        let mut handler = Handler {
                            kernel,
                            sink: &mut slot.sink,
                            world,
                            actor,
                            sigs: &sigs,
                            queueless: true,
                            contract: None,
                        };
                        let r = call_lifting(
                            engine,
                            inst,
                            Callee {
                                func: &func,
                                post: post.as_ref(),
                                result: result_ty,
                                list_bound,
                            },
                            &flats,
                            &mut handler,
                        );
                        let contract = handler.contract;
                        let globals = read_globals(engine, inst);
                        let metered = metered_of(limits.budget, globals.0);
                        match (r, contract) {
                            (Ok(value), None) => Ok(value),
                            (_, Some(_)) => Err(contract_outcome(kernel, act, metered)),
                            (Err(EngineCall::Trap(trap)), None) => Err(trap_outcome(
                                kernel,
                                act,
                                slot.sink.refusal(),
                                trap,
                                globals,
                                &limits,
                            )),
                            // The result could not be lifted: the contract fault of rule 8.
                            (Err(EngineCall::Lift(_)), None) => {
                                Err(contract_outcome(kernel, act, metered))
                            }
                            // Admission requires the post-return (RFC 003 rule 5), so a missing
                            // one is a module that was not admitted: RFC 002 rule 2.
                            (Err(EngineCall::NoPostReturn), None) => {
                                kernel.fault(act, FaultKind::Contract, 2, metered);
                                Err(Outcome::Faulted {
                                    kind: FaultKind::Contract,
                                    parameter: 2,
                                    metered,
                                })
                            }
                            (Err(EngineCall::Memory(text)), None) => Err(engine_failed(
                                kernel,
                                act,
                                &florn_engine::EngineError::Memory(text),
                            )),
                            // Nothing was read; the activation stays open for the domain to
                            // fault or discard.
                            (Err(EngineCall::ResultTooLong { len, bound }), None) => {
                                return Err(CallError::ResultTooLong { len, bound });
                            }
                        }
                    }
                }
            }
        };
        outcome.map_err(|o| self.settle_failure(o))
    }

    /// An open activation, and `export` a call its declaration permits at this point.
    fn permitted(
        &mut self,
        allowed: impl Fn(&Compound) -> Vec<&str>,
        export: &str,
    ) -> Result<(), CallError> {
        let Some(open) = self.open.as_ref() else {
            return Err(CallError::Refused("no compound activation is open"));
        };
        if allowed(&open.compound).contains(&export) {
            return Ok(());
        }
        // RFC 002 rule 44: "A call out of sequence is a contract violation."
        Err(self.fault(FaultKind::Contract, 44))
    }
}

impl<E: Engine> CallIn for Calls<'_, E> {
    fn open(&mut self, actor: Identity, export: &str, args: &[Val]) -> Result<(), CallError> {
        if self.open.is_some() {
            return Err(CallError::Refused("a compound activation is already open"));
        }
        if self.run.ended.is_some() {
            return Err(CallError::RunEnded);
        }
        let Some(slot) = self.run.actors.get(&actor) else {
            return Err(CallError::Refused("the actor is no module"));
        };
        let sigs = slot.world.clone();
        let Some(compound) = sigs.compound_of(export).cloned() else {
            return Err(CallError::Refused("no compound activation has this call"));
        };
        if compound.open != export {
            return Err(CallError::Refused("this is not the opening call"));
        }
        if !self.run.alive(actor) {
            return Err(CallError::Refused("the actor is dead"));
        }
        if !self.run.prepare_activation(actor) {
            return Err(if self.run.ended.is_some() {
                CallError::RunEnded
            } else {
                CallError::Refused("the actor cannot be activated")
            });
        }
        let instant = self.run.kernel.now();
        let act = self.run.kernel.begin(actor, self.event.clone());
        let budget = {
            let slot = self.run.actors.get_mut(&actor).expect("checked");
            slot.sink.reset();
            slot.limits.budget
        };
        self.open = Some(Open {
            actor,
            act: act.clone(),
            sigs,
            compound,
            instant,
        });
        // RFC 003 rule 12: the budget is set once, at the opening call.
        let set = {
            let Run {
                engine,
                kernel,
                actors,
                ..
            } = &mut *self.run;
            let slot = actors.get_mut(&actor).expect("checked");
            let inst = slot.instance.as_mut().expect("resident");
            set_budget(engine, inst, budget).map_err(|e| engine_failed(kernel, act, &e))
        };
        if let Err(outcome) = set {
            return Err(self.settle_failure(outcome));
        }
        self.invoke(export, args, None).map(|_| ())
    }

    fn call(
        &mut self,
        export: &str,
        args: &[Val],
        list_bound: Option<u32>,
    ) -> Result<Option<Val>, CallError> {
        self.permitted(|c| c.between.iter().map(String::as_str).collect(), export)?;
        self.invoke(export, args, list_bound)
    }

    fn commit(&mut self, export: &str, args: &[Val]) -> Result<(), CallError> {
        self.permitted(|c| vec![c.commit.as_str()], export)?;
        self.invoke(export, args, None)?;
        let open = self.open.take().expect("an activation is open");
        let Run {
            engine,
            kernel,
            actors,
            options,
            payloads,
            telemetry,
            telemetry_dropped,
            ..
        } = &mut *self.run;
        let slot = actors
            .get_mut(&open.actor)
            .expect("an open activation has a slot");
        let inst = slot.instance.as_mut().expect("resident while open");
        // RFC 003 rule 12: the budget the opening call set is the one read here, at the close.
        let (remaining, _) = read_globals(engine, inst);
        let metered = metered_of(slot.limits.budget, remaining);
        let outcome = committed_outcome(
            kernel,
            &mut NoSends,
            &mut slot.sink,
            open.actor,
            open.act,
            metered,
            Records {
                keep_payloads: options.keep_payloads,
                payloads,
                telemetry,
                telemetry_dropped,
                telemetry_capacity: options.telemetry_capacity,
                instant: open.instant,
            },
        );
        let failed = match &outcome {
            Outcome::Faulted {
                kind, parameter, ..
            } => Some(CallError::Faulted {
                kind: *kind,
                parameter: *parameter,
            }),
            _ => None,
        };
        self.run.settle(open.actor, outcome, open.instant);
        failed.map_or(Ok(()), Err)
    }

    fn discard(&mut self, export: &str, args: &[Val]) -> Result<(), CallError> {
        self.permitted(|c| vec![c.discard.as_str()], export)?;
        self.invoke(export, args, None)?;
        let open = self.open.take().expect("an activation is open");
        // Nothing is recorded; the stream is restored and the actor lives (RFC 012 rule 21).
        self.run.kernel.discard(open.act);
        if let Some(slot) = self.run.actors.get_mut(&open.actor) {
            slot.sink.reset();
        }
        Ok(())
    }

    fn fault(&mut self, kind: FaultKind, parameter: u32) -> CallError {
        let Some(open) = self.open.as_ref() else {
            return CallError::Refused("no compound activation is open");
        };
        let (actor, act) = (open.actor, open.act.clone());
        let metered = {
            let Run { engine, actors, .. } = &mut *self.run;
            let slot = actors
                .get_mut(&actor)
                .expect("an open activation has a slot");
            let inst = slot.instance.as_mut().expect("resident while open");
            let (remaining, _) = read_globals(engine, inst);
            metered_of(slot.limits.budget, remaining)
        };
        self.run.kernel.fault(act, kind, parameter, metered);
        self.settle_failure(Outcome::Faulted {
            kind,
            parameter,
            metered,
        })
    }

    fn run_ended(&self) -> bool {
        self.run.ended.is_some()
    }
}
