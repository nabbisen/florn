#!/usr/bin/env bash
# A control that has never failed has not been shown to work. This script plants one violation at
# a time in a scratch copy and checks that the control rejects it, and that the same control
# passes the copy without the violation. It changes nothing in the repository.
#   - ci/fuzz.sh: a fuzz run that exits 77, a list that is empty, a list that fails,
#     the dictionary that goes to the target of its name, the RSS limit of every target, the seeds
#     of the manifest target and of the runner's run.toml and record, and the record's input length
#   - ci/check-rfc-index.sh: an indexed file that is missing, a file that is not indexed
#   - ci/check-rfc-invariants.sh: a status line that disagrees with its folder, no status line,
#     a broken relative link (and a broken link inside a code fence, which is an example)
#   - ci/check-private-paths.sh: a tracked file that names the private folder
#   - ci/check-test-layout.sh: an inline test module (also on one line, behind cfg(all(test, ..)),
#     nested, under fuzz/), a loose #[test], test-only code that is not a module, a test module
#     under another name than tests; and the layout that is right passes
#   - ci/check-deps.py (on a `cargo metadata` file with one dependency planted): a second external
#     dependency of the circuit domain, normal or build, and an external dependency required as a
#     range fail; a development dependency passes (RFC 013 rule 19, control 2)
#   - ci/check-float-flags.sh (RFC 013 rule 19, control 3): `target-cpu`, `target-feature`, `-Z`,
#     an LLVM argument, fast-math and a nightly toolchain, each planted in the file a build reads it
#     from (.cargo/config, a workflow, rust-toolchain.toml, a Cargo.toml profile, a build.rs, a ci/
#     script); a comment that names one passes
#   - the circuit domain's clippy.toml (control 1): a scratch crate with the file as it is and one call
#     of every method it lists fails `cargo clippy -D warnings`, each method named, and the
#     operations the rule allows pass
#   - the browser kit's page opener: a mutated copy that never retries fails its test
set -uo pipefail
cd "$(dirname "$0")/.."
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
tested=0
failed=0

# expect pass|fail "what" command...: run the command and compare its outcome with the wish.
expect() {
  local want=$1 name=$2
  shift 2
  tested=$((tested + 1))
  local got=fail
  if "$@" >"$scratch/out" 2>&1; then got=pass; fi
  if [ "$got" = "$want" ]; then
    echo "ok    $name"
  else
    echo "FAIL  $name (wanted $want, got $got)"
    head -10 "$scratch/out" | sed 's/^/        /'
    failed=$((failed + 1))
  fi
}

