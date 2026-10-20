//! The package reader (RFC 004 section 6; RFC 006 section 7): any bytes as a ZIP archive.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = florn_model::package::Package::read(data, &florn_model::HostMaxima::native());
});
