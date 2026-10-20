#!/bin/sh
# Run every fuzz target for the seconds given (RFC 006 rule 7). Needs the nightly toolchain and
# cargo-fuzz. Usage: ci/fuzz.sh [seconds]
set -eu
seconds="${1:-60}"
cd "$(dirname "$0")/.."
for target in $(cargo +nightly fuzz list); do
  echo "== $target for ${seconds}s"
  cargo +nightly fuzz run "$target" -- -max_total_time="$seconds" -max_len=65536 2>&1 | tail -3
done
