# DOCX Contract Review Benchmark

This example shows how a common Word-document review skill can move from a
multi-turn LLM/tool loop into reviewed Devlish source and deterministic bytecode
execution.

The workflow reads raw text from a DOCX contract and records 10 assertions for
required NDA clauses: title, parties, effective date, confidentiality scope,
purpose, term, return/destroy obligations, governing law, and remedies.

Run it:

```bash
./scripts/build_wasm_runner.sh
node crates/devlish_wasm_runner/bin/devlish-runner.mjs benchmark \
  examples/docx_contract_review/benchmark.json
```

Expected summary:

```json
{
  "success": true,
  "estimated_avoided_tokens": 30938,
  "model_visible_byte_reduction_ratio": 1
}
```

The point is to convert the repeatable clause checklist into source that a
business or legal reviewer can read, while the compiled bytecode handles the
cheap repeat runs.
