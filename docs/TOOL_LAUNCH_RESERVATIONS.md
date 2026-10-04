# Durable one-attempt launch slots

`ToolReservations` is a native storage primitive for a future governed launcher.
It does not authorize or start a process. Production external execution remains
disabled, and the synthetic broker is not yet connected to this store.

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
terminal outcome recording and independent audit reporting remains required.

Sources: [exclusive creation and descriptor-relative access](https://man7.org/linux/man-pages/man2/open.2.html)
and [file and directory synchronization](https://man7.org/linux/man-pages/man2/fsync.2.html).
