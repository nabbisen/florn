//! The checkpoint decoder (RFC 003 rule 22): any bytes.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = florn_host::Checkpoint::decode(data);
    let _ = florn_host::ActorState::decode(data);
});
