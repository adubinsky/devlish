# Bytecode-In-WASM First Deliverables

Last updated: 2026-07-18
Status: Complete. The `devlish-runtime` npm package (`packages/devlish-runtime/`)
is the production API for embedding compiled Devlish in web and Node apps.
See `packages/devlish-runtime/README.md` for usage. The low-level wrapper at
`crates/devlish_wasm_runner/js/index.mjs` remains for direct WASM access but
most users should use `devlish-runtime` instead.

## Goal

Demonstrate a Devlish workflow that starts as human-reviewable `.dvl` source,
compiles to Devlish bytecode, runs inside a WebAssembly-hosted Devlish VM, and
is called from JavaScript in both browser and Node hosts.

The first demo should prove:

- the reviewed artifact is Devlish source plus inspectable bytecode, not
  generated host-language source
- a JavaScript host can call a Devlish WASM runner now and call it again later
- the WASM runner can call back into JavaScript for host effects
- run results, emitted outputs, and execution events are structured data
- the same bytecode can run through a local VM and the WASM VM

Current implemented slice:

- `devlish compile --target bytecode` emits a JSON `.dvlc` package
- `devlish disassemble` prints bytecode addresses, operands, constants, and
  Devlish source lines
- `devlish run *.dvlc.json --input '{...}'` executes bytecode through the local
  bytecode VM
- `examples/bytecode_wasm/review_score.dvl` demonstrates Ask, arithmetic,
  branch, Print, and Export
- `scripts/build_wasm_runner.sh` builds the WASM-hosted bytecode VM
- `crates/devlish_wasm_runner/js/index.mjs` provides the JavaScript host wrapper
- `examples/bytecode_wasm/node/run.mjs` runs the same bytecode from Node through
  `WebAssembly.instantiate`
- `crates/devlish_wasm_runner` now includes an npm-style runner package with
  `devlish-runner run` and `devlish-runner benchmark`
- `examples/xlsx_expected_cells` proves a Ruby-free Node/WASM runtime can run a
  precompiled XLSX expected-cell workflow and report deterministic token savings

Remaining first-demo slice:

- browser verification through a local static server
- shared fixtures proving local VM and WASM VM produce the same result in tests
- async pause/resume for missing Ask input

## XLSX Token-Savings Smoke Test

The XLSX smoke test uses Ruby only as a developer compile step. The user-facing
runtime path is Node/WASM:

```bash
npm --prefix crates/devlish_wasm_runner install

bundle exec ruby -Ilib bin/devlish compile \
  examples/xlsx_expected_cells/workflow.dvl \
  --target bytecode \
  --output examples/xlsx_expected_cells/workflow.dvlc.json

./scripts/build_wasm_runner.sh

node crates/devlish_wasm_runner/bin/devlish-runner.mjs \
  benchmark examples/xlsx_expected_cells/benchmark.json
```

Expected smoke output:

```json
{
  "success": true,
  "estimated_avoided_tokens": 16500,
  "model_visible_byte_reduction_ratio": 1
}
```

The benchmark reads `fixture.xlsx` with the Node host, preloads required cells
into `__xlsx_cells__`, runs the precompiled Devlish bytecode in WASM, writes an
assertion report, and writes a benchmark report. Provider token usage is
reported as unavailable in this MVP; savings are deterministic estimates from
captured model-visible byte counts.

A more realistic multi-turn agent-loop benchmark lives in
`examples/xlsx_due_diligence_packet/`. It models a common workbook review flow:
discover worksheet shape, inspect Summary, Legal, PPA, cost segregation,
domestic content, payroll, equipment, and document checklist sheets, reconcile
cross-sheet fields, then write and verify the report. The compiled path runs the
same checks from preloaded XLSX effects in one Ruby-free Node/WASM command.

```bash
node scripts/generate_xlsx_due_diligence_fixture.mjs
bundle exec ruby -Ilib bin/devlish compile \
  examples/xlsx_due_diligence_packet/workflow.dvl \
  --target bytecode \
  --output examples/xlsx_due_diligence_packet/workflow.dvlc.json
./scripts/build_wasm_runner.sh
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

## First Demo

Use one intentionally small workflow:

```text
Ask "Customer tier?" as customer tier
base score equals 10
bonus score equals 5
review score equals base_score plus bonus_score

If customer_tier is "priority"
  review score equals review_score plus 10

Print review_score
Export review_score to "tmp/review-score.json"
```

The demo should ship as:

- `examples/bytecode_wasm/review_score.dvl`
- `tmp/review_score.dvlc` or `tmp/review_score.dvlc.json`
- `tmp/review_score.disasm`
- `examples/bytecode_wasm/node/run.mjs`
- `examples/bytecode_wasm/browser/index.html`
- `pkg/devlish_wasm_runner/devlish_runner.wasm`
- `pkg/devlish_wasm_runner/index.mjs`

The `.dvlc.json` form is acceptable for the first spike if it preserves the
real bytecode sections. A packed binary `.dvlc` can replace the encoding once
the instruction set and source-map shape are stable.

## Work Order

### 1. Bytecode v0 Package

Status: implemented for the first workflow subset.

Add a narrow compiler target:

```bash
devlish compile examples/bytecode_wasm/review_score.dvl \
  --target bytecode \
  --output tmp/review_score.dvlc.json
