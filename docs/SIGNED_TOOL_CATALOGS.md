# Signed external-tool catalog selections

An executable hash only identifies bytes. The independent audit library now
binds a logical tool selection to a catalog covered by an approved release:
`ReleaseVerification::tool_catalog(id, exact_bytes)` checks the artifact ID,
`tool-catalog` role and exact digest using private verified state. A forged or
edited report cannot substitute those values. Catalog metadata is limited to
64 KiB; unknown fields and duplicate JSON fields fail parsing.

This is a library primitive, not a new command or launch capability. The current
verified CLI/HTTP path still rejects external execution. Existing host-effect
catalogs use `devlish-tool-catalog`; this separate artifact uses
`devlish-external-tool-catalog` and can be included in the same release under a
distinct artifact ID. The operator must select that ID; do not let model input
choose an alternative signed catalog.

## Example

The following synthetic catalog permits one exact argument vector. Its bytes,
the tool artifact and containment artifact must all be present in the signed
release. A normal dynamically linked system `grep` will not satisfy this image
profile; use a separately approved static executable when qualifying a launcher.

```json
{
  "format": "devlish-external-tool-catalog",
  "format_version": 1,
  "target": "x86_64-unknown-linux-gnu",
  "tools": [{
    "id": "public-grep",
    "artifact_id": "static-grep",
    "path": "/opt/devlish/tools/grep",
    "image_profile": "devlish-linux-static-x86_64-v1",
    "containment_id": "tool-isolation",
    "allowed_arguments": [["--fixed-strings", "--", "published", "/work/public.txt"]]
  }]
}
```

The target must match the release and the initial supported target exactly.
Each tool refers to a release artifact with role `tool` and a containment
artifact with role `containment`. Digests come from the verified manifest,
never from the request. Paths must be absolute, without NULs, empty components,
`.` or `..`; filesystem symlink checks happen later in the snapshot layer.
Tool IDs are case-sensitive ASCII letters, digits, hyphens and underscores,
1–128 bytes. At most 64 unique IDs and 32 unique argument alternatives per ID
are allowed. Each argument vector has at most 64 strings, each at most 4096 UTF-8
bytes, totaling at most 16384 bytes, with no NULs. An explicitly listed empty
vector permits a no-argument call; an empty alternatives list permits nothing
and is rejected as invalid configuration.

`select(id, arguments, now)` requires an exact argument match and a supplied
time at or after verified admission, strictly before the earliest release,
revocation or required-builder deadline. A selection retains the catalog's
argument vector, not mutable request memory. It exposes immutable tool,
containment, catalog and manifest commitments for later evidence recording.
Public presentation fields in a release report cannot change its authority,
target, baseline time or deadline.

## Preparing the selected image

The native core's `PreparedCatalogTool::load(selection)` consumes a selection,
copies and seals the file at its catalog path, verifies its catalog digest, then
applies the selected static-image profile to that same descriptor. It retains
both the immutable selection and checked image together. The caller cannot
provide an alternate path, digest, image or argument vector to this constructor.
Replacing the source pathname after preparation cannot change the sealed image.
Even a signed script or dynamic executable fails the format gate.

Preparation reads a local file but does not execute it. On unsupported platforms
it returns an error rather than falling back to ordinary process spawning.
Tests create ephemeral signed releases, replace a tool before and after loading,
mutate the original request arguments, and verify the retained selection and
descriptor. Linux CI checks the positive path; macOS checks refusal.

## Policy and assurance boundary

The catalog is an operator ceiling, not the decision-making agent. Devlish still
validates the plan and independently checks policy, declared exact-ID permission,
release permissions and effect budgets. For example, the NPPI/IP cases in
[the catalog-tool example](../examples/catalog_tool/README.md) deny a request to
read `/work/nppi.csv` or `/work/company-secret.txt`; those argument vectors are
also absent from this catalog. Do not widen the catalog merely because a model
requests another argument.

Catalog selection itself performs no filesystem access, execution or containment
qualification. It authenticates the containment artifact digest without
interpreting or enforcing its contents. A future launcher must consume the
[sealed snapshot](SEALED_TOOL_SNAPSHOTS.md) and
[checked static image](STATIC_TOOL_IMAGES.md), maintain durable admission and a
protected clock, recheck expiry at dispatch, enforce the actual containment
profile, close inherited descriptors, bound resources and gate output through
Devlish disclosure policy. An already returned selection is not automatically
revoked as time passes. Signed catalog membership does not prove that a tool
ran, that its code is benign, or that policy was enforced.

Tests use ephemeral synthetic signing keys and cover replaced catalogs, wrong
artifact roles, target/profile/path substitution, NPPI/IP argument changes,
duplicate entries, request bounds, expiry, and modified presentation fields:

```bash
cargo test --locked --manifest-path crates/devlish_audit/Cargo.toml --test releases catalog
```
