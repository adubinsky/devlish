//! Descriptor-table cleanup for a future isolated child launcher.
//! This does not authorize retained descriptors or configure standard I/O.

/// Immediately close every descriptor above stderr except an explicit, sorted
/// list of at most eight live close-on-exec setup descriptors. No allocation,
/// filesystem enumeration, maximum-fd guess or fallback loop is used.
///
/// # Safety
/// Call only in a fresh single-threaded fork child with a private descriptor
/// table. The caller must never return to code which owns closed descriptors,
/// unwind, run their Rust destructors, or open replacement descriptors before
/// those owners are abandoned. After any error the child must `_exit`; after
/// success it must finish allocation-free setup and exec or `_exit`.
/// The caller must first install controlled stdin/stdout/stderr and select
/// retained descriptors from trusted launcher state, never model arguments.
pub unsafe fn close_unlisted(keep: &[i32]) -> Result<(), &'static str> {
    #[cfg(target_os = "linux")]
    {
        if keep.len() > 8 {
            return Err("too many retained tool setup descriptors");
        }
        let mut previous = 2;
        // Validate every entry before closing anything. Keep descriptors must
        // disappear at exec, including the checked executable and error pipe.
        for &fd in keep {
            if fd <= previous {
                return Err("retained descriptors must be unique, ascending and above stderr");
            }
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
            if flags < 0 || flags & libc::FD_CLOEXEC == 0 {
                return Err("retained descriptor must be live and close-on-exec");
            }
            previous = fd;
        }
        let close = |first: u32, last: u32| {
            // SAFETY: caller owns this disposable child descriptor table. Range
            // is inclusive; holes are allowed. No pointer arguments are passed.
            if unsafe { libc::syscall(libc::SYS_close_range, first, last, 0u32) } != 0 {
                Err("tool descriptor cleanup failed or is unavailable")
            } else {
                Ok(())
            }
        };
        let mut first = 3u32;
        for &fd in keep {
            let fd = fd as u32; // validated positive i32, so fd + 1 cannot overflow
            if first < fd {
                close(first, fd - 1)?;
            }
            first = fd + 1;
        }
        close(first, u32::MAX)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = keep;
        Err("tool descriptor cleanup is unavailable on this platform")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn unsupported_platform_returns_without_closing_anything() {
        // Unsupported implementation has no side effects.
        assert!(unsafe { close_unlisted(&[]) }
            .unwrap_err()
            .contains("unavailable"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn child_keeps_only_explicit_setup_fds_and_invalid_lists_do_not_partially_close() {
        use std::{
            fs,
            os::fd::AsRawFd,
            time::{SystemTime, UNIX_EPOCH},
        };
        let path = std::env::temp_dir().join(format!(
            "devlish-descriptors-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, b"synthetic descriptor fixture").unwrap();
        let before = fs::File::open(&path).unwrap();
        let retained = fs::File::open(&path).unwrap();
        let after = fs::File::open(&path).unwrap();
        let retained_fd = retained.as_raw_fd();
        // Plain pipe deliberately lacks CLOEXEC; it must never be retained.
        let mut pipe = [0; 2];
        assert_eq!(unsafe { libc::pipe(pipe.as_mut_ptr()) }, 0);
        for keep_one in [false, true] {
            let child = unsafe { libc::fork() };
            assert!(child >= 0);
            if child == 0 {
                // No allocation, stdio, panics/unwinding or Rust destructors.
                unsafe {
                    for invalid in [
                        &[2][..],
                        &[retained_fd, retained_fd],
                        &[pipe[0]],
                        &[retained_fd, -1],
                        &[i32::MAX],
                    ] {
                        if close_unlisted(invalid).is_ok() {
                            libc::_exit(10);
                        }
                    }
                    if libc::fcntl(before.as_raw_fd(), libc::F_GETFD) < 0
                        || libc::fcntl(after.as_raw_fd(), libc::F_GETFD) < 0
                    {
                        libc::_exit(11);
                    }
                    let kept = [retained_fd];
                    if close_unlisted(if keep_one { &kept } else { &[] }).is_err() {
                        libc::_exit(12);
                    }
                    for fd in [before.as_raw_fd(), after.as_raw_fd(), pipe[0], pipe[1]] {
                        if libc::fcntl(fd, libc::F_GETFD) != -1
                            || *libc::__errno_location() != libc::EBADF
                        {
                            libc::_exit(13);
                        }
                    }
                    if keep_one {
                        if libc::fcntl(retained_fd, libc::F_GETFD) & libc::FD_CLOEXEC == 0 {
                            libc::_exit(14);
                        }
                        let mut byte = 0u8;
                        if libc::read(retained_fd, (&mut byte as *mut u8).cast(), 1) != 1
                            || byte != b's'
                        {
                            libc::_exit(15);
                        }
                    } else if libc::fcntl(retained_fd, libc::F_GETFD) != -1
                        || *libc::__errno_location() != libc::EBADF
                    {
                        libc::_exit(16);
                    }
                    libc::_exit(0);
                }
            }
            let mut status = 0;
            loop {
                let waited = unsafe { libc::waitpid(child, &mut status, 0) };
                if waited == child {
                    break;
                }
                assert_eq!(
                    std::io::Error::last_os_error().raw_os_error(),
                    Some(libc::EINTR)
                );
            }
            assert!(
                libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
                "child status {status}"
            );
        }
        for fd in [
            before.as_raw_fd(),
            retained_fd,
            after.as_raw_fd(),
            pipe[0],
            pipe[1],
        ] {
            assert!(
                unsafe { libc::fcntl(fd, libc::F_GETFD) } >= 0,
                "parent descriptor was closed"
            );
        }
        unsafe {
            libc::close(pipe[0]);
            libc::close(pipe[1]);
        }
        drop((before, retained, after));
        fs::remove_file(path).unwrap();
    }
}
