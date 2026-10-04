//! Fixed resource ceilings for a future disposable Linux tool child.
//! Not a wall-clock deadline, output bound, process sandbox or launch authority.

/// Lower soft AND hard resource limits, preserving any stricter inherited limit.
/// No allocation occurs. Unsupported platforms fail without changing limits.
///
/// # Safety
/// Call only in a disposable, single-threaded child, before untrusted code runs.
/// Limits affect the entire process and may be partially applied on failure.
/// After failure the child must `_exit`; after success, exec or `_exit` without
/// returning to the parent runtime or unwinding its Rust owners. The launcher
/// must separately eliminate privileges that could raise hard limits and apply
/// syscall restrictions, output bounds and an external wall-clock supervisor.
pub unsafe fn restrict_child() -> Result<(), &'static str> {
    #[cfg(target_os = "linux")]
    {
        use devlish_audit::tool_containment as profile;
        let ceilings = [
            (libc::RLIMIT_CPU, profile::CPU_SECONDS),
            (libc::RLIMIT_AS, profile::ADDRESS_SPACE_BYTES),
            (libc::RLIMIT_STACK, profile::STACK_BYTES),
            (libc::RLIMIT_CORE, 0),
            (libc::RLIMIT_FSIZE, 0),
            (libc::RLIMIT_NOFILE, profile::MAX_DESCRIPTORS),
            (libc::RLIMIT_MEMLOCK, 0),
        ];
        let mut limits = [libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        }; 7];
        // Read all current limits before mutation. Never relax an inherited
        // soft limit, including a zero limit, or attempt to raise a hard limit.
        for ((resource, ceiling), limit) in ceilings.iter().zip(limits.iter_mut()) {
            if unsafe { libc::getrlimit(*resource, limit) } != 0 {
                return Err("cannot inspect inherited tool resource limit");
            }
            let bound = (*ceiling as libc::rlim_t).min(limit.rlim_cur).min(limit.rlim_max);
            limit.rlim_cur = bound;
            limit.rlim_max = bound;
        }
        for ((resource, _), limit) in ceilings.iter().zip(limits.iter()) {
            if unsafe { libc::setrlimit(*resource, limit) } != 0 {
                return Err("cannot lower tool resource limit");
            }
            let mut actual = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            if unsafe { libc::getrlimit(*resource, &mut actual) } != 0
                || actual.rlim_cur != limit.rlim_cur
                || actual.rlim_max != limit.rlim_max
            {
                return Err("tool resource limit readback differs");
            }
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    Err("tool resource limits are unavailable on this platform")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn unsupported_platform_refuses_without_mutation() {
        assert!(unsafe { restrict_child() }
            .unwrap_err()
            .contains("unavailable"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn child_limits_preserve_stricter_bounds_and_reject_large_mappings() {
        // Arrange in the parent; no allocation or unwinding after fork.
        let mut parent = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        assert_eq!(
            unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut parent) },
            0
        );
        // Open before lowering NOFILE: unrelated parallel tests can occupy
        // every low-numbered descriptor inherited by this child.
        let file = unsafe {
            libc::syscall(
                libc::SYS_memfd_create,
                c"limit-test".as_ptr(),
                libc::MFD_CLOEXEC,
            )
        };
        assert!(file >= 0);
        let child = unsafe { libc::fork() };
        assert!(child >= 0);
        if child == 0 {
            unsafe {
                let tighter = libc::rlimit {
                    rlim_cur: 16.min(parent.rlim_cur),
                    rlim_max: parent.rlim_max,
                };
                if libc::setrlimit(libc::RLIMIT_NOFILE, &tighter) != 0 || restrict_child().is_err()
                {
                    libc::_exit(10);
                }
                let mut actual = libc::rlimit {
                    rlim_cur: 0,
                    rlim_max: 0,
                };
                if libc::getrlimit(libc::RLIMIT_NOFILE, &mut actual) != 0
                    || actual.rlim_cur != tighter.rlim_cur
                    || actual.rlim_max != tighter.rlim_cur
                {
                    libc::_exit(11);
                }
                for resource in [libc::RLIMIT_CORE, libc::RLIMIT_FSIZE, libc::RLIMIT_MEMLOCK] {
                    if libc::getrlimit(resource, &mut actual) != 0
                        || actual.rlim_cur != 0
                        || actual.rlim_max != 0
                    {
                        libc::_exit(12);
                    }
                }
                // Block the file-size signal so the failed operation can be
                // inspected without a signal handler or Rust unwinding.
                let mut blocked = std::mem::zeroed::<libc::sigset_t>();
                if libc::sigemptyset(&mut blocked) != 0
                    || libc::sigaddset(&mut blocked, libc::SIGXFSZ) != 0
                    || libc::sigprocmask(libc::SIG_BLOCK, &blocked, std::ptr::null_mut()) != 0
                {
                    libc::_exit(14);
                }
                if file < 0
                    || libc::ftruncate(file as i32, 1) != -1
                    || *libc::__errno_location() != libc::EFBIG
                {
                    libc::_exit(15);
                }
                libc::close(file as i32);
                let mapped = libc::mmap(
                    std::ptr::null_mut(),
                    1024 * 1024 * 1024,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                    -1,
                    0,
                );
                if mapped != libc::MAP_FAILED || *libc::__errno_location() != libc::ENOMEM {
                    libc::_exit(13);
                }
                libc::_exit(0);
            }
        }
        let status = wait_bounded(child);
        unsafe {
            libc::close(file as i32);
        }
        assert!(
            libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
            "child status {status}"
        );
        let mut after = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        assert_eq!(
            unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut after) },
            0
        );
        assert_eq!(
            (parent.rlim_cur, parent.rlim_max),
            (after.rlim_cur, after.rlim_max)
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn cpu_hard_limit_terminates_a_spinning_child() {
        let child = unsafe { libc::fork() };
        assert!(child >= 0);
        if child == 0 {
            unsafe {
                if restrict_child().is_err() {
                    libc::_exit(10);
                }
                loop {
                    std::hint::spin_loop();
                }
            }
        }
        let status = wait_bounded(child);
        assert!(libc::WIFSIGNALED(status), "child status {status}");
        assert_eq!(libc::WTERMSIG(status), libc::SIGKILL);
    }

    #[cfg(target_os = "linux")]
    fn wait_bounded(child: libc::pid_t) -> i32 {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut status = 0;
        loop {
            let result = unsafe { libc::waitpid(child, &mut status, libc::WNOHANG) };
            if result == child {
                return status;
            }
            if result == -1 {
                assert_eq!(
                    std::io::Error::last_os_error().raw_os_error(),
                    Some(libc::EINTR)
                );
            }
            if std::time::Instant::now() >= deadline {
                // Test cleanup is not evidence of CPU-limit enforcement.
                unsafe {
                    libc::kill(child, libc::SIGKILL);
                }
                while unsafe { libc::waitpid(child, &mut status, 0) } == -1
                    && std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR)
                {
                }
                panic!("child exceeded test supervisor deadline");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}
