# Governed address validation

Run from a directory containing `document.txt`, this workflow, and its policy:

```bash
devlish harness run workflow.dvl --provider openai --model gpt-4.1
```

Supply `OPENAI_API_KEY` and `GOOGLE_ADDRESS_VALIDATION_API_KEY` through the host
credential environment. The model sees the bounded document as untrusted data;
Google receives only an address copied literally from it. No secret appears in
the workflow. The independent policy restricts paths, prompt, payload, endpoint,
and output. `run-state.log` preserves fixed stage markers.

Provide `.devlish/limits.json` beside the source to limit the model and HTTP
calls to one each, file reads to one, writes to six, responses to one, and total
effect attempts to sixteen, with an instruction limit of 50,000. Without that
file the harness's default limits apply. Failures do not retry. A `needs_review`
result means the conservative acceptance conditions were not all satisfied.
This is not a deliverability guarantee or resumable transaction.
