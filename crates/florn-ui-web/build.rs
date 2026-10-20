//! On the guest target, the worker module exports its function table and lets it grow: the
//! worker places every guest export in it, and the Host calls through it (RFC 005 rule 7).
fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        println!("cargo::rustc-link-arg-cdylib=--export-table");
        println!("cargo::rustc-link-arg-cdylib=--growable-table");
    }
}
