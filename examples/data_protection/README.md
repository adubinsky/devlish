# Protecting borrower information and company IP

These are executable disclosure policies using only synthetic data. They
illustrate an operator selecting a restrictive policy for an entire sensitive
session. They do not ask a model to decide whether a payload is sensitive.

## Loan-review NPPI

`nppi.dvl` permits exactly:

- `LoanReview.record_decision` with the approved synthetic token `loan_AB12CD34`
  and one of two review statuses, with no extra fields.
- The fixed acknowledgement `Loan review recorded.`

Everything else is denied, including all model calls, arbitrary HTTP calls,
file writes, raw responses, substituted services, nested payloads, and unknown
tokens. The token is deliberately registered in policy; merely matching a
pattern would let a caller encode personal data in a token. A real deployment
needs a host-owned token registry and an authenticated internal LoanReview
adapter. The model must not supply the registry, service endpoint, or data
classification. This example does not install that adapter.

A business workflow could compute a review result locally from an application,
then submit only the token and status. These examples test the disclosure
boundary; they do not implement an underwriting decision or certify regulatory
compliance.

## Company IP

`company_ip.dvl` permits one exact reviewed announcement, posted to one exact
endpoint, using POST with no caller-supplied headers. A fixed acknowledgement
is also allowed. Source code, unreleased plans, altered announcement text,
extra body fields, header payloads, query strings, and lookalike hosts are denied.

The endpoint is under `.invalid` and must not receive live traffic. Tests use
a fake host. A real host adapter must disable redirects, enforce the approved
TLS destination, and insert authentication independently. A trusted endpoint
string alone does not constrain a redirect or a compromised service.

## Run and review

From the repository root, after building:

```bash
cargo build --manifest-path crates/devlish_core/Cargo.toml
cargo test --manifest-path crates/devlish_core/Cargo.toml --test data_protection

# Golden cases are ordinary Devlish evidence fixtures.
# Choose unused output paths if retaining reports from earlier runs.
crates/devlish_core/target/debug/devlish-core evidence examples/data_protection/nppi.dvl \
  --output /tmp/nppi-evidence.json
crates/devlish_core/target/debug/devlish-core evidence examples/data_protection/company_ip.dvl \
  --output /tmp/company-ip-evidence.json

# Evaluate a proposed effect as data; this does not send a model request.
crates/devlish_core/target/debug/devlish-core run examples/data_protection/nppi.dvl \
  --input '{"effect":"llm_complete","request":{"prompt":"Synthetic SSN 000-00-0000"}}' --quiet
```

The `.cases.json` files pair readable scenarios with exact decisions and
explanations. Integration tests also dispatch each scenario through the real
`PolicyHost` and assert that denied requests never reach a host implementation.
They check that synthetic sensitive values do not appear in decision logs.

## What these examples establish

The rules constrain the implemented host-effect boundary. They are not a
complete DLP system. In particular, `Print`, diagnostic output, program errors,
and unguarded entry points remain outside this interception boundary. Production
sensitive-data sessions must close those channels before handling live NPPI or
company secrets. Restrict log access: even hashes of low-entropy private values
can be guessed offline; hashing is not anonymization.

Do not expand these examples to permit free-form text merely because it lacks
an SSN pattern or claims to be public. Prefer host-classified data and an
explicit, reviewed declassification step. Keep permitted fields, recipients,
and payload sizes narrow and test each newly opened channel.

See [the provenance design](../../docs/POLICY_PROVENANCE.md) for the chain from
reviewed source to runtime, compiled policy, and authorized external tools.
