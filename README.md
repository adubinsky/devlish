# Devlish - Provable programs, written in English

Last updated: 2026-07-23
Status: Current entry point. Website: https://devlish.dev

Devlish is an English-first programming language for AI-era work: deterministic
workflows that can be read by non-programmers, compiled to bytecode, executed
natively or in the browser, and **verified after the fact**. Your AI can say it
did the work; Devlish lets you prove it.

The design rule: if a step can be deterministic, remove it from the model. If
it requires judgment, expose it as a named `Checkpoint`. Everything around the
checkpoint is compiled behavior with an identity, declared permissions,
assertions, evidence, and replay.

Devlish is implemented entirely in Rust. A native compiler (`devlish-core`)
parses the full language and emits bytecode. A shared VM (`devlish-vm`)
executes the bytecode natively on the command line or via WASM in browsers
and Node. No Ruby, Python, or Node runtime is needed.

## Getting Started

From a source checkout, install once (requires the Rust toolchain):

```bash
make install
```

This builds and installs the native application as `~/.local/bin/devlish`.
Put that directory on your PATH if it is not already there. Use `PREFIX` to
select another installation directory. Installed users need no Rust commands,
separate compilation step, or checkout to run the application.

## Three Application Modes

```bash
devlish                     # Interactive model prompt
devlish --run workflow.dvl  # Run a Devlish workflow; -r is equivalent
devlish --server            # Long-lived HTTP service; -s is equivalent
```

The prompt stays open between requests. Type `/exit` or send EOF to close it;
`/clear` starts a fresh conversation. Each turn executes a Devlish program,
enforces a Devlish policy, and creates a hash-chained decision log in
`.devlish/sessions/`. The bundled program validates a bounded JSON plan before executing its steps.
A project can supply `.devlish/agent.dvl` and `.devlish/policy.dvl` to define its
workflow and rules. Both are compiled and captured when the prompt opens.
The bundled permissions allow model calls and responses; tool execution requires
explicit agent permissions and policy authorization.

Model/provider settings use `~/.devlish/config.toml` (or `DEVLISH_CONFIG`);
credentials use environment variables or the existing credential resolver.
See [application setup](docs/APPLICATION.md) for OpenRouter configuration.

File mode imports and compiles the source in memory, executes it, and exits.
For example:

```bash
devlish -r examples/local_tools/tool.dvl --input '{"tool_request":{"tool_id":"ls","arguments":["-1"]}}'
```

Server mode listens on `127.0.0.1:7420` and stays in the foreground until
stopped, suitable for a service manager running it in the background. It hosts
requests; it does not continually run an agent. Set `DEVLISH_SERVE_TOKEN` to
require bearer authentication. See [HTTP caller guide](docs/SERVE_QUICKSTART.md).
The HTTP host currently has fewer effects than the file and prompt host; this
entry point does not add local-tool execution to HTTP requests.

Developer and audit interfaces remain available for existing integrations;
they are not additional steps required to use these three modes. Contributors
can use `make build` and `make test`. The browser playground remains available
at https://devlish.dev/playground.html.

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

## Outbound harness: Devlish calls your LLM

Flip the polarity: a `.dvl` program can call Anthropic, OpenAI, or Ollama.

```bash
devlish harness init-config
devlish harness run examples/outbound_classify/classify.dvl --provider anthropic
devlish serve --bind 127.0.0.1:7420   # HTTP API daemon
```

See `docs/ENVIRONMENT_ROADMAP_STATUS.md` and `docs/LANGUAGE_REFERENCE.md`.

## Executable effect policies

`devlish run` can enforce a separately supplied Devlish policy before each
host tool effect and persist its decision before dispatch:

```bash
devlish run examples/effect_policy/allowed.dvl \
  --policy examples/effect_policy/policy.dvl --policy-log allowed-policy.jsonl
```

Policies return an explicit boolean decision and an English explanation.
Malformed policies fail closed. This first increment supports CLI `run`;
offline replay reports are available with `--policy-evidence`. Server enforcement
and the Devlish agent loop are next.
See [Effect policies](docs/EFFECT_POLICY.md) for the contract and current limits,
[NPPI and company-IP examples](examples/data_protection/README.md) for tested
rules, and [Policy provenance](docs/POLICY_PROVENANCE.md) for artifact verification
and the signing roadmap. [Repeatable compliance reports](docs/COMPLIANCE_REPORTS.md)
cover application integrity, policy cases, and offline process reproduction.
The [governed agent example](examples/governed_agent/README.md) implements a finite
plan/validate/execute loop entirely in Devlish, with a separate effect policy
and deterministic model/service test adapters.

The separate [independent audit verifier](docs/INDEPENDENT_AUDIT_VERIFIER.md)
checks signatures and signed log receipts offline. Its results explicitly
separate authenticated bytes from proof of actual execution or enforcement.

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
- https://devlish.dev - website, playground, and rendered docs
