# Child resource ceilings

`tool_limits::restrict_child` is a native Linux building block for a future
launcher. It is not connected to production tool dispatch. Devlish must still
authorize an effect before the launcher applies its operating-system controls.

The initial fixed profile requests one second of CPU, 512 MiB of address space,
8 MiB of stack, at most 64 descriptor numbers, zero file growth, zero core-file
size and zero locked-memory allowance. Each soft and hard limit becomes the
minimum of that ceiling and both inherited limits. Stricter operator limits are
preserved. The launcher cannot request arbitrary model-supplied limits.

Call only in a disposable single-threaded child after preparing its descriptors
and immediately before execution. The helper reads all limits first, lowers them
without allocating, and verifies readback. Any failure requires `_exit`: earlier
limits may already have changed. Unsupported platforms refuse without fallback.
Existing mappings and open descriptors are not retroactively removed; descriptor
cleanup and the exec transition remain separate requirements.

These are resource ceilings, not complete containment. CPU time is not wall time;
a sleeping or blocked process still needs an independent supervisor. File-size
limits do not bound pipe output, and address space is not a resident-memory or
aggregate process-tree budget. Process creation needs a separate restriction.
Privileges capable of raising limits must be removed before executing a tool.
Zero core-file size alone does not prevent a host-configured piped core collector
from receiving memory; dumpability and host core policy need separate treatment.

Linux tests fork disposable children, check stricter inherited limits and parent
isolation, require oversized anonymous mappings and file growth to fail, and
require a CPU-bound child to die from the kernel CPU ceiling. A separate test
supervisor kills and reaps a stalled test after 20 seconds, then fails the test;
that cleanup is never counted as successful CPU enforcement. The composed signed
image test also applies these limits before executing its tiny static fixture.
macOS tests cover refusal only. Require the named positive tests in Linux CI.

```bash
cargo test --locked --manifest-path crates/devlish_core/Cargo.toml --lib tool_limits
```

Kernel semantics: [Linux getrlimit/setrlimit documentation](https://man7.org/linux/man-pages/man2/getrlimit.2.html).
Passing tests are development evidence, not independent execution attestation.
