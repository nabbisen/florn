#!/usr/bin/env bash
# RFC 006 §6: the words below never appear in the user interface. The check runs over every
# translation catalogue (locales/*.json under crates/); none exists before Phase 2, so the check
# passes trivially until then and bites from the first catalogue on.
set -euo pipefail
cd "$(dirname "$0")/.."
shopt -s nullglob globstar
files=(crates/**/locales/*.json)
if [ ${#files[@]} -eq 0 ]; then
  echo "no catalogues yet"; exit 0
fi
status=0
for w in 'signed' 'audit' 'role' 'permission' 'trusted module' '署名' '監査' 'ロール' '権限'; do
  if grep -n -i -w -- "$w" "${files[@]}"; then
    echo "forbidden word in a catalogue: $w"; status=1
  fi
done
exit $status
