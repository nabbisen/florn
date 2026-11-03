#!/bin/sh
# Run every fuzz target for the seconds given (RFC 006 rule 7). Needs the nightly toolchain and
# cargo-fuzz. Exits non-zero when a target crashes, when `cargo fuzz list` fails, or when it
# lists no target: a gate that cannot fail is not a gate. A crash leaves its input under
# fuzz/artifacts/<target>/, which CI uploads.
#
# Dictionaries: a file fuzz/dictionaries/<target>.dict is given to that target.
#
# Seeds: a fuzzer that starts from nothing spends its time on a format's grammar and seldom
# reaches a field (PR-34: the allocation of PR-32 was found in one of two seeded runs of 180
# seconds and in none of the unseeded). The manifest target starts from the shipped scenarios; the
# run.toml and record targets from files the runner makes of them (ci/fuzz-seeds.sh).
# Seeds are copied into the target's corpus directory, the one CI caches; never give a project
# directory as a second corpus argument, for libFuzzer writes new inputs to the first directory it
# is given. Seeds make a find likelier; they promise none. The runner is built when one of those
# two targets is about to run, or taken from $FLORN_RUNNER. The checkpoint target has no seeds yet:
# a checkpoint holds every actor's memory, so a real one is 1.2 to 5 MB, and from such seeds the
# corpus the job caches grew by 350 MB in two minutes (PR-34a; RSS 752 MB, 83 runs a second).
#
# Input length: 64 KiB, except for the run record, whose valid files reach 112 KiB (a seed longer
# than -max_len is cut off, and a cut ZIP is no file): 256 KiB.
#
# RSS limit: every target runs under one (libFuzzer's default of 2048 MB lets an allocation of
# 1.9 GB, the size PR-32's parser bug asked for, go unseen). Measured over 60 seconds each on
# 2026-11-03 the largest peak of the eight targets was 645 MB (manifest; checkpoint 499, control
# 240, module 541, package 500, record 589, run_toml 515, trace 534), so 1024 leaves 1.6 times the
# largest and still reports an allocation of half a gigabyte or more as a finding. Measure again
# when a target is added or its input length changes: run each with -print_final_stats=1 and read
# stat::peak_rss_mb.
#
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
seeds=""
trap 'rm -rf "$log" "$seeds"' EXIT
status=0
for target in $targets; do
  echo "== $target for ${seconds}s"
  dict=""
  if [ -f "fuzz/dictionaries/$target.dict" ]; then dict="-dict=$PWD/fuzz/dictionaries/$target.dict"; fi
  max_len=65536
  case "$target" in
    record) max_len=262144 ;;
  esac
  case "$target" in
    manifest)
      mkdir -p "fuzz/corpus/$target"
      for seed in scenarios/*.toml; do
        [ "$(basename "$seed")" = digests.toml ] || cp "$seed" "fuzz/corpus/$target/"
      done
      ;;
    run_toml | record)
      if [ -z "$seeds" ]; then
        runner="${FLORN_RUNNER:-}"
        if [ -z "$runner" ]; then
          cargo build --locked --release -p florn
          runner="$PWD/target/release/florn"
        fi
        seeds=$(mktemp -d)
        ci/fuzz-seeds.sh "$runner" "$PWD/scenarios" "$seeds"
      fi
      mkdir -p "fuzz/corpus/$target"
      cp "$seeds/$target"/* "fuzz/corpus/$target/"
      ;;
  esac
  # shellcheck disable=SC2086
  if cargo +nightly fuzz run "$target" -- -max_total_time="$seconds" -max_len="$max_len" -rss_limit_mb="$rss_limit_mb" $dict >"$log" 2>&1; then
    tail -3 "$log"
  else
    rc=$?
    tail -40 "$log"
    echo "FUZZ FAILURE: $target exited $rc"
    status=1
  fi
done
exit "$status"
