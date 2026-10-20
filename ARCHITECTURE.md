# Devlish Architecture

Last updated: 2026-10-19

## Overview

Devlish is an inspectable execution and verification layer implemented entirely
in Rust. Users write `.dvl` source in readable English, compile it to bytecode,
and run it natively or in browsers via WASM. Readable English is a property of
the language, not the product pitch.

Product direction for a Rails-like **server-first harness** (capability
packages, verified serve, WASM views without privileged effects) is specified
in [docs/SERVER_HARNESS_ARCHITECTURE.md](docs/SERVER_HARNESS_ARCHITECTURE.md)
and [docs/CAPABILITY_PACKAGES.md](docs/CAPABILITY_PACKAGES.md). Those docs are
architecture specs; they do not change current runtime behavior by themselves.

```text
.dvl source
  -> devlish_core (Rust parser + bytecode compiler)
  -> .dvlc.json bytecode package
  -> devlish_vm (platform-independent bytecode VM)
  -> native execution or WASM in browser/Node
```

## Crates

```text
crates/
  devlish_audit/      Offline independent verifier; never executes programs
  devlish_llm/        Provider adapters for governed model effects
  devlish_wasm_compiler/ WASM compiler interface
  devlish_core/       Compiler + CLI
    src/lib.rs         Full-language parser and bytecode emitter
    src/main.rs        CLI: compile, run, disassemble, validate, lint, new

  devlish_vm/          Bytecode VM
    src/lib.rs         HostEffects trait, opcodes and expression helpers

  devlish_wasm_runner/ WASM shell
    src/lib.rs         Implements HostEffects via JS host imports
    js/index.mjs       JavaScript host wrapper

```

## Compilation Pipeline

1. **Parse**: `.dvl` source to AST (Statement/Expression trees)
2. **Compile**: AST to bytecode instructions (register-based)
3. **Package**: Bytecode + constant pool + symbol table + source map + effect table
4. **Execute**: VM interprets bytecode, calls host for I/O effects

## Bytecode Format

JSON package (`.dvlc.json`):
- `constant_pool`: literal values (strings, numbers)
- `symbol_table`: variable names
- `instructions`: register-based opcodes
- `source_map`: bytecode address to source line mapping
- `effect_table`: declared side effects
- `imports`: required host functions
- `class_info` / `methods`: optional, for class-style programs
- `manifest`: optional, permission/boundary/caller declarations

## Host Effects

The VM is pure computation. All I/O goes through the `HostEffects` trait:
- `emit_event(event)`: structured execution events
- `write_file(request)`: file system writes
- `read_file(request)`: file system reads
- `http_request(method, url, body, headers)`: HTTP calls
- `http_download(url, path)`: download file from URL
- `read_xlsx_rows(path, sheet)`: read Excel rows
- `respond(value)`: structured output
- `file_copy`, `file_move`, `file_mkdir`, `file_delete`: filesystem mutations
- `file_exists`, `file_stat`, `file_list`, `file_glob`: filesystem queries
- `resolve_credential(key)`: credential resolution (never exposed to programs)

The native runner (`NativeHost`) implements these with `std::fs`, `ureq`,
`calamine`, and the `glob` crate. The WASM runner implements a subset
via JavaScript host imports (`devlish_host` module).

## Credential Store

The native runner loads credentials from `.env` files and CLI `--env` params:
1. CLI `--env KEY=VALUE` (highest priority)
2. Program-local `.env` (same directory as the `.dvl` file)
3. Global `~/.devlish/.env`
4. System environment variables

Credentials flow to host methods via `resolve_credential()`, never to
program variables.

## Program Manifest

Programs can declare a `Permissions:` / `Boundaries:` / `Callers:` header.
The manifest compiles into bytecode metadata and the VM enforces permissions
at runtime. Undeclared effects fail with "Permission denied" when a manifest
is present. Programs without a manifest are unrestricted.

## Entry Point

`bin/devlish` is a bash shim that execs the `devlish-core` Rust binary.
No Ruby, Python, or Node runtime is needed.


## Policy interception and repeatable reports

Policy-enabled CLI runs wrap the host with `devlish_vm::policy::PolicyHost`.
Each tool effect is evaluated by a separately loaded Devlish policy in an
isolated VM; the wrapper persists the decision before dispatch and its outcome
afterward. Recording failures block subsequent effects. Existing VM permission
checks remain in force. Ordinary diagnostic events remain outside this interception boundary. The prompt
and governed harness install executable policy; verified CLI and authenticated
HTTP sessions use shared signed admission and `GovernedRun`. Legacy HTTP/MCP
and browser routes do not install executable effect policy.

`--policy-evidence` explicitly retains sensitive effect exchanges in the policy
log. `report process` uses them as an offline host while rerunning the program
and policy, comparing decisions and the terminal result without live effects.
Application and policy reports bind the file baseline and golden cases to that
process report. These are tamper-evidence checks, not signature verification
or independent runtime attestation.

See [Effect policies](docs/EFFECT_POLICY.md),
[Compliance reports](docs/COMPLIANCE_REPORTS.md), and
[Policy provenance](docs/POLICY_PROVENANCE.md).


## Independent audit verifier

`crates/devlish_audit` builds a separate offline verifier without dependencies on
Devlish's compiler, VM, model providers or service host. It authenticates exact
bytes under operator-supplied Ed25519 trust keys, and checks signed receipts
against format-3 policy logs and independently supplied receipt digests.
It also checks signed release manifests and binds approved artifact identities
to reports and receipts. The native `run-verified` command consumes verified
program and policy buffers with a durable local rollback floor. The browser
compiler excludes this native crypto dependency. Neither component proves
runtime memory integrity. Protected signing, independent receipt custody and
protected execution remain separate work. See [Independent audit verifier](docs/INDEPENDENT_AUDIT_VERIFIER.md).