```

The v0 package should include:

- format version
- compiler version
- constant pool
- symbol table
- instruction stream
- source map
- effect table
- required host imports

The first instruction subset should be small:

- `CONST`
- `LOAD`
- `STORE`
- `ADD`
- `EQ`
- `JUMP`
- `JUMP_IF_FALSE`
- `PRINT`
- `ASK`
- `EXPORT`
- `RETURN`

### 2. Disassembler

Status: implemented for JSON `.dvlc` artifacts.

Add a disassembler before the WASM runner:

```bash
devlish disassemble tmp/review_score.dvlc.json
```

The output should show stable instruction addresses and Devlish source lines:

```text
0000 CONST        r0, 10                 ; line 2
0001 STORE        base_score, r0         ; line 2
0002 CONST        r1, 5                  ; line 3
0003 STORE        bonus_score, r1        ; line 3
```

This makes the compiled artifact reviewable before any browser or Node demo.

### 3. Local Bytecode VM

Status: implemented for the first workflow subset.

Add a small in-process VM that executes the same bytecode artifact:

```bash
devlish run tmp/review_score.dvlc.json --input '{"customer_tier":"priority"}'
```

This step proves the bytecode semantics independently of WASM. The local VM and
the WASM VM should share expected fixtures so regressions are obvious.

### 4. WASM VM

Status: implemented for the first workflow subset.

Build:

```bash
./scripts/build_wasm_runner.sh
```

Create a small WASM runner, likely under:

```text
crates/devlish_wasm_runner/
```

The runner should interpret Devlish bytecode. It should not receive generated
JavaScript for a specific workflow.

Initial exported functions:

- `devlish_init`
- `devlish_run`
- `devlish_resume`
- `devlish_result_ptr`
- `devlish_result_len`
- `devlish_free`

Initial imported host functions:

- `emit_event(ptr, len)`
- `request_input(ptr, len)`
- `write_file(ptr, len)`

For v0, structured values can cross the boundary as JSON strings in linear
memory. That is not the final ABI, but it is enough to prove browser and Node
integration without blocking on the WebAssembly Component Model.

### 5. JavaScript Host Wrapper

Status: implemented for the first synchronous host-effect subset.

Add a small wrapper:

```text
pkg/devlish_wasm_runner/index.mjs
```

Target API:

```javascript
const workflow = await loadDevlishWorkflow({
  wasmUrl: "/devlish_runner.wasm",
  bytecodeUrl: "/review_score.dvlc.json",
  host: {
    writeFile: request => {
      reports.push(request);
      return true;
    },
    emitEvent: event => events.push(event)
  }
});

const result = await workflow.run({ customer_tier: "priority" });
```

The same wrapper shape should work in Node with bytes loaded from the
filesystem. Host callbacks are synchronous in this first ABI; later `resume`
support can add asynchronous host effects.

### 6. Browser And Node Demos

Status: Node demo implemented and verified. Browser demo file exists; it still
needs Playwright/static-server verification.

Browser acceptance:

- loads `devlish_runner.wasm`
- loads `review_score.dvlc.json`
- calls the workflow from JavaScript
- receives event data
- renders final output and exported file request

Node acceptance:

- loads the same WASM runner and bytecode
- injects host input
- writes the export through a host-provided filesystem effect
- prints the structured result JSON

### 7. Event And Checkpoint Fit

The first event stream should align with the future debugging work:

- `run_started`
- `instruction_started`
- `instruction_finished`
- `variable_assigned`
- `effect_requested`
- `effect_completed`
- `output_emitted`
- `run_finished`
- `run_failed`

This lets trace output, tests, browser UI, and LLM run results reuse the same
execution data.

## Acceptance Criteria

The spike is complete when these commands or equivalent scripts work:

```bash
devlish compile examples/bytecode_wasm/review_score.dvl \
  --target bytecode \
  --output tmp/review_score.dvlc.json

devlish disassemble tmp/review_score.dvlc.json

devlish run tmp/review_score.dvlc.json \
  --input '{"customer_tier":"priority"}'

node examples/bytecode_wasm/node/run.mjs
```

And the browser demo can:

- instantiate the WASM runner
- call the Devlish workflow from JavaScript
- receive a structured result
- show emitted events
- prove that host effects are imported through JavaScript

## Non-Goals For The Spike

- full language coverage
- direct Devlish-to-WASM code generation
- production binary packing
- package signing
- optimized performance
- complete browser UI
- complete WebAssembly Component Model support

## Toolchain Notes

The first runner can be implemented in Rust and compiled to
`wasm32-unknown-unknown`, or in another WASM-capable implementation language if
that better fits the project. The implementation language is runtime
infrastructure, not generated per-workflow code.

The Devlish workflow artifact remains:

```text
.dvl source -> AST -> IR -> Devlish bytecode -> WASM VM
```

Direct WASM code generation remains a later optimization:

```text
.dvl source -> AST -> typed graph -> WASM module
```

The bytecode-in-WASM path gets browser and Node embedding first without forcing
the language to settle every direct codegen decision now.
