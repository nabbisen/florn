#!/usr/bin/env bash
# RFC 006 section 6: the words below never appear in the user interface. The check runs over
# every translation catalogue: the shell's labels (locales/*.json under crates/) and the codes'
# sentences (catalogue/*.json). None existed before Phase 2; the check bites from the first on.
set -euo pipefail
cd "$(dirname "$0")/.."
shopt -s nullglob globstar
files=(crates/**/locales/*.json crates/**/catalogue/*.json)
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
