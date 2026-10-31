#!/usr/bin/env bash
# The invariants of RFC 000 that the index check does not cover:
#   1. The Status line of every RFC names the state of the folder the file lives in: the first
#      state word in it (Proposed, Accepted, Implemented, Withdrawn, Superseded) is the folder's.
#      "Amended ..." history before it is fine; a file in done/ that begins "Proposed" is not.
#   2. Every relative link in an RFC, a handoff package or the RFC index resolves to a file.
#      Links in fenced code blocks and in inline code are examples, not links.
# Exits non-zero on the first violation of either, after reporting all of them.
set -euo pipefail
cd "$(dirname "$0")/.."
status=0

state_of_folder() {
  case "$1" in
    proposed) echo "Proposed" ;;
    accepted) echo "Accepted" ;;
    done) echo "Implemented" ;;
    archive) echo "Withdrawn|Superseded" ;;
  esac
}

for dir in proposed accepted done archive; do
  [ -d "rfcs/$dir" ] || continue
  want=$(state_of_folder "$dir")
  for f in rfcs/$dir/*.md; do
    [ -e "$f" ] || continue
    line=$(grep -m1 -E '^\*\*Status\.\*\*' "$f" || true)
    if [ -z "$line" ]; then
      echo "no Status line: $f"; status=1; continue
    fi
    first=$(printf '%s\n' "$line" | grep -oE '\b(Proposed|Accepted|Implemented|Withdrawn|Superseded)\b' | head -1 || true)
    if [ -z "$first" ]; then
      echo "Status line names no state: $f"; status=1
    elif ! printf '%s\n' "$first" | grep -qE "^($want)$"; then
      echo "Status says $first but the folder is $dir/: $f"; status=1
    fi
  done
done

# Links: strip fenced blocks and inline code, take the (target) of each [text](target).
strip_code() {
  awk '/^```/ { fence = !fence; next } !fence { gsub(/`[^`]*`/, ""); print }' "$1"
}
while IFS= read -r f; do
  base=$(dirname "$f")
  while IFS= read -r target; do
    case "$target" in
      http://*|https://*|mailto:*|'#'*) continue ;;
    esac
    path="${target%%#*}"
    [ -n "$path" ] || continue
    if [ ! -e "$base/$path" ]; then
      echo "broken link in $f: $target"; status=1
    fi
  done < <(strip_code "$f" | grep -oE '\]\([^)[:space:]]+\)' | sed -E 's/^\]\((.*)\)$/\1/' || true)
done < <(find rfcs -name '*.md' | sort)

exit $status
