#!/bin/sh
# Run every fuzz target for the seconds given (RFC 006 rule 7). Needs the nightly toolchain and
# cargo-fuzz. Exits non-zero when a target crashes, when `cargo fuzz list` fails, or when it
# lists no target: a gate that cannot fail is not a gate. A crash leaves its input under
# fuzz/artifacts/<target>/, which CI uploads. Usage: ci/fuzz.sh [seconds]
set -eu
seconds="${1:-60}"
cd "$(dirname "$0")/.."
targets=$(cargo +nightly fuzz list)
if [ -z "$targets" ]; then
  echo "FUZZ FAILURE: cargo fuzz list printed no target"
  exit 1
fi
log=$(mktemp)
trap 'rm -f "$log"' EXIT
status=0
for target in $targets; do
  echo "== $target for ${seconds}s"
  if cargo +nightly fuzz run "$target" -- -max_total_time="$seconds" -max_len=65536 >"$log" 2>&1; then
    tail -3 "$log"
  else
    rc=$?
    tail -40 "$log"
    echo "FUZZ FAILURE: $target exited $rc"
    status=1
  fi
done
exit "$status"
