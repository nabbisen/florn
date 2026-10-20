//! The run record reader (RFC 005 rule 29; RFC 006 section 7): any bytes as a record.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = florn_host::record::read(data);
});
