# Closing inherited tool descriptors

Filesystem restrictions do not revoke access through descriptors a child
inherits. An executor may have open credential files, sockets or journals; a
candidate tool must not inherit those capabilities.

The native Linux `tool_descriptors::close_unlisted(keep)` primitive closes every
descriptor above stderr except an explicit sorted list of at most eight live
setup descriptors. Each retained descriptor must already be close-on-exec.
Validation happens before any closure. The implementation uses inclusive
[`close_range`](https://man7.org/linux/man-pages/man2/close_range.2.html) ranges
through the maximum descriptor number, so sparse or high descriptors are not
missed by a guessed loop bound. Unsupported systems or syscall failures return
an error; there is no filesystem-enumeration fallback.

This is an unsafe child-setup API. The caller must use it only in a fresh,
single-threaded fork child with a private descriptor table. It must then exec
or `_exit`, never return to Rust code that owns the closed descriptors, unwind
or run their destructors. On any error, some ranges may already be closed;
the child must terminate. No production code currently calls this primitive,
and it does not enable tool dispatch.

The caller must first replace stdin/stdout/stderr with controlled endpoints.
Those descriptors are deliberately untouched. The retained list must come from
trusted launcher state, such as the sealed executable descriptor and a bounded
setup-error pipe; the model must never choose it. Retaining a descriptor does
not authenticate its type, contents or authority. Before entering candidate
code, successful exec must close these setup descriptors. Environment, working
directory, network, subsequent opens, resource limits, disclosure and the initial
exec transition still require separate controls.

Linux tests run closure only in forked children. They reject invalid retained
lists without partial closure, close synthetic file and pipe descriptors, keep
only the requested close-on-exec descriptor, handle an empty retained list and
confirm parent descriptors remain open. Other systems test explicit refusal.
The tests do not launch any candidate program or establish a complete sandbox.

```bash
cargo test --locked --manifest-path crates/devlish_core/Cargo.toml --lib tool_descriptors
```

Combine this layer with the [filesystem restrictions](TOOL_FILESYSTEM_RESTRICTIONS.md)
only after qualifying the complete launcher sequence. Neither layer alone
proves that policy governed an executed program.
