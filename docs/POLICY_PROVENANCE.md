# Policy provenance from source to execution

## Objective and current implementation

An auditor should be able to identify the reviewed source, the approved build,
the exact runtime and policy loaded, the authorized external programs, and the
rules responsible for each effect. A digest proves identity against a trusted
expected digest. It does not prove authorship, source correspondence, correctness,
or that a particular process actually executed those bytes.

Implemented locally:

- English disclosure policies with golden fixtures and interception tests in
  `examples/data_protection`.
- `artifact hash` and `artifact verify` for exact file bytes, including binaries
  such as `ls` and `grep`. These commands do not launch the inspected program.
- `run --policy-sha256` for a compiled policy. The runtime parses the same buffer
  it verified, eliminating a second path lookup between hash check and parsing.
- Policy and program artifact identities aligned with the existing evidence
  and release serialization: sorted-key pretty JSON from the parsed artifact.
- The exact verified file digest recorded separately in policy decision logs.
- Repeatable application-integrity, policy-case and offline process-replay reports,
  with explicit opt-in raw evidence and externally checkable report digests.
  See [Compliance reports](COMPLIANCE_REPORTS.md).

- Independent `devlish-audit` verification of exact-byte signatures and signed
  receipts against format-3 log chains and independently supplied receipt digests.
  This does not establish protected receipt issuance or actual execution.

- Signed release manifest verification and native `run-verified` admission bind
  approved runtime/program/policy bytes, scope and controls to a session, with
  a durable local rollback floor. These checks assume an operator-controlled
  profile, environment and state directory.

- Optional operator-pinned builder statements authenticate separate builder keys,
  build-input claims and exact output coverage. These are Devlish detached signatures,
  not verified GitHub CI/Sigstore attestations or proof of build execution.

Not implemented yet: trusted CI build attestations, enforced external-process launch,
OS isolation, production signed-receipt issuance and independent storage,
and mandatory enforcement on every entry point. The existing release registry's author/approver names are labels, not
cryptographic identities. This document specifies those remaining links.

## The chain and its evidence

| Link | Evidence | Enforced check |
| --- | --- | --- |
| Repository to approved source | Repository identity, immutable commit, reviewed policy source and transitive imports, protected review record | Approved repository and commit; no dirty/untracked source accepted as an official build |
| Source to compiler/runtime | Builder-signed provenance naming commit, workflow identity, build inputs, toolchain, lockfiles, target and resulting binary digest | Trusted builder/workflow and exact artifact digest; independent reproducible build where feasible |
| Policy source to compiled policy | Source/import digests, compiler binary digest, compiler options, compiled artifact and test-evidence digests | Approved compiler and source closure; compile in controlled workspace; verify golden tests |
| Compiled policy to authorized release | Signed release manifest binding source, artifact, tests, runtime, tool catalog and policy identity | Trusted release signer, purpose, environment, expiry, revocation and rollback floor |
| Authorized release to session | Verified manifest and loaded artifact digests, runtime measurement, session identity and effective permissions | Verify before any effect; consume verified bytes; prohibit caller-selected replacement policy |
| Session to external program | Tool identity, exact executable digest, dependency/image identity, constrained arguments/environment | Verify and execute through the same trusted launcher; reject unknown or changed tools |
| Effect to retained audit | Policy decision, outcome, run identity, sequence, prior hash, signed terminal receipt | Persist intent first, correlate outcome, anchor receipt outside the workload; detect missing completion |

Attestations are claims by a trusted builder, not a mathematical proof of correct
compilation. Reproducible builds strengthen source correspondence; the OS,
loader, verifier and deployment authority remain part of the trusted base.
Remote attestation can strengthen runtime measurement where the deployment
requires it, but a self-reported binary hash alone cannot establish runtime
integrity against a compromised host.

## Signing and build approach

Use a protected GitHub build workflow with actions pinned to immutable commits,
a pinned Rust toolchain, locked dependencies, a recorded target, and a controlled
build image. Produce separate attestations for the compiler/runtime and compiled
policy bundles. Include the full imported source closure, not just the top-level
`.dvl` file. Normalize build paths or record them: Devlish artifact metadata
currently includes paths, which can affect reproducibility.

