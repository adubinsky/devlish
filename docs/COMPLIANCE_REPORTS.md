# Repeatable compliance reports: tamper evidence first

Devlish now provides three scoped reports, followed by integrity verification
and an English explanation. The machine-readable report states exactly what
passed. Its name does not imply legal certification or coverage of every
possible behavior of an application.

Fractional evidence values must retain their exact IEEE-754 representation when
JSON is read back. The VM and independent verifier enable `serde_json`'s
`float_roundtrip` feature; native and browser runtimes inherit it through the VM.
The default fast parser can otherwise round some fractional clock results to a
neighboring value and make an untouched record fail its hash check. Regression
tests cover fixed clock-sized values and a signed receipt containing one such
value. This does not make floating-point arithmetic arbitrary-precision or
change the requirement to verify original evidence bytes. Do not repair old
evidence by rehashing it; newly built runtime/verifier binaries require fresh
release approval.

| Command | What it establishes |
| --- | --- |
| `report application` | Listed files match the expected digests in a supplied manifest |
| `report policy` | The compiled policy reproduces every supplied expected decision |
| `report process` | The supplied program, policy and input reproduce the recorded policy decisions, tool responses and VM result offline |
| `report verify` | A report's self-hash matches, optionally also matching an independently retained digest |
| `report explain` | A verified report's conclusion, rendered by an embedded Devlish program |

Each report includes a canonical `report_sha256`, the verifier executable's
on-disk digest, an explicit scope, and `signature_verified: false`. There is no
wall-clock timestamp in the report body: the same evidence and verifier produce
the same report. A fresh live run can produce different evidence, such as a
new clock value or model response. Replay uses the original recorded values.

## Application report

Create an operator-maintained manifest. Paths are relative to the manifest,
or absolute. Digests are exact-file SHA-256 values from your approved baseline.
The example values below are placeholders, not approved digests.

```json
{
  "format": "devlish-application-manifest",
  "format_version": 1,
  "files": [
    {"id": "runtime", "role": "runtime", "path": "/opt/devlish/devlish-core", "sha256": "REPLACE_WITH_APPROVED_64_HEX_DIGEST"},
    {"id": "workflow", "role": "program", "path": "workflow.dvlc.json", "sha256": "REPLACE_WITH_APPROVED_64_HEX_DIGEST"},
    {"id": "policy", "role": "policy", "path": "policy.dvlc.json", "sha256": "REPLACE_WITH_APPROVED_64_HEX_DIGEST"}
  ]
}
```

Additional roles include `source`, `configuration`, and `tool`, for example
an approved `grep` binary. At least one runtime entry is required. A process
report additionally requires matching program and policy entries. Missing,
changed or unreadable files produce a failing report and nonzero exit.

```bash
devlish report application application.json --output application-report.json
```

`artifact hash <file>` helps inventory files. Measuring arbitrary current files
and immediately accepting those measurements is not approval. This first phase
reports integrity against the supplied baseline; it does not authenticate the
baseline, verify a GitHub build, run an application's whole test suite, or prove
that the listed external tools were executed.

## Policy report

Compile the policy and provide named golden cases with `input.effect`,
`input.request`, and an exact `expected` decision containing `allow` and `reason`.
The existing NPPI and company-IP fixtures are compatible. An optional
`input.authority` supplies separate context for authority-aware policies. It is
offline fixture data, not authenticated live authority; nested request fields
cannot populate this channel. Policy reports explicitly record
`authority_authenticated: false`.

```bash
devlish compile examples/data_protection/nppi.dvl --output policy.dvlc.json
devlish report policy policy.dvlc.json examples/data_protection/nppi.cases.json \
  --output policy-report.json
```

Evaluation is isolated from external effects and bounded by the policy VM's
instruction limit. Empty fixtures, duplicate case names and malformed expected
decisions are rejected. Failed evaluations or different decisions produce a
failing report. The report records input/expected/actual hashes rather than raw
case payloads. Case names are retained for review, so do not put customer data
in their names. Passing supplied cases is not exhaustive verification.

The receipt authorization cases use the same command:

```bash
devlish compile examples/receipt_authority/authorize.dvl --output receipt-policy.json
devlish report policy receipt-policy.json examples/receipt_authority/cases.json \
  --output receipt-policy-report.json
devlish report explain receipt-policy-report.json
```

This repeats the Devlish decisions without signing receipts or contacting a key
backend. Recorded issuer decisions have their own receipt-issuer report described below.

## Capture a process, then report it offline

A hash-only trace cannot recreate external responses. Capture replay evidence
explicitly when running the compiled workflow:

```bash
devlish run workflow.dvlc.json --input '{"case_id":"synthetic-demo"}' \
  --policy policy.dvlc.json --policy-sha256 "$APPROVED_POLICY_SHA256" \
  --policy-log process.jsonl --policy-evidence --quiet
```

`--policy-evidence` includes raw effect requests and responses in the policy
log. This may contain NPPI, IP, prompts or caller-supplied credentials. The log
is created exclusively with mode 0600 on Unix; it is not encrypted. Store it
under restricted access. Without the flag, logs retain only hashes and cannot
support full process replay. The original input remains a separately supplied
file; its parsed JSON must match the input used in the run.

After collecting the application report, policy report and original input:

