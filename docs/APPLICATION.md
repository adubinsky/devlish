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

The bundled [prompt program](../runtime/prompt.dvl) calls the model and displays
its answer. Its [policy](../runtime/prompt-policy.dvl) explicitly allows these
two effects and denies others. This initial application prompt is conversational;
a general coding agent loop is separate work.

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
