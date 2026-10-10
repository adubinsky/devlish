# Executable effect policies

The first harness enforcement primitive is available through `devlish run`.
A trusted operator supplies a Devlish policy independently of the program being
run. The compiled policy evaluates each tool effect before the host executes it.
The policy is loaded once and pinned in memory for the run.

```bash
cargo build --manifest-path crates/devlish_core/Cargo.toml
crates/devlish_core/target/debug/devlish-core run examples/effect_policy/allowed.dvl \
  --policy examples/effect_policy/policy.dvl --policy-log allowed-policy.jsonl --quiet

# Denied before any provider call; no API key is needed.
crates/devlish_core/target/debug/devlish-core run examples/effect_policy/denied.dvl \
  --policy examples/effect_policy/policy.dvl --policy-log denied-policy.jsonl --quiet
```

Use a new log filename for every run. Existing paths are rejected, never
truncated or appended to. Installed CLIs expose the same flags after rebuilding.

For borrower-information and company-IP scenarios, see the
[data-protection examples](../examples/data_protection/README.md). The
[provenance design](POLICY_PROVENANCE.md) distinguishes current integrity checks
from the remaining signing and execution-attestation work.

To pin a deployed policy, compile it first and supply `--policy-sha256` with
an independently approved exact-file digest. Verification and parsing use the
same buffer. The decision log records that file digest separately from the
canonical artifact identity. `devlish artifact hash <file>` computes a digest;
`devlish artifact verify <file> --sha256 <digest>` checks one. Neither command
verifies a signature or proves a later external process executed the file.

## Writing rules

Policies use existing Devlish syntax. No model interprets these rules.

```text
Rule:
  id: harness.clock_only
  version: 1.0.0

Ask "Which effect is requested?" as effect
Ask "What are the effect arguments?" as request

If effect equals "clock_now":
  Respond with record with true as allow and "Reading the clock is permitted." as reason

If effect equals "respond":
  Respond with record with true as allow and "Returning the result is permitted." as reason

Respond with record with false as allow and "This policy permits only clock reads and responses." as reason
```

Every evaluation runs in a fresh VM without external effects or credentials,
with a 100,000-instruction limit. It must respond with a record containing a
boolean `allow` and a nonempty text `reason`. Errors, malformed decisions,
checkpoints, and missing responses are denials. A policy cannot override the
program's existing manifest restrictions. Manifest rejections happen before
host dispatch and therefore do not produce a policy decision record.

The policy receives `effect` and `request` as input. Request fields match the
host method's arguments:

| Effect | Request |
| --- | --- |
| `write_file`, `read_file`, `call_service`, `llm_complete`, `random_draw` | Existing host request record, unchanged |
| `http_request` | `method`, `url`, `body`, `headers` |
| `http_download` | `url`, `path` |
| `read_xlsx_rows` | `path`, `sheet` |
| `file_copy`, `file_move` | `source`, `destination` |
| `file_mkdir`, `file_delete`, `file_exists`, `file_stat`, `file_list` | `path` |
| `file_glob` | `pattern`, `directory` |
| `clock_now` | `kind` |
| `respond` | `value` |

Runtime diagnostic events and audit persistence are trusted host plumbing,
not tool calls. They are not filtered by this policy. In particular, this
primitive is not an output-redaction boundary for `Print` or debug logging.

## Execution records

Policy log format 3 retains the sorted-key pretty JSON artifact hashing introduced
in format 2 (format 1 used compact JSON). It additionally records the measured
runtime identity, event mode, evidence-capture mode and terminal result digest.
The JSONL log contains a start record, policy decisions, effect outcomes, and
a finish record. Every record has a sequence number and hash of the previous
record. Start records bind the run to policy identity and hashes of the
program and input; decisions include the exact policy artifact hash, effect,
request hash, boolean decision, and policy-authored explanation.

The host syncs a decision to storage before invoking an allowed effect. An
outcome records success or failure and the corresponding result or error
hash. By default, raw arguments, model output, and credentials are not copied into this
log. The explicit `--policy-evidence` option includes raw requests and responses
for offline replay; treat such logs as sensitive data. Policy-authored reasons are stored verbatim, so policies should use fixed
explanations instead of interpolating sensitive input.

A recording failure blocks subsequent tool effects and prevents the CLI from
reporting a successful run, even when the program catches the effect error.
An allowed decision without an outcome means the action's result is unknown:
it may have executed. It must not be automatically retried. A missing finish
record means the run did not record completion.

These hashes allow integrity checking when compared with trusted evidence;
they are not signatures. An attacker able to replace the entire log can
recompute the chain. Policies and logs must be protected from the executing
program by deployment permissions. Hash-only records cannot independently
reconstruct inputs or replay effects.

## Current scope and next steps

This increment provides the shared `devlish_vm::policy::PolicyHost` boundary
and CLI `run` integration. Unflagged runs retain their existing behavior.
`harness run` and `harness resume` also accept `--policy`, `--policy-log`, and
`--default-authorization`. `harness generate` uses a fixed authoring policy
and requires a fresh `--policy-log`. HTTP, MCP, and browser entry points do not
yet install this boundary.
A future governed service must install it from operator-owned configuration,
never from a request's proposed policy.

Offline policy-aware replay is available through `report process` for format 3
logs captured with `--policy-evidence`; see [Compliance reports](COMPLIANCE_REPORTS.md).
The older `--journal`/`replay` interface is separate and still cannot be combined
with `--policy`. Next are server-owned enforcement, result checks written in
Devlish, and durable checkpoint recovery. The agent loop will then be authored in Devlish over
these primitives.

This is not an operating-system sandbox. Path arguments are not canonicalized
by this wrapper; string checks cannot enforce filesystem containment in the
presence of symlinks or races. HTTP redirects and work performed inside an
allowed host service also need host-level constraints. Do not authorize broad
filesystem, network, or process capabilities based on this example alone.

## Host-supplied authority context

`EffectPolicy::evaluate_with_authority` adds a separate `authority` input for
host-owned decisions such as receipt issuance. Request fields cannot populate
this input. Ordinary `evaluate` and `PolicyHost` calls supply null authority.
The policy still runs without external effects and under the same instruction
budget. Supplying authority is a host assertion, not authentication or attestation;
the caller of this Rust API must protect its origin and bind any later action to
the evaluated snapshot.

See the [Devlish receipt authorization example](../examples/receipt_authority/README.md)
for rules, synthetic cases and the required protected-host integration contract.
