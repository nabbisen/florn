#!/usr/bin/env bash
# The browser application (RFC 005 rule 38): the Host's worker module and the shell, both for
# wasm32, the shell bound by wasm-bindgen into the page's files. The outputs are git-ignored;
# CI and a person build them the same way.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --locked --release --target wasm32-unknown-unknown -p florn-ui-web
cp target/wasm32-unknown-unknown/release/florn_ui_web.wasm crates/florn-ui-web/www/florn_host.wasm
cargo build --locked --release --target wasm32-unknown-unknown -p florn-ui-web --bin florn-ui-web --features shell
wasm-bindgen --target web --no-typescript --out-dir crates/florn-ui-web/www --out-name florn_shell target/wasm32-unknown-unknown/release/florn-ui-web.wasm
ls -la crates/florn-ui-web/www/florn_host.wasm crates/florn-ui-web/www/florn_shell.js crates/florn-ui-web/www/florn_shell_bg.wasm
