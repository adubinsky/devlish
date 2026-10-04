# External-tool launch qualification

External execution remains disabled in verified CLI/HTTP admission. The library
contains building blocks, not a production launcher. This checklist keeps their
assurance separate from the complete launch boundary.

| Gate | Current implementation | Remaining requirement |
| --- | --- | --- |
| Operator-approved identity and arguments | Signed catalog selections bound to private verified release state | Bind selection to the actual governed dispatch and current admission lock |
| Immutable executable bytes | Sealed Linux memfd, hash checked after sealing | Retain that descriptor through the complete production launch sequence |
| Restricted initial image | Static Linux x86-64 ELF profile | Qualify CPU/kernel assumptions and inherited loader/personality state |
| Filesystem data access | Empty Landlock ABI-3 allowlist primitive | Additional syscall restrictions; separate approved input profile if files are needed |
| Inherited capabilities | Close unlisted descriptors in a disposable child | Configure and bound standard I/O; authenticate every retained descriptor |
| Network, IPC, process creation and later execution | Not implemented | Enforce and adversarially test before enabling dispatch |
| Resource and output limits | Fixed child resource ceilings (see TOOL_RESOURCE_LIMITS.md) | Wall deadline, bounded output, process controls, termination and cleanup |
| Policy, disclosure and evidence | Declared tool effect, Devlish policy and replay tested with fake adapters | Connect real launcher, record identity/outcome commitments, release only approved output |

## Composed Linux test

`isolated_synthetic_image_executes_from_the_retained_descriptor_after_path_replacement`
links a tiny static assembly fixture that checks its argument count and exits
with status 37. An ephemeral synthetic release signs its catalog identity and
bytes. The test selects the exact approved arguments, prepares the sealed image,
then replaces the original pathname with non-executable data.

In a disposable fork child, trusted test setup closes standard I/O, applies the
filesystem restriction, closes all other descriptors except the checked image,
lowers child resource limits, and calls `execveat` on that retained descriptor with an empty environment. The
parent requires the expected exit status. No source pathname is reopened during
execution. The original inspection-only linker test remains separate.

This executes only the repository-authored synthetic test fixture. The host
linker is a test/build dependency. It does not run arbitrary model-selected
programs, use production credentials, contact a provider, or enable dispatch.
The test proves these primitives compose on the tested Linux kernel, including
the exec transition; it does not qualify a hostile arbitrary executable. The
fixture has no file/network I/O, and syscall filtering, output bounds or wall deadlines cannot be inferred from
its successful exit.

```bash
cargo test --locked --manifest-path crates/devlish_core/Cargo.toml --lib \
  isolated_synthetic_image_executes
```

This test exists only on Linux x86-64. A command reporting zero tests on another
platform is not qualification. Inspect the hosted Linux job, exact revision and
named test result. A passing test is development evidence, not signed runtime
attestation; audit reports continue to leave execution-origin and actual policy
enforcement assurance false.
