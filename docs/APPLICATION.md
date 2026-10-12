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

## Interactive harness mode

Bare `devlish`, `devlish harness`, and `devlish harness interactive` open the harness.
`help` and `/help` list local commands without calling a model. `/run FILE` runs
the literal file path through the existing governed harness, including policy,
limits, and audit recording. Use `devlish harness run` for additional flags.

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

## Harness artifact generation

The harness can ask the configured live model to author a program and its
acceptance policy from an operator-written text contract:

```bash
devlish harness generate contract.txt --output-dir /absolute/challenge \
  --policy-log /absolute/generation-policy.jsonl
```

The output directory must exist. The harness refuses existing `program.dvl`
or `policy.dvl` paths. The model receives the current language reference,
grammar, effect-policy reference, and contract; it returns JSON string fields
`program` and `policy`. A fixed authoring workflow calls the model and writes
those strings unchanged through the shared policy-governed runtime. The
fixed authoring policy permits model completion, response, and writes to only
these two operator-selected paths. The generated policy never governs its own
generation. Generation does not execute the generated program.

The command prints a session path under `~/.devlish/sessions/`. That session
captures the authoring workflow and returned artifact text, including failed
runs. Sessions may contain sensitive model output; keep them local. A failed
write can leave a partial artifact pair: inspect the log before retrying, and
use a new output directory for another generation.

Acceptance is a separate invocation with the generated policy and a fresh log:

```bash
devlish harness run /absolute/challenge/program.dvl \
  --policy /absolute/challenge/policy.dvl \
  --policy-log /absolute/acceptance-policy.jsonl \
  --default-authorization deny-unless-allowed
```

Compare both artifact files with the generation session's `result.response`
fields before acceptance. Generation provenance alone does not prove that an
acceptance contract passed.

## Governed harness execution

`harness run` and `harness resume` require an independent policy. Select it with
`--policy FILE`, or place `.devlish/policy.dvl` or `policy.dvl` beside the source.
The default posture is `deny-unless-allowed`; override it explicitly with
`--default-authorization`. Absent permission declarations grant no external
effects on governed service runs. A policy cannot expand declared permissions.

Every invocation creates a fresh hash-chained log beside the source under
`.devlish/sessions/`, unless `--policy-log FILE` names a new log. Existing logs
are never overwritten. Recorder failures stop effects and prevent success.

Execution controls come from `--limits FILE` or `.devlish/limits.json` beside
the source. They contain exactly these fields:

```json
{"instruction_limit":50000,"effect_budget":{"total":16,"per_effect":{"read_file":1,"write_file":6,"llm_complete":1,"http_request":1,"respond":1}}}
```

Without a limits file, each run is capped at 50,000 instructions and 100 effect
attempts. Limits are captured before dispatch in `execution_limits_captured`.
Denied and failed attempts count; omitted per-effect caps still share the total.
Malformed controls fail before effects. Limits never grant permissions, and
resume uses fresh operator controls rather than permissions from saved output.
Resume reruns the source with checkpoint input; it does not reconcile uncertain
external effects or guarantee exactly-once execution.

The shared file host honors append mode, preserving state markers. Google
Address Validation credentials remain host-owned: only the exact POST endpoint
receives `X-Goog-Api-Key`, with redirects disabled. Session result files can
contain program output; effect audit evidence remains hashed by default.

## Default model provider

New configurations prefer OpenRouter (`openai/gpt-4o-mini`). If an implicit
OpenRouter selection has no nonempty configured API key and OpenAI has one,
Devlish selects OpenAI before making a request, using its configured model.
An explicit `--provider` or program provider choice is respected. Failed
requests never trigger a second provider call. Configure credentials using
`OPENROUTER_API_KEY` and `OPENAI_API_KEY`, or provider `api_key_env` overrides.
Existing configuration files retain their explicit settings.

## Restricted harness authoring

`harness generate` validates the entire returned artifact pair before either
file is installed. Both artifacts must be nonempty flat Devlish of at most
128 KiB each and compile successfully; the policy must have a valid Rule
identity. Imports and modules are rejected before any filesystem resolution.
The generated policy cannot perform external effects.

Without `--requirements FILE`, generated programs receive no external authority.
To approve a limited program, provide operator-owned JSON, for example:

```json
{
  "permissions": [
    {"kind":"read_file","scope":"document.txt"},
    {"kind":"write_file","scope":"run-state.log"},
    {"kind":"llm_complete"},
    {"kind":"http_request","scope":"https://addressvalidation.googleapis.com/v1:validateAddress"}
  ],
  "write_contents": {
    "run-state.log": ["started\n","document_loaded\n","extraction_requested\n","extraction_validated\n","validation_requested\n","completed\n","needs_review\n"]
  }
}
```

```bash
devlish harness generate contract.txt --output-dir /absolute/new-artifacts \
  --policy-log /absolute/new-generation.jsonl --requirements requirements.json
```

Model declarations must be a subset of these exact permission records. Reads
and HTTP requests require literal approved paths/endpoints. Writes require a
literal approved path and literal text from `write_contents`; dynamic model
output cannot become file content. Python and shell output extensions are
prohibited. Process execution, services, broad filesystem authority, copies,
moves, downloads, routes, and direct Print are outside this authoring profile.
Consequently shell and chmod requests are rejected even inside branches or
recovery blocks. Other permitted effects still require declarations and policy
approval at execution time.

Requirements are validated before the model request and their hash is recorded
before dispatch. Rejected model output produces a failed effect outcome and no
artifact writes. Installation creates fresh files exclusively, refuses existing
files and symlinks, syncs their content, and uses mode 0600 on Unix. A failure
installing the second file can still leave the first file; inspect the audit.

This restricts authoring capabilities, not arbitrary text by programming-language
heuristics. Operator-approved literal content is trusted. It does not prove the
model-generated policy implements the intended business contract: review both
artifacts before separately running them. The profile deliberately forbids
subprocesses; general OS containment for approved external tools remains a
separate boundary. It does not sandbox Codex or user shell commands.

Structured responses accept complete JSON, Markdown JSON fences, and one layer
of JSON string encoding. OpenAI JSON requests use JSON object response mode
([API documentation](https://help.openai.com/en/articles/8555517-function-calling-in-the-openai-api)).
Plan shape validation remains mandatory; parsing does not manufacture steps or
retry failed model calls.
