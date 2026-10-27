//! The browser shell (RFC 005 rule 38): Leptos on the main thread. The page's loader
//! (`www/app.js`) initialises this module and calls `shell_start`; the worker module and the
//! test page of 0.1 are this crate's library, which this binary does not link.
#![forbid(unsafe_code)]

#[cfg(target_arch = "wasm32")]
mod shell;

fn main() {}
