# devlish-runtime

Run compiled Devlish business rules in any JavaScript environment. Zero dependencies, Web Worker execution by default.

## Install

```bash
npm install devlish-runtime
```

## Quick Start

```javascript
import { runTool } from "devlish-runtime";

// Your compiled .dvl bytecode (output of `devlish compile`)
const bytecode = await fetch("/rules/pricing.dvlc.json").then(r => r.json());

const result = await runTool(bytecode, {
  customer_tier: "enterprise",
  deal_size: 500000
});

if (result.success) {
  console.log("Discount:", result.response);
}
```

## API

### `loadTool(options): Promise<DevlishTool>`

Load a compiled Devlish tool for repeated execution.

```javascript
import { loadTool } from "devlish-runtime";

const tool = await loadTool({
  bytecode: compiledBytecodeJson,
  mainThread: false  // default: uses Web Worker
});

const result = await tool.run({ input_field: "value" });
tool.dispose(); // clean up Worker
```

**Options:**

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `bytecode` | `object \| string` | required | Compiled bytecode JSON |
| `mainThread` | `boolean` | `false` | Skip Web Worker, run synchronously |
| `instructionLimit` | `number` | `10000000` | Max instructions before termination |
| `expectedSha256` | `string` | none | SHA-256 hex digest of the bytecode JSON string; load fails with `ArtifactError` on mismatch |

The returned tool exposes `tool.info` (`ToolInfo`) with metadata extracted from
the artifact: `formatVersion`, `compilerVersion`, `sourceHash`, `sourcePath`,
the manifest's declared `permissions`, and `rule` — the governance identity
(`id`, `version`, `author`, `effectiveFrom`, `effectiveUntil`) for a governed
rule (a `Rule:` manifest section), or `null` for an ungoverned artifact.

### `runTool(bytecode, input?): Promise<RunResult>`

One-shot convenience. Loads, runs, and disposes in one call. Always runs on the main thread.

### `selectVersion(artifacts, asOfDate): RuleVersion`

Given several artifacts that are versions of the same governed rule, returns the
one whose effective window is in force on `asOfDate` (`YYYY-MM-DD`) — how a
compliance recomputation runs the rule that was legally in force on a
transaction date. Throws `ArtifactError` if the date is not a real calendar
date, if an artifact is ungoverned or names a different rule id, if no version
is in force, or if more than one applies (overlapping windows). `isValidIsoDate`
is exported for the same calendar-date check the compiler uses.

### `RunResult`

```typescript
interface RunResult {
  success: boolean;
  error?: string;
  responded?: boolean;
  response?: unknown;
  context?: Record<string, unknown>;
  results?: Record<string, unknown>;
  trapped?: boolean;
}
```

`trapped` is set when the run died in a WASM trap (a Rust panic inside the VM).
The runtime automatically replaces the WASM instance before the next run, on
both the main-thread and Worker paths, so callers only need to retry.

## Artifact Contract

`loadTool` validates every artifact before instantiating the sandbox:

- `format` must be `"devlish-bytecode"` and `format_version` must be one this
  runtime supports (exported as `SUPPORTED_FORMAT_VERSIONS`). When the compiler
  changes the bytecode format incompatibly it bumps `format_version`; an older
  runtime rejects the artifact at load with a "recompile" error instead of
  failing mid-execution. That version gate is the migration policy.
- The package must be structurally sound (`instructions` and `constant_pool`
  arrays present). Violations throw `ArtifactError` at load, not at run.
- With `expectedSha256`, the exact bytecode string is hashed (WebCrypto) and
  compared before anything executes, for tamper/corruption detection when
  artifacts are fetched or cached outside your build.

**Distribution model:** bytecode is compiled locally or in CI with
`devlish compile tool.dvl > tool.dvlc.json` and shipped as a static asset of
your application (bundled import, public file, or CDN object). The runtime
never fetches or compiles anything remotely. Version artifacts like any other
asset (content-hash the filename or pin with `expectedSha256`); rollback is
shipping the previous artifact.

## Security

The WASM sandbox enforces:

- **No network access**: Tools declaring `http_request` permissions are rejected at load time.
- **No filesystem access**: Tools declaring `filesystem` permissions are rejected at load time.
- **Instruction limit**: Prevents infinite loops (default 10M instructions).
- **Memory isolation**: Each tool runs in its own WASM linear memory.

## How It Works

1. Your `.dvl` file is compiled to bytecode JSON (`devlish compile`)
2. `devlish-runtime` loads the bytecode into a WASM VM
3. Execution happens off main thread in a Web Worker
4. Results are returned as structured JSON

The WASM binary is inlined as base64 in the package for zero-config bundler compatibility.

## Browser Embed Example

```html
<script type="module">
  import { runTool } from "https://esm.sh/devlish-runtime";

  const bytecode = {
    format: "devlish-bytecode",
    format_version: 0,
    constant_pool: [100, 0.15],
    instructions: [
      { op: "CONST", dest: "price", const: 0 },
      { op: "CONST", dest: "discount", const: 1 },
      { op: "MUL", dest: "savings", left: "price", right: "discount" },
      { op: "RESPOND", value: "savings" }
    ]
  };

  const result = await runTool(bytecode);
  document.getElementById("output").textContent = result.response;
</script>
```
