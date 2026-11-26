//! The guest SDK for the controller world of RFC 012 rule 18 (`florn:circuit@0.1.0`): the core
//! world with the circuit's `signals` import (`read-input`, `set-output`) and the `on-sample`
//! export. A guest of this world implements [`Controller`] and exports it with [`controller!`];
//! [`read_input`] answers from the activation's starting state (RFC 002 rule 47) and
//! [`set_output`] is recorded as a `send` on the signal port (RFC 012 rule 11), applied at commit.
//! Everything else, from the declaration to the distributions, is `florn-guest`'s.
//!
//! One crate per world: the bindings of a world are generated once, with the contract-identity
//! section of that world, and two worlds in one crate would generate the included core interface
//! twice (RFC 012 rule 25).
//!
//! Implemented by PR-47 of `rfcs/handoffs/012-circuit-domain/task-breakdown-pr-plan.md`.
#![no_std]

extern crate alloc;

pub use florn_guest::{Actor, PortId};

/// The bindings of the controller world, as `wit-bindgen` generates them; `export_controller!`
/// exports an implementation of both guest traits. The [`controller!`] macro writes that for you.
pub mod bindings {
    wit_bindgen::generate!({
        path: ["wit/core", "wit/circuit"],
        world: "florn:circuit/controller",
        pub_export_macro: true,
        export_macro_name: "export_controller",
        default_bindings_module: "florn_guest_circuit::bindings",
        generate_all,
    });
}

/// The value held on the signal connected to this `signal-in` port, as it was when the activation
/// began, in the unit of that signal (RFC 012 rule 4); `None` when the port is on no signal.
pub fn read_input(port: PortId) -> Option<f64> {
    bindings::florn::circuit::signals::read_input(port)
}

/// Set the value of the signal connected to this `signal-out` port, in the unit of that signal.
/// Recorded as a send effect, applied at commit and held until the next `set_output` on the same
/// port. A value that is not finite is a contract violation: the activation faults.
pub fn set_output(port: PortId, value: f64) {
    bindings::florn::circuit::signals::set_output(port, value);
}

/// A sampled controller: an [`Actor`] that runs at its sample instants. `index` counts the samples
/// of this actor from 0; the instant is `florn_guest::host::now()`.
pub trait Controller: Actor {
    fn on_sample(&mut self, index: u64);
}

/// Export a [`Controller`] as the guest of the controller world: the core's three activations as
/// `florn_guest::actor!` binds them, plus `on-sample`.
#[macro_export]
macro_rules! controller {
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

        impl $crate::bindings::exports::florn::circuit::controller_events::Guest for __FlornGuest {
            fn on_sample(now: u64, index: u64) {
                ::florn_guest::actor::set_now(now);
                __FLORN_ACTOR.with(|a| <$t as $crate::Controller>::on_sample(a, index));
            }
        }

        $crate::bindings::export_controller!(__FlornGuest);
    };
}
