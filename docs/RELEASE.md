# Controlled releases: the registry

Evidence bundles ([EVIDENCE.md](EVIDENCE.md)) prove a rule version was tested;
the audit log ([AUDIT.md](AUDIT.md)) proves what ran. The **release registry**
closes the loop: it is the lifecycle that turns an approved artifact into the
only thing production runs.

## The registry

`registry.json` (one per rules repo) is an append-only, hash-chained event
log. Every lifecycle action appends one event carrying the sha256 of the
previous event, so editing or removing any interior event breaks the chain
(`devlish release verify`). Like any bare hash chain, a whole-registry
rewrite regenerates a valid chain -- `release verify` prints the latest
event's sha256 so operators can anchor it externally (the Sigstore/Rekor
workstream, DEVL-118, closes this end to end). Releases are never edited, only superseded; the
current status of each `rule@version` is derived by folding its events:

```text
propose -> draft -> approve -> approved -> publish -> published -> retire -> retired
                                              ^                                 |
                                              +--------- publish (rollback) ----+
```

Git provides transport and history; the chain provides the audit semantics
independent of git.

## Walkthrough

**1. Propose.** Compiles the rule, runs its golden cases ([evidence](EVIDENCE.md)),
and adds a draft entry binding the artifact hash to the evidence hash. A rule
whose cases fail cannot enter the registry.

```bash
devlish release propose pricing_tier.dvl --author andrew
# proposed pricing.tier@1.0.0 as draft (artifact cd3a6b10..., evidence 8f4e2a91...)
```

The artifact hash is the canonical (sorted-keys pretty) serialization of the
compiled bytecode -- the same form evidence bundles and audit records hash, so
all three agree on artifact identity.

**2. Approve, by a second party.** The recorded author cannot approve their
own release:

```bash
devlish release approve pricing.tier@1.0.0 --approver andrew
# error: separation of duties: andrew authored pricing.tier@1.0.0 and cannot approve it

devlish release approve pricing.tier@1.0.0 --approver dana
# approved pricing.tier@1.0.0
```

**3. Publish.** Only approved releases publish, and effective windows
(declared in the rule's `Rule:` section) may not overlap another published
version of the same rule id:

```bash
devlish release publish pricing.tier@1.0.0
# published pricing.tier@1.0.0
```

**4. Run under governance.** `--governed` refuses any artifact whose hash is
not currently a published release. A tampered source file compiles to a
different hash and is refused; `--as-of` (effective-date resolution) checks
every candidate, so it resolves over published releases only:

```bash
devlish run pricing_tier.dvl --governed registry.json --input '{"amount": 150}'
devlish run v1.dvl v2.dvl --as-of 2026-07-01 --governed registry.json
```

**5. Retire and roll back.** Retiring removes an artifact from what may run.
Rollback is publishing a previously approved version again -- a new event,
the old artifact hash; nothing is deleted:

```bash
devlish release retire pricing.tier@2.0.0
devlish release publish pricing.tier@1.0.0   # rollback
```

**Inspect and verify at any time:**

```bash
devlish release list
# published  pricing.tier@1.0.0  artifact cd3a6b10...
# retired    pricing.tier@2.0.0  artifact 91d0f34c...
devlish release verify
# registry OK: 7 event(s), hash chain intact
```

## Notes

- All commands take `--registry <path>` (default `./registry.json`). Only
  `propose` creates a missing registry; every other verb errors on an absent
  file rather than fail-open "verifying" nothing.
- `--governed` is a property of the `run` command (and implicit runs). Other
  execution paths (MCP server, REPL) do not consult the registry yet.
- Author/approver names are unauthenticated labels compared case- and
  whitespace-insensitively; cryptographic identities arrive with DEVL-117.
- Writes are atomic (write-then-rename). The registry expects a single writer
  at a time -- the human cadence of release approval; coordinate through git.
- Proposing looks for `<rule>.cases.json` next to the rule (or `--cases`) and
  writes the evidence report to `<rule>.evidence.json` (or `--evidence-output`).
- A duplicate `rule@version` cannot be proposed; bump the rule's `version`.
