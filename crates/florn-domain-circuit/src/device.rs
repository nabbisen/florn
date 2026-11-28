//! The guest device model (RFC 012 rules 19 to 21; RFC 013 rules 5, 8, 9 and 16): the world
//! `florn:circuit/device`, and the calls the solver makes into the one guest of that world a
//! scenario may have, inside a step.
//!
//! The solver calls a device through [`Device`]: `begin` opens the compound activation of an
//! attempted step (or of one solve of the initial analysis), `evaluate` is called once per Newton
//! iteration, `accept` is the committing close and `reject` the discarding one. [`Calls`] plays that
//! over the Host's call-in handle, checking what the guest returns **before** the solver uses it
//! (rule 20): the length bound is the handle's, checked before the list is lifted, and a result of
//! another length, with a value that is not finite or with a last entry that is neither 0 nor 1 is
//! `circuit.stamp-invalid` and the device faults. [`NoDevice`] is the plant without one.

use florn_abi::core_world::Signature;
use florn_abi::{Ty, Val};
use florn_control::{Code, Scope, Value};
use florn_host::compound::{CallError, CallIn, Compound};
use florn_host::domain::{Action, DomainWorld};
use florn_kernel::{FaultKind, Identity};

use crate::plant::GuestPins;
use crate::read::note;

/// `florn:circuit/device@0.1.0`, the world of a device model (RFC 012 rule 19).
pub const WORLD: &str = "florn:circuit/device@0.1.0";
/// The interface the world exports, `device-model` (RFC 012 rule 19, amended 2026-11-27).
pub const INTERFACE: &str = "florn:circuit/device-model@0.1.0";
pub const BEGIN_STEP: &str = "florn:circuit/device-model@0.1.0#begin-step";
pub const EVALUATE: &str = "florn:circuit/device-model@0.1.0#evaluate";
pub const ACCEPT_STEP: &str = "florn:circuit/device-model@0.1.0#accept-step";
pub const REJECT_STEP: &str = "florn:circuit/device-model@0.1.0#reject-step";

/// The Fault parameter of a contract violation of RFC 012 rule 19 (a `send`, a timer, in a device)
/// and of rule 20 (a result that is not a stamp): the RFC's number times 1,000 plus the rule's
/// (RFC 012 rule 20).
pub const SEND_IN_DEVICE: u32 = 12_019;
pub const STAMP_INVALID: u32 = 12_020;

/// The world of RFC 012 rule 19: the compound activation of RFC 002 rules 44 and 45. Its calls
/// are `begin-step`, `evaluate` any number of times, then `accept-step` or `reject-step`.
pub fn world() -> DomainWorld {
    let list = || Ty::list(Ty::F64);
    DomainWorld {
        name: WORLD.to_string(),
        imports_ids: vec![],
        exports_ids: vec![INTERFACE.to_string()],
        imports: vec![],
        exports: vec![(
            INTERFACE.to_string(),
            vec![
                Signature {
                    name: "begin-step",
                    params: vec![("start", Ty::U64), ("h", Ty::F64)],
                    result: None,
                },
                Signature {
                    name: "evaluate",
                    params: vec![("trial", list())],
                    result: Some(list()),
                },
                Signature {
                    name: "accept-step",
                    params: vec![("solution", list())],
                    result: None,
                },
                Signature {
                    name: "reject-step",
                    params: vec![],
                    result: None,
                },
            ],
        )],
        compounds: vec![Compound::new(
            BEGIN_STEP,
            &[EVALUATE],
            ACCEPT_STEP,
            REJECT_STEP,
        )],
        records: vec![],
    }
}

/// What a device answered at a trial point (RFC 012 rule 19): the currents into its pins, the
/// Jacobian row-major, and whether it limited a junction in this evaluation.
#[derive(Clone, Debug, PartialEq)]
pub struct Stamp {
    pub currents: Vec<f64>,
    pub jacobian: Vec<f64>,
    pub limited: bool,
}

/// Why a call into the device did not give what the solver asked: the device is gone from this
/// step on, or the run has ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lost {
    /// The device faulted: one Fault record is in the trace and the actor is dead (rule 17), or the
    /// domain judged its result invalid (rule 20: `reason` says how). The step is discarded and
    /// attempted again without it.
    Faulted {
        kind: FaultKind,
        parameter: u32,
        /// `Some` for a result the domain judged: the text of `circuit.stamp-invalid`.
        invalid: Option<&'static str>,
    },
    /// The Host refused the call and changed nothing (the actor is dead or cannot be activated):
    /// the device is absent from this step on, with no record of its own.
    Gone,
    /// The run ended in the call (a fault under `stop-run`, or the Host's): the advance is
    /// abandoned where it stands and the plant keeps its last accepted step (rule 17).
    Ended,
}

/// The notifications of a device lost (RFC 012 rule 17): `circuit.stamp-invalid` when the result it
/// answered was no stamp (rule 20), and `circuit.device-faulted`, once, for any loss.
pub fn lost_notes(lost: &Lost, device: &GuestPins) -> Vec<Action> {
    let mut actions = Vec::new();
    if let Lost::Faulted {
        invalid: Some(reason),
        ..
    } = lost
    {
        actions.push(Action::Notify(note(
            Code::CircuitStampInvalid,
            Scope::actor(device.actor),
            None,
            vec![
                Value::Name(device.name.clone()),
                Value::Text((*reason).to_string()),
            ],
        )));
    }
    actions.push(Action::Notify(note(
        Code::CircuitDeviceFaulted,
        Scope::actor(device.actor),
        None,
        vec![Value::Name(device.name.clone())],
    )));
    actions
}

