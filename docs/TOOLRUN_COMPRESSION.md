# Toolrun Compression

Devlish can save tokens in two different ways:

1. Compile stable workflows so the LLM stops orchestrating every step.
2. Compress tool output while workflows are still being discovered.

`devlish-toolrun` proves the second path. It is a Rust command wrapper that runs
ordinary shell commands, stores raw output locally, and returns compact,
typed JSON to the LLM.

## Runtime Shape

```text
LLM asks for command
  -> devlish-toolrun exec -- <command>
  -> command runs normally
  -> raw stdout/stderr saved to .devlish/toolruns/
  -> model receives compact structured report
  -> raw_ref can be requested only when needed
```

This is intentionally earlier than full Devlish compilation. It optimizes the
discovery phase, where agents still need tools but do not need every line of
every command in context.

## Relationship To The Ruby Compiler

This does not require rewriting the Ruby parser or compiler in Rust.

The boundaries are separate:

- Ruby compiler: Devlish source to AST/IR/bytecode.
- WASM runner: deterministic bytecode execution.
- Rust toolrun: host command execution and model-visible output compression.

Those can coexist. The native compiler migration starts in `crates/devlish_core`,
which compiles the document/assertion workflow subset directly from Devlish
source to the existing bytecode package. `toolrun` does not depend on that
migration, but both pieces can later share the same Rust core.

## First Adapters

The proof includes command-specific summaries for high-frequency development
commands:

- `bundle exec rspec`
- `npm test` and `node --test`
- `git status`
- `rg` and `grep`
- generic fallback with bounded samples

The important design rule is to preserve raw output by reference. Compression
should reduce default context, not erase evidence.

## Next Integration Step

The practical next step is an agent hook or wrapper convention:

```bash
devlish-toolrun exec -- bundle exec rspec
devlish-toolrun exec -- npm test
devlish-toolrun exec -- git status --short --branch
```

After enough runs, Devlish can mine repeated command patterns and promote them
into reviewed `.dvl` workflows. That gives a ladder:

```text
compressed tool calls -> discovered repeatable flow -> reviewed Devlish source -> bytecode/WASM run
```

This is the bridge between RTK-style output savings and Devlish-style workflow
savings.
