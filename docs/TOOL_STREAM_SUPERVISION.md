# Private tool streams and bounded collection

`ToolStreams` prepares controlled standard streams for a future Linux launcher.
It never spawns or authorizes a process. Production dispatch remains disabled;
the synthetic broker experiment now uses this collector after its explicit
second-execution denial.

The fixed signed profile limits stdin, stdout and stderr to 65,536 bytes each
and requests a 5,000 ms wall deadline. Stream construction starts that deadline;
the broker must check it before approving execution. A private local socket
carries stdin in one direction. Independent pipes carry stdout and stderr.
Only the parent endpoints are nonblocking; the child's installed descriptors
remain blocking. Original endpoints are moved above descriptor 2, so installing
stdio cannot clobber another source descriptor.

The parent drains both outputs while delivering input, rather than writing all
input before reading output. It handles partial operations and interrupted calls,
closes stdin when delivery completes, and checks child status and time while
polling, and checks the deadline again immediately before returning a capture.
A deterministic clock regression covers expiry during the final iteration. Input sends use MSG_NOSIGNAL to avoid changing global signal handling.
Delivery means bytes accepted by the local transport, not proof the program read
or used them.

On normal exit, complete stdout/stderr EOF and completed input delivery, the
collector returns a private capture with the exit code and raw bytes. Nonzero
exit codes are preserved. These bytes remain untrusted: the governing Devlish
policy must separately approve any disclosure, including model prompts, client
responses and logs. Captures intentionally have no automatic debug formatter.

On overrun, deadline, I/O failure or abnormal exit, no partial output is returned.
The exclusive child owner kills and reaps a still-live child on error or unwind.
Deadline expiry initiates termination when the supervisor next runs; scheduling
or an uninterruptible kernel task can delay cleanup. This is not a real-time
return guarantee or process-tree containment.

## Required launch ordering

1. Authorize the exact input and tool intent, then prepare streams before fork.
2. In the disposable child, install stdio, close every unlisted descriptor, and
   apply the other authenticated controls. Any setup failure requires `_exit`.
   Never unwind through Rust owners of replaced or closed child descriptors.
3. In the parent, drop the child's original endpoints with `into_parent`.
4. Complete broker setup, check the stream deadline before the one-time grant,
   and make every later execution fail closed.
5. Transfer exclusive ownership of the unreaped child to `collect`. No other
   thread or signal handler may reap it; the caller must retain permission to
   kill it. The child must be unable to fork or change credentials.
6. Record the outcome and apply disclosure policy before releasing any capture.

Abrupt supervisor process death, durable recovery, job-wide containment and
actual production journal integration remain separate work. The module's unsafe
child/collection APIs require these ownership and isolation preconditions; they
do not infer them from a caller's PID or booleans.

Linux tests force stdin backpressure while a child fills both output pipes before
reading input, exercise exact limits and nonzero exit, overrun either output,
refuse oversized input, reject early input closure, and terminate a blocked child
at the wall deadline. Failed collection is followed by an explicit reaping check.
macOS only tests unsupported-platform refusal. Cross-compilation checks types and
locked-libc compatibility; actual kernel behavior still requires Linux tests.

Sources: [Linux send flags](https://man7.org/linux/man-pages/man2/send.2.html) and
[poll semantics](https://man7.org/linux/man-pages/man2/poll.2.html).

The composed Linux broker fixture receives synthetic public input through controlled
stdin, echoes it to stdout, writes a fixed stderr diagnostic and exits with status
37. It applies the signed image, Landlock, descriptor, resource and syscall controls
before exec. The broker checks the stream deadline before granting execution and
transfers exclusive child ownership to collection. This is test composition, not
a production authority, durable journal or approved disclosure path.
