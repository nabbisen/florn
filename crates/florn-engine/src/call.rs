//! Calling an export that returns a value and lifting what it returns (RFC 002 rule 2): the part
//! of the contract that is the same for every adapter, so that it is written once, over the
//! [`Engine`] trait. After a result that is or holds a list or a string has been lifted, the
//! module's `cabi_post_<export>` is called once with the result pointer, so that the guest frees
//! the buffers it allocated for the result; without it each call would leave them in the guest's
//! memory until the memory limit traps the guest.

use florn_abi::{AbiError, Flat, Ty, Val, lift_results};

use crate::{Engine, ImportHandler, Trap};

/// Why a call that returns a value failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallError {
    /// The export, or its post-return, trapped (the Host classifies it by the globals, as for
    /// any activation).
    Trap(Trap),
    /// The result could not be lifted: the contract fault of RFC 002 rule 8. The post-return is
    /// not called then: the result is not valid and the guest's buffers stay its own.
    Lift(AbiError),
    /// The result needs a post-return and the caller resolved none. Admission refuses a module
    /// without one (`module.export-missing`), so this is a Host defect, not a guest's.
    NoPostReturn,
    /// The engine gave no memory to lift from.
    Memory(String),
}

/// Call `func` with `args` and lift its `result`. When the result needs a post-return
/// ([`Ty::needs_post_return`]), call `post` once with the result pointer after the lift: inside
/// the same activation, so the guest's metering still runs, and with the same `handler`.
pub fn call_lifting<E: Engine>(
    engine: &mut E,
    inst: &mut E::Instance,
    func: &E::Func,
    post: Option<&E::Func>,
    args: &[Flat],
    result: Option<&Ty>,
    handler: &mut dyn ImportHandler,
) -> Result<Option<Val>, CallError> {
    let flats = engine
        .call(inst, func, args, handler)
        .map_err(CallError::Trap)?;
    let mut lifted = Ok(None);
    engine
        .with_memory(inst, &mut |mem| lifted = lift_results(mem, result, &flats))
        .map_err(|e| CallError::Memory(e.to_string()))?;
    let value = lifted.map_err(CallError::Lift)?;
    if result.is_some_and(Ty::needs_post_return) {
        let post = post.ok_or(CallError::NoPostReturn)?;
        // A result that needs a post-return is always returned through a pointer (it has more
        // than one flat value), and `lift_results` has just checked that it is one `i32`.
        engine
            .call(inst, post, &flats, handler)
            .map_err(CallError::Trap)?;
    }
    Ok(value)
}
