# First Linux filesystem restriction layer

`LandlockRuleset::deny_file_access()` creates an empty allowlist covering all
filesystem rights introduced through Linux Landlock ABI 3. It requires that ABI
to be enabled; there is no fallback to fewer rights. Creating the ruleset does
not restrict the caller. `restrict_current_thread()` sets `no_new_privs` and
applies the rules to that thread and its future descendants.

This native primitive is not connected to Devlish tool dispatch. No production
executor is changed by loading this module. A future launcher must apply it in
a fresh child before any candidate instruction runs. Restriction is irreversible;
the child must terminate if any setup step fails, including after `no_new_privs`
was set. Never apply it to a shared server worker or rely on it to restrict
other existing threads.

The handled rights deny new file read/write access, directory reads, filesystem
execution, removal, creation, cross-directory references and truncation. Newer
Landlock rights are not claimed by this fixed profile. It does not hide path
metadata, block `chmod`/`chown`/extended-attribute changes, prevent `O_PATH`
lookups, close existing descriptors, block network or
IPC, limit resources, constrain executable memory, or authenticate a child.
It is one restriction layer, not a complete sandbox.

The kernel's [Landlock documentation](https://docs.kernel.org/userspace-api/landlock.html)
describes rights inheritance and the distinction between previously opened
descriptors and new opens. Devlish's Linux test makes this distinction explicit:
the child cannot newly open, truncate, create or unlink the synthetic file, but
can still read the descriptor inherited from its parent. A launcher must close
unapproved descriptors separately. The test runner itself remains unrestricted;
the forked child performs only syscall operations and exits without running
Rust destructors or allocating after fork.

```bash
cargo test --locked --manifest-path crates/devlish_core/Cargo.toml --lib tool_landlock
```

On Linux, this test requires working ABI 3 and fails if it is missing; it does
not turn an unsupported kernel into a successful qualification. Other systems
test explicit refusal. Linux CI must pass before claiming this kernel behavior
was tested. These tests do not qualify external-tool execution or the full
deployment environment.

## Remaining launch gates

The initial empty allowlist intentionally cannot run a path-reading `grep`
workflow. An approved read-only input design requires a separate, reviewed
profile and tests. Do not widen this profile implicitly to make a tool work.
The Devlish policy must still govern arguments, input disclosure and output.

The next launcher must combine signed selection and image preparation with
[descriptor closure](TOOL_DESCRIPTOR_ISOLATION.md), an empty controlled environment, safe working directory,
network/IPC/process restrictions, resource/output limits, protected expiry
checks and terminal recording. It must account for the transition into the
first executable and prohibit later unapproved executable transitions.

An `execveat` filter that checks only the executable descriptor number and
`AT_EMPTY_PATH` is insufficient: an absolute pathname causes the kernel to
ignore that descriptor. See [execveat semantics](https://man7.org/linux/man-pages/man2/execveat.2.html).
Numeric descriptor reuse and inherited anonymous executable objects also need
explicit tests. The [seccomp documentation](https://docs.kernel.org/userspace-api/seccomp_filter.html)
explains that syscall filters cannot dereference pointer arguments and do not
constitute a sandbox by themselves. Do not claim a one-time approved execution
from a descriptor-number filter alone.
