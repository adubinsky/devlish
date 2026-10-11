# Initial tool execution authorization in Devlish

This executable policy prototype makes the initial-execution decision readable
in Devlish. It is part of DEVL-220 and uses only synthetic fixtures. It is not yet
connected to a production broker or the Linux broker experiment. Production tool
dispatch remains disabled.

The rule permits `continue_initial_tool_exec` only for a reserved initial launch
whose request matches independently supplied session, tool, release, catalog,
executable, argument and containment commitments. Active admission, a prior
allow decision from the governing tool policy, ready controls and a matching
trusted-setup notification are required. Unknown fields, caller-provided launch
claims, later execution, consumed/uncertain grants and substituted commitments
are denied with fixed English explanations.

Run its 27 cases and input-channel/sequence tests:

```bash
cargo test --locked --manifest-path crates/devlish_core/Cargo.toml --test tool_execution_authority
```

Generate a repeatable report using the ordinary offline policy-report workflow:

```bash
devlish compile examples/tool_execution_authority/authorize.dvl --output tool-policy.json
devlish report policy tool-policy.json examples/tool_execution_authority/cases.json \
  --output tool-policy-report.json
devlish report explain tool-policy-report.json
```

The report checks deterministic decisions on supplied cases. Its
`authority_authenticated` finding remains false. Passing cases do not prove that
a host enforced controls or that a tool actually executed.

## Protected host obligations

The model's ordinary tool request remains only `tool_id` and `arguments`.
The request shown here is a broker-internal prepared intent; model JSON must not
be promoted into it or into the separate authority channel.

| Authority fields | Required independent basis |
| --- | --- |
| session_id, tool_id | Executor-owned session and exact signed catalog selection; nonempty canonical identities |
| release_sha256, catalog_sha256, tool_sha256, containment_sha256 | Retained bytes verified under current operator trust, with canonical lowercase SHA-256 digests |
| arguments_sha256 | Digest of the exact retained argument vector under a defined, consistent encoding, not a caller-supplied digest |
| phase, notification_matched | Trusted initial child setup and validated kernel notification; matching pointers alone is insufficient |
| admission_active | Current release, revocation, clock and rollback/admission checks at dispatch |
| policy_allowed | Actual prior tool-effect policy and permission decision for this exact intent |
| controls_ready | Successful application of every control required by the authenticated profile, including independent timeout/output supervision |
| reservation_state | Exclusive broker-owned grant, never model-provided state |
| evidence_source | `protected-launcher` only after constructing this descriptor from those checks |

After Devlish allows, the broker must record the decision and irreversibly
consume the grant before continuing the syscall. Recording failure, errors,
uncertain continuation outcomes or consumed state must not trigger another
execution. Later candidate requests are denied even when their commitments match.
Protected storage, cancellation and recovery still need production integration.

The policy is pure: evaluating the same reserved synthetic state twice produces
the same answer. It cannot reserve atomically, authenticate JSON, protect a key,
stop privileged process modification or prove that evidence is truthful. Tests
change independently supplied host state to exercise consumed/uncertain decisions;
that is a decision contract, not a concurrency mechanism. Raw paths, arguments,
NPPI and company IP are not echoed in explanations. Hashes do not encrypt data.
