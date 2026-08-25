#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [ -x /opt/homebrew/opt/rustup/bin/cargo ]; then
  export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fi

cargo build \
  --manifest-path "$ROOT/crates/devlish_wasm_runner/Cargo.toml" \
  --release \
  --target wasm32-unknown-unknown

WASM_OUTPUT="$ROOT/crates/devlish_wasm_runner/target/wasm32-unknown-unknown/release/devlish_wasm_runner.wasm"

cp "$WASM_OUTPUT" \
  "$ROOT/crates/devlish_wasm_runner/devlish_runner.wasm"
mkdir -p "$ROOT/pkg/devlish_wasm_runner"
cp "$WASM_OUTPUT" \
  "$ROOT/pkg/devlish_wasm_runner/devlish_runner.wasm"
chmod 0644 \
  "$ROOT/crates/devlish_wasm_runner/devlish_runner.wasm" \
  "$ROOT/pkg/devlish_wasm_runner/devlish_runner.wasm"
cp "$ROOT/crates/devlish_wasm_runner/js/index.mjs" \
  "$ROOT/pkg/devlish_wasm_runner/index.mjs"

echo "Built $ROOT/pkg/devlish_wasm_runner/devlish_runner.wasm"
