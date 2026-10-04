# Independent audit verifier: first increment

`devlish-audit` is a separate offline executable with no dependency on the Devlish
compiler, VM, model providers or service host. It verifies a detached Ed25519
signature over exact file bytes using explicitly supplied operator trust keys.
It never executes the candidate artifact or discovers keys from its envelope.
This is the first increment of DEVL-223, not the completed end-to-end audit chain.

## Build and synthetic demonstration

From the repository root:

```bash
cargo build --locked --release --manifest-path crates/devlish_audit/Cargo.toml
cargo run --locked --manifest-path crates/devlish_audit/Cargo.toml \
  --example signing_demo -- /tmp/devlish-signature-demo
crates/devlish_audit/target/release/devlish-audit verify \
  /tmp/devlish-signature-demo/evidence.txt \
  --signature /tmp/devlish-signature-demo/signature.json \
  --trust /tmp/devlish-signature-demo/trust.json \
  --purpose audit-evidence
```

Use a new demo directory. The example generates an ephemeral private key in
memory and writes only synthetic evidence, a signature and a public trust
configuration. It is not a production signer or an authorized Devlish release.
Never adopt a trust configuration simply because it accompanies an artifact.

Changing even whitespace in the evidence makes verification fail. Setting the
key's `revoked` field to true also makes verification fail. The command returns
JSON and exit code 0 for a valid authorized signature; malformed input, an
unknown/revoked key, unauthorized purpose or an invalid signature returns JSON
with `signature_verified: false` and a nonzero exit code. No unsigned fallback
exists. All three options are required and may appear in any order.

## Signature and trust format

The signature envelope contains exactly these fields:

```json
{
  "format": "devlish-detached-signature",
  "format_version": 1,
  "algorithm": "ed25519",
  "key_id": "operator-release-key",
  "purpose": "release-artifact",
  "signature_hex": "<128 hexadecimal characters>"
}
```

The separately provisioned operator trust configuration is:

```json
{
  "format": "devlish-audit-trust",
  "format_version": 1,
  "keys": [{
    "id": "operator-release-key",
    "public_key_hex": "<64 hexadecimal characters: raw Ed25519 public key>",
    "purposes": ["release-artifact"],
    "revoked": false
  }]
}
```

Purposes are `release-artifact`, `release-manifest`, `build-statement`,
`audit-evidence` and `audit-receipt`. Each key must have a unique
ID and public key, a nonempty list of distinct allowed purposes, and an explicit
revocation flag. Unknown fields and duplicate JSON fields are rejected.

The signed message is this byte concatenation:

```text
ASCII("devlish-detached-signature-v1") || 0x00 ||
ASCII(purpose) || 0x00 || exact_file_bytes
```

