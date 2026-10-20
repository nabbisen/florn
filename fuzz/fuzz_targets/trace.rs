//! The trace record decoder (RFC 005 rule 20): any bytes; a decoded record re-encodes to itself.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Some(r) = florn_kernel::trace::decode(data) {
        assert_eq!(florn_kernel::trace::decode(&florn_kernel::trace::encode(&r)), Some(r));
    }
});
