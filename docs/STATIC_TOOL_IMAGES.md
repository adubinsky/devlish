# Initial external-tool image profile

`StaticToolImage::from_snapshot(snapshot, profile)` consumes a
[sealed snapshot](SEALED_TOOL_SNAPSHOTS.md), inspects those same bytes, and retains
the same descriptor. It never reopens the source pathname. The only current
profile is `devlish-linux-static-x86_64-v1`, available for native Linux x86-64.
Other profiles/platforms fail closed. This is a format gate for a future launcher;
it does not enable external execution in verified CLI/HTTP admission.

Sealing an executable file is insufficient when execution also loads an
interpreter or dynamic libraries. The initial profile therefore excludes scripts,
dynamic executables, shared objects and static PIE. It accepts a restricted
ELF64 little-endian x86-64 `ET_EXEC` layout with no `PT_INTERP` or `PT_DYNAMIC`
header. It requires an explicit non-executable GNU stack declaration, a
file-backed entry in a readable/executable segment, and ascending, disjoint
4 KiB load pages with no writable/executable segment. This prevents approving
one segment as read/execute while a second load remaps its page writable.
The ELF structures are described in the System V ABI's
[ELF header](https://gabi.xinuos.com/elf/02-eheader.html) and
[program loading](https://gabi.xinuos.com/elf/07-pheader.html) references.

Parsing checks truncation, arithmetic overflow, header dimensions, known segment
types, file bounds, alignment and declared memory sizes. The profile limits the
header table to 128 entries and total declared load memory to 256 MiB. Loads must
stay within the initial 47-bit user address range and above the null page. These
are deliberately narrow profile restrictions; a valid executable outside them
is unsupported rather than implicitly trusted. The returned declared load size
is not a measurement of actual runtime memory consumption.

The typed image exposes the retained digest, entry address, declared load size,
profile and borrowed descriptor. It cannot be constructed from an unsealed file
or a caller-supplied inspection report. Inspection does not authenticate the
expected digest's authority: the caller still needs protected release/catalog
approval. [Signed catalog selections](SIGNED_TOOL_CATALOGS.md) bind those
commitments to a logical ID and exact argument vector; they still do not launch
the image or qualify containment.

## Verification and remaining work

```bash
cargo test --locked --manifest-path crates/devlish_core/Cargo.toml --lib tool_image
```

Portable parser tests exercise synthetic layouts, scripts, architecture and
loader substitutions, writable code/stack, entry bounds, overlapping pages,
misalignment, duplicate stack headers, truncation, oversized metadata and header
mutations. Linux x86-64 tests additionally verify descriptor retention after
source substitution. A test uses the host `cc` to link a tiny static assembly
fixture, then seals and inspects it without executing it. That compiler is a
test/build dependency, not a runtime tool adapter or approved release authority.
The hosted Linux job is required to validate these platform-specific tests.

A separate [composed qualification test](TOOL_LAUNCH_QUALIFICATION.md) executes
only a tiny synthetic fixture through a signed selection, sealed image,
filesystem restriction and descriptor cleanup. It does not enable production
dispatch or qualify the missing containment controls.

This is not a complete ELF validator, CPU compatibility test, proof of benign
machine code, or OS sandbox. A static program can still issue syscalls or load
additional data/code itself. Before enabling launch, bind a signed catalog to
the image and request, constrain filesystem/network/process effects, clear
inherited loader/personality state, restrict executable mappings and subsequent
exec, bound resource use and output, and qualify the actual kernel behavior.
The runtime's own loader/dependencies also remain in the trusted base. Do not
infer execution provenance or policy enforcement from this format inspection.
