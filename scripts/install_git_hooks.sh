#!/bin/sh
set -eu
root=$(git rev-parse --show-toplevel)
configured=$(git config --get core.hooksPath || true)
if [ -n "$configured" ]; then
  case "$configured" in
    /*) hooks=$configured ;;
    *) hooks="$root/$configured" ;;
  esac
else
  hooks=$(git rev-parse --git-path hooks)
fi
mkdir -p "$hooks"
if [ -e "$hooks/pre-commit" ] && ! cmp -s "$root/.githooks/pre-commit" "$hooks/pre-commit"; then
  echo "Existing pre-commit hook requires integration. No hook overwritten." >&2
  exit 1
fi
cp "$root/.githooks/pre-commit" "$hooks/pre-commit"
chmod +x "$hooks/pre-commit"
echo "Execution boundary pre-commit hook installed."
