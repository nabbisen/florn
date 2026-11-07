#!/bin/sh
# Tests live beside the code they test and not in it: for src/foo.rs the tests are src/foo/tests.rs
# (for a crate root, src/tests.rs), declared in the source file as
#
#     #[cfg(test)]
#     mod tests;
#
# This fails for a file that is not a test file and holds
#   - a test module with a body (`#[cfg(test)] mod name { ... }`), or any other item behind
#     `#[cfg(test)]` (`use`, `fn`): such an item can only be reached by the crate's own unit tests,
#     so it belongs in tests.rs;
#   - a declaration of a test module under another name than `tests` (`mod foo_tests;`): one
#     tests.rs per source file; when it grows too large it splits into src/foo/tests/<part>.rs;
#   - a `#[test]` function outside a test file.
# A test file is a file called tests.rs, or one below a directory called tests (a crate's
# integration tests under tests/, and tests/ below a module). `#[cfg(not(test))]` is not test code.
# Comments are skipped; a quoted string is blanked before the attribute is read.
#
# Usage: ci/check-test-layout.sh      (from anywhere; it checks crates/ and fuzz/ of this tree)
set -eu
cd "$(dirname "$0")/.."

files=$(find crates fuzz -name '*.rs' -not -path '*/target/*' -not -path '*/tests/*' -not -name tests.rs | sort)
if [ -z "$files" ]; then
  echo "TEST LAYOUT FAILURE: no source file found under crates/ and fuzz/"
  exit 1
fi

# shellcheck disable=SC2086
awk '
function trim(s) { sub(/^[ \t]+/, "", s); sub(/[ \t]+$/, "", s); return s }
# What follows a `#[cfg(test)]` attribute must be the declaration `mod tests;` and nothing else.
function follows(code, text, at,   shown) {
  if (code != "mod tests;") {
    shown = text; if (length(shown) > 60) shown = substr(shown, 1, 60) "..."
    if (code ~ /^mod [[:alnum:]_]+;$/)
      printf "%s:%d: a test module is called tests, one per source file (%s); put it in src/<module>/tests.rs\n", FILENAME, at, shown
    else
      printf "%s:%d: test-only code outside a tests file (%s); move it to src/<module>/tests.rs\n", FILENAME, at, shown
    bad++
  }
}
FNR == 1 { pending = 0 }
{
  line = trim($0)
  if (line == "" || line ~ /^\/\//) next
  code = line
  gsub(/"([^"\\]|\\.)*"/, "\"\"", code)
  if (pending) {
    if (code ~ /^#\[/) next
    follows(code, pending_text " " line, pending_line)
    pending = 0
  }
  if (code ~ /^#\[(cfg|cfg_attr)\(/ && code !~ /not\(test\)/ && code ~ /(^|[^[:alnum:]_])test([^[:alnum:]_]|$)/) {
    rest = code
    sub(/^#\[[^]]*\)\][ \t]*/, "", rest)
    if (rest == code || rest == "") { pending = 1; pending_line = FNR; pending_text = line; next }
    follows(rest, line, FNR)
    next
  }
  if (code ~ /^#\[([[:alnum:]_]+::)*(test|bench)\]/) {
    printf "%s:%d: a test function outside a tests file; move it to src/<module>/tests.rs\n", FILENAME, FNR
    bad++
  }
}
END {
  if (bad) { printf "TEST LAYOUT FAILURE: %d place(s)\n", bad; exit 1 }
}
' $files
echo "test layout: ok ($(echo "$files" | wc -l | tr -d ' ') source files, no inline test code)"
