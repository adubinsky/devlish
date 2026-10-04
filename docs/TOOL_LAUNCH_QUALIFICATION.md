# External-tool launch qualification

External execution remains disabled in verified CLI/HTTP admission. The library
contains building blocks, not a production launcher. This checklist keeps their
assurance separate from the complete launch boundary.

| Gate | Current implementation | Remaining requirement |
| --- | --- | --- |
| Operator-approved identity and arguments | Signed catalog selections and checked fixed containment declarations bound to private verified release state | Bind selection to the actual governed dispatch and current admission lock |
| Immutable executable bytes | Sealed Linux memfd, hash checked after sealing | Retain that descriptor through the complete production launch sequence |
| Restricted initial image | Static Linux x86-64 ELF profile | Qualify CPU/kernel assumptions and inherited loader/personality state |
| Filesystem data access | Empty Landlock ABI-3 allowlist primitive | Additional syscall restrictions; separate approved input profile if files are needed |
| Inherited capabilities | Close unlisted descriptors and install private bounded stdio in the synthetic composition | Authenticate every retained descriptor in production dispatch |
| Network, IPC, process creation and later execution | Minimal syscall gate (see TOOL_SYSCALL_GATE.md), not integrated | Qualify initial-exec broker and complete launch integration before enabling dispatch |
| Resource and output limits | Fixed resource ceilings plus private bounded stream collector (see TOOL_STREAM_SUPERVISION.md) | Integrate deadline checks and collection with the production broker, policy and journal |
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

## One-time execution broker experiment

`synthetic_broker_allows_initial_image_and_denies_absolute_path_reexecution`
adds the syscall gate to a second, test-only composition. The trusted fork child
blocks inherited signal handlers during setup, disables ordinary same-user
tracing, applies the filesystem/descriptor/resource restrictions, and transfers
its seccomp listener over a private socket. It closes that socket and its local
listener before requesting the initial descriptor-based execution.

The parent test verifies the notification ABI, child identity, syscall and exact
arguments of its known fork continuation. It consumes a local one-time grant
before continuing that initial execution. The executed assembly then attempts
`execveat(-1, "/proc/self/exe", ...)`. Its absolute pathname would ignore the
invalid descriptor; the second notification is denied with EPERM. The fixture
must observe that denial, echo synthetic public stdin to stdout, write a fixed
stderr diagnostic and exit with the expected status. The broker checks the shared
stream deadline before the first grant. After the second denial it closes its
listener and transfers exclusive child ownership to the bounded stream collector.
The test requires exact stdout, stderr, exit status and completed reaping; failures
withhold output and clean up the child.

This qualifies a kernel mechanism using known synthetic setup. The helper is
compiled only for Linux x86-64 tests and supplies no production broker API.
It does not authorize continuation of arbitrary candidate-supplied pointers.
The grant's safety depends on trusted pre-exec code and private, unchanged setup
memory, not on matching pointer numbers alone. Ordinary tracing protection may
reset on exec; it is not hostile-admin or runtime-memory attestation. Production
needs a broker bound to governed admission, protected identity, signed controls,
durable effect recording, bounded I/O and cancellation before dispatch can open.