# A scratch tree with the scripts and the RFCs, to be spoiled.
fresh() {
  rm -rf "$scratch/t"
  mkdir -p "$scratch/t/ci"
  cp ci/*.sh "$scratch/t/ci/"
  cp -r rfcs "$scratch/t/rfcs"
}

# --- ci/fuzz.sh, with a fake cargo ---------------------------------------------------------
mkdir -p "$scratch/bin"
cat >"$scratch/bin/cargo" <<'EOF'
#!/bin/sh
case "$*" in
  *"fuzz list"*)
    if [ -n "${FAKE_LIST_RC:-}" ]; then exit "$FAKE_LIST_RC"; fi
    printf '%s\n' ${FAKE_TARGETS-alpha beta} ;;
  *"fuzz run"*) [ -z "${FAKE_LOG:-}" ] || echo "$*" >>"$FAKE_LOG"; exit "${FAKE_RC:-0}" ;;
esac
EOF
chmod +x "$scratch/bin/cargo"
# The script runs in a scratch tree of its own: it copies seeds into a corpus directory, and this
# script changes nothing in the repository.
mkdir -p "$scratch/f/ci" "$scratch/f/scenarios" "$scratch/f/fuzz"
cp ci/fuzz.sh ci/fuzz-seeds.sh "$scratch/f/ci/"
cp scenarios/*.toml "$scratch/f/scenarios/"
cp -r fuzz/dictionaries "$scratch/f/fuzz/dictionaries"
fuzz() { env "$@" PATH="$scratch/bin:$PATH" "$scratch/f/ci/fuzz.sh" 1; }
expect pass "fuzz.sh passes when every target runs clean" fuzz FAKE_RC=0
expect fail "fuzz.sh fails when a target exits 77" fuzz FAKE_RC=77
expect fail "fuzz.sh fails when the list of targets is empty" fuzz FAKE_TARGETS=
expect fail "fuzz.sh fails when cargo fuzz list fails" fuzz FAKE_LIST_RC=1
# The dictionary is passed to the target whose name it bears, and to no other.
dict_given() {
  : >"$scratch/args"
  fuzz FAKE_LOG="$scratch/args" FAKE_TARGETS="manifest module" >/dev/null 2>&1 || return 1
  [ "$(grep -c -- '-dict=.*/fuzz/dictionaries/manifest.dict' "$scratch/args")" = 1 ] &&
    [ "$(grep -c -- '-dict=' "$scratch/args")" = 1 ] &&
    ! grep -- 'fuzz run module' "$scratch/args" | grep -q -- '-dict='
}
expect pass "fuzz.sh gives fuzz/dictionaries/manifest.dict to the manifest target only" dict_given
# Every target runs under an RSS limit below libFuzzer's default of 2048 MB.
rss_given() {
  : >"$scratch/args"
  fuzz FAKE_LOG="$scratch/args" FAKE_TARGETS="manifest module" >/dev/null 2>&1 || return 1
  [ "$(grep -c -- '-rss_limit_mb=1024' "$scratch/args")" = 2 ]
}
expect pass "fuzz.sh runs every target under -rss_limit_mb=1024" rss_given
# The manifest target starts from the shipped scenarios (not digests.toml), and no other target does.
seeds_given() {
  rm -rf "$scratch/f/fuzz/corpus"
  fuzz FAKE_TARGETS="manifest module" >/dev/null 2>&1 || return 1
  [ -f "$scratch/f/fuzz/corpus/manifest/echo-pair.toml" ] &&
    [ ! -e "$scratch/f/fuzz/corpus/manifest/digests.toml" ] &&
    [ ! -e "$scratch/f/fuzz/corpus/module" ] &&
    [ "$(ls "$scratch/f/fuzz/corpus/manifest" | wc -l)" = "$(($(ls scenarios/*.toml | wc -l) - 1))" ]
}
expect pass "fuzz.sh copies the shipped scenarios into the manifest corpus and nowhere else" seeds_given

# The seeds of the runner's formats (ci/fuzz-seeds.sh), with a fake runner that writes a ZIP with a
# run.toml in it and a few bytes of checkpoint, and exits with 3 for zero-delay as the real one does.
cat >"$scratch/bin/florn" <<'FAKE'
#!/bin/sh
out=""; ck=""; scen=""
while [ $# -gt 0 ]; do
  case "$1" in
    run | --trace | --payloads) ;;
    --out) out=$2; shift ;;
    --checkpoint-out) ck=$2; shift ;;
    --modules | --seed | --checkpoint-at) shift ;;
    *) scen=$1 ;;
  esac
  shift
done
[ -z "${FAKE_RUNNER_RC:-}" ] || exit "$FAKE_RUNNER_RC"
python3 -I -c 'import sys, zipfile; z = zipfile.ZipFile(sys.argv[1], "w"); z.writestr("run.toml", "x = 1\n"); z.close()' "$out"
[ -z "$ck" ] || printf 'CKPT' >"$ck"
case "$scen" in *zero-delay*) exit 3 ;; esac
FAKE
chmod +x "$scratch/bin/florn"
runner_seeds() {
  rm -rf "$scratch/f/fuzz/corpus"
  fuzz FLORN_RUNNER="$scratch/bin/florn" FAKE_TARGETS="run_toml record checkpoint module" >/dev/null 2>&1 || return 1
  c="$scratch/f/fuzz/corpus"
  [ "$(ls "$c/run_toml" | wc -l)" -gt 5 ] &&
    [ "$(ls "$c/run_toml" | wc -l)" = "$(ls "$c/record" | wc -l)" ] &&
    grep -q '^x = 1' "$c/run_toml/zero-delay-plain.toml" &&
    [ "$(head -c 2 "$c/record/zero-delay-plain.florn-run")" = PK ] &&
    [ ! -e "$c/checkpoint" ] && [ ! -e "$c/module" ] && [ ! -e "$c/manifest" ]
}
expect pass "fuzz.sh seeds run_toml and record from the runner's files, and no other target" runner_seeds
# A runner that fails is a broken gate, not a missing seed.
expect fail "fuzz.sh fails when the runner cannot make the seeds" fuzz FLORN_RUNNER="$scratch/bin/florn" FAKE_RUNNER_RC=1 FAKE_TARGETS="record"
# A seed longer than -max_len would be cut off: the record target, whose files are longer, gets more.
max_len_given() {
  : >"$scratch/args"
  fuzz FLORN_RUNNER="$scratch/bin/florn" FAKE_LOG="$scratch/args" FAKE_TARGETS="record checkpoint module" >/dev/null 2>&1 || return 1
  grep 'fuzz run record' "$scratch/args" | grep -q -- '-max_len=262144 ' &&
    grep 'fuzz run checkpoint' "$scratch/args" | grep -q -- '-max_len=65536 ' &&
    grep 'fuzz run module' "$scratch/args" | grep -q -- '-max_len=65536 '
}
expect pass "fuzz.sh gives the record target the input length its files need, and the others 64 KiB" max_len_given

