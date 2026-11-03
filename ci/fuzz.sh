#!/bin/sh
# Run every fuzz target for the seconds given (RFC 006 rule 7). Needs the nightly toolchain and
# cargo-fuzz. Exits non-zero when a target crashes, when `cargo fuzz list` fails, or when it
# lists no target: a gate that cannot fail is not a gate. A crash leaves its input under
# fuzz/artifacts/<target>/, which CI uploads. A file fuzz/dictionaries/<target>.dict is given to
# that target as its dictionary, and the manifest target starts from the shipped scenarios. Every target runs under an RSS limit (libFuzzer's default of 2048
# MB lets an allocation of 1.9 GB, the size PR-32's parser bug asked for, go unseen): the largest
# peak of the eight targets over 60 seconds each was 645 MB (manifest; checkpoint 499, control 240,
# module 541, package 500, record 589, run_toml 515, trace 534; 2026-11-03), so 1024 leaves 1.6 times
# the largest and still reports an allocation of half a gigabyte or more as a finding. Measure again
# when a target is added: run each with -print_final_stats=1 and read stat::peak_rss_mb.
# Usage: ci/fuzz.sh [seconds]
set -eu
seconds="${1:-60}"
rss_limit_mb=1024
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
  # A dictionary of the target's name, if there is one, gives the fuzzer the shapes of its input.
  dict=""
  if [ -f "fuzz/dictionaries/$target.dict" ]; then dict="-dict=$PWD/fuzz/dictionaries/$target.dict"; fi
  # The manifest target starts from the shipped scenarios, which are valid manifests: from nothing
  # it spends its time on the TOML grammar and seldom reaches a quantity (PR-34: the allocation of
  # PR-32 was found in one of two seeded runs of 180 seconds and in none of the unseeded). They are
  # copied into the target's corpus directory, the one CI caches; never give a project directory as
  # a second corpus argument, for libFuzzer writes new inputs to the first directory it is given.
  # This makes a find likelier; it promises none.
  if [ "$target" = manifest ]; then
    mkdir -p "fuzz/corpus/$target"
    for seed in scenarios/*.toml; do
      [ "$(basename "$seed")" = digests.toml ] || cp "$seed" "fuzz/corpus/$target/"
    done
  fi
  # shellcheck disable=SC2086
  if cargo +nightly fuzz run "$target" -- -max_total_time="$seconds" -max_len=65536 -rss_limit_mb="$rss_limit_mb" $dict >"$log" 2>&1; then
    tail -3 "$log"
  else
    rc=$?
    tail -40 "$log"
    echo "FUZZ FAILURE: $target exited $rc"
    status=1
  fi
done
exit "$status"
