#!/usr/bin/env bash
# Spin up `devlish serve`, exercise /v1/health + /v1/run with an outbound LLM
# call (OpenAI or OpenRouter), then tear the daemon down.
#
# Usage:
#   ./scripts/llm_serve_harness.sh
#   PROVIDER=openrouter ./scripts/llm_serve_harness.sh
#   BIND=127.0.0.1:7421 ./scripts/llm_serve_harness.sh
#
# Requires OPENAI_API_KEY or OPENROUTER_API_KEY. Anthropic is intentionally
# not used here. When neither key is set for the chosen provider, the script
# still verifies health then exits 0 (health-only).

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PROVIDER="${PROVIDER:-openai}"
MODEL="${MODEL:-}"
BIND="${BIND:-127.0.0.1:17420}"
LOG_LEVEL="${LOG_LEVEL:-debug}"
CLASSIFY="${CLASSIFY:-examples/outbound_classify/classify.dvl}"
TIMEOUT_SECS="${TIMEOUT_SECS:-90}"

resolve_devlish() {
  if [[ -x "$ROOT/crates/devlish_core/target/debug/devlish-core" ]]; then
    echo "$ROOT/crates/devlish_core/target/debug/devlish-core"
  elif [[ -x "$ROOT/crates/devlish_core/target/release/devlish-core" ]]; then
    echo "$ROOT/crates/devlish_core/target/release/devlish-core"
  elif [[ -x "$ROOT/bin/devlish" ]]; then
    echo "$ROOT/bin/devlish"
  elif command -v devlish-core >/dev/null 2>&1; then
    command -v devlish-core
  else
    echo "devlish-core binary not found; building debug…" >&2
    (cd "$ROOT/crates/devlish_core" && cargo build -q)
    echo "$ROOT/crates/devlish_core/target/debug/devlish-core"
  fi
}

DEVLISH="$(resolve_devlish)"
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/devlish-serve-harness.XXXXXX")"
SERVER_LOG="$WORKDIR/serve.log"
PID_FILE="$WORKDIR/serve.pid"

cleanup() {
  local code=$?
  if [[ -f "$PID_FILE" ]]; then
    local pid
    pid="$(cat "$PID_FILE")"
    if kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
    fi
  fi
  if [[ "${KEEP_WORKDIR:-0}" != "1" ]]; then
    rm -rf "$WORKDIR"
  else
    echo "kept workdir: $WORKDIR" >&2
  fi
  exit "$code"
}
trap cleanup EXIT INT TERM

live_llm=0
case "$PROVIDER" in
  openai)
    MODEL="${MODEL:-gpt-4o-mini}"
    if [[ -n "${OPENAI_API_KEY:-}" ]]; then
      live_llm=1
    else
      echo "OPENAI_API_KEY unset; health-only mode" >&2
    fi
    ;;
  openrouter)
    MODEL="${MODEL:-openai/gpt-4o-mini}"
    if [[ -n "${OPENROUTER_API_KEY:-}" ]]; then
      live_llm=1
    else
      echo "OPENROUTER_API_KEY unset; health-only mode" >&2
    fi
    ;;
  *)
    echo "unsupported PROVIDER=$PROVIDER (use openai or openrouter)" >&2
    exit 2
    ;;
esac

echo "==> starting serve on $BIND (log-level=$LOG_LEVEL)"
"$DEVLISH" serve --bind "$BIND" --log-level "$LOG_LEVEL" >"$SERVER_LOG" 2>&1 &
echo $! >"$PID_FILE"

BASE="http://$BIND"
deadline=$((SECONDS + 15))
until curl -sf "$BASE/v1/health" >/dev/null 2>&1; do
  if (( SECONDS >= deadline )); then
    echo "serve failed to become healthy" >&2
    echo "--- serve.log ---" >&2
    cat "$SERVER_LOG" >&2 || true
    exit 1
  fi
  sleep 0.1
done

health="$(curl -sf "$BASE/v1/health")"
echo "==> health: $health"
echo "$health" | grep -q '"ok": *true'

if [[ "$live_llm" != "1" ]]; then
  echo "==> skipped live LLM run (no API key for $PROVIDER)"
  echo "PASS (health-only)"
  exit 0
fi

run_body="$WORKDIR/run.json"
python3 - "$CLASSIFY" "$PROVIDER" "$MODEL" >"$run_body" <<'PY'
import json, sys
path, provider, model = sys.argv[1], sys.argv[2], sys.argv[3]
print(json.dumps({
    "source": open(path).read(),
    "source_path": path,
    "provider": provider,
    "model": model,
    "input": {},
}))
PY

echo "==> POST /v1/run provider=$PROVIDER model=$MODEL"
response_file="$WORKDIR/response.json"
http_code="$(curl -sS -o "$response_file" -w '%{http_code}' \
  -X POST "$BASE/v1/run" \
  -H 'Content-Type: application/json' \
  --max-time "$TIMEOUT_SECS" \
  --data @"$run_body")"

echo "==> HTTP $http_code"
if [[ "$http_code" != "200" ]]; then
  echo "run failed:" >&2
  cat "$response_file" >&2
  echo "--- serve.log ---" >&2
  cat "$SERVER_LOG" >&2 || true
  exit 1
fi

python3 - "$response_file" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
if data.get("error"):
    raise SystemExit(f"unexpected error payload: {data}")
response = data.get("response") if isinstance(data.get("response"), dict) else data
print("top-level keys:", sorted(data.keys()) if isinstance(data, dict) else type(data).__name__)
if isinstance(response, dict) and "classification" in response:
    print("classification:", response["classification"])
PY

echo "PASS"
