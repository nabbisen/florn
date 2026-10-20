//! The scenario manifest parser (RFC 004 rule 39): any text, never a panic, bounded memory.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = florn_model::Manifest::parse(text, &florn_model::HostMaxima::native());
    }
});
