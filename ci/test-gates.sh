#!/usr/bin/env bash
# A control that has never failed has not been shown to work. This script plants one violation at
# a time in a scratch copy and checks that the control rejects it, and that the same control
# passes the copy without the violation. It changes nothing in the repository.
#   - ci/fuzz.sh: a fuzz run that exits 77, a list that is empty, a list that fails
#   - ci/check-rfc-index.sh: an indexed file that is missing, a file that is not indexed
#   - ci/check-rfc-invariants.sh: a status line that disagrees with its folder, no status line,
#     a broken relative link (and a broken link inside a code fence, which is an example)
#   - ci/check-private-paths.sh: a tracked file that names the private folder
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
  *"fuzz run"*) exit "${FAKE_RC:-0}" ;;
esac
EOF
chmod +x "$scratch/bin/cargo"
fuzz() { env "$@" PATH="$scratch/bin:$PATH" ci/fuzz.sh 1; }
expect pass "fuzz.sh passes when every target runs clean" fuzz FAKE_RC=0
expect fail "fuzz.sh fails when a target exits 77" fuzz FAKE_RC=77
expect fail "fuzz.sh fails when the list of targets is empty" fuzz FAKE_TARGETS=
expect fail "fuzz.sh fails when cargo fuzz list fails" fuzz FAKE_LIST_RC=1

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
