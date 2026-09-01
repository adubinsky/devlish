# Devlish

Your AI can say it did the work. Devlish lets you prove it.

Last updated: 2026-09-01
Status: Current entry point. Website: https://www.devlish.com

Devlish is an inspectable execution and verification layer for AI work: if a
step can be deterministic, remove it from the model; if it requires judgment,
expose it as a named `Checkpoint`. Everything around the checkpoint is compiled
behavior with an identity, declared permissions, assertions, evidence, and
replay.

Readable English source is a property of the language, not the headline. A
native Rust compiler (`devlish-core`) parses `.dvl` files and emits bytecode.
A shared VM (`devlish-vm`) executes that bytecode natively on the command line
or via WASM in browsers and Node. No Ruby, Python, or Node runtime is needed
to compile or run. MIT licensed.

```dvl
Operations's Invoice Reviewer:
  review invoice using invoice amount:
    review_needed equals false
    review_needed equals true if invoice amount >= 10000
    escalation_label equals "standard"
    escalation_label equals "priority" if review_needed == true
    respond with escalation_label

  classify risk using invoice amount:
    risk equals "low"
    risk equals "medium" if invoice amount >= 5000
    risk equals "high" if invoice amount >= 25000
    respond with risk
```

From `docs/course/04-methods-and-classes/examples/04_invoice_reviewer.dvl`.

## Getting Started

```bash
# Requires Rust (https://rustup.rs)
git clone https://github.com/adubinsky/devlish
cd devlish
./install.sh          # builds devlish-core and adds `devlish` to your PATH
```

```bash
# Create a project and write a program
devlish new my_project
cd my_project

cat > hello.dvl << 'EOF'
Ask "What is your name?" as user name
Print user name
EOF

# Compile and run
devlish run hello.dvl --input '{"user_name": "World"}'

# Or compile to bytecode first
devlish compile hello.dvl --output hello.dvlc.json
devlish run hello.dvlc.json --input '{"user_name": "World"}'

# Validate syntax
devlish validate hello.dvl
```

No install at all: the full compiler and VM also run in your browser at
https://www.devlish.com/playground.html (WebAssembly; nothing you type is
uploaded).

## CLI Commands

```text
devlish compile <file.dvl> [--output path.dvlc.json]   Compile to bytecode
devlish run <file> [--input json] [--method name] [--env KEY=VALUE] [--quiet]
                                                       Run a .dvl or .dvlc.json file
devlish validate <file.dvl>                            Check syntax (alias: lint)
devlish disassemble <file.dvlc.json>                   Show bytecode instructions
devlish fmt <file.dvl>                                 Format a source file
devlish repl                                           Interactive read-eval-print loop
devlish new <project_name>                             Create a new project
devlish mcp [--tools-dir dir]                          Start MCP server (JSON-RPC over stdio)
devlish course                                         Interactive beginner course
devlish evidence <rule.dvl>                            Run golden cases, emit signed evidence report
devlish audit-verify <log.jsonl>                       Verify the hash chain of an audit log
devlish replay <log.jsonl>                             Re-run a journaled run offline, verify output
devlish release <verb>                                 Release lifecycle: propose, approve,
                                                       publish, retire, list, verify
devlish version | help
```

The `run` command auto-compiles `.dvl` files in memory. Implicit file
arguments work: `devlish script.dvl` is the same as `devlish run script.dvl`.

## Governance and Verification

This is what separates Devlish from both rules engines and agent frameworks:
a governed rule's execution is provable after the fact.

- **Rule identity** (`Rule:` manifest section): a dotted `id`, semver
  `version`, optional author and `effective from` / `effective until` dates,
  validated at compile time and embedded in the bytecode.
- **Program manifest** (`Permissions:` / `Boundaries:` / `Callers:`): declared
  host effects, enforced by the VM at runtime. Undeclared effects fail with
  "Permission denied"; they are not a dialog box.
- **Evidence bundles** (`devlish evidence`): run a rule's golden cases against
  the exact compiled artifact and emit a tamper-evident, machine-readable
  report (artifact sha256, per-case hashes, `report_sha256`). Non-zero exit on
  any failure, so it can gate a release. See `docs/EVIDENCE.md`.
- **Audit log** (`devlish run --audit-log`): every governed run appends one
  hash-chained record binding output to rule id/version, artifact hash, input
  and output hashes, and runtime. `devlish audit-verify` detects any modified,
  reordered, or deleted record. See `docs/AUDIT.md`.
- **Effect journaling and replay** (`devlish run --journal`, `devlish replay`):
  archive a run's exact bytecode, input, and every host-effect exchange, then
  re-execute offline against the journaled responses and verify the output
  hash. Any divergence exits non-zero. Credentials never enter the journal.
