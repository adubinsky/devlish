# PDF Transfer Packet Benchmark

This example shows how a common PDF skill workflow can move from a multi-turn
LLM/tool loop into reviewed Devlish source and deterministic bytecode execution.

The workflow reads the text from a PDF transfer packet and records 10 assertions
for the fields an agent would otherwise search for across several PDF tool calls:
transfer election language, parties, project identity, site address, credit value,
placed-in-service date, and required supporting documents.

Run it:

```bash
./scripts/build_wasm_runner.sh
node crates/devlish_wasm_runner/bin/devlish-runner.mjs benchmark \
  examples/pdf_transfer_packet/benchmark.json
```

Expected summary:

```json
{
  "success": true,
  "estimated_avoided_tokens": 35105,
  "model_visible_byte_reduction_ratio": 1
}
```

The point is not that PDF text search is hard to script. The point is that this
is a common skill-shaped loop: extract, inspect, ask the model what matters,
search again, then write a report. Devlish turns the repeatable checklist into
human-reviewable source that can run without another model/tool conversation.
