# Devlish Toolrun

`devlish-toolrun` is a Rust proof-of-concept for reducing model-visible tool
output. It runs a command, saves the raw stdout/stderr locally, and returns a
compact JSON report that an LLM can consume without paying for the full terminal
log.

This does not replace the Ruby parser or compiler. It sits beside them as a
host-side tool-call proxy.

## Usage

```bash
cargo run -- exec -- bundle exec rspec
cargo run -- exec -- npm test
cargo run -- exec -- git status --short --branch
cargo run -- exec -- rg "Read PDF text" lib spec examples
```

The command output is stored under `.devlish/toolruns/` by default, which is
ignored by Git. The model sees a compact report:

```json
{
  "schema_version": "devlish-toolrun-report-v0",
  "command": ["bundle", "exec", "rspec"],
  "adapter": "rspec",
  "status": "pass",
  "raw": {
    "ref": ".devlish/toolruns/...",
    "stdout_bytes": 12000,
    "stderr_bytes": 0,
    "total_bytes": 12000
  },
  "summary": {
    "examples": 447,
    "failures": 0,
    "duration_s": "9.5"
  },
  "token_accounting": {
    "raw_bytes": 12000,
    "model_visible_bytes": 420,
    "estimated_raw_tokens": 3000,
    "estimated_model_visible_tokens": 105,
    "estimated_tokens_saved": 2895
  }
}
```

## Adapters

- `rspec`: extracts example count, failure count, and duration.
- `node_test`: extracts TAP footer counts from `node --test` and `npm test`.
- `git_status`: reports branch and changed-file count.
- `search`: summarizes `rg` and `grep` match counts with a bounded sample.
- `generic`: stores the full raw log and returns a bounded head/tail sample.

## Why Rust

The hot path is command execution, output scanning, byte counting, and JSON
emission. Rust is a good fit because this binary can be small, fast, standalone,
and safe to run as a shell wrapper or agent hook.

The Devlish language compiler can remain Ruby until the AST/compiler roadmap is
ready. This tool helps before that migration by shrinking deterministic command
I/O in real time.
