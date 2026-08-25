# XLSX Due-Diligence Packet Benchmark

This example shows how a repeatable spreadsheet review can move from an
LLM-driven tool loop into a reviewed Devlish workflow that runs as bytecode in a
Node/WASM runner.

## What It Checks

The workbook fixture models a due-diligence packet with these sheets:

- `Summary`
- `Legal`
- `PPA`
- `Cost Seg`
- `Domestic Content`
- `Payroll`
- `Equipment`
- `Controls`
- `Documents`

The Devlish workflow reads 23 cells and records 22 assertions. It checks site
name and address consistency, credit value, cost segregation support, domestic
content evidence, prevailing wage evidence, equipment origin counts, formula
health, and required document presence.

## Why This Matters

The baseline represents a common agent workflow:

1. Inspect workbook shape.
2. Read candidate sheets and ranges.
3. Decide which cells matter.
4. Reconcile values across worksheets.
5. Write a report.
6. Read the report back before replying.

That loop is useful the first time. Once the checks are known, the repeated work
should be a compiled tool run instead of another chain of prompts and tool
responses.

## Before And After

The generated benchmark report is written to:

```text
tmp/xlsx_due_diligence_packet/benchmark-report.json
```
Current fixture numbers:

| Metric | Before: agent/tool loop | After: compiled Devlish WASM CLI | Savings |
| --- | ---: | ---: | ---: |
| Prompt bytes | 27,020 | 0 | 27,020 |
| Tool-output bytes | 135,000 | 0 | 135,000 |
| Model-visible bytes | 162,020 | 0 | 162,020 |
| Estimated tokens | 40,505 | 0 | 40,505 |
| Agent-observed tool calls | 16 | 0 | 16 |
| Model checkpoints | 9 | 0 | 9 |

The compiled run still emits runner internals, including WASM events and XLSX
cell preload results. Those are not counted as model-visible prompt or tool-loop
bytes in this benchmark contract.

## Run It

Install runner dependencies:

```bash
npm --prefix crates/devlish_wasm_runner install
```

Generate the workbook fixture:

```bash
node scripts/generate_xlsx_due_diligence_fixture.mjs
```

Compile the workflow:

```bash
bundle exec ruby -Ilib bin/devlish compile \
  examples/xlsx_due_diligence_packet/workflow.dvl \
  --target bytecode \
  --output examples/xlsx_due_diligence_packet/workflow.dvlc.json
```

Build the WASM runner:

```bash
./scripts/build_wasm_runner.sh
```

Run the benchmark:

```bash
node crates/devlish_wasm_runner/bin/devlish-runner.mjs benchmark \
  examples/xlsx_due_diligence_packet/benchmark.json
```

Expected summary:

```json
{
  "success": true,
  "estimated_avoided_tokens": 40505,
  "model_visible_byte_reduction_ratio": 1
}
```
