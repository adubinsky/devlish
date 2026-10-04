# Minimal child syscall gate

`tool_syscalls::install` adds a Linux x86-64 seccomp filter to a disposable child.
It is a building block, not a launcher. Production external-tool dispatch remains
disabled. A separate synthetic composition tests a one-time broker experiment; there is
no production execution broker yet. See TOOL_LAUNCH_QUALIFICATION.md.

The fixed gate permits only:

- `read` on descriptor 0;
- `write` on descriptors 1 and 2;
- `close`, `exit` and `exit_group`;
- `sendmsg` on one trusted setup socket, solely to transfer the listener;
- `execveat` through a userspace notification listener, never unconditional allow.

Other native syscalls return EPERM, including sockets, process creation, pathname
access, descriptor duplication, new mappings, tracing, io_uring and limit changes.
The filter checks the syscall architecture and kills alternate ABI or x32 calls.
Descriptor comparisons check all 64 argument bits to reject truncation aliases.
These restrictions support tiny static programs, not arbitrary static libc tools.
A future broader profile needs its own review and qualification.

The caller must install controlled standard streams, eliminate inherited
capabilities, apply resource and filesystem restrictions, and normalize privileges
and signals before installation. The setup socket must be private, above stderr
and close-on-exec. The caller transfers the returned close-on-exec listener to the
supervisor, closes its local listener and setup socket, and only then attempts
execution. Candidate code must never possess the listener. Descriptor liveness is
checked here; socket provenance is the trusted launcher's responsibility.

Every exec request needs a separate broker decision. Closing the last listener
causes pending/future notification requests to fail rather than execute. The
planned broker must recognize trusted initial setup, consume its authorization
before continuing the first execution, and refuse every subsequent execution.
Checking an executable descriptor alone is insufficient: an absolute pathname
can ignore that descriptor. Pointer values likewise do not authenticate pointed-to
memory. No generic notification continuation for model-supplied code is authorized
by this helper.

Linux tests require real kernel installation, rejection of disallowed calls and
64-bit descriptor aliases, allowed standard-output/setup-channel paths, refusal
of exec without a broker, and fatal rejection of actual i386/x32 syscall attempts.
A bounded test supervisor fails and reaps stalled children. Unsupported platforms
have an explicit refusal test. Development tests do not attest production execution.

Sources: [kernel seccomp documentation](https://docs.kernel.org/userspace-api/seccomp_filter.html)
and [userspace notification semantics](https://man7.org/linux/man-pages/man2/seccomp_unotify.2.html).
