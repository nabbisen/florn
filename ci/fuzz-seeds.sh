#!/bin/sh
# Seeds for the fuzz targets that read the runner's files (RFC 006 rule 7): valid `run.toml` and
# run record files, made by the runner from the shipped scenarios (no checkpoint files: see
# ci/fuzz.sh). A fuzzer that
# starts from nothing spends its time on the container's grammar; started from a valid file it
# reaches the fields. The files are written to <out>/run_toml and <out>/record;
# ci/fuzz.sh copies them into the targets' corpus directories (never give a directory as a
# second corpus argument: libFuzzer writes new inputs to the first one it is given).
#
# A run that exits with 0 is a seed; so is one that exits with 3, the run fault of RFC 001 rule 13
# (`zero-delay` ends so by design). Any other exit, or a file that is empty, fails the script: a
# seed that cannot be made is a broken gate, and the fuzz job must say so and not run on without.
# A file larger than the target's `-max_len` (see ci/fuzz.sh) would be cut off and be no file at
# all, so it is not kept.
#
# Usage: ci/fuzz-seeds.sh <runner> <scenarios directory> <out directory>
set -eu
runner="$1"
scenarios="$2"
out="$3"
record_max=262144

mkdir -p "$out/run_toml" "$out/record"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fail() {
  echo "SEED FAILURE: $*"
  exit 1
}

# run <scenario> <out file> [more arguments]: a run of a shipped scenario, seed 1.
run() {
  scenario="$1"
  file="$2"
  shift 2
  rc=0
  "$runner" run "$scenarios/$scenario.toml" --modules "$scenarios/modules" --seed 1 --out "$file" "$@" >/dev/null 2>&1 || rc=$?
  case "$rc" in
    0 | 3) ;;
    *) fail "$scenario: the runner exited with $rc" ;;
  esac
  [ -s "$file" ] || fail "$scenario: $file is empty"
}

size() {
  wc -c <"$1" | tr -d ' '
}

# Run records, and the `run.toml` inside each (a record is a ZIP).
for name in echo-pair dv-triangle net-blackhole net-link-down hostile-loop zero-delay neighbour net-delay; do
  for variant in plain full; do
    record="$out/record/$name-$variant.florn-run"
    if [ "$variant" = full ]; then
      run "$name" "$record" --trace --payloads
    else
      run "$name" "$record"
    fi
    if [ "$(size "$record")" -gt "$record_max" ]; then
      rm -f "$record"
      echo "seed $name-$variant: larger than $record_max bytes, not kept"
      continue
    fi
    python3 -I -c 'import sys, zipfile; sys.stdout.buffer.write(zipfile.ZipFile(sys.argv[1]).read("run.toml"))' \
      "$record" >"$out/run_toml/$name-$variant.toml" || fail "$name-$variant: no run.toml in the record"
    [ -s "$out/run_toml/$name-$variant.toml" ] || fail "$name-$variant: run.toml is empty"
  done
done

count() {
  set -- "$1"/*
  echo "$#"
}
echo "seeds: $(count "$out/run_toml") run.toml, $(count "$out/record") records"
