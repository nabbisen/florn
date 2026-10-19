//! The actor trait and the glue that exports it (RFC 002 §5, §6): one activation per call,
//! `now()` for library code, the actor held in one cell for the instance's lifetime.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::{ActorId, Port, PortId, Tick, TimerId};

/// What a guest implements. `init` is the first activation (rule 36); the others are one
/// activation each. A panic traps and the activation faults (rule 40).
pub trait Actor: Sized {
    /// `config` holds the declared parameters in order (rule 14); decode it with the `Config`
    /// type `declare!` generated.
    fn init(actor: ActorId, ports: &[Port], config: &[u8]) -> Self;
    fn on_message(&mut self, port: PortId, payload: &[u8]);
    fn on_timer(&mut self, timer: TimerId);
}

static NOW: AtomicU64 = AtomicU64::new(0);

/// The `now` argument of the current activation (RFC 002 rule 50).
pub fn now() -> Tick {
    NOW.load(Ordering::Relaxed)
}

#[doc(hidden)]
pub fn set_now(now: Tick) {
    NOW.store(now, Ordering::Relaxed);
}

/// The cell that holds the actor between activations.
///
/// SAFETY of `Sync`: a guest instance runs on one thread, one activation at a time (RFC 001 rule
/// 21; RFC 002 rule 37: a guest is never on the stack twice), so no two references to the actor
/// exist at once.
pub struct Cell<T>(UnsafeCell<Option<T>>);

unsafe impl<T> Sync for Cell<T> {}

impl<T> Cell<T> {
    pub const fn new() -> Self {
        Self(UnsafeCell::new(None))
    }

    /// Install the actor `init` built.
    pub fn install(&self, actor: T) {
        // SAFETY: see the type's invariant; `init` is the first activation, nothing else runs.
        unsafe { *self.0.get() = Some(actor) };
    }

    /// Run one activation on the actor. A call before `init` is a contract violation the Host
    /// never makes; it traps rather than continuing.
    pub fn with<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        // SAFETY: see the type's invariant; `f` is the only user of the actor during the call.
        let slot = unsafe { &mut *self.0.get() };
        match slot {
            Some(a) => f(a),
            None => trap(),
        }
    }
}

impl<T> Default for Cell<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Trap: on the guest target `unreachable`, elsewhere a panic (tests run natively).
pub fn trap() -> ! {
    #[cfg(target_arch = "wasm32")]
    {
        core::arch::wasm32::unreachable()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        panic!("the guest trapped")
    }
}

/// Export an [`Actor`] as the guest of the core world: the bindings' trait is implemented over
/// one cell holding the actor, `now` is stored for [`now`], and `export_guest!` binds it.
#[macro_export]
macro_rules! actor {
    ($t:ty) => {
        #[doc(hidden)]
        pub struct __FlornGuest;

        static __FLORN_ACTOR: $crate::actor::Cell<$t> = $crate::actor::Cell::new();

        impl $crate::bindings::exports::florn::core::actor::Guest for __FlornGuest {
            fn init(
                now: u64,
                actor: u32,
                ports: $crate::__alloc::vec::Vec<$crate::Port>,
                config: $crate::__alloc::vec::Vec<u8>,
            ) {
                $crate::actor::set_now(now);
                let a = <$t as $crate::Actor>::init(actor, &ports, &config);
                __FLORN_ACTOR.install(a);
            }
            fn on_message(now: u64, port: u32, payload: $crate::__alloc::vec::Vec<u8>) {
                $crate::actor::set_now(now);
                __FLORN_ACTOR.with(|a| <$t as $crate::Actor>::on_message(a, port, &payload));
            }
            fn on_timer(now: u64, timer: u32) {
                $crate::actor::set_now(now);
                __FLORN_ACTOR.with(|a| <$t as $crate::Actor>::on_timer(a, timer));
            }
        }

        $crate::bindings::export_guest!(__FlornGuest);
    };
}
