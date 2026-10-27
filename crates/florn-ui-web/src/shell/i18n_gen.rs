//! The catalogue of labels (RFC 007 rules 14 to 17), generated at compile time from
//! `locales/en.json` and `locales/ja.json` (`[package.metadata.leptos-i18n]`): every key
//! checked, a key missing in either language a build error.
#![allow(unused_imports, dead_code, deprecated, clippy::all)]
leptos_i18n::load_locales!();
