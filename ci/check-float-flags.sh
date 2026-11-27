#!/usr/bin/env bash
# RFC 013 rule 19, control 3: no target-feature or code-generation flag changes float behaviour
# anywhere in the workspace. Results that are the same bits on every engine (NUM-001, NUM-002)
# need the same arithmetic: no `target-cpu` or `target-feature` (fused multiply-add, SIMD, a
# different float unit), no `-Z` option (nightly's unstable code generation), no LLVM argument and
# no fast-math or contraction flag, wherever a build could be given one:
#   - `.cargo/config` and `.cargo/config.toml` at any depth (build.rustflags, target.*.rustflags)
#   - the workflows (`RUSTFLAGS`, `RUSTDOCFLAGS`, a `cargo -Z...` line)
#   - `rust-toolchain` and `rust-toolchain.toml` (a nightly toolchain is what lets `-Z` work)
#   - every `Cargo.toml` (profiles, `rustflags`)
#   - the scripts of `ci/` and every `build.rs` (a `cargo:rustc-flags`, an exported `RUSTFLAGS`)
# The fuzz job's `cargo +nightly fuzz` is not a flag: cargo-fuzz builds its own targets, which no
# result is compared from. A comment (a line that begins with `#` or `//` after any space) is not a flag.
# Usage: ci/check-float-flags.sh [root]   (default: the repository)
set -uo pipefail
root=${1:-$(cd "$(dirname "$0")/.." && pwd)}
cd "$root" || exit 2

# The flags and where they are written: one token per alternative.
forbidden='target[-_]cpu|target[-_]feature|llvm[-_]args|fast[-_]math|ffast|fp[-_]contract|fp-model|unsafe[-_]fp|(^|[^A-Za-z0-9_-])-Z[A-Za-z ]|unstable-options|channel *= *"nightly'

files=$(
  {
    find . \( -path ./target -o -path '*/node_modules' -o -path ./.git \) -prune -o \
      \( -name config -o -name config.toml \) -path '*/.cargo/*' -print
    find . \( -path ./target -o -path '*/node_modules' -o -path ./.git \) -prune -o \
      \( -name Cargo.toml -o -name rust-toolchain -o -name rust-toolchain.toml -o -name build.rs \) -print
    find .github -type f \( -name '*.yml' -o -name '*.yaml' \) 2>/dev/null
    find ci -maxdepth 2 -type f \( -name '*.sh' -o -name '*.py' -o -name '*.js' \) -not -path '*/node_modules/*' 2>/dev/null
  } | sort -u
)

bad=0
for f in $files; do
  # This script and the script that tests it name the flags on purpose.
  case "$f" in ./ci/check-float-flags.sh | ./ci/test-gates.sh | ci/check-float-flags.sh | ci/test-gates.sh) continue ;; esac
  hits=$(grep -nE "$forbidden" "$f" | grep -vE '^[0-9]+:[[:space:]]*(#|//)' || true)
  if [ -n "$hits" ]; then
    while IFS= read -r line; do
      echo "$f:$line"
    done <<<"$hits"
    bad=1
  fi
done
if [ "$bad" -ne 0 ]; then
  echo "error: a flag that can change float behaviour is set (RFC 013 rule 19, control 3)" >&2
  exit 1
fi
echo "no target-feature, target-cpu, -Z, LLVM or fast-math flag is set"
