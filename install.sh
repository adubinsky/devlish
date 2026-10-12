#!/usr/bin/env bash
# Compatibility entry point; the build/install implementation lives in Makefile.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
exec make -C "$SCRIPT_DIR" install "$@"
