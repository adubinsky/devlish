# Sealed tool byte snapshots on Linux

`devlish_core::tool_snapshot::SealedToolSnapshot::from_path(path, expected_sha256)`
is a native primitive for DEVL-220. It retains exact checked bytes in a sealed
Linux memory-backed file. It does not execute anything, validate executable
format, authenticate a catalog or enable the `run_tool` effect in verified
CLI/HTTP admission. Other platforms return an explicit unsupported error with
no filesystem fallback.

The expected lowercase SHA-256 must come from independently protected operator
configuration. The input must be a normal absolute path of at most 4,096 bytes,
with no empty, dot or parent components. Each component is opened relative to
its retained parent descriptor with symlink following disabled. This rejects
symlinks in ancestor directories as well as the final component. The opened
source must be a regular file of at most 64 MiB; nonblocking opens avoid waiting
for a FIFO writer, and a bounded copy also rejects files that grow beyond the
limit during reading.

The implementation copies into a private `memfd`, applies and reads back the
write, growth, shrinkage and sealing seals, then hashes the sealed copy itself.
No descriptor escapes before those checks succeed. A matching snapshot exposes
its digest, size and a borrowed descriptor. Descriptors are close-on-exec when
constructed. Replacing, editing or unlinking the source pathname cannot alter
the retained bytes. Kernel seals apply to every descriptor alias and prevent
shared writable mappings; private copy-on-write mappings do not modify the
underlying object. See the Linux man-pages for
[memfd creation](https://man7.org/linux/man-pages/man2/memfd_create.2.html) and
[file sealing](https://man7.org/linux/man-pages/man2/F_ADD_SEALS.2const.html).

This differs from `artifact verify`, which is an inspection command. A future
launcher must consume the retained object, without reopening the original path,
and still verify its approved executable format/platform, catalog identity,
arguments, dependency/loader identities and containment. Merely hashing a file
and later executing its pathname leaves a replacement gap. Holding a descriptor
to an ordinary mutable file alone also leaves an in-place modification gap.

## Tests and limits

```bash
cargo test --locked --manifest-path crates/devlish_core/Cargo.toml tool_snapshot
```

Linux tests use synthetic non-executable bytes. They check source modification,
replacement and removal, writes/resizing through descriptor aliases, shared
writable mappings, changed digests, symlinks at multiple depths, malformed paths,
FIFOs, directories and oversized sparse files. On other platforms the test checks
explicit refusal. A macOS pass does not establish that the Linux tests ran;
inspect the Linux CI job and revision for that result.

This is byte immutability under the running kernel, not execution provenance,
OS containment or hostile-root resistance. The library assumes a trusted kernel
and an uncompromised calling process. It does not enforce ownership of source
files, verify signatures, validate the supplied digest's authority, reject hard
links, pin mount identities, impose a hard deadline on a hostile filesystem, or
check executable permissions. Those are separate launcher/deployment decisions.
No claim that an authorized external program executed may be derived from this
primitive or its test results.

The separate [static tool image gate](STATIC_TOOL_IMAGES.md) can now inspect the
same sealed descriptor for the initial Linux x86-64 profile. Neither layer launches
the image or establishes the remaining catalog and containment controls.