/// The solver's view of the guest device.
pub trait Device {
    /// Whether there is a device to call.
    fn present(&self) -> bool;
    /// Open the compound activation of a step that begins at tick `start` and is `h` seconds long
    /// (`0.0` for a solve of the initial analysis).
    fn begin(&mut self, start: u64, h: f64) -> Result<(), Lost>;
    /// One evaluation at the trial voltages of the device's pins.
    fn evaluate(&mut self, trial: &[f64]) -> Result<Stamp, Lost>;
    /// The committing close, with the converged pin voltages.
    fn accept(&mut self, solution: &[f64]) -> Result<(), Lost>;
    /// The discarding close.
    fn reject(&mut self) -> Result<(), Lost>;
}

/// The plant with no guest device.
pub struct NoDevice;

impl Device for NoDevice {
    fn present(&self) -> bool {
        false
    }
    fn begin(&mut self, _: u64, _: f64) -> Result<(), Lost> {
        Ok(())
    }
    fn evaluate(&mut self, _: &[f64]) -> Result<Stamp, Lost> {
        unreachable!("the solver evaluates a device that is present only")
    }
    fn accept(&mut self, _: &[f64]) -> Result<(), Lost> {
        Ok(())
    }
    fn reject(&mut self) -> Result<(), Lost> {
        Ok(())
    }
}

/// The device played over the Host's call-in handle.
pub struct Calls<'a> {
    calls: &'a mut dyn CallIn,
    actor: Identity,
    /// The number of pins, the `n` of the result's length `n + n² + 1`.
    pins: usize,
}

impl<'a> Calls<'a> {
    pub fn new(calls: &'a mut dyn CallIn, actor: Identity, pins: usize) -> Self {
        Self { calls, actor, pins }
    }

    fn lost(&mut self, e: CallError) -> Lost {
        match e {
            CallError::Faulted { kind, parameter } => Lost::Faulted {
                kind,
                parameter,
                invalid: None,
            },
            CallError::ResultTooLong { .. } => {
                // Neither lifted nor given its post-return: the only close is the fault (rule 20).
                self.invalid("the result is longer than n + n² + 1 values")
            }
            CallError::RunEnded => Lost::Ended,
            // Nothing was changed: the device is absent from here on.
            CallError::Refused(_) => Lost::Gone,
        }
    }

    /// The Fault of a result the domain judges invalid (rule 20).
    fn invalid(&mut self, reason: &'static str) -> Lost {
        let e = self.calls.fault(FaultKind::Contract, STAMP_INVALID);
        match e {
            CallError::RunEnded => Lost::Ended,
            _ => Lost::Faulted {
                kind: FaultKind::Contract,
                parameter: STAMP_INVALID,
                invalid: Some(reason),
            },
        }
    }
}

fn floats(values: &[f64]) -> Val {
    Val::List(values.iter().map(|v| Val::F64(*v)).collect())
}

impl Device for Calls<'_> {
    fn present(&self) -> bool {
        true
    }

    fn begin(&mut self, start: u64, h: f64) -> Result<(), Lost> {
        let r = self
            .calls
            .open(self.actor, BEGIN_STEP, &[Val::U64(start), Val::F64(h)]);
        r.map_err(|e| self.lost(e))
    }

    fn evaluate(&mut self, trial: &[f64]) -> Result<Stamp, Lost> {
        let n = self.pins;
        let bound = (n + n * n + 1) as u32;
        let result = match self.calls.call(EVALUATE, &[floats(trial)], Some(bound)) {
            Ok(Some(Val::List(values))) => values,
            Ok(_) => return Err(self.invalid("the result is not a list of numbers")),
            Err(e) => return Err(self.lost(e)),
        };
        let mut numbers = Vec::with_capacity(result.len());
        for v in &result {
            match v {
                Val::F64(x) => numbers.push(*x),
                _ => return Err(self.invalid("the result is not a list of numbers")),
            }
        }
        if numbers.len() != n + n * n + 1 {
            return Err(self.invalid("the result is not n + n² + 1 values"));
        }
        if numbers.iter().any(|x| !x.is_finite()) {
            return Err(self.invalid("a value of the result is not finite"));
        }
        let flag = numbers[n + n * n];
        let limited = if flag == 1.0 {
            true
        } else if flag == 0.0 {
            false
        } else {
            return Err(self.invalid("the last entry is neither 0 nor 1"));
        };
        Ok(Stamp {
            currents: numbers[..n].to_vec(),
            jacobian: numbers[n..n + n * n].to_vec(),
            limited,
        })
    }

    fn accept(&mut self, solution: &[f64]) -> Result<(), Lost> {
        let r = self.calls.commit(ACCEPT_STEP, &[floats(solution)]);
        r.map_err(|e| self.lost(e))
    }

    fn reject(&mut self) -> Result<(), Lost> {
        let r = self.calls.discard(REJECT_STEP, &[]);
        r.map_err(|e| self.lost(e))
    }
}

#[cfg(test)]
mod tests;
