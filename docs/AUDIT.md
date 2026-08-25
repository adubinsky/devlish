# Execution provenance: the audit log

Evidence bundles ([docs/EVIDENCE.md](EVIDENCE.md)) prove a rule version was
tested before release. The **audit log** answers the after-the-fact question:
*which rule version produced this result, from what input, on which runtime?*
Every run of a governed rule (one with a `Rule:` manifest section) emits one
provenance record at completion. Ungoverned programs emit nothing and pay no
overhead.

## The record

```json
{
  "artifact_sha256": "cd3a6b10...",
  "input_sha256": "44136fa3...",
  "instruction_count": 3,
  "output_sha256": "b4987fc0...",
  "prev_sha256": null,
  "rule_id": "pricing.tier",
  "rule_version": "1.0.0",
  "runtime": { "kind": "native", "version": "0.1.0" },
  "success": true,
  "timestamp": 1784729668
}
```

- `artifact_sha256` — hash of the canonical (sorted-keys pretty) serialization
  of the compiled bytecode: the same form evidence bundles hash, so an audit
  record and an evidence report for the same artifact agree. Stable under
  reformatting of the bytecode file.
- `input_sha256` / `output_sha256` — canonical (sorted-keys compact) JSON
  hashes of the run input and of the VM's own result envelope. On failure the
  hashed object is exactly `{"error": <message>, "success": false}`. Note the
  hash covers the VM envelope, not any wrapper a runner adds around it: the
  native CLI pretty-prints (or, for `Respond with`, prints only the response
  value), and runner failure wrappers may add fields. To recompute, start
  from the VM envelope, serialize sorted-keys compact, and hash.
- `runtime` — which runner executed the rule (`native` or `wasm`) and its
  version. Both runtimes produce the identical record shape and identical
  hashes for the same artifact and input.
- `prev_sha256` — hash chain link, added by the log writer (below). Absent
  from records delivered to `onAuditRecord`; chaining is the persister's job.
- `success` is recorded on failures too: a governed run that fails still
  leaves a record. A run that pauses at a checkpoint (agent interaction)
  records `"paused": true` alongside `success`; the resumed run emits its
  own record.

If the audit log cannot be written, the run fails. A governed run whose
provenance cannot be persisted must not report success.

## Native: `--audit-log`

```bash
devlish run pricing.dvl --input '{"amount": 100}' --audit-log audit.jsonl
# or
export DEVLISH_AUDIT_LOG=audit.jsonl
```

Records append as JSON lines. Each record's `prev_sha256` is the SHA-256 of
the previous line's exact bytes (`null` for the first record), and the chain
continues across process restarts, so the log is tamper-evident end to end.

## Verifying a log

```bash
devlish audit-verify audit.jsonl
```

Walks the chain and exits non-zero at the first line whose `prev_sha256` does
not match, which catches a modified, reordered, or deleted interior record.
A bare hash chain cannot see edits at or after its final record (tail
truncation, or rewriting the last record while keeping its `prev_sha256`), so
`audit-verify` also prints the latest record's sha256 -- anchor that value
externally (the Sigstore/Rekor workstream, DEVL-118) to close the gap. The
chain hashes each logical line (as split on newlines, blank lines skipped),
and concurrent writers are serialized with an advisory file lock on
platforms that support one; on platforms without advisory locking, keep a
single writer per log.

## Effect journaling and deterministic replay

An audit record proves *which* rule ran; it cannot by itself prove the result
was honest, because rules call live effects (HTTP, files) and pause for
Checkpoint judgment. Re-running against the live world gives a different
answer, so a bare re-run proves nothing. `--journal` closes that gap:

```bash
devlish run pricing.dvl --audit-log audit.jsonl --journal audit.jsonl.attachments
```

For each governed run this archives, as one content-addressed attachment
(`<sha256>.json`, linked from the record via `journal_sha256`):

- the exact parsed bytecode and the full run input (not just their hashes),
- every host-effect exchange in order: request and response for HTTP calls,
  file reads, stats, globs, service calls, and the write effects the run
  attempted,
- the event-emission setting (it changes the result envelope).

Credentials never enter the journal: they are resolved inside the host,
below the journaled boundary, and auth headers are injected after it. The
journal does contain full request/response bodies, so store attachments with
the same care as the data the rule processed.

```bash
devlish replay audit.jsonl                      # replay the last record
devlish replay audit.jsonl --line 3             # or a specific one
```

Replay verifies the attachment against its content address, checks the
archived bytecode and input against the record's hashes, then re-executes the
bytecode feeding effects from the journal instead of the live world. Every
effect request must match the journal in kind, shape, and order; the output
hash and instruction count must match the record. Any divergence fails with
a nonzero exit -- and after this, a mismatch is itself evidence: either the
archive was tampered with or the toolchain changed.

Governed execution is deterministic by construction: the VM has no clock or
RNG builtins (date helpers are pure arithmetic on argument values), JSON
serialization is canonical (sorted keys), and float semantics are defined
(IEEE-754 f64, whole-valued results normalize to integers -- saturating at
the 64-bit integer bounds -- and NaN/infinity are hard errors). Identical
bytecode + input + effect responses yield byte-identical output; this is
tested. `replay` verifies the log's hash chain before trusting a record;
external anchoring of the tail (DEVL-118) remains the guard against
whole-log rewrites.

## Browser / Node: `onAuditRecord`

Embedding apps persist records in their own store:

```ts
const tool = await loadTool({
  bytecode,
  onAuditRecord: (record) => auditStore.append(record),
});
```

The callback fires once per governed run (never for ungoverned tools) with
the record shape above, minus `prev_sha256`. The WASM sandbox has no clock,
so `timestamp` is stamped at delivery.

Two caveats: a run that dies in a WASM trap (a runtime panic) never reaches
the audit path, so a trapped result (`trapped: true`) carries no record; and
the one-shot `runTool` helper does not accept `onAuditRecord` -- use
`loadTool` for governed rules whose provenance you persist.
