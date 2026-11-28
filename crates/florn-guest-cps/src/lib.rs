//! The guest SDK for the composed world of RFC 012 rule 22 (`florn:cps@0.1.0`, world
//! `networked-controller`): the core world with the network's `link` import and `router-events`
//! export and the circuit's `signals` import and `controller-events` export. A controller that is
//! on a network and acts on a plant is **one actor with one world** (RFC 002 rule 48; ARCH-010):
//! the network domain delivers its frames to `on-message` and its link changes to `on-link`, the
//! circuit domain delivers its samples to `on-sample`, and neither domain owns the world.
//!
//! A guest of this world implements [`NetworkedController`] and exports it with
//! [`networked_controller!`]; [`link_info`], [`read_input`] and [`set_output`] are the two
//! domains' imports as their own SDKs give them. Everything else, from the declaration to the
//! distributions, is `florn-guest`'s.
//!
//! One crate per world: the bindings of a world are generated once, with the contract-identity
//! section of that world (RFC 012 rule 25).
//!
//! Implemented by PR-50 of `rfcs/handoffs/012-circuit-domain/task-breakdown-pr-plan.md`.
#![no_std]

extern crate alloc;

pub use florn_guest::{Actor, PortId};

/// The bindings of the composed world, as `wit-bindgen` generates them; `export_networked_controller!`
/// exports an implementation of the guest traits. The [`networked_controller!`] macro writes that
/// for you.
pub mod bindings {
    wit_bindgen::generate!({
        path: ["wit/core", "wit/network", "wit/circuit", "wit/cps"],
        world: "florn:cps/networked-controller",
        pub_export_macro: true,
        export_macro_name: "export_networked_controller",
        default_bindings_module: "florn_guest_cps::bindings",
        generate_all,
    });
}

pub use bindings::florn::network::link::InterfaceInfo;

/// The state of one of this actor's interfaces (RFC 008 rule 15), as it was when the activation
/// began.
pub fn link_info(port: PortId) -> InterfaceInfo {
    bindings::florn::network::link::link_info(port)
}

/// The value held on the signal connected to this `signal-in` port, as it was when the activation
/// began, in the unit of that signal (RFC 012 rule 4); `None` when the port is on no signal.
pub fn read_input(port: PortId) -> Option<f64> {
    bindings::florn::circuit::signals::read_input(port)
}

/// Set the value of the signal connected to this `signal-out` port, in the unit of that signal.
/// Recorded as a send effect, applied at commit and held until the next `set_output` on the same
/// port; from `on_message` or `on_timer` it is an input change at a tick that is not a sample
/// (RFC 012 rules 9 and 22). A value that is not finite is a contract violation.
pub fn set_output(port: PortId, value: f64) {
    bindings::florn::circuit::signals::set_output(port, value);
}

/// A controller on a network acting on a plant: an [`Actor`] (whose `on_message` takes the frames
/// of its interfaces) that also learns when an interface's link goes up or down and runs at its
/// sample instants. `index` counts the samples from 0; the instant is `florn_guest::host::now()`.
pub trait NetworkedController: Actor {
    fn on_link(&mut self, port: PortId, up: bool);
    fn on_sample(&mut self, index: u64);
}

/// Export a [`NetworkedController`] as the guest of the composed world: the core's three
/// activations as `florn_guest::actor!` binds them, plus `on-link` and `on-sample`.
#[macro_export]
macro_rules! networked_controller {
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

        impl $crate::bindings::exports::florn::network::router_events::Guest for __FlornGuest {
            fn on_link(now: u64, port: u32, up: bool) {
                ::florn_guest::actor::set_now(now);
                __FLORN_ACTOR.with(|a| <$t as $crate::NetworkedController>::on_link(a, port, up));
            }
        }

        impl $crate::bindings::exports::florn::circuit::controller_events::Guest for __FlornGuest {
            fn on_sample(now: u64, index: u64) {
                ::florn_guest::actor::set_now(now);
                __FLORN_ACTOR.with(|a| <$t as $crate::NetworkedController>::on_sample(a, index));
            }
        }

        $crate::bindings::export_networked_controller!(__FlornGuest);
    };
}
