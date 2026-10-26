//! The guest SDK (RFC 002 §8): bindings generated from the contract of record, one declaration
//! that writes the `florn.manifest` section and the config decoder, the distributions of RFC 001
//! rule 30 over `random`, encoders for declared telemetry channels, `now()`, and the `std` and
//! `no_std` profiles with an allocator and a panic handler that traps.
//!
//! The SDK is not the contract (RFC 002 rule 51): a guest written without it that implements the
//! world and carries the manifest conforms. `unsafe` appears here in the generated bindings, in
//! the manifest's link section, and in the one cell that holds the actor; each is stated where
//! it is.
//!
//! Implemented by PR-10 of `rfcs/handoffs/005-host-architecture/task-breakdown-pr-plan.md`.
#![no_std]
#![cfg_attr(not(feature = "std"), allow(unused_imports))]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

/// `alloc`, for the macros' expansions.
#[doc(hidden)]
pub extern crate alloc as __alloc;

pub mod actor;
pub mod manifest;
pub mod random;
pub mod telemetry;

/// The bindings of the core world, as `wit-bindgen` generates them; `export_guest!` exports an
/// implementation of `exports::florn::core::actor::Guest`. The `actor!` macro writes that for you.
pub mod bindings {
    wit_bindgen::generate!({
        path: "wit",
        world: "florn:core/guest",
        pub_export_macro: true,
        export_macro_name: "export_guest",
        default_bindings_module: "florn_guest::bindings",
        generate_all,
    });
}

pub use actor::{Actor, now};
pub use bindings::florn::core::types::Port;
pub use manifest::Reader;
pub use random::{Random, Source};

/// The identities of the contract, by their WIT names.
pub type Tick = u64;
pub type ActorId = u32;
pub type PortId = u32;
pub type TimerId = u32;
pub type ChannelId = u32;

/// The host interface (RFC 002 §5): every call records an effect that is committed only if the
/// activation returns normally.
pub mod host {
    use crate::bindings::florn::core::host;

    /// Ask for delivery of `payload` through `port` of this actor (rule 23).
    pub fn send(port: crate::PortId, payload: &[u8]) {
        host::send(port, payload);
    }
    /// `on-timer(timer)` after `delay` ticks; a pending timer of the same identity is replaced (rules 26, 27).
    pub fn set_timer(timer: crate::TimerId, delay: crate::Tick) {
        host::set_timer(timer, delay);
    }
    /// Drop a pending timer; nothing happens if none is pending (rule 28).
    pub fn cancel_timer(timer: crate::TimerId) {
        host::cancel_timer(timer);
    }
    /// The next value of this actor's stream (rule 30). The distributions of [`crate::Random`]
    /// are built on it.
    pub fn random() -> u64 {
        host::random()
    }
    /// Report on a declared channel, in its record layout (rule 32).
    pub fn report(channel: crate::ChannelId, payload: &[u8]) {
        host::report(channel, payload);
    }
    /// Text for the author on channel 0 (rule 33).
    pub fn log(text: &str) {
        host::report(0, text.as_bytes());
    }
}

/// The `no_std` profile's allocator (RFC 002 rule 50), on the guest target only.
#[cfg(all(feature = "no-std", not(feature = "std"), target_arch = "wasm32"))]
#[global_allocator]
static ALLOCATOR: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;

/// The `no_std` profile's panic handler: a panic is a trap, and the activation faults.
#[cfg(all(feature = "no-std", not(feature = "std"), target_arch = "wasm32"))]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    core::arch::wasm32::unreachable()
}
