//! The module validator and transformation (RFC 003 sections 2 and 3): any bytes as a module.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = florn_admission::Admission::default().admit(
        "fuzz",
        data,
        &florn_admission::ClassLimits::defaults(),
    );
});
