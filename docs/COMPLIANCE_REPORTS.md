# Repeatable compliance reports: tamper evidence first

Devlish now provides three scoped reports, followed by integrity verification
and an English explanation. The machine-readable report states exactly what
passed. Its name does not imply legal certification or coverage of every
possible behavior of an application.

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
The existing NPPI and company-IP fixtures are compatible.

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
