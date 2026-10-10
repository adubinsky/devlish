# Typed JEV document batches (DEVL-216)

The `devlish_llm::jev` adapter evaluates readable document text against named,
versioned questions using TypeSafe's fixed HTTPS endpoint. It is separate from
the existing chat completion providers. A failed JEV request never falls back
to OpenAI or another model.

## Run the synthetic example

From the repository root:

```bash
cargo run --manifest-path crates/devlish_llm/Cargo.toml --example jev_document_batch
```

The default run validates a recorded response fixture without contacting
the provider. The response was captured from a live synthetic smoke test on
October 9, 2026, using `jev-1.13.0` (577 input tokens, 116 output tokens). The
batch includes document classification, a liability assertion, a yes/no
question, and a rubric score. To evaluate the same synthetic document live,
provide `JEV_API_KEY` through your existing credential environment and run:

```bash
cargo run --manifest-path crates/devlish_llm/Cargo.toml --example jev_document_batch -- --live
```

The example prints the typed result to stdout and safe execution events to
stderr. Results are application data and should go into protected persistence,
not general logs. No real seller document is included in this example.

```bash
cargo test --manifest-path crates/devlish_llm/Cargo.toml
```

## Contract

- `Batch` carries correlation, idempotency and document identifiers, readable
  text, requested model, and a version for each question. The HTTP body carries
  only `state`, `model`, and `questions`.
- `Choice` uses explicit outcomes such as `matches`, `differs`, and
  `not_in_document`. `Score` returns a fractional probability-weighted rubric
  value. `Noul` returns P(yes), not a selected boolean or its confidence.
- This initial adapter supports string instructions and criteria. The wider
  TypeSafe API also supports structured instructions and criteria.
- Responses must contain exactly the requested question IDs and matching
  answer types. Probabilities, confidence, choice keys, score bounds, weighted
  scores and rubric legends are validated. A pinned requested model must
  match the actual returned model. Alias requests preserve the actual model.
- Results preserve question versions, token usage, optional provider request
  ID, local identifiers and SHA-256 of the exact UTF-8 text sent. That digest
  is not the source PDF checksum. DealStar must retain its source checksum
  separately.
- JEV does not provide arbitrary number/date extraction or document page
  citations through these three primitives. Do not manufacture evidence or
  provenance fields. Use explicit assertions about values or deterministic
  extraction where appropriate.

The API boundary is documented in the [official API reference](https://docs.typesafe.ai/api).

## Execution and logs

`evaluate` accepts a `CredentialResolver` and an event callback. Callers can
route events to the existing harness logger and error monitoring provider.
Credentials are resolved only under `JEV_API_KEY`. Requests have a 30-second
timeout and a 1 MiB encoded request/response limit. Redirects and environment
HTTP proxies are disabled.

Explicit 429 and 529 responses receive at most two retries, with 1- and
2-second backoff. Headers and payload remain identical across retries. The
idempotency header is a correlation aid: TypeSafe does not document a server
deduplication guarantee. Transport failures are marked outcome-uncertain and
are not retried automatically. Callers should investigate before replaying.

Events include accepted, started, retry, decoded, validation_failed, failed
and completed. The allowlist includes correlation ID, document ID/text digest,
question IDs/versions, model, attempt, elapsed time, request ID and token usage.
It excludes document text, instructions, answers, credentials, raw HTTP bodies
and signed URLs. Use opaque identifiers, not customer names, for metadata.
Errors expose categories and HTTP status without private provider messages.
The callback should be fast and should not panic.

## DealStar integration still to build

This slice supplies a Rust API and runnable example. It does not yet expose a
Devlish language statement, connect Rails jobs, change checklist state, write
human activity history or forward events to Sentry automatically. The generic
`Ask the model` statement remains unchanged. Before adding a document batch
statement, give its journal a redacted input/output policy so raw document text
and typed answers are not accidentally saved in generic execution logs.

DealStar should keep deterministic folder presence checks local, use this
provider for semantic checks in shadow mode, persist immutable results, and
apply calibrated decision thresholds in a separate policy layer. A model
answer is not human approval and must not mark a checklist item done. Route
uncertainty and conflicts to review/report findings. Persist user-readable
activity explaining every route and checklist state change with the same
correlation ID, alongside existing development/staging diagnostics.
