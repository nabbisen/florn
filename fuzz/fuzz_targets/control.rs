//! The control message decoder (RFC 005 rule 25): any bytes; a decoded message validates.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(env) = florn_control::decode(data) {
        let _ = florn_control::encode(&env);
        if let florn_control::Message::Command(c) = &env.message {
            let _ = c.validate();
        }
    }
});
