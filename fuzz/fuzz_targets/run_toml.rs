//! `run.toml` of a run record (RFC 005 rule 29): any text.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        if let Some(r) = florn_host::RunToml::parse(text) {
            let _ = r.to_toml();
        }
    }
});
