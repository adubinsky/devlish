# Tool output disclosure policy

This executable Devlish example separates permission to run a tool from permission
to disclose its output. It is a pure policy example; production tool execution and
its output adapter remain disabled. It does not classify arbitrary text or claim
that a string labeled public is actually public.

The protected host supplies a separate authority record containing the exact
session, operation, destination, stdout and stderr. It also supplies independently
established classifications for both streams, current admission, capture completion
and durable outcome recording. The caller supplies only the proposed disclosure.
The policy allows model or client disclosure only when every binding matches,
both streams are public, and all three host conditions hold. Unknown labels deny.

Synthetic examples include a borrower account in stdout (NPPI), unreleased design
information in stderr (company IP), unrecorded/partial output, changed destination,
changed content and caller attempts to supply classifications or authority.
Reasons are fixed English sentences and never echo captured data.

Run the executable cases:

```bash
cargo test --locked --manifest-path crates/devlish_core/Cargo.toml --test tool_output_disclosure
```

Generate a repeatable policy report with the built CLI:

```bash
devlish-core compile examples/tool_output_disclosure/authorize.dvl --output /tmp/tool-output-policy.json
devlish-core report policy /tmp/tool-output-policy.json examples/tool_output_disclosure/cases.json > /tmp/tool-output-report.json
devlish-core report explain /tmp/tool-output-report.json
```

The report commits to the policy and case bytes, checks every expected decision and
reason, and explicitly leaves authority authentication false. The integration test
runs the report twice, compares exact bytes, verifies its digest and checks that
its English explanation preserves that limitation.

## Integration contract

The native adapter must retain the bounded capture privately, bind authority to
those exact bytes and the protected session/effect identity, record the terminal
outcome durably, and check current release admission immediately before disclosure.
Classification must come from a deterministic operator-approved rule or inherited
protected data labels, never the model or candidate tool. Unclassified content
remains withheld. Non-text output requires an explicit encoding/classification
policy; this example permits text only.

Evaluate the compiled Devlish policy through the separate host authority channel.
An allow decision does not send data: the adapter must record the disclosure
intent, send only the approved bytes to the bound destination, and record the
outcome. Uncertain sends must not be retried automatically. Public logs, diagnostics,
files and all other destinations need their own policies; this example denies them.
Neither stream is independently released if the other is private.

The sample authority objects in cases.json are test inputs, not authentication.
A compromised host can fabricate labels and completion claims. These tests establish
repeatable policy behavior, not execution attestation, durable recording or delivery.
Raw policy replay inputs contain the capture and require protected evidence storage.
Ordinary reports should retain commitments and fixed reasons, not raw streams.
