# Devlish Serve Quick Start

Last updated: 2026-09-20

`devlish --server` is a long-lived HTTP daemon around the same compile/run/lint
service used by the CLI and MCP. Callers build applications by talking JSON
over HTTP — no Devlish embedding required.

## What a caller sees

1. Start one daemon process (`devlish --server`).
2. `GET /v1/health` to confirm it is up.
3. `POST /v1/compile`, `/v1/run`, `/v1/lint` (or `/v1/validate`) with JSON bodies.
4. Optional: `POST /v1/harness/sessions` for an outbound-LLM run. This route
   currently returns an execution result; durable HTTP session management is
   separate work.
5. Stop the process (SIGINT / kill). There is no separate shutdown protocol.

Auth is optional. If `DEVLISH_SERVE_TOKEN` is set, every route except
`/v1/health` requires `Authorization: Bearer <token>`.

Outbound LLM calls (`Ask the model` in source) use the server process
environment for credentials (`OPENAI_API_KEY`, `OPENROUTER_API_KEY`,
`ANTHROPIC_API_KEY`, or Ollama locally). Prefer OpenAI or OpenRouter when
Anthropic is unavailable.

## Quick start (5 minutes)

### 1. Build

```bash
make install
```

This installs the native `devlish` application in `~/.local/bin`; put that
directory on PATH if needed.

### 2. Start the daemon

```bash
# From the repo root
export OPENAI_API_KEY=sk-...          # or OPENROUTER_API_KEY
# optional: export DEVLISH_SERVE_TOKEN=secret

devlish --server
```

Flags / env:

| Flag / env | Meaning |
|---|---|
| `--bind HOST:PORT` | Listen address (default `127.0.0.1:7420`) |
| `--tools-dir DIR` | Optional tools directory (reserved for MCP-style tool discovery) |
| `--log-level LEVEL` | `error` \| `info` \| `debug` (default `info`) |
| `DEVLISH_LOG` | Same as `--log-level` when the flag is omitted |
| `--quiet` / `-q` | Forces log level `error` |
| `DEVLISH_SERVE_TOKEN` | Bearer token for non-health routes |

Debug logging prints request/response previews and LLM prompt sizes on stderr.
Info logging prints listen address and each `METHOD path -> status`.

### 3. Health check

```bash
curl -s http://127.0.0.1:7420/v1/health
# {"ok":true,"service":"devlish","version":"0.1.0"}
```

### 4. Compile

```bash
curl -s -X POST http://127.0.0.1:7420/v1/compile \
  -H 'Content-Type: application/json' \
  -d '{"source":"Print \"hi\"\n"}'
```

Returns the bytecode package JSON on success, or `{ "error", "diagnostics" }`
with HTTP 400 on failure.

### 5. Run (including outbound LLM)

```bash
curl -s -X POST http://127.0.0.1:7420/v1/run \
  -H 'Content-Type: application/json' \
  -d @- <<'EOF'
{
  "source": "Permissions:\n  Call language models\n  Clock\n\nGet the current time as started_at\nnote equals \"Customer asked for a refund.\"\nprompt equals \"Classify as billing, product, or other. Reply JSON {\\\"category\\\":\\\"...\\\"}. Note: Customer asked for a refund.\"\nAsk the model with prompt expecting json as classification\nRespond with record with classification as classification and started_at as started_at\n",
  "provider": "openai",
  "model": "gpt-4o-mini",
  "input": {}
}
EOF
```

Or point at a file on the server host:

```bash
curl -s -X POST http://127.0.0.1:7420/v1/run \
  -H 'Content-Type: application/json' \
  -d '{
    "source_path": "examples/outbound_classify/classify.dvl",
    "provider": "openai",
    "model": "gpt-4o-mini",
    "input": {}
  }'
```

`/v1/run` body fields:

| Field | Required | Meaning |
|---|---|---|
| `source` | one of source / source_path | Devlish source text |
| `source_path` | one of source / source_path | Path readable by the daemon |
| `input` | no | JSON object bound as program input (default `{}`) |
| `provider` | no | `openai`, `openrouter`, `anthropic`, `ollama` |
| `model` | no | Provider model id |

### 6. Lint / validate

```bash
curl -s -X POST http://127.0.0.1:7420/v1/lint \
  -H 'Content-Type: application/json' \
  -d '{"source":"Print \"hi\"\n"}'
```

`/v1/validate` is an alias of `/v1/lint`.

### 7. Harness session (optional)

```bash
curl -s -X POST http://127.0.0.1:7420/v1/harness/sessions \
  -H 'Content-Type: application/json' \
  -d '{
    "source_path": "examples/outbound_classify/classify.dvl",
    "provider": "openai",
    "model": "gpt-4o-mini"
  }'
```

Same body shape as `/v1/run`. Resume:

```bash
curl -s -X POST http://127.0.0.1:7420/v1/harness/sessions/<id>/resume \
  -H 'Content-Type: application/json' \
  -d '{ "source_path": "...", "input": { "...": "..." } }'
```

## Minimal client sketch (caller app)

```python
import json, os, urllib.request

BASE = os.environ.get("DEVLISH_URL", "http://127.0.0.1:7420")
TOKEN = os.environ.get("DEVLISH_SERVE_TOKEN")

def call(method, path, body=None):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(f"{BASE}{path}", data=data, method=method)
    if data is not None:
        req.add_header("Content-Type", "application/json")
    if TOKEN:
        req.add_header("Authorization", f"Bearer {TOKEN}")
    with urllib.request.urlopen(req) as resp:
        return json.load(resp)

assert call("GET", "/v1/health")["ok"]
result = call("POST", "/v1/run", {
    "source_path": "examples/outbound_classify/classify.dvl",
    "provider": "openai",
    "model": "gpt-4o-mini",
})
print(json.dumps(result, indent=2))
```

## CLI equivalents (same service layer)

```bash
# One-shot run (no daemon)
bin/devlish run examples/outbound_classify/classify.dvl \
  --provider openai --model gpt-4o-mini --log-level debug

# Session transcript under ~/.devlish/sessions/
bin/devlish harness run examples/outbound_classify/classify.dvl \
  --provider openai --model gpt-4o-mini
```

## Spin-up / tear-down test harness

```bash
# Uses OPENAI_API_KEY (or PROVIDER=openrouter + OPENROUTER_API_KEY)
./scripts/llm_serve_harness.sh
```

The script starts serve on `127.0.0.1:17420`, checks health, optionally posts
`/v1/run` against `examples/outbound_classify/classify.dvl`, then kills the
daemon. Set `KEEP_WORKDIR=1` to inspect logs under the temp directory.

## Logging for operators

- Default: `info` — listen line, each request `METHOD path -> status`, LLM
  completion summaries.
- `--log-level debug` / `DEVLISH_LOG=debug` — request/response body previews,
  LLM prompt character counts, VM instruction events when running via CLI.
- `--quiet` — errors only.

## Endpoints summary

| Method | Path | Body |
|---|---|---|
| GET | `/v1/health` | — |
| POST | `/v1/compile` | `{ source, source_path? }` |
| POST | `/v1/run` | `{ source \| source_path, input?, provider?, model? }` |
| POST | `/v1/lint` | `{ source }` |
| POST | `/v1/validate` | `{ source }` |
| POST | `/v1/harness/sessions` | same as `/v1/run` |
| POST | `/v1/harness/sessions/:id/resume` | same as `/v1/run` |
