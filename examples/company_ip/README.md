# Protecting company IP in a Devlish-authored harness

A release-note assistant receives a private internal design draft. The model may
choose the order of two public excerpts. It cannot write publication text, choose
a destination, approve a private document, or request arbitrary tools.

The complete workflow is in `agent.dvl`; the independent effect rules are in
`policy.dvl`. Rust supplies compilation, governed execution, recording and fake
host adapters for tests. No second model is needed to interpret an allow/deny
rule: the publication text must equal one of two explicit strings paired with
its approved excerpt ID, and the channel must equal `public-release-notes`.

```bash
cargo test --locked --manifest-path crates/devlish_core/Cargo.toml --test company_ip

devlish compile examples/company_ip/agent.dvl --output company-ip-agent.json
devlish compile examples/company_ip/policy.dvl --output company-ip-policy.json
devlish report policy company-ip-policy.json examples/company_ip/policy.cases.json \
  --output company-ip-policy-report.json
devlish report explain company-ip-policy-report.json
```

## What the model can decide

A valid model plan contains both excerpt IDs exactly once, in either order:

```json
{
  "steps": [
    {"action": "publish_excerpt", "excerpt_id": "public_integration"},
    {"action": "publish_excerpt", "excerpt_id": "public_overview"}
  ]
}
```

Devlish validates the entire plan before calling `publicnotes.create`. An invalid
second step prevents the first publication too. Extra fields, arbitrary text,
unknown IDs/actions and repeated steps are rejected. The program supplies the
public text and destination locally. Neither private drafts nor service results
are incorporated into prompts, publications or the completion response.

The prompt itself is fixed and independently allowed by the policy. A modified
agent cannot send a draft to the model, substitute private or encoded text into
a publication, or return private text directly. The policy uses exact permitted
values, not keyword detection or an LLM's judgment about confidentiality.

`publication_requested` is a caller preference, not an authorization claim. A
false value exits before contacting the model. A true value grants no access to
private material; the separate policy still permits only the two public excerpts.
The example does not implement document classification or public-content approval.
An operator must approve changes to the policy and release bundle independently.

## Tests and audit evidence

`plan.cases.json` has 19 plan scenarios; `policy.cases.json` has 17 effect cases.
The six integration tests cover both approved orders, malformed and malicious
plans, invalid input, modified-agent egress attempts, encoded private text,
failures at all nine recording positions, either publication tool failing, and
repeatable decisions. Tool replies contain synthetic private material and hostile
instructions; they cannot cause another model call or tool action.

The shared `GovernedRun` enforces a 50,000-instruction limit and four total effect
attempts: at most one model call, two service calls and one response. Denied or
failed attempts consume budget. A recorder or tool failure stops the run without
retry; already completed effects are not rolled back. A response delivered before
its outcome/final-record failure cannot be retracted by this runner. The HTTP
adapter separately buffers responses until final recording succeeds.

The report integration test writes application and policy reports, then replays
the captured execution twice with identical output and no live effects. Replay
capture is explicitly enabled only for this synthetic test; its raw model/tool
responses are sensitive evidence. Normal test runs use digest-only records and
check that private markers do not appear in them. Neither these reports nor the
unsigned test application manifest prove execution origin or regulatory compliance.
Signed deployment admission and independent receipt verification are separate.

## Deployment boundary

This example is executable against fake adapters only. It neither publishes
anything nor contacts a provider. The model route in `catalog.json` is a placeholder
that must be replaced with an operator-approved model before signing a deployment.
There is no production PublicNotes adapter: one must authenticate to a fixed,
operator-controlled destination, preserve policy enforcement, and handle uncertain
outcomes and idempotency before this becomes a publication service. The existing
NPPI workflow is in `../governed_agent`.
