//! The compound activation and the call-in handle (RFC 002 rules 44 and 45; RFC 003 rule 12;
//! RFC 012 findings F-2 and F-3).
//!
//! A domain contract may declare that a sequence of calls into one actor forms **one activation**
//! ([`Compound`]): an opening call, calls permitted between, a committing close and a discarding
//! close. Within it one budget (the global is set at the opening call and read at the closing),
//! one set of quotas, one record (written at the committing close, or none at the discarding
//! one), and a fault in any call ends the whole as a fault.
//!
//! The Host plays the sequence for a domain through a [`CallIn`], which it gives to
//! [`Domain::event_calling`](crate::domain::Domain::event_calling) while the domain handles an
//! event. The domain decides what to call and when to close; the Host lowers the arguments, calls
//! the export, lifts the result (through `florn_engine::call_lifting`, so that a list or string
//! result's post-return runs once, inside the activation and under its budget), measures, and
//! commits or discards. The handle never outlives the call that was given it: an activation still
//! open when the domain returns is discarded.

use florn_abi::Val;
use florn_kernel::{FaultKind, Identity};

/// The calls that form one compound activation, by their canonical export names (RFC 002 rule
/// 44). RFC 002 rule 45 and RFC 012 rule 20 are the circuit device's:
/// `begin-step`, then `evaluate` any number of times, then `accept-step` or `reject-step`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compound {
    /// The opening call.
    pub open: String,
    /// The calls permitted between the opening and the closing, each any number of times.
    pub between: Vec<String>,
    /// The closing call that commits.
    pub commit: String,
    /// The closing call that discards.
    pub discard: String,
}

impl Compound {
    pub fn new(open: &str, between: &[&str], commit: &str, discard: &str) -> Self {
        Self {
            open: open.into(),
            between: between.iter().map(|s| s.to_string()).collect(),
            commit: commit.into(),
            discard: discard.into(),
        }
    }

    /// Whether `export` is one of the calls of this compound activation.
    pub fn has(&self, export: &str) -> bool {
        self.open == export
            || self.commit == export
            || self.discard == export
            || self.between.iter().any(|b| b == export)
    }
}

/// Why a call through the handle did not give a result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallError {
    /// The activation ended in a **Fault** (RFC 001 rule 24): one Fault record is in the trace,
    /// the actor is dead, and nothing is open. The domain stops calling this actor. The guest
    /// trapped, ran out of budget or stack or memory, broke the contract (a lifting failure, a
    /// call out of sequence, a `send` or a timer in a device), or the domain asked for it with
    /// [`CallIn::fault`].
    Faulted { kind: FaultKind, parameter: u32 },
    /// A list result named more elements than the bound the domain gave. Nothing of the list was
    /// read and its post-return was not called; the activation is **still open**. The domain
    /// decides: [`CallIn::fault`], or [`CallIn::discard`].
    ResultTooLong { len: u32, bound: u32 },
    /// The call was not made and nothing changed: there is nothing open, or something already is,
    /// the actor is not alive or has no compound activation, or the export is not one the
    /// declaration permits here. A defect of the domain, not of the guest.
    Refused(&'static str),
    /// The run ended in the call (a Host fault: the instance could not be held, or the engine
    /// failed); nothing is open and the domain abandons what it was doing.
    RunEnded,
}

/// The call-in handle (RFC 012 finding F-3): the domain's way of running a guest inside an event
/// it handles. One compound activation is open at a time (RFC 001 rule 21).
pub trait CallIn {
    /// Open a compound activation of `actor` by its opening call `export` with `args`: the
    /// kernel begins the activation for the event being handled, the budget is set once, and the
    /// opening call is made. An actor that is swapped out is brought in first (RFC 009 rule 13).
    fn open(&mut self, actor: Identity, export: &str, args: &[Val]) -> Result<(), CallError>;

    /// A call between the opening and the closing. When the export returns a list, `list_bound`
    /// is the most elements the domain will take, checked against the length the guest names
    /// **before** the list is lifted (RFC 012 rule 20). Returns the lifted result.
    fn call(
        &mut self,
        export: &str,
        args: &[Val],
        list_bound: Option<u32>,
    ) -> Result<Option<Val>, CallError>;

    /// The committing close: the closing call, then the kernel commits the effects the guest
    /// recorded in all the calls, as one record, with the work of all of them. Sends are refused
    /// (a guest of such a world has no queue presence, RFC 012 rule 19).
    fn commit(&mut self, export: &str, args: &[Val]) -> Result<(), CallError>;

    /// The discarding close: the closing call, then the kernel discards the activation: nothing
    /// recorded, the effects dropped, the actor's random stream restored, the actor alive
    /// (RFC 012 rule 21).
    fn discard(&mut self, export: &str, args: &[Val]) -> Result<(), CallError>;

    /// End the open activation as a Fault of the contract the domain judged broken (for example
    /// a result of the wrong shape): the Fault record with `kind` and `parameter`, the actor dead.
    /// Returns the error the failed call would have returned.
    fn fault(&mut self, kind: FaultKind, parameter: u32) -> CallError;

    /// Whether the run has ended: a fault under `stop-run` ends it, and the domain abandons the
    /// advance where it stands.
    fn run_ended(&self) -> bool;
}

/// A handle that refuses everything: for a caller that has no run to call into.
pub struct NoCalls;

impl CallIn for NoCalls {
    fn open(&mut self, _: Identity, _: &str, _: &[Val]) -> Result<(), CallError> {
        Err(CallError::Refused("no call-in handle"))
    }
    fn call(&mut self, _: &str, _: &[Val], _: Option<u32>) -> Result<Option<Val>, CallError> {
        Err(CallError::Refused("no call-in handle"))
    }
    fn commit(&mut self, _: &str, _: &[Val]) -> Result<(), CallError> {
        Err(CallError::Refused("no call-in handle"))
    }
    fn discard(&mut self, _: &str, _: &[Val]) -> Result<(), CallError> {
        Err(CallError::Refused("no call-in handle"))
    }
    fn fault(&mut self, kind: FaultKind, parameter: u32) -> CallError {
        CallError::Faulted { kind, parameter }
    }
    fn run_ended(&self) -> bool {
        false
    }
}
