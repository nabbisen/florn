//! The guest SDK for the device world of RFC 012 rule 19 (`florn:circuit@0.1.0`, world `device`):
//! the core world of `florn-guest` with the compound activation the solver plays into the guest
//! inside a step. A guest device model implements [`Device`], declares itself with
//! `florn_guest::declare!` under `world: "florn:circuit/device@0.1.0"` with one port kind of type
//! `circuit/pin` and a `[circuit]` table `kind = "device"`, and exports with [`device!`].
//!
//! The solver calls the guest `begin_step`, `evaluate` once per Newton iteration and then
//! `accept_step` or `reject_step` (RFC 012 rules 19 to 21). [`Trial`] keeps what the guest carries
//! from step to step apart from what a step tries, so that `reject_step` is a drop; [`pnjlim`] is
//! SPICE3's junction limiter with the literals of RFC 013 rule 8, the same bits as the built-in
//! diode's; [`math`] is the one place that calls `libm` (RFC 013 rule 19) and has no float method the rule disallows.
//!
//! One crate per world: the bindings of a world are generated once, with the contract-identity
//! section of that world (RFC 012 rule 25). The bindings export `cabi_post_evaluate`, the
//! post-return of the list `evaluate` returns (finding F-16).
//!
//! Implemented by PR-49 of `rfcs/handoffs/012-circuit-domain/task-breakdown-pr-plan.md`.
#![no_std]

extern crate alloc;

use alloc::vec::Vec;

pub mod math;

pub use florn_guest::{Actor, PortId};
pub use math::{VT, pnjlim, vcrit};

/// The bindings of the device world, as `wit-bindgen` generates them; `export_device!` exports an
/// implementation of both guest traits. The [`device!`] macro writes that for you.
pub mod bindings {
    wit_bindgen::generate!({
        path: ["wit/core", "wit/circuit"],
        world: "florn:circuit/device",
        pub_export_macro: true,
        export_macro_name: "export_device",
        default_bindings_module: "florn_guest_device::bindings",
        generate_all,
    });
}

/// What a device carries between steps, kept apart from what a step tries (RFC 012 rule 25): the
/// **committed** state, which only `accept_step` changes, and the **trial** state a step works on.
/// `begin` starts a step from the committed state, `accept` makes the trial state the committed
/// one, and `reject` drops it, so that a rejected step leaves the committed state as it was.
#[derive(Clone, Debug, Default)]
pub struct Trial<S: Clone> {
    committed: S,
    trial: S,
}

impl<S: Clone> Trial<S> {
    pub fn new(state: S) -> Self {
        Self {
            trial: state.clone(),
            committed: state,
        }
    }

    /// The state as of the last accepted step.
    pub fn committed(&self) -> &S {
        &self.committed
    }

    /// The state of the step in hand.
    pub fn trial(&mut self) -> &mut S {
        &mut self.trial
    }

    /// A step begins: the trial state is the committed one.
    pub fn begin(&mut self) {
        self.trial = self.committed.clone();
    }

    /// The step is accepted: the trial state is committed.
    pub fn accept(&mut self) {
        self.committed = self.trial.clone();
    }

    /// The step is rejected: the trial state is dropped.
    pub fn reject(&mut self) {
        self.trial = self.committed.clone();
    }
}

/// A device model: an [`Actor`] the solver calls inside a step. `start` is the tick the step begins
/// at and `h` its length in seconds (`0.0` for a solve of the initial analysis); `trial` holds the
/// voltages of the device's pins in ascending port identity, and `evaluate` returns the `n` currents
/// into the pins, the `n × n` Jacobian row-major, and one more entry, `1.0` when the device limited
/// a junction and `0.0` when it did not: `n + n² + 1` values (RFC 012 rule 19). A call that sends,
/// or sets or cancels a timer, is a contract violation.
pub trait Device: Actor {
    fn begin_step(&mut self, start: u64, h: f64);
    fn evaluate(&mut self, trial: &[f64]) -> Vec<f64>;
    fn accept_step(&mut self, solution: &[f64]);
    fn reject_step(&mut self);
}

/// Export a [`Device`] as the guest of the device world: the core's three activations as
/// `florn_guest::actor!` binds them (the Host calls `init` only), plus the four calls of the
/// compound activation.
#[macro_export]
macro_rules! device {
    ($t:ty) => {
        #[doc(hidden)]
        pub struct __FlornGuest;

        static __FLORN_ACTOR: ::florn_guest::actor::Cell<$t> = ::florn_guest::actor::Cell::new();

        impl $crate::bindings::exports::florn::core::actor::Guest for __FlornGuest {
            fn init(
                now: u64,
                actor: u32,
                ports: ::florn_guest::__alloc::vec::Vec<$crate::bindings::florn::core::types::Port>,
                config: ::florn_guest::__alloc::vec::Vec<u8>,
            ) {
                ::florn_guest::actor::set_now(now);
                let ports: ::florn_guest::__alloc::vec::Vec<::florn_guest::Port> = ports
                    .into_iter()
                    .map(|p| ::florn_guest::Port {
                        id: p.id,
                        kind: p.kind,
                        name: p.name,
                    })
                    .collect();
                let a = <$t as ::florn_guest::Actor>::init(actor, &ports, &config);
                __FLORN_ACTOR.install(a);
            }
            fn on_message(now: u64, port: u32, payload: ::florn_guest::__alloc::vec::Vec<u8>) {
                ::florn_guest::actor::set_now(now);
                __FLORN_ACTOR.with(|a| <$t as ::florn_guest::Actor>::on_message(a, port, &payload));
            }
            fn on_timer(now: u64, timer: u32) {
                ::florn_guest::actor::set_now(now);
                __FLORN_ACTOR.with(|a| <$t as ::florn_guest::Actor>::on_timer(a, timer));
            }
        }

        impl $crate::bindings::exports::florn::circuit::device_model::Guest for __FlornGuest {
            fn begin_step(start: u64, h: f64) {
                // The guest's `now()` inside the activation is the step's start (RFC 012 rule 20).
                ::florn_guest::actor::set_now(start);
                __FLORN_ACTOR.with(|a| <$t as $crate::Device>::begin_step(a, start, h));
            }
            fn evaluate(
                trial: ::florn_guest::__alloc::vec::Vec<f64>,
            ) -> ::florn_guest::__alloc::vec::Vec<f64> {
                __FLORN_ACTOR.with(|a| <$t as $crate::Device>::evaluate(a, &trial))
            }
            fn accept_step(solution: ::florn_guest::__alloc::vec::Vec<f64>) {
                __FLORN_ACTOR.with(|a| <$t as $crate::Device>::accept_step(a, &solution));
            }
            fn reject_step() {
                __FLORN_ACTOR.with(|a| <$t as $crate::Device>::reject_step(a));
            }
        }

        $crate::bindings::export_device!(__FlornGuest);
    };
}

#[cfg(test)]
mod tests;