The purpose is checked against the CLI request and the trusted key's allowed
purposes. Signing context prevents relabeling release signatures as audit
evidence. This small versioned format is not a Sigstore bundle, certificate
chain, signed release manifest, or substitute for those future integrations.
Cryptographic verification uses the pinned
[ring Ed25519 verifier](https://docs.rs/ring/0.17.14/ring/signature/struct.UnparsedPublicKey.html).
No custom cryptographic algorithm is implemented.

## Meaning and limits of the result

Success establishes that the exact file snapshot was signed by a key authorized
in the supplied trust configuration. The output includes the artifact digest,
trust-configuration digest, signer ID and public-key digest. These allow a
reviewer to identify the bytes and trust inputs used; they do not authenticate
the trust configuration itself. Results contain no wall-clock timestamp and are
repeatable for identical inputs.

The generic `verify` command always separately marks these claims false:

- Log-chain verification and independently retained anchor verification.
- Replay verification.
- Execution-origin verification and policy-enforcement verification.

Even a validly signed forged history does not change those fields. Candidate
content is opaque: its claims cannot promote the verifier's assurance. The
output is not itself signed, and a malicious party can fabricate or edit it;
independent auditors must run their own trusted verifier against the inputs.

Provision the verifier and public keys through independently trusted channels.
The binary built by the commands above is **not a publisher-signed distribution**.
An attacker who replaces the verifier, its OS, or the supplied trust file can
undermine verification. Static local revocation information must be maintained
by the operator; online revocation, expiry, trusted time and rollback floors
are not implemented. Verification reads one bounded file snapshot; it does not
bind that snapshot to a later process launch or detect in-memory injection.

Regular files only: candidate limit 64 MiB, signature and trust limit 64 KiB each.
Larger binaries must currently fail rather than silently bypass verification.
The verifier has no network access code, signing command, private-key storage,
or runtime credential handling. A signature over `audit-evidence` authenticates
bytes, not the truth or completeness of a log or a signer's execution history.

## Verify a signed checkpoint or terminal receipt

`verify-log` additionally verifies an existing format-3 policy log against a
signed receipt. The receipt has its own `audit-receipt` signature purpose; an
ordinary signed document cannot be relabeled as a receipt.

```json
{
  "format": "devlish-audit-receipt",
  "format_version": 1,
  "kind": "terminal",
  "session_id": "operator-session-identifier",
  "release_manifest_sha256": "<64 hexadecimal characters>",
  "log_file_sha256": "<exact log file SHA-256>",
  "log_head_sha256": "<last record_sha256>",
  "record_count": 42
}
```

`kind` is `checkpoint` or `terminal`. Supply the expected receipt digest, session
and release identity independently of the candidate evidence:

```bash
crates/devlish_audit/target/release/devlish-audit verify-log evidence.jsonl \
  --receipt receipt.json --signature receipt.signature.json \
  --trust /operator/trust.json --receipt-sha256 "$RETAINED_RECEIPT_SHA256" \
  --session-id "$EXPECTED_SESSION_ID" --release-sha256 "$EXPECTED_RELEASE_SHA256"
```

The receipt signature covers its exact file bytes, including whitespace. The
verifier checks the receipt's signature, retained digest, log file digest, chain
head, count, sequence, previous hashes and effect intent/outcome correspondence.
Every nonempty log must start with a format-3 run-start record and contain only
supported records. A terminal receipt requires a final unpaused finish and no
unmatched authorized intent. A failed run can have a valid terminal receipt;
`recorded_execution_succeeded` distinguishes it from success. A checkpoint may
have an unresolved intent, reported as uncertain without retrying the effect.
The command checks one exact snapshot, not a checkpoint prefix of a longer file.
Individual log records are limited to 1 MiB.

`log_chain_verified` and `retained_receipt_digest_matched` can be true. They do
not verify how the caller obtained or retained that digest. If an attacker can
replace the trust configuration and the supposed independent digest, this
boundary is lost. There is no remote audit storage client yet. A missing or
mismatching anchor fails; there is no unanchored success fallback for this command.

Existing format-3 logs do not carry host-generated session IDs or verified
release identities. Therefore session/release association is explicitly a
**receipt-signer assertion**, checked against the operator's expected values.
`release_manifest_verified`, `replay_verified`, `execution_origin_verified` and
`policy_enforcement_verified` remain false. Native session binding and independent
release verification are subsequent work, not inferred from the receipt.

For a local synthetic demonstration against an existing policy log:

```bash
cargo run --locked --manifest-path crates/devlish_audit/Cargo.toml \
  --example receipt_demo -- evidence.jsonl /tmp/devlish-receipt-demo
```

This writes `receipt.json`, `signature.json` and `trust.json`, and prints the
values to use for the verification options. It generates an ephemeral demo key
and uses a clearly synthetic session and all-zero release digest. It is a test
fixture generator, not independent custody or production receipt issuance.

## Independently check saved issuance records

After a terminal receipt has been issued, verify the retained pending and
completed records against the execution log, current operator trust and
independently retained expectations:

```bash
devlish-audit verify-issuance evidence.jsonl \
  --pending operation.pending.json --completed operation.completed.json \
  --trust /operator/trust.json --expectations /operator/issuance-expectations.json
```

The expectations file has exactly these fields:

```json
{
  "tenant_id": "operator-tenant",
  "session_id": "operator-session",
  "key_id": "operator-receipt-key",
  "key_public_sha256": "<SHA-256 of the raw 32-byte Ed25519 public key>",
  "receipt_sha256": "<independently retained exact receipt digest>",
  "release_manifest_sha256": "<expected release manifest digest>"
}
```

Obtain these values independently of the candidate issuance records. The
command recomputes the terminal operation ID, checks the record identities and
receipt digests, then verifies the actual signature, current key purpose and
revocation, exact receipt bytes and complete log. A saved successful
`signature_verification` report is never used as authority. Missing completion,
checkpoint receipts, substituted metadata and revoked keys fail closed. This is
read-only: verification cannot reopen a slot or retry signing.

Passing means `issuance_records_consistent: true`, with a nested freshly
computed receipt verification. The output binds both record snapshots and the
expectations to hashes and is repeatable for identical inputs. The saved
records themselves are unsigned. The receipt currently signs the session and
release, not the tenant label, so `tenant_binding_authenticated` remains false.
`stored_verification_trusted`, `signer_authorization_verified`,
`issuer_history_replayed`, `execution_origin_verified` and
`policy_enforcement_verified` also remain false. This check does not prove
reservation ordering, unique issuance, Devlish authorization or protected key
custody. It does not authenticate the release manifest; use `verify-release`
for that separate check.

## Next increments

- DEVL-119 / DEVL-217: publisher-signed verifier releases and build provenance;
  signed release manifests, identity/validity/environment and rollback checks.
- DEVL-118 / DEVL-223: protected issuance of independently retained checkpoints and terminal receipts,
  independent storage and integration with existing offline replay reports.
- DEVL-224 / DEVL-225: protected executor and execution-bound signing authority.
- DEVL-226: optional qualified attestation for hostile-host deployments.

Keep orchestration and disclosure policy in Devlish. This native component is
the narrow cryptographic verification mechanism, not a new agent orchestrator.

## Signed release admission (DEVL-217, first increment)

`verify-release` verifies an exact-byte manifest under the new
`release-manifest` signature purpose. A `release-artifact` signature cannot be
relabelled or reused. This remains a native offline verification primitive; it
does not move agent orchestration or business policy out of Devlish.

```bash
cargo run --manifest-path crates/devlish_audit/Cargo.toml -- verify-release manifest.json \
  --signature signature.json --trust operator-trust.json \
  --requirements operator-requirements.json --artifacts operator-artifacts.json
```

The manifest has the following shape. Replace the illustrative digest and
artifact list with the actual complete release before signing exact JSON bytes
with `signing_message(Purpose::ReleaseManifest, bytes)`:

```json
{
  "format": "devlish-release-manifest",
  "format_version": 1,
  "release_id": "nppi-demo-2",
  "environment": "staging",
  "target": "aarch64-apple-darwin",
  "sequence": 2,
  "valid_from": 1790985600,
  "valid_until": 1791072000,
  "repository": "https://github.com/adubinsky/devlish",
  "commit": "REPLACE_WITH_APPROVED_COMMIT",
  "workflow": "release",
  "policy_id": "nppi",
  "policy_version": "1",
  "artifacts": [
    {"id": "runtime", "role": "runtime", "sha256": "REPLACE_WITH_SHA256"}
  ]
}
```

Required artifact roles are `runtime`, `compiler`, `policy`, `tool-catalog`,
`permissions`, `containment`, `source-closure`, and `build-attestation`.
Additional external programs such as `ls` or `grep` use `tool`. Every artifact
has a unique ASCII alphanumeric/underscore/hyphen ID and a lowercase SHA-256
hex digest. All supplied artifact snapshots must match. Digests authenticate
bytes only: this increment does not interpret the catalog, permission or
containment documents, verify builder attestations, or ensure the catalog lists
every external program. Such semantics remain a prerequisite for deployment.

Operator requirements are separate from the candidate manifest:

```json
{
  "format": "devlish-release-requirements",
  "format_version": 1,
  "environment": "staging",
  "target": "aarch64-apple-darwin",
  "repository": "https://github.com/adubinsky/devlish",
  "commit": "REPLACE_WITH_APPROVED_COMMIT",
  "workflow": "release",
  "policy_id": "nppi",
  "policy_version": "1",
  "minimum_sequence": 2,
  "require_recorded_controls": true,
  "evaluated_at": 1791000000,
  "revocations_valid_from": 1790985600,
  "revocations_valid_until": 1791072000,
  "revoked_manifest_sha256": [],
  "authorized_release_keys": ["release-authority"]
}
```

The release authority must also be an unrevoked key in the existing operator
trust file with purpose `release-manifest`. A builder key is not implicitly a
release authority. Scope fields must match exactly; both validity windows use
Unix seconds and include the start but exclude the end. A signed release below
the supplied sequence floor is rejected, as is a revoked manifest digest.

The artifact mapping is an operator-supplied JSON array, with one entry per
manifest ID. Relative paths resolve beside the mapping file:

```json
[
  {"id": "runtime", "path": "bin/devlish-core"},
  {"id": "grep", "path": "/usr/bin/grep"}
]
```

Candidate manifests cannot choose paths to read. Files are read as bounded
regular-file snapshots; they are never executed. Changing a listed tool fails
verification, but replacing it after verification is still possible until the
protected launcher is implemented (DEVL-220).

For repeatable historical audits, evaluation time is explicit and the report
includes the exact requirements digest, time, floor and manifest digest. This
is **not a live deployment authorization**: a caller could supply an old time,
stale trust or a lower floor. The operator must protect those inputs outside
model/user-writable storage. This increment neither reads a trusted clock nor
atomically advances durable rollback state.

The release tests in `crates/devlish_audit/tests/releases.rs` generate
throwaway signing keys and synthetic artifacts. They cover repeatability,
wrong scope, expired/future releases and revocation windows, rollback, revoked
keys/releases, authority separation, missing/duplicate artifacts, manifest
mutation, and replacement of an external tool through the real CLI.

Remaining DEVL-217 work includes authenticated builder provenance (DEVL-119),
protected/fresh trust and revocation distribution, and protected execution across all runtime entry points.
The governed CLI additions below provide local rollback state and a limited host-effect catalog. Receipt issuance and mandatory runtime loading must consume
the verified identity rather than merely accept a caller's digest. Reports
continue to set `build_provenance_verified`, `execution_origin_verified` and
`policy_enforcement_verified` to false.


### Bind reports and receipts to the verified release

Add `--evidence evidence.json` to `verify-release` to verify the release and
cross-check an application report, policy report, signed receipt, and format-3
log in one invocation. No saved verification report is accepted as a trust
credential. The same log snapshot is used throughout.

```json
{
  "application_report": "application-report.json",
  "policy_report": "policy-report.json",
  "log": "run.jsonl",
  "receipt": "receipt.json",
  "signature": "receipt.sig.json",
  "trust": "operator-receipt-trust.json",
  "retained_receipt_sha256": "REPLACE_WITH_INDEPENDENTLY_RETAINED_DIGEST",
  "session_id": "expected-session"
}
```

Paths resolve beside this operator-supplied evidence file. Protect the retained
digest and receipt trust independently. The receipt must name the exact verified
manifest digest. The log must record a release-approved runtime file digest,
policy canonical digest and program canonical digest; include a `program`
artifact in the manifest. Canonical policy/program digests are calculated from
the verified JSON snapshots using the existing pretty-JSON hashing convention.
Every effect decision must use the start record's policy identity.

Application file entries must match release artifact IDs, roles and exact
hashes, and include runtime and policy entries. This binding currently supports
`runtime`, `policy`, `program`, and `tool` application roles; reports with
`source` or `configuration` roles are rejected until their mapping is defined.
The policy report must name the same policy file as the application report,
and the log must match that policy and an application runtime entry. Existing
core reports are unchanged; the independent verifier emits a separate binding
result alongside the release and receipt results.

The combined result may set `release_manifest_verified` on the receipt, while
`report_claims_independently_verified`, `execution_origin_verified` and
`policy_enforcement_verified` remain false. Self-hashed passing reports can be
fabricated: matching approved identities does not prove their tests ran. This
command does not replay the process, authenticate the golden-case baseline,
issue receipts, or install mandatory enforcement at runtime. Those remain
separate work.

## Operator-selected execution and durable admission

The CLI now supports `run-verified`. The operator sets
`DEVLISH_VERIFIED_PROFILE` to a profile that selects the program, policy,
release authority, artifact mapping and admission-state file. When this
variable is set, all commands except `run-verified`, help and version are
blocked. Server, MCP, harness, REPL and ordinary `run` have no silent fallback;
they need dedicated governed adapters before they can be enabled in this mode.
Without this environment variable, existing development commands still work.

```json
{
  "format": "devlish-verified-profile",
  "format_version": 1,
  "manifest": "manifest.json",
  "signature": "signature.json",
  "trust": "operator-trust.json",
  "requirements": "operator-requirements.json",
  "admission_state": "admission.json",
  "runtime_id": "runtime",
  "program_id": "agent",
  "policy_id": "policy",
  "permissions_id": "permissions",
  "catalog_id": "tool-catalog",
  "containment_id": "containment",
  "allow_raw_evidence": false,
  "artifacts": [
    {"id": "agent", "path": "agent.dvlc.json"},
    {"id": "policy", "path": "policy.dvlc.json"}
  ]
}
```

Paths resolve beside the profile. Add mappings for every other manifest
artifact. The runtime mapping is always replaced with the current executable's
path, so a caller cannot offer a different signed file in its place. Program
and policy must be compiled JSON. Policy Rule ID/version must agree with the
manifest. The runner consumes the exact verified program/policy buffers,
without reopening or compiling the path. A runtime file recheck before the
start record rejects changes since admission; this still does not measure
process memory or exclude injection.

Provision the state file once, before enabling the execution profile:

```bash
devlish-audit init-admission admission.json --requirements operator-requirements.json
DEVLISH_VERIFIED_PROFILE=operator-profile.json devlish-core run-verified \
  --policy-log session-001.jsonl --session-id session-001 --input '{}'
```

The execution command accepts only input, a new policy-log path, session ID and
optional `--policy-evidence`. It selects neither a different policy nor a
provider override. Raw evidence capture requires both `allow_raw_evidence: true`
in the operator profile and the caller flag. The profile setting defaults to
false; a forbidden evidence request fails before log creation or dispatch.
Execution uses the host clock instead of the requirements document's historical
evaluation time. The start record includes both the original requirements
hash and the effective requirements hash, release identity and session ID.
Receipt verification checks recorded session/release associations when present.

On Unix, admission takes a nonblocking exclusive OS file lock and retains it
for the run. State is scoped to environment, target, repository and policy ID.
It rejects lower sequences and different manifest bytes at an already accepted
sequence, and persists the new floor before dispatch. State must already
exist as a private, single-link regular file; symlinks are rejected. Missing or
malformed state fails closed. An interrupted in-place state update can require
operator recovery; it never silently resets the floor. Initialization uses
exclusive creation and cannot overwrite existing state. Other platforms fail
closed until a supported locking implementation is added.

The development `bin/devlish` wrapper refuses `run-verified`, `serve-verified`,
and every invocation with `DEVLISH_VERIFIED_PROFILE` set (including an empty
value). It cannot authenticate a discovered binary before executing it. Use the
operator-selected absolute native executable path, verified and installed through
an independently trusted deployment process. Development release/debug/PATH
resolution remains available only without a verified-mode request. This refusal
is a guardrail against accidental bootstrap bypass, not an atomic hash-and-exec
implementation or protection against replacing the wrapper/interpreter.

These are in-process admission controls, not a protected executor. The service
launcher, environment, profile, trust, clock and state directory must be under
operator control. A user who can replace the state or unset the profile can
bypass this local boundary. The current host still shares process privileges
with the runtime; policies and file permissions are not a substitute for OS
containment. Root compromise, signer isolation, fresh revocation distribution,
external-process catalog/containment support and disclosure controls for other adapters
remain separate work. Signed receipt issuance has not been added.


### Enforced release controls in the verified CLI

The selected permissions, catalog and containment snapshots must use these
schemas. Include each as a hashed artifact in the signed release and an entry
in the operator profile mapping:

```json
{
  "format": "devlish-runtime-permissions",
  "format_version": 1,
  "allowed_effects": ["llm_complete", "respond"],
  "instruction_limit": 100000
}
```

```json
{
  "format": "devlish-tool-catalog",
  "format_version": 1,
  "host_effects": ["llm_complete", "respond"]
}
```

```json
{
  "format": "devlish-containment-profile",
  "format_version": 1,
  "mode": "in-process"
}
```

Effects must be known native host operations. Duplicates, unknown effects and
permissions outside the catalog are rejected before execution. The allowed
set intersects the Devlish policy: neither can grant something the other
denies. The signed instruction limit must be between 1 and 10,000,000 and is
applied to the program VM. This is a VM instruction budget, not a wall-clock
or network timeout. The only supported containment mode is explicitly
`in-process`; claiming hardware or OS isolation fails admission. External
program launch is not part of this host-effect catalog.

The log records the selected control hashes, allowed effects and instruction
limit. Offline process replay uses those recorded limits and the redacted
diagnostic mode, preserving repeatability. That replay agreement does not
independently authenticate the recorded control values; release approval and
protected execution remain separate checks.

For `run-verified`, only a policy-approved Respond emits program data to stdout.
Automatic VM result/context dumps and raw errors are suppressed. Admission
errors emit a diagnostic digest. Policy decision reasons are represented by
fixed text plus a reason digest in ordinary logs. Debug event output and the
legacy audit sink are disabled. An operator-authorized `--policy-evidence` captures
sensitive inputs and exchanges for replay and needs protected storage. These
changes apply to the verified CLI profile, not ordinary development commands.

## Prepare an unsigned receipt from checked history

`verify-release` can prepare receipt bytes instead of checking an existing
signature. Supply `--prepare-receipt receipt-request.json` (mutually exclusive
with `--evidence`):

```json
{
  "log": "run.jsonl",
  "session_id": "session-001",
  "kind": "terminal",
  "output": "unsigned-receipt.json"
}
```

The verifier first checks the release and artifact snapshots. It then validates
the full log chain and effect ordering, requires a recorded session, and checks
runtime/policy/program identities against the release. Head, count, log digest
and release digest are derived by the verifier. A caller cannot supply those
fields through this request. `terminal` requires a finished, unpaused run;
`checkpoint` may describe incomplete history and uncertain effects.

Output uses exclusive creation, private Unix permissions and synced writes.
No private key is loaded and nothing is signed. The result explicitly reports
`receipt_signed: false` and `signer_authorized: false`. A separate authority
must authorize signing and independently retain the receipt digest. Fabricated
but internally consistent logs remain possible without protected execution;
receipt preparation does not change that trust limit.

## Devlish receipt authorization contract

The [receipt authority example](../examples/receipt_authority/README.md) expresses
terminal-receipt authorization as Devlish rules with repeatable synthetic cases.
The VM exposes an explicit host-supplied authority input, separate from caller
request JSON. Ordinary policy-host evaluation has no authority context.

This policy is connected to a native in-process issuer, not a protected signing
service. The issuer performs durable reservation and calls a backend trait only
after preflight and final Devlish approval. There is no production key backend
or network endpoint; authority authentication remains a host responsibility.
A service must independently derive that state, authorize a tenant-scoped key,
reserve issuance, and sign the exact retained bytes whose digest the policy
approved. The example documents those obligations and denies stronger execution
assurance claims.

## Durable local terminal-receipt reservation

`ReleaseVerification::reserve_terminal_receipt` validates and prepares a terminal
receipt before creating an exclusive reservation in an existing private directory.
The directory is operator-selected, belongs to the service identity and must have
no group/other permission bits. This Unix-only library primitive does not create
a signing endpoint or load private keys.

The reservation name commits to tenant, session and terminal kind. Release digest,
receipt bytes and signing key do not change that name, so switching them cannot
open a competing terminal slot for the same tenant/session. The pending record
stores exact prepared bytes and their digest, plus the operator-selected key ID
and public-key fingerprint (`ReceiptKey`). Changing the key behind the same ID
cannot complete an existing reservation. The checked directory is resolved to an
absolute path before reservation, so subsequent working-directory changes do
not redirect completion. Exclusive creation, file sync and
directory sync complete before a reservation is returned. Partial records and
abandoned reservations block later acquisition; they are never silently deleted.

The non-cloneable reservation provides the bytes to a separately authorized
backend. `complete` consumes the reservation and verifies its returned signature
against those retained bytes, the reserved key ID and fingerprint, the audit-receipt domain and
current explicit trust. Only then does it exclusively persist a completion
record. Both records remain in place. Failure, including a failed completion
write, leaves the slot unavailable pending operator reconciliation. There is no
automatic retry, release/reset operation or restart recovery API yet.

This is durable local state under operator custody, not a protected service.
The caller must choose the directory, authenticate tenant/session and authorize
signing through the Devlish policy before contacting a backend. The primitive
itself does not verify that policy ran; completion explicitly records
`signer_authorization_verified: false`. Directory ownership/mode checks do not
prove administrator resistance, safe ACLs, ancestor custody or immutable storage.
A user who can remove/replace service state can defeat its guarantees. Independent
receipt retention, backend reconciliation and protected execution remain open.

Tests cover concurrent reservation, different tenant/session isolation, changed
receipt/key conflicts, abandonment, unsafe directory/identities, exact-byte
signature matching, wrong purpose/key, same-ID key replacement, revocation,
relative directory binding and no-overwrite completion.
The integration test starts from release-verified log history. Power-loss and
production key-backend behavior have not been exercised.


The native core's `ReceiptIssuer` now orchestrates the Devlish policy, reservation
and backend adapter. It checks a current operator trust snapshot against the
pinned key before either approval, and it records both decisions before signing.
See the [issuer contract and tests](../examples/receipt_authority/README.md#native-in-process-issuer).
This integration does not upgrade the independent verifier's execution-assurance
claims or add restart recovery, tenant authentication or protected key custody.

## Receipt authorization replay is a separate check

`devlish report receipt-issuer` can now reproduce the Devlish decisions in an
operator-enabled issuer journal against an independently retained digest. It
runs no signing backend and reports denial or uncertainty separately from recorded
completion. This compiler/VM-backed replay is deliberately separate from the
small `devlish-audit` executable, which has no VM dependency. Combining the two
checks still does not authenticate authority snapshots or prove protected
execution. See [receipt authorization replay](COMPLIANCE_REPORTS.md#replay-recorded-receipt-authorization).


### Approved model routing in verified runs

A signed catalog that permits `llm_complete` must now include `llm_route`. Missing
or invalid routes reject admission before creating a run log or dispatching an
effect. Existing signed catalogs with model permission must be reissued with an
approved route; non-model catalogs are unchanged.

```json
{
  "format": "devlish-tool-catalog",
  "format_version": 1,
  "host_effects": ["llm_complete", "respond"],
  "llm_route": {
    "format": "devlish-model-route",
    "format_version": 1,
    "provider": "openrouter",
    "model": "synthetic/replace-with-approved-model",
    "credential": "OPENROUTER_API_KEY",
    "timeout_seconds": 30,
    "max_request_bytes": 16384,
    "max_response_bytes": 65536,
    "max_tokens": 1024
  }
}
```

The model value above is a placeholder, not a recommendation or available model.
For this first governed adapter, the only supported provider is `openrouter` and
the destination is fixed to `https://openrouter.ai/api/v1/chat/completions`.
`DEVLISH_CONFIG`, per-user provider defaults, request URLs, headers and conflicting
model/provider fields cannot redirect it. Unknown route/request fields fail.
The signed catalog digest in the session binds these settings to the release.
The Devlish effect policy still approves each prompt and response effect.

The adapter uses a fresh client, disables redirects and environment proxies,
requires HTTPS, applies connect/request timeouts and bounds request JSON and
response bytes before parsing. Timeout must be 1–120 seconds, request size
1–1,048,576 bytes, response size 1–8,388,608 bytes and output token budget
1–32,768. Serialized request overhead counts against the byte budget. Plain
text completions remain untrusted; `expect_json` requires strict JSON content,
without accepting Markdown fences or repair by another model. Provider error
bodies are not returned as diagnostic text. Transport failures are uncertain
and do not trigger automatic replay/retry.

This controls the request sent to OpenRouter using its documented
[completion API](https://openrouter.ai/docs/api_reference/overview). It does not
attest which upstream model/provider actually processed it, fix account-side
routing/retention settings or prove provider compliance. OpenRouter can choose
upstream providers under its own routing rules. The credential resolver and host
OS/TLS/DNS remain trusted; signing a credential lookup name does not protect the
credential itself. The HTTP client's system DNS lookup can exceed its timeout;
there is no strict process-wide deadline or network containment claim. Production
identity isolation, downstream routing constraints and protected adapters remain
required. Local test servers use synthetic keys; no live-provider test is implied.


### Shared admitted sessions

The native library exposes `verified_session::VerifiedSession` for trusted
adapters. `admit(operator_profile, session_id, capture_evidence)` verifies the
current executable and exact artifact buffers, enforces the signed controls and
policy identity, then durably advances and holds the rollback lock. The fields
containing the program, policy and controls are private; the session is neither
cloneable nor deserializable. `execute(input, new_log_path, host)` consumes it,
rechecks the validity window, creates the bound log and calls the shared governed
runner. The admission lock remains held through execution and final recording.
Replacing artifact paths after admission cannot replace the retained program or
policy. Relative deployment paths are anchored to the admission working directory
without resolving symlinks, so a later working-directory change cannot redirect
an adapter using that source location. The built-in verified CLI and HTTP adapters
now select credentials separately, without program-local dotenv lookup. Log-path collisions fail before any host effect and preserve old evidence.

`run-verified` uses this API directly. Other adapters must authenticate their
clients, select operator-controlled profiles/log paths, install protected host
adapters and approved response sinks, and suppress operator diagnostics before
exposing a route. Session IDs only correlate records; they do not establish a
user or tenant identity. A valid API object is local admission evidence, not
proof of protected execution. It cannot make an arbitrary host implementation
trustworthy. The validity recheck is at execution startup, not continuous
revocation monitoring during effects. Legacy HTTP/harness/MCP routes remain disabled under a verified profile.
The separate `serve-verified` adapter below uses this shared admission path.


### Authenticated local service

`serve-verified` stays running and executes each request as a separate admitted
session. The operator selects the signed profile and a private Unix log directory;
clients can supply only a correlation ID and application input. It accepts only
literal loopback addresses, defaulting to `127.0.0.1:7421`.

```bash
mkdir -m 700 verified-logs
# Set DEVLISH_SERVE_TOKEN from an operator-managed random 32-byte secret,
# encoded as 64 hexadecimal characters. Do not commit it or put it in a URL.
# If DEVLISH_CREDENTIALS_DIR is set, supply a private file named
# DEVLISH_SERVE_TOKEN there instead; the environment token is ignored.
DEVLISH_VERIFIED_PROFILE=/operator/deployment/profile.json \
  /opt/devlish/runtime/devlish-core serve-verified --log-dir verified-logs
```

Both `GET /v1/health` and `POST /v1/run` require exactly one
`Authorization: Bearer <token>` header. A run body is JSON:

```json
{"session_id":"review-001","input":{"example":"application input"}}
```

The service reads its token from the verified credential source at startup.
A configured mounted source never falls back to `DEVLISH_SERVE_TOKEN` in the
environment, including when the file is missing, malformed or unsafe. It uses
the same file ownership, permissions, link and size checks as model credentials.
The token remains fixed in memory for that process; rotating or revoking it
requires stopping the old service and starting it with the new token. Changing
the file alone does not revoke existing access. Per-request credential-source
validation does not reload the authentication token.

Unknown fields, chunked bodies and bodies over 65,536 bytes are rejected.
Profiles, source, policies, model routes, credentials, filesystem paths and raw
evidence capture cannot be chosen through this API. Startup validates the
release before listening; each run re-admits it, including current validity and
artifact checks. Health reports local liveness, not current release validity or
attested execution. Admission failure returns a generic 503.

Successful requests return `success`, `session_id`, `paused` and `responses`.
Only policy-approved `Respond` values enter `responses`, with at most 16 values
and 64,000 aggregate serialized bytes. They are staged until execution and the
final audit record succeed. A recorded response outcome means acceptance into
this buffer, not proven delivery to the HTTP client. No private VM result,
checkpoint state, policy reason or provider diagnostic is returned. Execution
failure discards the buffer and returns a generic 500; earlier business effects
may still have happened.

Logs are digest-only and use exclusive `<session_id>.jsonl` files. An existing
name returns 409 without re-execution or log replacement, including after an
interrupted run. There is no automatic retry, resume or cross-ID deduplication:
using a new ID can repeat business effects. Session IDs are correlation labels,
not authenticated user or tenant identities.

This is a single-operator local service, not a production isolation boundary.
The bearer token authenticates possession only; there is no per-user/tenant
authorization, TLS listener or protected OS identity. The operator must protect
the profile, environment, log directory and its parent directories. Same-user
or root compromise remains outside these guarantees. The current HTTP transport
bounds application body/output buffers but does not enforce hard socket read
or connection-count limits; a slow local client can block this sequential
service. Deploying it to untrusted clients requires availability controls and
protected execution/credential isolation. No successful response claims that
signatures independently prove policy enforcement.

### Signed effect-attempt budgets

Signed runtime permissions can include an `effect_budget` alongside the instruction
limit and effect allowlist:

```json
{
  "total": 4,
  "per_effect": {"llm_complete": 1, "call_service": 2, "respond": 1}
}
```

`total` must be an integer from 1 through 10,000. Per-effect limits must be
integers from zero through `total`, and their names must appear in the approved
permission allowlist. Omitted per-effect entries share only the total cap. A
zero per-effect limit forbids dispatch of that effect. Unknown fields, malformed
limits and references to unapproved effects fail admission before a log is created.

Every attempted host effect consumes the total cap and its applicable per-effect
cap before policy evaluation or dispatch. Policy denials, permission denials and
host errors consume attempts too. A Devlish `Try`/`Otherwise` cannot refund or
reset them. Exhaustion records a denial before returning an error; other effect
kinds may still run if their limits and the remaining total permit it. Recording
failure continues to block all later effects. The native wrapper provides the
counter; Devlish still validates plans, selects steps and authorizes effect data.

CLI and authenticated HTTP sessions use these same limits. The session start
binds the configured budget, and offline process replay reconstructs fresh
counters to reproduce the decisions, including denials. This remains evidence
replay, not proof of real-world execution. Standalone policy reports evaluate
Devlish rules in isolation; process reports additionally reproduce release limits.

Budgets are per execution and count calls, not money, model tokens, elapsed time
or successful business writes. A new session obtains a new budget; this is not
cross-session rate limiting or safe retry. Existing version-1 permission files
with the field omitted or null retain their instruction limit without a separate
effect-count cap. Adding a budget changes the signed permission bytes and requires
an approved release. The finite agent example explicitly requests one model call,
two service calls and one public response.

### Independent recorded-control consistency

Release-bound receipt verification now reports `recorded_controls_match_release`.
It is true only when the log supplies verified-release control metadata and the
independent verifier checks that:

- Permission, catalog and containment digests belong to the verified signed release.
- The recorded runtime digest agrees with the run identity.
- The effect allowlist, instruction limit and optional attempt budget match the
  signed permission snapshot; allowlist ordering may differ, duplicates may not.
- Every recorded allowed effect belongs to that allowlist and falls within the
  total and per-effect attempt budgets. Denied attempts consume these counters too.

The checks use retained verified permission data, without reopening artifacts,
loading a VM or consulting a model. Receipt preparation applies the same checks
before reservation/signing. The verifier streams records and keeps counters only
for signed per-effect limits. Supported permission metadata is limited to 64 KiB;
release hashing still accepts larger opaque artifacts but cannot give them this
control-consistency assurance. An unsupported permission format or inconsistent
claimed controls causes release-bound receipt verification to fail.

Standalone receipt signature/chain verification and older logs without control
metadata leave this flag false. Removing metadata cannot earn the stronger flag.
The flag describes the consistency of recorded assertions with an approved release.
It does not prove that instructions or effects actually ran, that the instruction
limit was observed, that the Devlish policy allowed those calls, or that filesystem
and network containment was enforced. Execution origin, policy enforcement and
policy replay remain separate, unverified claims in this independent verifier.

Operators can set `"require_recorded_controls": true` in their independently
trusted release-requirements file. Release verification echoes this as
`recorded_controls_required`. Receipt preparation, reservation and release-bound
receipt verification then reject a log without control metadata, including a
correctly signed legacy receipt. The log, receipt or manifest cannot lower this
operator setting. Omission defaults to false for older evidence; null and
non-boolean values are rejected. Protect this requirements file independently
of candidate evidence, like the trust-key configuration.

This requirement applies when preparing or verifying release-bound receipts.
Checking release files alone still does not evaluate any run, and standalone
`verify-log` does not apply release requirements. Use `verify-release` with receipt
evidence for the stronger check. This option demands recorded-control consistency,
not actual-execution assurance.

### Explicit verified credential sources

Verified CLI and HTTP adapters no longer read either the user's
`~/.devlish/.env` or a program-local `.env`. This is a change for verified
mode: move needed credentials to the operator process environment or select a
mounted directory with an absolute `DEVLISH_CREDENTIALS_DIR` path. Legacy development-mode
credential lookup is unchanged.

When the directory is set, it is the only credential source: a missing, unsafe
or invalid file does not fall back to the environment. Each filename is the
approved credential lookup name, such as `OPENROUTER_API_KEY`; separators and
other non-identifier characters are rejected. On Unix, the directory must be
owned by root or the effective service user, with no group/other mode permissions.
Files must be regular, have one hard link, the same ownership restrictions, and
no group/other mode permissions. Symlink credential leaves and the final directory
component are rejected, including directory paths ending in `/` or `/.`. Parent
traversal (`..`) is rejected; ancestor paths remain operator-controlled. FIFOs are rejected.
The opened directory descriptor is retained for that execution, so pathname
replacement cannot redirect lookups. Directory mode is checked at source creation;
file metadata is checked when resolving each credential.

Files are at most 8,192 bytes of UTF-8 text. One terminal newline (LF or CRLF) is
accepted; empty values, NULs and embedded newlines are rejected. Environment-only
credentials use the same content limits. No secret value or path is returned in
verified diagnostics. An invalid configured directory rejects CLI startup before
the run log and rejects HTTP startup or a later request with a generic error.

This supports operator-mounted secrets without claiming secret isolation by
itself. It checks Unix mode bits, not ACLs, and relies on the operator to protect
ACLs, parent paths, environment and credential source integrity. Same-user/root
processes and a compromised host remain outside its protection. A separate service
identity and protected execution deployment are still required. No secrets are
placed in the signed catalog; the catalog pins the credential lookup name.

### Linux deployment qualification candidate

The [Linux service recipe](../deploy/linux/README.md) and unit template connect
verified admission and mounted credentials to a dedicated service identity.
They have not been executed or qualified on Linux. The recipe lists ownership,
bootstrap, debugger/loader, credential, interruption and availability acceptance
checks, and distinguishes requested restrictions from observed enforcement.
It does not increase the verifier's execution-assurance claims.

### Separately authenticated builder statements

Operator release requirements may now include `build_requirements`. When present,
all `build-attestation` artifacts must contain supported, builder-signed bundles.
The builder key must be authorized by this operator configuration, unrevoked,
permitted for `build-statement`, and different from the release approval key.
Key separation is checked using public-key digests, not just key labels.

This authenticates claims and links approved bytes. It does not yet verify
GitHub OIDC/Sigstore identity, a protected CI job, discovery of the complete source
closure, an actual compiler invocation, or reproducibility. Accordingly,
`builder_statements_authenticated` may be true while `build_provenance_verified`,
`execution_origin_verified` and `policy_enforcement_verified` remain false.

The optional operator object has this shape; replace every placeholder with an
independently approved value, not a value copied blindly from the candidate:

```json
{
  "build_requirements": {
    "authorized_builder_keys": ["toolchain-builder", "policy-builder"],
    "toolchain": {
      "workflow": "approved-toolchain-build-workflow",
      "build_definition_sha256": "<64 lowercase hex characters>",
      "dependencies_sha256": "<64 lowercase hex characters>",
      "builder_image_sha256": "<64 lowercase hex characters>",
      "options_sha256": "<64 lowercase hex characters>"
    },
    "policy": {
      "workflow": "approved-policy-build-workflow",
      "build_definition_sha256": "<64 lowercase hex characters>",
      "dependencies_sha256": "<64 lowercase hex characters>",
      "builder_image_sha256": "<64 lowercase hex characters>",
      "options_sha256": "<64 lowercase hex characters>"
    }
  }
}
```

Builder workflows are pinned independently for toolchain and policy builds. They
need not equal the release workflow. The authorized builder-key list applies to
both build kinds; phase-specific key permissions are not implemented. The four
input digests name the operator's expected build definition, dependency evidence,
builder image and options record. The verifier compares these signed claims to
expected digests; it does not fetch images, execute tools or discover dependencies.
Omitting `build_requirements` or setting it to null keeps legacy behavior: build
artifacts are hash checked as opaque bytes, with authenticated-builder status false.
An empty key list in a configured object is rejected, not treated as an opt-out.

A bundle is at most 64 KiB and has exactly these top-level fields:

```json
{
  "format": "devlish-build-bundle",
  "format_version": 1,
  "statement": "<exact UTF-8 JSON statement text, preserving whitespace>",
  "signature": {
    "format": "devlish-detached-signature",
    "format_version": 1,
    "algorithm": "ed25519",
    "key_id": "policy-builder",
    "purpose": "build-statement",
    "signature_hex": "<signature over the exact statement bytes with the Devlish purpose prefix>"
  }
}
```

The statement text must parse as this strict schema:

| Field | Required meaning |
| --- | --- |
| `format`, `format_version` | `devlish-build-statement`, `1` |
| `kind` | `toolchain` or `policy` |
| `repository`, `commit`, `release_workflow`, `target` | Match the release manifest; commit is a 40- or 64-character lowercase Git object ID |
| `valid_from`, `valid_until` | Unix seconds; evaluation time is inside the half-open validity interval |
| `source_closure_sha256` | Digest of a supplied, verified `source-closure` artifact |
| `compiler_sha256` | Required approved compiler digest for policy builds; absent or null for toolchain builds |
| `inputs` | Exactly the operator's corresponding toolchain or policy input object above |
| `subjects` | Nonempty array of `{ "id": "artifact-id", "sha256": "..." }` |

Toolchain subjects may cover only runtime/compiler artifacts. Policy subjects may
cover only program/policy artifacts. Every such artifact in the release must be
covered exactly once across all bundles; subject IDs and digests must match the
supplied verified snapshots. Unknown, missing, duplicate or mismatched subjects
fail admission. Whitespace changes after signing invalidate the statement signature;
duplicate/unknown JSON fields in the bundle, signature or statement are rejected.

Build the statement and detached signature separately, then embed the exact
statement text and signature object in the bundle. Add its exact digest under a
`build-attestation` role in the release manifest and sign that manifest using the
separate release authority. Use the existing `verify-release` command and native
verified profile; both consume these requirements. Standalone `verify --purpose
build-statement` checks a signature only, without scope, inputs or output coverage.

Tests sign synthetic toolchain and policy statements with separate ephemeral keys.
They reject changed statement bytes, wrong purposes/authorities, revoked builders,
wrong repository/commit/workflows/target, unapproved inputs/source/compiler,
missing/duplicate outputs, expiry, oversized bundles and ambiguous signature fields.
A native CLI test admits a complete synthetic build chain, then rejects a fresh
execution before logging or dispatch after builder revocation. No production
builder, CI signing backend or real build attestation is provisioned by these tests.

The report’s `admission_valid_until` is the earliest expiry among the release,
revocation information, and all required builder statements. Native sessions
recheck this exclusive deadline before dispatch, including when an admitted
session has been held in memory. This is an admission deadline, not a promise
to interrupt an already running effect at expiry.
