#!/bin/sh
# Publish the consumable crates and their chain to crates.io in dependency order (the owner's
# token; `cargo login` first). `cargo publish --workspace` orders the crates and waits for the
# index between them; the crates with `publish = false` (the kit, the browser crates, the UI,
# the test guests) are skipped. Usage: ci/publish.sh [--dry-run]
set -eu
cd "$(dirname "$0")/.."
cargo publish --workspace --locked "$@"