GitHub artifact attestations provide signed build-provenance claims. Verification
must require the intended repository, workflow identity and approved source
revision; accepting any successful attestation is insufficient.
[GitHub artifact attestations](https://docs.github.com/en/actions/concepts/security/artifact-attestations)

Use a separately authorized release identity to sign the release manifest. A
Sigstore bundle can carry signature verification evidence; verification should
pin the expected signer identity and OIDC issuer. Builder authorization and
production release authorization are separate policies.
[Sigstore verification](https://docs.sigstore.dev/cosign/verifying/verify/)

The provenance format should follow SLSA's model of subjects, build definition,
resolved dependencies and run details rather than inventing an incompatible
source/build record.
[SLSA provenance](https://slsa.dev/spec/v1.2/provenance)

A release manifest must cover at least:

- Release identity, intended environment, validity window, revocation reference,
  monotonically increasing release sequence, and authorized policy id/version.
- Source repository/commit and source-closure digest.
- Compiler and runtime exact-file digests, platform and build-attestation digests.
- Each policy's exact-file digest and canonical artifact digest; evidence digest.
- Tool catalog digest, effective permission configuration and containment profile.
- Hash-format version and signature purpose, preventing interpretation as a
  different kind of signed document.

Sign the manifest's exact bytes. Keep the verifier's trust roots and minimum
release sequence outside model-writable storage. Never fetch verification keys
from an untrusted manifest and treat them as authorized merely because the
signature validates.

## Exact file hashes versus artifact identity

Two digest domains are intentional:

1. `file_sha256`: exact distributed bytes, including whitespace. Use for
   signatures, downloads, external binaries and `--policy-sha256`.
2. `artifact_sha256`: SHA-256 of parsed bytecode serialized as sorted-key pretty
   JSON by Devlish. Use to correlate policy evaluation, evidence and releases.

A whitespace change preserves artifact identity but breaks an exact-file pin.
The tests cover this distinction. Policy log formats 2 and 3 align artifact hashes
with governance; initial format 1 used compact JSON for policy/program hashes.
Format 3 adds runtime/event/evidence metadata and a terminal result commitment.
Do not compare the two formats as though they used the same hash domain.

Example commands supported now:

```bash
# Inspection only: a locally computed digest is not an approval.
devlish artifact hash /bin/ls
devlish artifact hash /usr/bin/grep

# Expected values must come from an independently verified release manifest.
devlish artifact verify /bin/ls --sha256 "$APPROVED_LS_SHA256"
devlish artifact verify /usr/bin/grep --sha256 "$APPROVED_GREP_SHA256"

devlish compile examples/data_protection/nppi.dvl --output /tmp/nppi.dvlc.json
devlish artifact verify /tmp/nppi.dvlc.json --sha256 "$APPROVED_POLICY_SHA256"

# The program is evaluated as a candidate tool, governed by the pinned policy.
# Replace these deployment paths with approved local artifacts.
devlish run /opt/devlish/workflows/loan-review.dvlc.json \
  --policy /opt/devlish/policies/nppi.dvlc.json \
  --policy-sha256 "$APPROVED_POLICY_SHA256" \
  --policy-log /var/log/devlish/unique-session.jsonl
```

The same hash command can measure the Devlish runtime, but trusting an unverified
runtime to attest to itself is circular. Bootstrap with an independently trusted
verifier or deployment system. The native `run-verified` command now enforces
program and runtime digests against a signed manifest. It does not protect
process memory or prevent a host administrator from replacing local trust
configuration. See [verified CLI admission](INDEPENDENT_AUDIT_VERIFIER.md#operator-selected-execution-and-durable-admission).

## External programs: `ls`, `grep`, and successors

Prefer Devlish's native directory listing and text operations when adequate;
they reduce the number of independently authorized executables. Where an external
program is necessary, register an absolute executable location, digest, platform,
and allowed argument grammar in the signed tool catalog. Never resolve a
model-supplied executable through PATH or run an arbitrary shell command.

The launcher must:

1. Resolve an authorized tool ID into operator-owned configuration. Hash actual
   file bytes using the runtime's hashing primitive, not a tool-supplied digest.
2. Bind verification to execution. On platforms with an appropriate file-handle
   execution primitive, execute the verified object; otherwise require immutable,
   protected deployment storage with a documented platform-specific guarantee.
   Hashing a path and later spawning that path is vulnerable to replacement.
3. Pin or isolate dynamic libraries, interpreters, scripts, plugins, locale and
   other behavior-affecting inputs. Hashing the `grep` executable alone does not
   identify its complete execution environment. Treat symlinks explicitly.
4. Construct arguments without a shell; constrain flags, workspace paths and
   stdin. Constrain cwd, environment (including loader overrides), inherited
   descriptors, network, filesystem, process count, time and output size.
5. Route stdout/stderr back through the policy disclosure boundary before they
   reach the model or client. A valid `grep` binary can still disclose secrets.
6. Record tool identity and digest, sandbox profile, request commitment, exit
   status and output commitments. Do not store raw NPPI in audit records.

The current hash checker is a preflight diagnostic, not this launcher. Replacing
a binary after `artifact verify` is not prevented by that command. Do not claim
that it proves which executable a later process ran.

## Audit completion and privacy

Use host-generated session IDs, durable sequence numbers, and a receipt that
binds the final chain head and record count to the verified release manifest.
Sign and retain the receipt in a separate audit service or write-once store.
This makes whole-log replacement or suffix deletion detectable relative to a
trusted receipt. An interrupted run cannot manufacture a successful receipt;
unmatched effect intent remains uncertain and requires reconciliation.

Publicly publishing raw hashes of sensitive, guessable values can leak
information through dictionary attacks. Retain evidence under access control;
use keyed commitments or encrypted evidence where appropriate, with verifier
access governed separately. Do not publish customer material to a public
transparency log; publish only suitably designed release/audit commitments.

## Next implementation gates

1. Verify signed release manifests with explicit trust roots and test tampering,
   wrong signer/issuer, wrong commit/workflow, expired/revoked releases and rollback.
2. Install mandatory verified-release loading in server, CLI agent, MCP and browser
   hosts, and close diagnostic/output disclosure channels for sensitive sessions.
3. Build a platform-specific external-tool launcher and test PATH substitution,
   binary replacement between check and exec, loader injection and output leaks.
4. Build on the implemented offline policy-aware replay and tamper tests with
   signed audit receipts and durable recovery for interrupted effects.

No end-to-end chain should be advertised as proven until these gates are met.

## Independent signature verification

The first standalone `devlish-audit` increment verifies detached signatures over
exact bytes against explicit operator trust keys, and checks format-3 log chains
against signed receipts and independently supplied receipt digests. It does not
yet authenticate build provenance, provide protected receipt issuance/storage,
or establish actual execution.
See [Independent audit verifier](INDEPENDENT_AUDIT_VERIFIER.md) for usage and limits.
