# Devlish Architecture

Last updated: 2026-07-10

## Overview

Devlish is an English-first programming language implemented entirely in Rust.
Users write `.dvl` source files in natural English syntax, compile them to
bytecode, and run them natively or in browsers via WASM.

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
  devlish_core/       Compiler + CLI (~5,100 lines)
    src/lib.rs         Full-language parser, bytecode emitter, 40 tests
    src/main.rs        CLI: compile, run, disassemble, validate, lint, new

  devlish_vm/          Bytecode VM (1,159 lines)
    src/lib.rs         HostEffects trait, 42+ opcodes, 26 built-in functions

  devlish_wasm_runner/ WASM shell (110 lines)
    src/lib.rs         Implements HostEffects via JS host imports
    js/index.mjs       JavaScript host wrapper

  devlish_toolrun/     LLM token compression (662 lines)
    src/main.rs        Command output summarizer for agent workflows
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
