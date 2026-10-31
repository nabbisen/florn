#!/usr/bin/env bash
# Every RFC file is listed in rfcs/README.md under the folder it lives in, and every listed file
# exists. RFC 000: the folder is the source of truth for an RFC's state; the index must agree
# with it. Both directions fail the script: a file the index does not list, and an index line
# whose file is missing (the handoff packages' links are checked too).
set -euo pipefail
cd "$(dirname "$0")/.."
status=0
for dir in proposed accepted done archive; do
  [ -d "rfcs/$dir" ] || continue
  for f in rfcs/$dir/*.md; do
    [ -e "$f" ] || continue
    rel="./$dir/$(basename "$f")"
    if ! grep -qF "($rel)" rfcs/README.md; then
      echo "not indexed: $f"; status=1
    fi
  done
done
# The loop reads from a process substitution, not a pipe: a pipe would run it in a subshell and
# lose `status`, which is how an index line without a file once passed.
while read -r rel; do
  if [ ! -e "rfcs/$rel" ]; then
    echo "indexed but missing: rfcs/$rel"; status=1
  fi
done < <(grep -oE '\(\./(proposed|accepted|done|archive|handoffs)/[^)]+\)' rfcs/README.md | tr -d '()' || true)
exit $status
