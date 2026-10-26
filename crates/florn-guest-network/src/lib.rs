//! The guest SDK for the router world of RFC 008 §6 (`florn:network@0.1.0`): the core world
//! with the network's `link` import and `router-events` export. A guest of this world
//! implements [`Router`] and exports it with [`router!`]; [`link_info`] answers from the
//! activation's starting state (RFC 002 rule 47). Everything else, from the declaration to the
//! distributions, is `florn-guest`'s.
//!
//! One crate per world: the bindings of a world are generated once, with the contract-identity
//! section of that world, and two worlds in one crate would generate the included core
//! interface twice.
//!
//! Implemented by PR-16 of `rfcs/handoffs/008-network-domain/task-breakdown-pr-plan.md`.
#![no_std]

extern crate alloc;

pub use florn_guest::{Actor, PortId};

/// The bindings of the router world, as `wit-bindgen` generates them; `export_router!` exports
/// an implementation of both guest traits. The [`router!`] macro writes that for you.
pub mod bindings {
    wit_bindgen::generate!({
        path: ["wit/core", "wit/network"],
        world: "florn:network/router",
        pub_export_macro: true,
        export_macro_name: "export_router",
        default_bindings_module: "florn_guest_network::bindings",
        generate_all,
    });
}

pub use bindings::florn::network::link::InterfaceInfo;

/// The state of one of this actor's interfaces (RFC 008 rule 15), as it was when the
/// activation began: enabled and the node up, the per-port cost, the mtu, the bandwidth.
pub fn link_info(port: PortId) -> InterfaceInfo {
    bindings::florn::network::link::link_info(port)
}

/// A router: an [`Actor`] that also learns when an interface's link goes up or down.
pub trait Router: Actor {
    fn on_link(&mut self, port: PortId, up: bool);
}

/// Export a [`Router`] as the guest of the router world: the core's three activations as
/// `florn_guest::actor!` binds them, plus `on-link`.
#[macro_export]
macro_rules! router {
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
                __FLORN_ACTOR.with(|a| <$t as $crate::Router>::on_link(a, port, up));
            }
        }

        $crate::bindings::export_router!(__FlornGuest);
    };
}