# --- ci/check-rfc-index.sh -------------------------------------------------------------------
fresh
expect pass "the index check passes the tree" "$scratch/t/ci/check-rfc-index.sh"
fresh
rm "$scratch"/t/rfcs/done/001-*.md
expect fail "the index check fails for an indexed RFC whose file is missing" "$scratch/t/ci/check-rfc-index.sh"
fresh
cp "$scratch"/t/rfcs/done/001-*.md "$scratch/t/rfcs/done/099-unindexed.md"
expect fail "the index check fails for an RFC that is not indexed" "$scratch/t/ci/check-rfc-index.sh"
fresh
rm "$scratch/t/rfcs/handoffs/009-depth/README.md"
expect fail "the index check fails for an indexed handoff package whose file is missing" "$scratch/t/ci/check-rfc-index.sh"

# --- ci/check-rfc-invariants.sh ----------------------------------------------------------------
fresh
expect pass "the invariants pass the tree" "$scratch/t/ci/check-rfc-invariants.sh"
fresh
sed -i '0,/^\*\*Status\.\*\* Implemented/s//**Status.** Proposed/' "$scratch"/t/rfcs/done/001-*.md
expect fail "a done RFC whose status says Proposed fails" "$scratch/t/ci/check-rfc-invariants.sh"
fresh
f=$(ls "$scratch"/t/rfcs/accepted/*.md | head -1)
sed -i '0,/^\*\*Status\.\*\* Accepted/s//**Status.** Implemented/' "$f"
expect fail "an accepted RFC whose status says Implemented fails" "$scratch/t/ci/check-rfc-invariants.sh"
fresh
sed -i '/^\*\*Status\.\*\*/d' "$scratch"/t/rfcs/done/002-*.md
expect fail "an RFC without a status line fails" "$scratch/t/ci/check-rfc-invariants.sh"
fresh
printf '\nSee [nothing](../done/999-nothing.md).\n' >>"$scratch"/t/rfcs/done/003-*.md
expect fail "a broken relative link fails" "$scratch/t/ci/check-rfc-invariants.sh"
fresh
printf '\n```\n[an example](../done/999-nothing.md)\n```\nand `[inline](../x.md)` too.\n' >>"$scratch"/t/rfcs/done/003-*.md
expect pass "a broken link in a code fence or inline code is an example, not a link" "$scratch/t/ci/check-rfc-invariants.sh"

# --- ci/check-private-paths.sh, in a scratch repository -----------------------------------------
rm -rf "$scratch/g"
mkdir -p "$scratch/g/ci"
cp ci/check-private-paths.sh "$scratch/g/ci/"
git init -q "$scratch/g"
printf 'a public file\n' >"$scratch/g/README.md"
(cd "$scratch/g" && git add -A)
expect pass "the private-path check passes a tree that names none" "$scratch/g/ci/check-private-paths.sh"
printf 'see .git-exclude/reviewed/x\n' >"$scratch/g/NOTES.md"
(cd "$scratch/g" && git add NOTES.md)
expect fail "the private-path check fails a tracked file that names the private folder" "$scratch/g/ci/check-private-paths.sh"

# --- ci/check-test-layout.sh, in a scratch tree of crates ------------------------------------------
# A layout that is right: tests in tests.rs files (one nested module inside one is fine), integration
# tests below tests/, a comment that names the attribute, code that is only for non-test builds, and
# a declaration with another attribute between.
layout() {
  rm -rf "$scratch/l"
  mkdir -p "$scratch/l/ci" "$scratch/l/crates/demo/src/parse" "$scratch/l/crates/demo/tests" "$scratch/l/fuzz/fuzz_targets"
  cp ci/check-test-layout.sh "$scratch/l/ci/"
  printf '//! demo\n// #[cfg(test)] in a comment\n#[cfg(not(test))]\npub fn only_outside_tests() {}\n\n#[cfg(test)]\nmod tests;\n' >"$scratch/l/crates/demo/src/lib.rs"
  printf 'use super::*;\n\n#[test]\nfn works() {}\n\n#[cfg(test)]\nmod nested {\n    #[test]\n    fn n() {}\n}\n' >"$scratch/l/crates/demo/src/tests.rs"
  printf 'pub fn parse() {}\n\n#[cfg(test)]\n#[allow(clippy::module_inception)]\nmod tests;\n' >"$scratch/l/crates/demo/src/parse.rs"
  printf 'use super::*;\n#[test]\nfn works() {}\n' >"$scratch/l/crates/demo/src/parse/tests.rs"
  printf 'pub fn inner() {}\n' >"$scratch/l/crates/demo/src/parse/inner.rs"
  printf '#[test]\nfn integration() {}\n' >"$scratch/l/crates/demo/tests/it.rs"
  printf 'fn main() {}\n' >"$scratch/l/fuzz/fuzz_targets/t.rs"
}
# plant <file> <text>: append text to a file of the scratch tree.
plant() { printf '%b' "$2" >>"$scratch/l/$1"; }
layout_check() { "$scratch/l/ci/check-test-layout.sh"; }

expect pass "the test-layout check passes the tree" ci/check-test-layout.sh
layout
expect pass "the test-layout check passes tests.rs files, integration tests, comments and cfg(not(test))" layout_check
layout
plant crates/demo/src/lib.rs '\n#[cfg(test)]\nmod more {\n    #[test]\n    fn t() {}\n}\n'
expect fail "the test-layout check fails an inline test module" layout_check
layout
plant crates/demo/src/lib.rs '\n#[cfg(test)] mod more { }\n'
expect fail "the test-layout check fails an inline test module written on one line" layout_check
layout
plant crates/demo/src/parse/inner.rs '\n#[cfg(all(test, unix))]\nmod tests {\n}\n'
expect fail "the test-layout check fails an inline module behind cfg(all(test, ..)) in a nested file" layout_check
layout
plant crates/demo/src/lib.rs '\n#[test]\nfn loose() {}\n'
expect fail "the test-layout check fails a #[test] function outside a tests file" layout_check
layout
plant crates/demo/src/lib.rs '\n#[cfg(test)]\nuse std::fmt;\n'
expect fail "the test-layout check fails test-only code that is not a module (a use)" layout_check
layout
plant crates/demo/src/parse.rs '\n#[cfg(test)]\nmod parse_tests;\n'
expect fail "the test-layout check fails a test module declared under another name than tests" layout_check
layout
plant fuzz/fuzz_targets/t.rs '\n#[cfg(test)]\nmod tests {\n}\n'
expect fail "the test-layout check fails an inline test module under fuzz/" layout_check

# --- ci/check-deps.py: the circuit domain's direct external dependencies (RFC 013 rule 19) --------
if command -v cargo >/dev/null 2>&1 && command -v python3 >/dev/null 2>&1; then
  cargo metadata --format-version 1 --locked >"$scratch/meta.json" 2>/dev/null
  # Plant a change in one package's direct dependencies: `add NAME REQ KIND PACKAGE` adds one,
  # `loosen NAME PACKAGE` rewrites an exact requirement into a range.
  plant_dep() {
    python3 - "$scratch/meta.json" "$scratch/planted.json" "$@" <<'PY'
import json, sys
src, dst, op = sys.argv[1:4]
meta = json.load(open(src))
if op == "add":
    name, req, kind, package = sys.argv[4:8]
    pkg = next(p for p in meta["packages"] if p["name"] == package)
    pkg["dependencies"].append({"name": name, "req": req, "kind": None if kind == "normal" else kind})
elif op in ("feature", "defaults"):
    name, package, feature = sys.argv[4:7]
    pkg = next(p for p in meta["packages"] if p["name"] == package)
    for d in pkg["dependencies"]:
        if d["name"] == name:
            if op == "feature":
                d["features"] = d.get("features", []) + [feature]
            else:
                d["uses_default_features"] = True
else:
    name, package = sys.argv[4:6]
    pkg = next(p for p in meta["packages"] if p["name"] == package)
    for d in pkg["dependencies"]:
        if d["name"] == name:
            d["req"] = "^" + d["req"].lstrip("=")
json.dump(meta, open(dst, "w"))
PY
  }
  deps_check() { python3 ci/check-deps.py --metadata "$1"; }
  expect pass "check-deps.py passes the workspace as it is" deps_check "$scratch/meta.json"
  plant_dep add toml "=1.1.8" normal florn-domain-circuit
  expect fail "check-deps.py fails a second external dependency of the circuit domain" deps_check "$scratch/planted.json"
  plant_dep add toml "=1.1.8" build florn-domain-circuit
  expect fail "check-deps.py fails a second external dependency of the circuit domain (build)" deps_check "$scratch/planted.json"
  plant_dep add toml "^1" dev florn-domain-circuit
  expect pass "check-deps.py allows a development dependency in the circuit domain" deps_check "$scratch/planted.json"
  plant_dep loosen libm florn-domain-circuit
  expect fail "check-deps.py fails libm required as a range" deps_check "$scratch/planted.json"
  plant_dep feature libm florn-domain-circuit arch
  expect fail "check-deps.py fails libm with a feature (arch lets it call the platform)" deps_check "$scratch/planted.json"
  plant_dep defaults libm florn-domain-circuit x
  expect fail "check-deps.py fails libm with its default features" deps_check "$scratch/planted.json"
  plant_dep loosen blake3 florn-kernel
  expect fail "check-deps.py fails an external dependency of another crate required as a range" deps_check "$scratch/planted.json"
else
  echo "skip  the dependency check's controls: cargo and python3 are needed"
fi

# --- ci/check-float-flags.sh: flags that change float behaviour (RFC 013 rule 19, control 3) -------
flags() { ci/check-float-flags.sh "$scratch/w"; }
fresh_tree() {
  rm -rf "$scratch/w"
  mkdir -p "$scratch/w/ci" "$scratch/w/.github/workflows" "$scratch/w/crates/c"
  cp rust-toolchain.toml "$scratch/w/"
  printf '[workspace]\nmembers = ["crates/c"]\n\n[profile.release]\nopt-level = 3\n' >"$scratch/w/Cargo.toml"
  printf '[package]\nname = "c"\n' >"$scratch/w/crates/c/Cargo.toml"
  printf 'env:\n  RUSTFLAGS: "-D warnings"\njobs:\n  a:\n    steps:\n      - run: cargo test\n' >"$scratch/w/.github/workflows/ci.yml"
  printf 'fn main() { println!("cargo:rerun-if-changed=build.rs"); }\n' >"$scratch/w/crates/c/build.rs"
  printf '#!/bin/sh\ncargo build --release\n' >"$scratch/w/ci/build.sh"
}
fresh_tree
expect pass "check-float-flags.sh passes a tree with no such flag" flags
expect pass "check-float-flags.sh passes the repository as it is" ci/check-float-flags.sh
plant() { # plant <file> <line>: append a line, check, and put the tree back
  fresh_tree
  mkdir -p "$(dirname "$scratch/w/$1")"
  printf '%s\n' "$2" >>"$scratch/w/$1"
}
plant .cargo/config.toml '[build]'
printf 'rustflags = ["-C", "target-cpu=native"]\n' >>"$scratch/w/.cargo/config.toml"
expect fail "check-float-flags.sh fails target-cpu in .cargo/config.toml" flags
plant .cargo/config '[target.x86_64-unknown-linux-gnu]'
printf 'rustflags = ["-C", "target-feature=+fma"]\n' >>"$scratch/w/.cargo/config"
expect fail "check-float-flags.sh fails target-feature in .cargo/config (no extension)" flags
plant crates/c/.cargo/config.toml 'build.rustflags = ["-Ctarget-cpu=skylake"]'
expect fail "check-float-flags.sh fails a flag in a crate's own .cargo/config.toml" flags
plant .github/workflows/ci.yml '  RUSTFLAGS_EXTRA: "-C target-feature=+avx2"'
expect fail "check-float-flags.sh fails target-feature in a workflow" flags
plant .github/workflows/ci.yml '      - run: cargo +nightly -Zbuild-std test'
expect fail "check-float-flags.sh fails a -Z option in a workflow" flags
plant .github/workflows/ci.yml '      - run: RUSTFLAGS="-C llvm-args=-ffp-contract=fast" cargo build'
expect fail "check-float-flags.sh fails an LLVM argument in a workflow" flags
plant rust-toolchain.toml 'x = 1'
sed -i 's/^channel = .*/channel = "nightly-2026-11-01"/' "$scratch/w/rust-toolchain.toml"
expect fail "check-float-flags.sh fails a nightly toolchain in rust-toolchain.toml" flags
plant Cargo.toml 'rustflags = ["-C", "target-cpu=native"]'
expect fail "check-float-flags.sh fails a rustflags line in a Cargo.toml profile" flags
plant crates/c/Cargo.toml '[profile.release]'
printf 'rustflags = ["-Cfast-math"]\n' >>"$scratch/w/crates/c/Cargo.toml"
expect fail "check-float-flags.sh fails fast-math in a crate's Cargo.toml" flags
plant crates/c/build.rs 'fn flags() { println!("cargo:rustc-flags=-C target-cpu=native"); }'
expect fail "check-float-flags.sh fails a build.rs that sets target-cpu" flags
plant ci/build.sh 'export RUSTFLAGS="-C target-feature=+sse4.2"'
expect fail "check-float-flags.sh fails an exported RUSTFLAGS in a ci/ script" flags
plant .cargo/config.toml '[unstable]'
printf 'build-std = ["core"]\n' >>"$scratch/w/.cargo/config.toml"
expect fail "check-float-flags.sh fails an [unstable] table in .cargo/config.toml" flags
plant .github/workflows/ci.yml '      - run: RUSTC_BOOTSTRAP=1 cargo build'
expect fail "check-float-flags.sh fails RUSTC_BOOTSTRAP in a workflow" flags
plant ci/build.sh 'export RUSTC_BOOTSTRAP=1'
expect fail "check-float-flags.sh fails RUSTC_BOOTSTRAP in a ci/ script" flags
plant .cargo/config.toml '[build]'
printf 'rustc-wrapper = "/usr/bin/wrapper"\n' >>"$scratch/w/.cargo/config.toml"
expect fail "check-float-flags.sh fails build.rustc-wrapper" flags
plant .cargo/config.toml '[build]'
printf 'rustc-workspace-wrapper = "/usr/bin/wrapper"\n' >>"$scratch/w/.cargo/config.toml"
expect fail "check-float-flags.sh fails build.rustc-workspace-wrapper" flags
plant .github/workflows/ci.yml '      - run: RUSTC_WRAPPER=sccache cargo build'
expect fail "check-float-flags.sh fails RUSTC_WRAPPER in a workflow" flags
plant Cargo.toml 'codegen-backend = "cranelift"'
expect fail "check-float-flags.sh fails a code-generation backend in a Cargo.toml" flags
plant ci/build.sh '# RUSTFLAGS with target-cpu=native would break NUM-001'
expect pass "check-float-flags.sh passes a comment that names a flag" flags
plant Cargo.toml '# rustflags = ["-C", "target-cpu=native"]   (a comment)'
expect pass "check-float-flags.sh passes a commented flag in a Cargo.toml" flags
plant .github/workflows/ci.yml '      - run: cargo +nightly fuzz run manifest'
expect pass "check-float-flags.sh passes the fuzz job's cargo +nightly fuzz" flags

# --- the circuit domain's clippy.toml (RFC 013 rule 19, control 1) --------------------------------
if command -v cargo >/dev/null 2>&1 && cargo clippy --version >/dev/null 2>&1 && command -v python3 >/dev/null 2>&1; then
  c="$scratch/clippy"
  rm -rf "$c"
  mkdir -p "$c/src"
  cp crates/florn-domain-circuit/clippy.toml "$c/clippy.toml"
  # The toolchain of the repository: the scratch crate is outside it, so rustup is told.
  cp rust-toolchain.toml "$c/"
  libm_req=$(sed -n 's/^libm = { version = "\(=[0-9.]*\)", default-features = false }/\1/p' Cargo.toml)
  printf '[package]\nname = "scratch"\nversion = "0.0.0"\nedition = "2024"\n\n[dependencies]\nlibm = { version = "%s", default-features = false }\n' "$libm_req" >"$c/Cargo.toml"
  # One use of every method and function the file lists, by its path (a path used as a value is a
  # use the lint sees, whatever the method's arity).
  python3 -I - "$c" <<'PY'
import re, sys
root = sys.argv[1]
toml = open(root + "/clippy.toml").read()
paths = re.findall(r'path = "([^"]+)"', toml)
out = ["#![allow(dead_code, unused)]"]
for k, p in enumerate(paths):
    use = p + "::<i32>" if p.endswith("::to_int_unchecked") else p  # the one generic method
    out.append(f"pub fn use_{k}() {{ let _ = {use}; }}")
open(root + "/src/lib.rs", "w").write("\n".join(out) + "\n")
open(root + "/paths", "w").write("\n".join(paths) + "\n")
PY
  clippy_in() { (cd "$c" && cargo clippy --offline -q -- -D warnings); }
  expect fail "the circuit domain's clippy.toml makes cargo clippy fail on the float methods and the libm functions" clippy_in
  # Every listed path is named in the failure.
  (cd "$c" && cargo clippy --offline -q -- -D warnings) >"$scratch/clippy.out" 2>&1
  missing=""
  for m in $(cat "$c/paths"); do
    grep -q "disallowed method \`$m\`" "$scratch/clippy.out" || missing="$missing $m"
  done
  tested=$((tested + 1))
  if [ -z "$missing" ] && [ "$(wc -l <"$c/paths")" -ge 100 ]; then
    echo "ok    the failure names every path of the list ($(wc -l <"$c/paths") of them: f64, f32, libm)"
  else
    echo "FAIL  the failure does not name:$missing"
    failed=$((failed + 1))
  fi
  # The operations the rule allows pass: sqrt, abs, floor, ceil, trunc, copysign, arithmetic, and
  # the three libm functions of the table.
  printf 'pub fn ok(x: f64, y: f64) -> f64 { x.sqrt() + x.abs() + x.floor() + x.ceil() + x.trunc() + x.copysign(y) + x * y - x / y + libm::exp(x) + libm::log(x) + libm::cbrt(x) }\n' >"$c/src/lib.rs"
  expect pass "the operations the rule allows, and the three libm functions of the table, pass cargo clippy" clippy_in
  # The cases the verifier of PR-48 walked round the first list: methods outside the 25.
  for call in 'x.sin_cos()' 'x.asinh()' 'x.acosh()' 'x.atanh()' 'x.round()' 'x.fract()' 'x.signum()' 'x.recip()' 'x.clamp(y, y)' 'x.rem_euclid(y)' 'x.to_degrees()' 'x.to_radians()' 'x.algebraic_add(y)' '(x as f32).exp() as f64'; do
    printf 'pub fn bad(x: f64, y: f64) -> f64 { let _ = %s; x }\n' "$call" >"$c/src/lib.rs"
    expect fail "a std $call placed in the crate fails cargo clippy" clippy_in
  done
  printf 'pub fn bad(x: f64) -> f64 { libm::sin(x) }\n' >"$c/src/lib.rs"
  expect fail "libm::sin, a function the table does not name, fails cargo clippy" clippy_in
  # An `allow` hides nothing: the crate's root forbids the lint, and a forbidden lint cannot be allowed.
  printf '#![forbid(clippy::disallowed_methods)]\n#[allow(clippy::disallowed_methods)]\npub fn bad(x: f64) -> f64 { x.exp() }\n' >"$c/src/lib.rs"
  expect fail "an allow of disallowed_methods under the crate's forbid fails" clippy_in
  printf '#[allow(clippy::disallowed_methods)]\npub fn bad(x: f64) -> f64 { x.exp() }\n' >"$c/src/lib.rs"
  expect pass "an allow of disallowed_methods hides a std exp without the forbid (what the forbid is for)" clippy_in
  tested=$((tested + 1))
  if grep -q '^#!\[forbid(clippy::disallowed_methods)\]' crates/florn-domain-circuit/src/lib.rs; then
    echo "ok    the crate's root forbids clippy::disallowed_methods"
  else
    echo "FAIL  the crate's root does not forbid clippy::disallowed_methods"
    failed=$((failed + 1))
  fi
  # And the file with its list emptied lets every method through: the list is what holds.
  printf 'pub fn bad(x: f64) -> f64 { x.exp() }\n' >"$c/src/lib.rs"
  : >"$c/clippy.toml"
  expect pass "an empty clippy.toml lets a std exp through (the list is what holds)" clippy_in
elif [ -n "${CI:-}" ]; then
  # In CI a control that cannot run is a control that does not exist.
  tested=$((tested + 1))
  failed=$((failed + 1))
  echo "FAIL  the clippy control could not run: cargo with clippy and python3 are needed in CI"
else
  echo "skip  the clippy control: cargo with clippy and python3 are needed"
fi

# --- ci/check-float-methods.py: the list is every method of the toolchain (control 1) ------------
if command -v python3 >/dev/null 2>&1 && command -v rustc >/dev/null 2>&1; then
  docs="$(rustc --print sysroot)/share/doc/rust/html/std"
  if [ -f "$docs/primitive.f64.html" ]; then
    expect pass "check-float-methods.py passes the list as it is" python3 -I ci/check-float-methods.py
    toml=crates/florn-domain-circuit/clippy.toml
    for gone in 'f64::asinh' 'f32::sin_cos' 'f64::algebraic_add' 'libm::sin' 'f64::exp'; do
      grep -v "path = \"$gone\"" "$toml" >"$scratch/trimmed.toml"
      expect fail "check-float-methods.py fails a list without $gone" python3 -I ci/check-float-methods.py --clippy-toml "$scratch/trimmed.toml"
    done
    { cat "$toml"; } | sed 's/^disallowed-methods = \[/disallowed-methods = [\n    { path = "f64::not_a_method", reason = "x" },/' >"$scratch/extra.toml"
    expect fail "check-float-methods.py fails a list with a name that is no method of the toolchain" python3 -I ci/check-float-methods.py --clippy-toml "$scratch/extra.toml"
    # A toolchain that has a method the list lacks: a copy of the documentation with one more.
    mkdir -p "$scratch/docs"
    cp "$docs/primitive.f64.html" "$docs/primitive.f32.html" "$scratch/docs/"
    sed -i 's#<details class="toggle method-toggle" open><summary><section id="method.sqrt" class="method">#<details class="toggle method-toggle" open><summary><section id="method.next_gen_sin" class="method"></section></summary></details><details class="toggle method-toggle" open><summary><section id="method.sqrt" class="method">#' "$scratch/docs/primitive.f64.html"
    expect fail "check-float-methods.py fails a toolchain with a method the list lacks" python3 -I ci/check-float-methods.py --docs "$scratch/docs"
    expect pass "check-float-methods.py passes the documentation copied unchanged" python3 -I ci/check-float-methods.py --docs "$docs"
  elif [ -n "${CI:-}" ]; then
    tested=$((tested + 1))
    failed=$((failed + 1))
    echo "FAIL  the method list's control could not run: rust-docs is a component of rust-toolchain.toml and is missing"
  else
    echo "skip  the method list's control: the toolchain's documentation is not installed"
  fi
fi

# --- the browser kit's page opener ----------------------------------------------------------------
if command -v node >/dev/null 2>&1; then
  expect pass "the opener's retry rule passes its test" node ci/browser/test-open.mjs
  mkdir -p "$scratch/open"
  cp ci/browser/open.js ci/browser/test-open.mjs "$scratch/open/"
  sed -i 's/if (!retry || !isTransient(err, err.pageCrashed)) throw err;/throw err;/' "$scratch/open/open.js"
  expect fail "an opener that never retries fails the test" node "$scratch/open/test-open.mjs"
else
  echo "skip  the opener's test: node is not installed"
fi

echo
echo "$tested controls tested, $failed wrong"
[ "$failed" -eq 0 ]
