# Receipt authorization in Devlish

This example makes receipt-signing decisions reviewable as Devlish rules. It is
an executable authorization contract for DEVL-225, not a protected signer. It
loads no key and issues no signature. Every fixture is synthetic.

`authorize.dvl` permits one narrowly scoped operation: a terminal audit receipt
for recorded history. It compares the caller's tenant, session, release, receipt
digest and key selection against independently supplied authority state. It
rejects revoked/inactive keys, unavailable signing, incomplete or paused runs,
unreserved or consumed issuance, unknown fields and stronger execution claims.

Run the cases from the repository root:

```bash
cargo test --locked --manifest-path crates/devlish_core/Cargo.toml --test receipt_authority
```

`cases.json` contains the expected English decision for every scenario. The test
runs each case twice to check repeatability. A separate test proves that ordinary
host policy evaluation cannot obtain authority by nesting it inside a request,
and that a denied service call never reaches the underlying host.

## Two input channels

`EffectPolicy::evaluate_with_authority(effect, request, authority)` evaluates a
compiled Devlish policy without external effects, under the existing instruction
budget. Request and authority remain separate top-level values. The ordinary
`evaluate` and `PolicyHost` paths supply null authority. Request fields cannot
replace it.

This separation prevents accidental request merging; it does not authenticate
JSON or protect the process. A Rust caller able to supply both arguments can
fabricate both. A production signing service must own the authority channel,
pinned policy, session records and keys independently of the agent.

The authoritative descriptor must be derived from validated native state, not
copied from uploaded documents or request booleans:

| Field | Host obligation |
| --- | --- |
| tenant_id, session_id | Resolve authenticated tenant and executor-owned session; reject missing or empty identities. |
| release_sha256 | Verify release admission under operator trust and retain its exact manifest digest. |
| receipt_sha256 | Prepare receipt from a checked immutable log snapshot, retain exact receipt bytes, and compute its canonical lowercase SHA-256. |
| key_id, key_active | Resolve an active receipt-purpose key authorized for this tenant/environment; evaluate current revocation state. |
| signer_available | Establish authorized backend availability; failure must not select a fallback key. |
| terminal_ready | Derive from the verified log's finished/unpaused state with no unmatched effect intent. A failed but completed run may still receive a truthful receipt. |
| reservation_state | Atomically reserve this operation under service-owned state before evaluation; never accept the caller's claim of a reservation. |
| evidence_source | Use `verified-history` only after independent release and log verification. |
| assurance_profile | Use `recorded-history`; neither this policy nor a signature proves governed execution. |

The policy checks types and allowed fields; the host must validate canonical
identities and digests before constructing the descriptor. The tests substitute
synthetic state for that still-unimplemented protected host.

## Integration sequence

1. Authenticate the caller outside the model/agent process and select the
   operator-pinned authorization policy and tenant-scoped receipt key.
2. Verify release and log history, then prepare exact receipt bytes. Retain these
   bytes inside the service; do not accept arbitrary caller-selected bytes.
3. Acquire an exclusive durable reservation for the session, receipt kind and
   digest. A conflicting or consumed reservation fails closed.
4. Supply the requested operation and independently constructed authority state
   separately to the Devlish policy. Errors, missing decisions and denials stop
   issuance. Persist the policy identity, authority-state commitment and decision
   in the signer's independent audit record before contacting the key backend.
5. Sign those same retained bytes using the existing `audit-receipt` signing
   domain. Key material must never enter Devlish variables, logs or model input.
6. Persist the result and reservation transition before responding. Retry may
   return the stored identical receipt, but must not sign a competing receipt.
   A crash with uncertain backend outcome requires reconciliation, not blind retry.
7. Retain receipt commitments outside the executor's control. A failed retention
   operation cannot be reported as independently anchored evidence.

The pure policy cannot implement atomic reservation, prevent concurrent duplicate
issuance, authenticate a key backend, protect storage from administrators or
prove log truth. Tests of `reserved` and `consumed` show the decision contract,
not a working concurrency mechanism. The audit library now supplies an exclusive durable local reservation and
validated completion primitive; see the [reservation guide](../../docs/INDEPENDENT_AUDIT_VERIFIER.md#durable-local-terminal-receipt-reservation).
It is not yet wired to this policy or a signing service. Protected integration,
restart reconciliation and the remaining controls stay DEVL-225/DEVL-224 work. Key provisioning, rotation, revocation and incident response likewise need
an operator-controlled backend; no software secret is embedded in this example.