- **Controlled releases** (`devlish release`): an append-only, hash-chained
  registry maps each `rule@version` through draft/approved/published/retired,
  with separation of duties (an author cannot self-approve). `devlish run
  --governed <registry>` refuses any artifact not currently published. See
  `docs/RELEASE.md`.
- **Effective-date resolution** (`devlish run --as-of YYYY-MM-DD`): run the
  rule version that was in force on a given date, for compliance
  recomputation under the historically correct rule.

## Architecture

```text
crates/
  devlish_core/     Rust compiler + CLI (~10,400 lines, 103 tests)
  devlish_vm/       Platform-independent bytecode VM (2,987 lines)
  devlish_wasm_runner/  WASM shell for browser/Node (221 lines)
  devlish_toolrun/  Command output compression for LLM agents (662 lines)
packages/
  devlish-runtime/  npm package: TypeScript wrapper, Web Worker, base64 WASM
```

The compiler and VM share no platform-specific code. The WASM runner is a
thin shell that implements the `HostEffects` trait via JavaScript host
imports. The native runner implements it with real filesystem I/O.

## Language Features

Devlish supports:

- **Variables and arithmetic**: `score equals base plus bonus`
- **Control flow**: `If`, `Otherwise`, `While`, `Until`, `For each`, `Break`, `Continue`
- **Collections**: `list of`, `record with`, `append`, `pop`, `filter`, `map`, `sort`
- **Built-in functions**: `count of`, `first of`, `uppercase`, `trim`, `split`, `join`, 26 total
- **Assertions**: `Expect value equals "x" as "test-id"`
- **Validation sentences**: `amount must be at most 10000`, `Require ... otherwise fail with`
- **Checkpoints**: `Checkpoint "prompt"` pauses execution and returns structured
  context for an LLM or human, then resumes
- **File I/O**: `Read XLSX cell`, `Read PDF text`, `Export to path`
- **Filesystem operations**: `Copy file`, `Move file`, `Create directory`, `Delete file`, `Check if exists`, `Get file info`, `List files`, `Find files matching`
- **HTTP requests**: `Get the url at`, `Post to`, `Put to`, `Delete the url at`, `Download`
- **Structured output**: `Respond with` (exit 0), `Fail with record` (exit 1)
- **Error handling**: `Fail with`, `Require condition`, `Try`/`Otherwise`
- **Rule governance**: `Rule:` header with id, version, effective dates
- **Program manifest**: `Permissions:`, `Boundaries:`, `Callers:` header for declaring and enforcing access
- **Credentials**: `.env` file support, CLI `--env KEY=VALUE`, secure resolution chain
- **Class-style modules**: `Module's ClassName:` with methods, inheritance, `respond with`

See `docs/LANGUAGE_REFERENCE.md` for the full authoring guide.

## MCP: Tools Your LLM Calls

The MCP server ships in the CLI. Point it at a folder of `.dvl` files and each
one becomes a callable tool for Claude, GPT, or any MCP client:

```bash
devlish mcp --tools-dir ./tools/

# e.g. register with Claude Code:
claude mcp add devlish -- devlish mcp --tools-dir /path/to/tools
```

`Ask` lines define the tool's input schema; `Respond with` returns typed JSON
and `Fail with` returns structured errors the model can parse and retry.
Describe tools with types in `devlish.toml` manifests for richer discovery.

## WASM Embedding

Compiled Devlish programs run in browsers and Node via the `devlish-runtime`
npm package:

```bash
npm install devlish-runtime
```

```javascript
import { loadTool } from "devlish-runtime";

const tool = await loadTool({
  bytecode: await fetch("/rules/pricing.dvlc.json").then(r => r.json()),
  instructionLimit: 1_000_000
});
const result = await tool.run({ customer_tier: "priority" });
tool.dispose();
```

Execution runs in a Web Worker by default. Tools requiring HTTP or filesystem
permissions are rejected at load time. `loadTool` validates the artifact
format and optionally verifies `expectedSha256`; `tool.info.rule` surfaces a
governed artifact's identity, and `onAuditRecord` lets the embedding app
persist audit records. See `packages/devlish-runtime/README.md` for the full
API.

## Documentation

- `docs/LANGUAGE_REFERENCE.md` - authoring guide
- `docs/AUDIT.md` - execution provenance audit log
- `docs/EVIDENCE.md` - test evidence bundles
- `docs/RELEASE.md` - controlled release workflow
- `docs/NATIVE_COMPILATION_PLAN.md` - compiler and VM roadmap
- `docs/BYTECODE_WASM_FIRST_DELIVERABLES.md` - WASM integration status
- `extensions/devlish-vscode/` - VS Code language support
- https://www.devlish.com - website, playground, and rendered docs
