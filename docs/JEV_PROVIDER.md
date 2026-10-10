# Typed JEV document batches (DEVL-216)

The `devlish_llm::jev` adapter evaluates readable document text against named,
versioned questions using TypeSafe's fixed HTTPS endpoint. `Ask JEV with batch
as evaluation` compiles to the existing `LLM_COMPLETE` operation and
`llm_complete` host effect. It uses the same manifest permission, executable
policy, release permissions, attempt budget, dispatch deadline and audit
recorder as other governed model calls. It never falls back to another provider.

Live calls are available only through an authenticated model route in a
verified session. Plain `run`, legacy `harness run`, and the legacy HTTP/MCP
service cannot dispatch JEV. The verified CLI and verified service share the
same admitted session and native host. A parser statement alone is not admission.

## The DVL workflow

See [check.dvl](../examples/jev_checklist/check.dvl),
[policy.dvl](../examples/jev_checklist/policy.dvl), and their catalog and
permission controls. The essential call is:

```text
Permissions:
  Call language models

Ask "Which document batch should be checked?" as batch
Ask JEV with batch as evaluation
Respond with evaluation
```

This last statement requests a disclosure effect too. The separate policy must
authorize the result and destination before the native response sink receives it.
The complete example permits only a fixed synthetic document and exact versioned
questions. It does not permit checklist writes or human approval.

The approved catalog route pins `provider: jev`, an explicit model version,
`credential: JEV_API_KEY`, timeout and byte limits. JEV routes reject model
aliases, arbitrary endpoints, alternate credentials and chat-only token controls.
The authenticated release binds this catalog and permissions along with the
program, policy and runtime. Changing a catalog file invalidates its release pin.

Run the full example with real security checks and recorded provider data offline:

```bash
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
  cargo test --locked --manifest-path crates/devlish_core/Cargo.toml --test jev_checklist
```

For an actual deployed invocation, the operator must build and approve the new
runtime and compiled DVL/policy, include the catalog/permissions in the signed
release, and provide the protected profile and credentials:

```bash
DEVLISH_VERIFIED_PROFILE=/operator/jev/profile.json \
  devlish-core run-verified --policy-log /protected/logs/new-session.jsonl \
  --session-id document-check-001 --input '{"batch": <document-batch-object>}'
```

The input placeholder must be replaced with valid JSON. This is an invocation
shape, not a prebuilt production release. Existing approval, signing, trust,
revocation and admission-state requirements remain unchanged. Do not substitute
a test-generated signing key for the operator's production trust.

## Recorded fixture verification

The standalone Rust example has been removed. Recorded synthetic responses live
under `crates/devlish_llm/tests/fixtures/` and are exercised by adapter unit tests:

```bash
cargo test --locked --manifest-path crates/devlish_llm/Cargo.toml
```

Live document checks must enter through the admitted, governed runtime described
above. Provider libraries and test fixtures are infrastructure, not deployable
user workflow entry points. See `AGENTS.md` for the mandatory execution boundary.

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
  match the actual returned model. Verified JEV routes require pinned versions.
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

Live transport is crate-private and dispatched by `ApprovedModel` after native
verified admission and effect authorization. Credentials resolve from the
operator-owned credential environment/directory, not program-local dotenv files
or caller-selected keys. Safe provider events go through the native logger.
The signed route supplies timeout and byte limits (each JEV payload is capped
at 1 MiB). Redirects and environment HTTP proxies are disabled.

Every authorized model effect makes exactly one provider attempt. There are no
hidden transport retries, including 429/529 responses. Any retry must be a new
governed effect, receive another policy decision, and consume an attempt budget.
The idempotency header is a correlation aid, not a documented server deduplication
guarantee. Transport failures are outcome-uncertain. Investigate before retrying.

Events include accepted, started, decoded, validation_failed, failed
and completed. The allowlist includes correlation ID, document ID/text digest,
question IDs/versions, model, attempt, elapsed time, request ID and token usage.
It excludes document text, instructions, answers, credentials, raw HTTP bodies
and signed URLs. Use opaque identifiers, not customer names, for metadata.
Errors expose categories and HTTP status without private provider messages.
Ordinary verified policy logs retain request/result/error hashes and policy
decisions without raw text or answers. Full typed exchanges for offline replay
require the existing operator-authorized evidence opt-in and protected storage.
Verified completion does not return the VM's private context/results. This
sample permits disclosure only through a policy-authorized `Respond` effect.

## DealStar integration still to build

This slice now supplies a DVL statement, approved JEV route, governed execution
tests, and synthetic program/policy/catalog examples. It does not yet connect
Rails jobs, change checklist state, write human activity history or forward
events to Sentry automatically. Those application connections remain to build.

DealStar should keep deterministic folder presence checks local, use this
provider for semantic checks in shadow mode, persist immutable results, and
apply calibrated decision thresholds in a separate policy layer. A model
answer is not human approval and must not mark a checklist item done. Route
uncertainty and conflicts to review/report findings. Persist user-readable
activity explaining every route and checklist state change with the same
correlation ID, alongside existing development/staging diagnostics.
