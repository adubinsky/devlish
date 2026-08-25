#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

cd "$SCRIPT_DIR"

cargo build --release --target wasm32-unknown-unknown

mkdir -p "$SCRIPT_DIR/pkg"
cp "$SCRIPT_DIR/target/wasm32-unknown-unknown/release/devlish_wasm_compiler.wasm" \
   "$SCRIPT_DIR/pkg/devlish_compiler.wasm"

echo "Built: $SCRIPT_DIR/pkg/devlish_compiler.wasm"
