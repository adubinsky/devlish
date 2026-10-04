# A Devlish-authored planning harness

This is the first finite agent slice for DEVL-208. Planning, plan validation,
tool selection, local processing and completion are all written in `agent.dvl`.
The separate `policy.dvl` governs effects at the host boundary. Rust supplies the
existing compiler/VM, recorder and test adapters; it does not select or execute
the plan's steps on behalf of this program.

Run the synthetic tests without credentials or network access:

```bash
cargo test --locked --manifest-path crates/devlish_core/Cargo.toml --test governed_agent
```

You can compile and inspect the program and policy independently:

```bash
devlish compile examples/governed_agent/agent.dvl --output agent.json
devlish compile examples/governed_agent/policy.dvl --output agent-policy.json
devlish report policy agent-policy.json examples/governed_agent/policy.cases.json \
  --output agent-policy-report.json
devlish report explain agent-policy-report.json
```

## What happens

Two local synthetic case records contain a `needs_review` flag and private
borrower information. The program checks the flag types before contacting the
model. It sends a fixed approved prompt containing only two synthetic opaque
tokens, asking the model to choose the order in which to record their statuses.
Names, SSNs, internal notes and review flags are absent from the prompt.

An acceptable model response is:

```json
{
  "steps": [
    {"action": "record_review", "loan_token": "loan_CD56EF78"},
    {"action": "record_review", "loan_token": "loan_AB12CD34"}
  ]
}
```

Devlish validates the entire plan before either business tool runs: exactly two
steps, both tokens once, only `record_review`, exact field counts and types. An
invalid second step prevents the first write too. Model text is never compiled,
evaluated, used as a URL, or allowed to select a provider or credential.

Only after validation does Devlish iterate through the plan. It derives each
status from the corresponding local flag and invokes `loanreview.create` with
only `loan_token` and `status`. The tool's returned data cannot become a new
model prompt, step or public response. The final response is fixed:
`Review statuses recorded.` These statuses are synthetic workflow flags, not
credit decisions or financial recommendations.

The fixed prompt and permitted effects are enforced again in `policy.dvl`.
Changing the agent to send a private record to the model, include a borrower in
tool arguments, invent a status or return private data is blocked by that
separate policy. The policy restricts effect payloads; the agent's complete-plan
validation and instruction budget enforce sequencing and counts.

## Bounds and evidence

There is one model call and at most two business tool calls per run.
`permissions.json` sets a 50,000-instruction VM limit and lists the same host
effects as `catalog.json`: `llm_complete`, `call_service`, `respond`. The test
host consumes these files and wraps every effect in `PolicyHost`. Every decision
is recorded before dispatch; an unrecordable decision prevents the effect.

`plan.cases.json` contains 18 readable plan scenarios: both approved orders and
16 malformed, repeated, extra-field or unauthorized plans. Tests also cover
invalid private inputs, independently denied egress, failure at each of eight
decision/outcome recording positions, and failure of either business tool.
Deterministic fake-model replies reproduce the same recorded decisions.
`policy.cases.json` adds 14 golden effect decisions, independently runnable with
`report policy`. The shared `GovernedRun` used by the verified CLI also runs all 18 plans and
produces the same decisions and host calls. An integration test captures a synthetic execution and runs the
application, policy and process reports twice. The process report reproduces the
saved model/tool exchanges without any live effects and does not claim runtime
attestation.

A tool or recording failure stops the current run. Already completed effects
are not rolled back. A missing outcome means uncertainty; neither this program
nor its test runner automatically retries or replans. Durable idempotency and
safe recovery across a new run remain required before production use.

## Integration boundary

The model and LoanReview service in these tests are in-memory adapters. There is
no production LoanReview implementation in this example. Verified CLI model calls now require the catalog's `llm_route`: a fixed OpenRouter
HTTPS endpoint, exact model, credential lookup name and bounded transport. The
example catalog uses a deliberately synthetic model; replace it with an approved
model before compiling/signing a deployment. No live provider request is made
by these example tests. Protected credential storage and LoanReview service
identity still need operator-controlled adapters.
These example tokens are fixed allowlists, not a production tokenization system.

The shared `governed_run::GovernedRun` runs the pinned program and policy with
a finite instruction budget, disables debug events, intersects host permissions,
redacts policy diagnostics and requires the final run record before reporting
success. It returns only `response_emitted` and `paused`; private VM envelopes
and checkpoint contents are inaccessible through that result. `run-verified`
now uses this runner. The authenticated loopback `serve-verified` adapter uses shared signed admission
and creates the same bound start record. Other adapters must do so too. This API alone
does not verify releases or authenticate its caller.

A host must expose only the policy-approved response. The VM's internal context
and results contain private inputs and untrusted model/tool results; dumping
that envelope, raw diagnostics or debug events would bypass the intended output
boundary. Responses are still immediate effects: if recording subsequently fails, an
already delivered response cannot be retracted, and the runner returns failure.
Tests disable debug events and assert what reaches the model, service,
response sink and default digest-only recorder. Sensitive process-replay capture
requires separate operator consent and protected storage.

For signed deployment, the exact compiled agent, policy, permissions and catalog
must be bound into an approved release; these source examples do not authenticate
a runtime or prove protected execution. Current verified-profile mode continues
to block legacy harness/server/MCP routes. This example establishes the
Devlish control loop and its tests, not completed production isolation.
