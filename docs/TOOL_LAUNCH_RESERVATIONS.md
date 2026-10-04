# Durable one-attempt launch slots

`ToolReservations` is a native storage primitive for a future governed launcher.
It does not authorize or start a process. Production external execution remains
disabled. The synthetic broker now reserves a slot before fork and consumes it
after Devlish approval, before its first kernel continuation.

The operator supplies a private directory owned by the service identity, a tenant,
a session and a positive effect number. The store hashes a domain-separated JSON
identity to name the slot. Release, tool and argument changes do not change that
slot identity, so they cannot reopen the same attempt. Session/effect identities
must come from protected durable host state, never caller retry payloads.

Reservation uses exclusive creation relative to a retained directory descriptor.
It writes the authenticated catalog selection's release, catalog, tool, argument
and containment commitments, then syncs the file and directory before returning
an opaque token. Raw arguments and executable paths are omitted. The record is
bounded to 4 KiB. Catalog membership alone is not current admission, authorization,
image verification or containment enforcement.

The token can be consumed once by value. Consumption writes and syncs a second
record before returning a digest of the expected two-record evidence. The broker
must require successful consumption before continuing execution, in addition to
all admission, policy, identity, deadline and containment checks. A consumed token
is bookkeeping, not a capability that bypasses those checks.

Dropping a token leaves the slot occupied. Existing files, including partial files
and symlinks, are never reopened or overwritten. Failed writes or syncs return no
usable token and never remove the slot. A consumed or uncertain attempt cannot be
automatically retried after restart. This intentionally prefers a withheld action
to a duplicate action; there is no automatic reconciliation or recovery API.

## Storage trust and limits

The configured directory path must be absolute and have normal components; final
symlinks are rejected. Ancestors must be operator-controlled. The open descriptor
pins the directory for the current process if its pathname is replaced. Across
restarts, the operator must protect the directory's stable identity and contents;
a replaced, deleted or rolled-back store can defeat the guarantee. Ordinary Unix
ownership checks do not protect against a compromised service identity or root.

Use a qualified durable local filesystem with reliable exclusive creation and
sync semantics. Network filesystems, replicated storage, disk-controller power
loss behavior and host rollback protection are not qualified here. Tests exercise
reopening, concurrent reservation attempts, partial occupied slots, real write
failure, path replacement and symlink refusal. They do not simulate power loss.

These records are unsigned local evidence. The returned digest can anchor a later
check only when retained independently; it does not authenticate the writer or
prove policy enforcement. Integration with the broker, protected admission state,
production terminal-outcome integration remains required; the native completion
primitive and independent record verification are described below.

Sources: [exclusive creation and descriptor-relative access](https://man7.org/linux/man-pages/man2/open.2.html)
and [file and directory synchronization](https://man7.org/linux/man-pages/man2/fsync.2.html).

## Composed broker checks

The allowed synthetic case persists consumption before continuing the first exec,
rechecks the stream deadline after persistence, and compares the saved bytes with
the returned digest after collection. The denied private-export case leaves an
unconsumed reservation and never enters the candidate tool. Both cases reopen the
store and require a repeat reservation of the same effect to fail. Child descriptor
cleanup excludes the slot file and directory from candidate inheritance.

These tests still use synthetic admission/authority state. They do not implement
a production session counter, outcome reconciliation, or a broker service.

## Independent anchored verification

Supply the reservation file and its independently retained digest with a signed
release and exact tool request:

```bash
devlish-audit --text verify-release manifest.json \
  --signature signature.json --trust operator-trust.json \
  --requirements operator-requirements.json --artifacts operator-artifacts.json \
  --tool-catalog tools --tool-request tool-request.json \
  --tool-containment containment.json \
  --tool-reservation slot.jsonl --reservation-sha256 RETAINED_SHA256
```

Both reservation options require a tool catalog/request, and must appear together.
The verifier freshly validates the release and selection, reads at most 8 KiB,
checks the supplied digest, validates one reserved record, at most one linked
consumption record and then at most one linked completion record, recomputes the operation identity, and compares every binding
to the authenticated selection. Unknown/duplicate fields and incomplete records
are rejected. The signed containment profile remains an optional separate check.

JSON and English findings state whether the bytes match the supplied anchor,
whether the selection matches, and whether the local record says consumed. Writer
authentication, actual execution and policy enforcement remain false. No raw
arguments, executable paths or tenant/session names appear in the reservation
report. A digest supplied by the writer with the file is not an independent anchor.

A retained earlier reserved record cannot establish that no later action occurred.
Neither reserved nor consumed evidence proves execution completion, success, or
safe retry. This command does not launch anything or reconcile an uncertain action.


## Durable terminal capture

On Linux, `ConsumedLaunch::record_capture` takes ownership of the supervisor's
complete `CapturedOutput`. It appends a third `completed` record and syncs the slot
before returning `RecordedCapture`. A failed write or sync returns no capture;
the consumed slot remains occupied and cannot be automatically retried. Crashes
before completion persistence leave uncertainty. There is no reopen or recovery
API. The protected host must bind the captured child to the same slot; these
native objects do not authenticate that relationship independently.

The record links to the exact consumed-record prefix and records the normal exit
code (0–255), lengths and SHA-256 digests of both bounded streams. It contains no
raw output. A nonzero exit is a completed capture, not a successful command.
Stream errors, deadline expiry and abnormal exits never produce `CapturedOutput`
and therefore cannot enter this completion API. Their slot remains uncertain.

The independent verifier accepts the optional third record, rejects malformed,
reordered, duplicate, oversized or incorrectly linked evidence, and reports the
recorded completion and exit code. An earlier independently anchored prefix still
cannot establish that no later action happened. The report always denies any
claim that it authorizes disclosure or proves actual execution/policy enforcement.
Hashes and lengths can leak information through guessing or correlation; protect
this metadata and its anchors as audit evidence, even though raw output is omitted.

The synthetic Linux broker now persists and independently verifies completion
alongside the returned capture. Production admission/session state,
classification, disclosure journaling and the output adapter remain required.
The [Devlish disclosure rule](../examples/tool_output_disclosure/README.md) states
the separate conditions for releasing output; persistence alone does not allow it.
