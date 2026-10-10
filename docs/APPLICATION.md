# Running Devlish

Install from source with `make install`. The application is installed as
`~/.local/bin/devlish`; that directory must be on PATH. `make install PREFIX=/usr/local`
selects another installation prefix. Build tooling is a contributor requirement,
not a command sequence users repeat when running a workflow.

The application has three entry points:

```bash
devlish
devlish --run workflow.dvl
devlish --server
```

`-r` and `-s` are short forms of the last two modes. `devlish --help` describes
optional settings. Existing developer and audit subcommands remain compatible.

## Model configuration

The prompt uses the existing provider configuration. For OpenRouter, set
`OPENROUTER_API_KEY` in your environment and put your chosen model in
`~/.devlish/config.toml`:

```toml
default_provider = "openrouter"
default_model = "YOUR_OPENROUTER_MODEL"
```

`DEVLISH_CONFIG` can select a different configuration file. OpenAI, Anthropic,
and Ollama are also supported by the existing adapter. No provider request is
made just by opening or closing the prompt.

## Interactive mode

Type a natural-language request at `devlish>`. The terminal UI supplies the
conversation to a Devlish program; Devlish calls the model and governs the
response. Successful turns remain in the in-memory conversation. Failed turns
are not added to that conversation. `/clear` clears it; `/exit`, `/quit`, and EOF
exit. The conversation is bounded to 128 KiB before submitting a turn.

The bundled [prompt program](../runtime/prompt.dvl) asks for a JSON plan with
one through eight steps, validates the entire plan, then executes it in order.
Each step has exactly `action` and `payload`. Supported actions are `ask_model`
(text prompt), `run_tool` (a record with exactly `tool_id` and a list of text
`arguments`), and `respond` (text). Exactly the final step must respond.
For example, a conversational answer is:

```json
{"steps":[{"action":"respond","payload":"Hello."}]}
```

Plans are data, never compiled source. Unknown actions, extra fields, malformed
payloads, and oversized plans fail before any planned effect. Each dispatched
effect still passes the VM's declared permissions, captured policy, and durable
recorder. The bundled permissions and [policy](../runtime/prompt-policy.dvl)
allow model calls and responses only. To enable a specific tool, copy the prompt
program to `.devlish/agent.dvl`, add its explicit `Run catalog tool "tool_id"`
permission, and supply a policy allowing the corresponding request.
Model or tool observations cannot append steps or trigger replanning; the final
response is the validated plan's text. There are no automatic retries.

Every prompt turn, including custom agents, has a host-enforced limit of 50,000
VM instructions and nine effect attempts: at most eight model calls (including
planning), seven tool calls, and one response. Denied and failed attempts count.
These are per-turn call limits, not token or monetary budgets. An operator can
supply `.devlish/limits.json` with exactly `instruction_limit` (1 through
10,000,000) and `effect_budget` (the existing `total`/`per_effect` budget format).
The prompt validates and captures this file once at startup; malformed limits
stop startup. Declaring a larger budget grants no new effects or destinations.
For example, a workflow that records local phase markers can declare:

```json
{"instruction_limit":50000,"effect_budget":{"total":16,"per_effect":{"llm_complete":1,"http_request":1,"read_file":1,"write_file":6,"respond":1}}}
```

See [exact external JSON keys](EXTERNAL_JSON_KEYS.md) for preserving API field
and URL path casing. A recording or execution failure stops the turn; already completed effects are not rolled back.

A project can replace the program with `.devlish/agent.dvl` and the policy with
`.devlish/policy.dvl`. The program receives `conversation` as a serialized,
role-labelled conversation string. These files are compiled once on startup;
edits apply when opening a new prompt. Compilation failures stop startup.
A custom program's local tools use the same local/PATH guardrail as file mode,
require declared capabilities, and must pass the captured policy.

`DEVLISH_DEFAULT_AUTHORIZATION` selects `allow-unless-forbidden` (the prompt
default) or `deny-unless-allowed`. It only resolves explicit policy abstentions.
An explicit prohibition always blocks; malformed policy decisions also block.
The built-in policy makes explicit decisions, so changing the default does not
remove its prohibitions.

Each submitted turn creates a new, exclusive `.devlish/sessions/*.jsonl` log.
Logs record commitments and decisions; raw conversation and effect capture are
disabled. Custom policy reasons are recorded verbatim, so policy authors must
avoid embedding sensitive request data in them. These logs are tamper evidence,
not proof of a protected runtime.
Prompt execution uses the ordinary local profile, not signed production admission.

## File mode

`devlish -r workflow.dvl` loads source, compiles it in memory, runs it, and exits.
No manual compile command is required. Existing execution settings, including
JSON input and explicit policy selection, remain available as optional flags.
See [the local tool example](../examples/local_tools/README.md) for policy posture
and real program execution.

## Server mode

`devlish -s` starts the existing long-lived HTTP service at `127.0.0.1:7420`.
It runs in the foreground until interrupted. A service manager can supervise
this same command as a background daemon; this mode does not detach or create
an operating-system service automatically. Optional `--bind HOST:PORT` changes
the address. `DEVLISH_SERVE_TOKEN` enables bearer authentication.

The HTTP service currently uses its existing shared service host, with a smaller
set of effects than the prompt/file host. Session persistence, governed local
HTTP tool dispatch, and signed production service setup are separate capabilities.
See [the endpoint guide](SERVE_QUICKSTART.md).
