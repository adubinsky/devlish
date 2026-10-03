# Environment Roadmap Status

Tracks Approach C (outbound-LLM focus) implementation against the plan.

Last updated: 2026-09-20

## Shipped in this pass

### Shared service layer
- `devlish_core::service` — structured `service_compile` / `service_run` / `service_lint`
- MCP `compile` / `run` / `lint` call the shared service

### Outbound LLM harness (Jira DEVL-208)
- English: `Ask the model with <prompt> as <name>` (+ optional model string, `expecting json`)
- Permission: `Call language models`
- VM opcode `LLM_COMPLETE` + `HostEffects::llm_complete`
- Crate `crates/devlish_llm` — Anthropic / OpenAI / OpenRouter / Ollama + `~/.devlish/config.toml`
- CLI: `devlish harness run|resume|init-config`
- Sessions under `~/.devlish/sessions/`
- Children: DEVL-209 .. DEVL-214
- DEVL-214: ReplayHost unit coverage for `llm_complete` / `clock_now` / `random_draw`; live spin-up/down via `scripts/llm_serve_harness.sh` (OpenAI/OpenRouter)

### HTTP daemon
- `devlish serve --bind 127.0.0.1:7420 [--log-level info|debug|error]`
- Routes: `/v1/health`, `/v1/compile`, `/v1/run`, `/v1/validate`, `/v1/lint`, `/v1/harness/sessions`, resume
- Auth: `DEVLISH_SERVE_TOKEN` bearer
- Caller guide: `docs/SERVE_QUICKSTART.md`

### Logging (DEVL-109)
- `--log-level` / `DEVLISH_LOG` / `--quiet` (= error)
- Debug: HTTP body previews, LLM prompt sizes, VM instruction events

### Language Tier-0 partial (DEVL-138 / DEVL-139)
- `Get the current time as <name>` → `CLOCK_NOW` (journaled)
- `Draw a random number [between low and high] as <name>` → `RANDOM_DRAW` (journaled)
- Permissions: `Clock`, `Randomness`

## Remaining language cut

| Ticket | Status | Notes |
|--------|--------|-------|
| DEVL-136 L6 arithmetic | Done in tree; close in Jira | modulo / // / ** |
| DEVL-135 L5 bytes | Deferred | Needs tagged bytes value type |
| DEVL-137 L7 bitwise | Deferred | Until struct demand |
| DEVL-140 L10 formatting | Deferred | Prompt templates use string concat for now |
| DEVL-141 L11 iterators | Deferred | |
| DEVL-142 L12 typed errors | Partial | `Fail with record` exists; richer taxonomy later |
| DEVL-186 L13 concurrency | Deferred | |
| DEVL-143..157 Tier-1 | In progress | math constants shipped; json placeholder; rest follow PYTHON_STDLIB_PARITY_PLAN |
| DEVL-177..190 | Deferred | Effect-gated tourist modules |
| DEVL-201..207 | Open | Reliability bugs — next pass |

## Example

`examples/outbound_classify/` — classify a support note via `Ask the model`.