```bash
devlish report process workflow.dvlc.json policy.dvlc.json input.json \
  process.jsonl application-report.json policy-report.json \
  --output process-report.json

devlish report explain process-report.json
```

The report checks:

1. Every log sequence number, prior hash, record hash and completion marker.
2. Policy, program and input identities against the run, and exact artifact
   identities against the parent application/policy reports.
3. Each request and recorded response against its commitment; each allowed
   decision has a matching outcome.
4. A fresh offline execution: the actual policy runs again; the host returns
   only matching recorded responses. No filesystem writes, network requests,
   model calls or other live tool effects occur during replay.
5. Replayed decisions, outcomes and terminal VM-result hash against the trace.

The process report binds both parent report hashes and the trace's exact-file
hash, final chain head and record count. Input or artifact changes, reordered
records, altered effects and missing completion are rejected. A hash-only or
older log is refused rather than treated as replayable. Logs are currently
format 3; formats 1 and 2 did not record the runtime/event/evidence metadata and
terminal result digest needed for these checks.

`passed` means the recorded execution was reproduced. A denied tool call can
correctly produce a passing report with `execution_succeeded: false` and a
nonzero `denied_effects` count. A checkpoint pause is not completed execution.
`task_success_verified` remains false: replay alone does not establish that the
business goal was correct or fulfilled. Use task-specific Devlish assertions
and expected-output fixtures to establish that separate claim.

The runtime identity is an on-disk measurement reported by the original host.
It is linked to the application manifest, but it is not independently attested
proof of the process image that ran. The separately recorded verifier digest
identifies the reporter used to check the evidence.

## Keep an independent anchor

Store `report_sha256` from the final process report outside the writable
application/evidence directory. That digest transitively commits to the two
parent reports and trace. Do not derive your expected anchor from the report
you are currently trying to authenticate.

```bash
devlish report verify process-report.json --sha256 "$RETAINED_PROCESS_REPORT_SHA256"
```

Without `--sha256`, verification checks internal consistency only. An attacker
can rewrite a whole report/chain and recompute ordinary hashes. A separately
retained original digest detects that replacement. `report verify` does not
rerun the application or policy checks; repeat the report commands to do that.
It can confirm integrity of a failing report, so inspect `reported_checks_passed`
as well as integrity success.

Report output files use exclusive creation: choose a new path for a repeated
report or compare stdout. Raw evidence is never included in the process-report
body. Protect even hash-only reports, because low-entropy private inputs can
be guessed from their hashes.

## Tamper resistance follows this evidence baseline

The next phase adds independently verified signatures and build provenance,
protected policy/manifest storage, mandatory verified loading on every entry
point, constrained external-tool launching, and an external signed audit receipt.
It must also close diagnostic/output channels before real sensitive workloads.

Evidence first makes those future controls testable: replacing a binary,
changing a policy, forging a decision, removing a record, or replaying different
input already has an explicit verification outcome. Resistance then prevents
or independently authenticates those changes rather than merely detecting
inconsistency. See [Policy provenance](POLICY_PROVENANCE.md) for the trust-chain
and platform-specific launcher design.

## Independent signature verification

The first standalone `devlish-audit` increment verifies detached signatures over
exact bytes against explicit operator trust keys, and checks format-3 log chains
against signed receipts and independently supplied receipt digests. It does not
yet authenticate build provenance, provide protected receipt issuance/storage,
or establish actual execution.
See [Independent audit verifier](INDEPENDENT_AUDIT_VERIFIER.md) for usage and limits.

## Replay recorded receipt authorization

The native issuer can record one durable journal per signing attempt using
`ReceiptJournal::create` and explicit operator opt-in through
`ReceiptIssuer::with_replay_evidence`. This opt-in stores raw request and authority
snapshots. Keep journals in operator-controlled storage with access appropriate
to the input data. Default issuer records contain digests only and cannot replay.
Files are created exclusively with mode 0600 on Unix; each append is synced, and
creation also syncs the parent directory. Failed, out-of-order or oversized
writes poison the recorder. Closed journals cannot accept another attempt.
Records are limited to 1 MiB each and four records per attempt. Replay requires
the final newline terminator and applies the 4 MiB journal/64 MiB compiled-policy
limits while reading, before parsing candidate files.

Retain the completed journal's exact SHA-256 independently, then run:

```bash
devlish report receipt-issuer receipt-policy.json issuer.jsonl \
  --sha256 "$RETAINED_ISSUER_JOURNAL_SHA256" --output issuer-report.json
devlish report explain issuer-report.json
```

Use a lowercase hexadecimal digest. Computing a new digest from suspect evidence
at verification time does not establish independent retention. Replay checks the
anchor, chain, phase order, exact policy identity, unchanged request and authority
transition, and reruns the Devlish preflight/final decisions without a backend.
It checks that the recorded outcome identifies the authorized operation and
receipt. Missing outcomes remain unresolved; there is no implicit successful
completion or automatic retry. Hash-only journals cannot pass this report.

A passing report means recorded decisions reproduced. It separately identifies
`completed`, `denied` or `uncertain` signing status; passing replay is not successful
issuance. Authority snapshots and protected execution remain unauthenticated.
The report does not verify a receipt signature or prove that a denied request
never reached a compromised signer. Use `devlish-audit verify-issuance` for the
separate cryptographic check. The Devlish-authored English explanation preserves
these distinctions, without consulting a model.
