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

Purposes are `release-artifact`, `audit-evidence` and `audit-receipt`. Each key must have a unique
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

## Next increments

- DEVL-119 / DEVL-217: publisher-signed verifier releases and build provenance;
  signed release manifests, identity/validity/environment and rollback checks.
- DEVL-118 / DEVL-223: protected issuance of independently retained checkpoints and terminal receipts,
  independent storage and integration with existing offline replay reports.
- DEVL-224 / DEVL-225: protected executor and execution-bound signing authority.
- DEVL-226: optional qualified attestation for hostile-host deployments.

Keep orchestration and disclosure policy in Devlish. This native component is
the narrow cryptographic verification mechanism, not a new agent orchestrator.
