# Real local programs with operator-selected policy defaults

The ordinary `devlish --run` command can execute programs found directly in its
startup working directory or operator PATH directories. `tool_id` is a simple
program name using ASCII letters, digits, `_`, `-`, `.`, or `+` (excluding `.`
and `..`), such as `ls` or `script.sh`; arguments are a list passed directly to the program.
The existing `Run catalog tool` syntax and declared tool permissions are reused.
This local profile resolves the name through directories rather than a signed
catalog. The working directory and PATH are captured once, outside model input.

Build and run a real directory listing, from the repository root:

```bash
make install
devlish --run examples/local_tools/tool.dvl \
  --quiet --input '{"tool_request":{"tool_id":"ls","arguments":["-1"]}}' \
  --policy examples/local_tools/policy.dvl --policy-log /tmp/local-listing.jsonl \
  --default-authorization allow-unless-forbidden
```

Use a new log filename on every run; existing logs are never overwritten. The
listing is captured privately and the program prints a fixed acknowledgement.
Add `--policy-evidence` only when you want sensitive requests/results included in
the protected replay log. Default logs retain commitments rather than raw output.

Change the posture to `deny-unless-allowed`: this listing is blocked because the
example policy explicitly abstains for that request. Change the program name to
`grep`: it is denied in BOTH postures because the example contains an explicit
prohibition. Extra listing arguments are also explicitly denied.

## Model proposes the request; Devlish governs execution

With your existing `OPENROUTER_API_KEY` and a configured model, run:

```bash
devlish --run examples/local_tools/agent.dvl \
  --quiet --provider openrouter --model YOUR_OPENROUTER_MODEL \
  --policy examples/local_tools/policy.dvl --policy-log /tmp/local-agent.jsonl \
  --default-authorization allow-unless-forbidden
```

The Devlish program sends a fixed public prompt, receives a tool request, runs the
approved listing, and returns the fixed acknowledgement. It does not send captured
filenames back to the model. This is a bounded one-step example, not a general
coding agent. Automated tests execute real local programs without provider calls.

## Default authorization

A policy can return `decision` equal to `allow`, `deny`, or `abstain`, plus a
nonempty English `reason`. Existing boolean `allow` decisions still work.

- Explicit allow and deny retain their meaning in both postures.
- Only abstention uses the operator-selected default.
- Missing, malformed, conflicting, or failed policy decisions block execution.
- The caller cannot set posture in a tool request.

`--default-authorization` requires `--policy` and `--policy-log`. The selected
posture is pinned in policy identity and recorded with each decision's origin;
offline process replay restores it from the recorded session. Policy reports also
accept `--default-authorization`; process replay rejects a policy report made with
a different default posture. With no flag,
existing behavior remains deny-by-default for abstentions. A policy is one Devlish
program returning one decision: write explicit prohibitions before permissive
branches, as in this example. This setting never converts an explicit denial into
an allowance and does not remove program-declared capability checks.

## Scope of the location guardrail

Canonical executable targets must remain under the working directory or one of
the canonical PATH directories. An out-of-root symlink fails; an in-root symlink
is accepted. Empty, relative, missing or non-directory PATH entries are ignored.
The local folder is searched first. Tool IDs cannot contain paths; a local symlink
can name a nested local program. A package-manager symlink pointing outside every
approved root needs its real directory included in operator PATH.

The child receives no inherited credentials or loader environment: only the
pinned PATH and C locale. stdin is closed. Each output stream is limited to 64 KiB,
execution/capture to five seconds, and executable inspection to 64 MiB. Abnormal
exit, capture failure, invalid UTF-8 or exceeded limits withhold all output.
Nonzero normal exit codes are preserved for the Devlish program to interpret.

This is a location guardrail, not a filesystem/network sandbox. Allowed programs
can access files and network using the current user identity, launch other programs,
or interpret arguments themselves. No shell is inserted by Devlish. Process-group
cleanup is best effort; deliberately detached children are not contained.
Concurrent file replacement is not prevented by the inspection hash. Results mark
immutable execution and OS containment as unverified. Keep the local directories
and PATH under your control. Stronger signed/contained admission remains a separate
profile; this change enables ordinary CLI execution, not verified or HTTP dispatch.
