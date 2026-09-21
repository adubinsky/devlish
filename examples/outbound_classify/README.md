# Outbound classify example

Devlish program that **calls** an LLM (Approach C harness polarity).

## Run with harness CLI

```bash
cargo run -p devlish-core -- harness init-config
# edit ~/.devlish/config.toml if needed
export OPENAI_API_KEY=sk-...   # or OPENROUTER_API_KEY
cargo run -p devlish-core -- harness run examples/outbound_classify/classify.dvl \
  --provider openai --model gpt-4o-mini
```

## Run with HTTP daemon

```bash
cargo run -p devlish-core -- serve --bind 127.0.0.1:7420 --log-level debug
curl -s http://127.0.0.1:7420/v1/health
curl -s http://127.0.0.1:7420/v1/run \
  -H 'content-type: application/json' \
  -d @- <<'JSON'
{
  "source_path": "examples/outbound_classify/classify.dvl",
  "provider": "openai",
  "model": "gpt-4o-mini"
}
JSON
```

Or use the spin-up / tear-down harness:

```bash
./scripts/llm_serve_harness.sh
```

Caller-oriented daemon docs: `docs/SERVE_QUICKSTART.md`.

Requires a configured provider key (`OPENAI_API_KEY` or `OPENROUTER_API_KEY`).
Without a key, the program fails at the `Ask the model` effect with a clear
missing-credential error.
