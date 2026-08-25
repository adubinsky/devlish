# Devlish Core

`devlish-core` is the native Rust parser and bytecode compiler track for
Devlish.

The current slice compiles the document/assertion workflows used by the
token-savings examples plus the first normal workflow shape from the
bytecode/WASM demo:

- `Read XLSX cell "Sheet!A1" as value`
- `Read PDF text "packet.pdf" as packet text`
- `Read DOCX text "contract.docx" as contract text`
- `Expect value equals "expected" as "id"`
- `Expect packet_text contains "clause" as "id"`
- `Expect value is present as "id"`
- `Expect value is not spreadsheet error as "id"`
- `Export assertions to assertion report path`
- `Ask "Prompt?" as answer`
- `score equals base_score plus 10`
- `If answer is "priority"` with an indented body
- `Print score`
- `Export score to "tmp/score.json"`

It emits the same `devlish-bytecode` JSON package shape consumed by the current
Ruby VM and Rust/WASM runner.

## Usage

```bash
cargo run -- compile ../../examples/xlsx_expected_cells/workflow.dvl \
  --target bytecode \
  --output /tmp/workflow.dvlc.json

cargo run -- compile ../../examples/bytecode_wasm/review_score.dvl \
  --target bytecode \
  --output /tmp/review_score.dvlc.json
```

## Why This Exists

The long-term Devlish direction is a single Rust-based core that owns parsing,
AST linting, bytecode compilation, runtime verification, and eventually binary
packaging. Ruby can remain a compatibility and authoring surface, but users
should not need a Ruby environment to compile their own `.dvl` files.

This crate starts that migration without changing the bytecode format or the
existing runner contract.
