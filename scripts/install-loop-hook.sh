#!/usr/bin/env bash
# Install the challenge-loop post-commit hook into this Devlish checkout.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/scripts/git-hooks/post-commit"
DST="$ROOT/.git/hooks/post-commit"
if [[ ! -d "$ROOT/.git" ]]; then
  echo "Not a git checkout: $ROOT" >&2
  exit 1
fi
cp "$SRC" "$DST"
chmod +x "$DST"
echo "Installed $DST"
