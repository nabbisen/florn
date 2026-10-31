#!/usr/bin/env bash
# A public file cites no private path: the folder that holds the working papers is excluded
# from the repository, so a reference to it in a tracked file points at nothing for a reader.
# Fails when a tracked file other than .gitignore names it.
set -euo pipefail
cd "$(dirname "$0")/.."
hits=$(git grep -l -F '.git-exclude' -- . ':!.gitignore' ':!ci/check-private-paths.sh' ':!ci/test-gates.sh' || true)
if [ -n "$hits" ]; then
  echo "a private path is named in a public file:"
  echo "$hits"
  exit 1
fi
