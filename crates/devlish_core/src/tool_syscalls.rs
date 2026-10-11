//! Minimal Linux x86-64 syscall gate. Execution ALWAYS needs a separate broker.
//! Installation alone does not authorize execution or constitute a full sandbox.

#[derive(Debug)]
pub struct ExecListener {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fd: std::os::fd::OwnedFd,
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
impl std::os::fd::AsFd for ExecListener {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

/// Install a fixed filter and return its close-on-exec notification listener.
/// Only exit, close, stdin read, stdout/stderr write, setup-channel sendmsg and
/// broker-mediated execveat are recognized. All other native syscalls fail EPERM;
/// alternate architectures and x32 calls kill the process. No allocation occurs.
///
/// # Safety
/// Use only in a disposable, single-threaded child after all other OS setup.
/// `transfer_fd` must be a trusted private CLOEXEC socket above stderr, used only
/// to pass this listener to a protected supervisor and then closed before exec.
/// Controlled standard I/O, descriptor cleanup, privileges, signal state and
/// limits are separate prerequisites. On error immediately `_exit`; on success
/// never unwind or return to the parent runtime. The candidate must NEVER own
/// the listener. A broker must validate trusted initial setup and consume its
/// one-time authorization before continuing an exec; no such broker is provided
/// here. Merely matching a descriptor or pointer value is not authorization.
pub unsafe fn install(transfer_fd: i32) -> Result<ExecListener, &'static str> {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        use std::os::fd::FromRawFd;
        if transfer_fd <= 2 {
            return Err("invalid syscall-filter transfer descriptor");
        }
        let flags = unsafe { libc::fcntl(transfer_fd, libc::F_GETFD) };
        if flags < 0 || flags & libc::FD_CLOEXEC == 0 {
            return Err("syscall-filter transfer descriptor must be live and close-on-exec");
        }
        let mut code = filter(transfer_fd as u32);
        let program = libc::sock_fprog {
            len: code.len() as u16,
            filter: code.as_mut_ptr(),
        };
        if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
            return Err("cannot prevent privilege gain before syscall restriction");
        }
        let fd = unsafe {
            libc::syscall(
                libc::SYS_seccomp,
                libc::SECCOMP_SET_MODE_FILTER,
                libc::SECCOMP_FILTER_FLAG_NEW_LISTENER,
                &program,
            )
        };
        if fd < 0 {
            return Err("syscall notification filter is unavailable");
        }
        // NEW_LISTENER creates a CLOEXEC descriptor. No fcntl is allowed after
        // installation. The caller transfers it before any candidate execution.
        Ok(ExecListener {
            fd: unsafe { std::os::fd::OwnedFd::from_raw_fd(fd as i32) },
        })
    }
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    {
        let _ = transfer_fd;
        Err("tool syscall filter is unavailable on this platform")
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn filter(transfer_fd: u32) -> [libc::sock_filter; 40] {
    const fn stmt(code: u16, k: u32) -> libc::sock_filter {
        libc::sock_filter {
            code,
            jt: 0,
            jf: 0,
            k,
        }
    }
    const fn eq(k: u32, jt: u8, jf: u8) -> libc::sock_filter {
        libc::sock_filter {
            code: 0x15,
            jt,
            jf,
            k,
        }
    }
    const LOAD: u16 = 0x20; // BPF_LD | BPF_W | BPF_ABS
    const RET: u16 = 0x06; // BPF_RET | BPF_K
    let allow = stmt(RET, libc::SECCOMP_RET_ALLOW);
    let deny = stmt(RET, libc::SECCOMP_RET_ERRNO | libc::EPERM as u32);
    let kill = stmt(RET, libc::SECCOMP_RET_KILL_PROCESS);
    [
        stmt(LOAD, 4),
        eq(0xc000003e, 1, 0),
        kill, // AUDIT_ARCH_X86_64
        stmt(LOAD, 0),
        libc::sock_filter {
            code: 0x35,
            jt: 0,
            jf: 1,
            k: 0x40000000,
        },
        kill,
        eq(libc::SYS_exit as u32, 0, 1),
        allow,
        eq(libc::SYS_exit_group as u32, 0, 1),
        allow,
        eq(libc::SYS_close as u32, 0, 1),
        allow,
        eq(libc::SYS_execveat as u32, 0, 1),
        stmt(RET, libc::SECCOMP_RET_USER_NOTIF),
        // read: all 64 bits must describe fd 0 (do not accept truncation aliases).
        eq(libc::SYS_read as u32, 0, 7),
        stmt(LOAD, 20),
        eq(0, 1, 0),
        deny,
        stmt(LOAD, 16),
        eq(0, 0, 1),
        allow,
        deny,
        // write: exactly stdout or stderr.
        eq(libc::SYS_write as u32, 0, 8),
        stmt(LOAD, 20),
        eq(0, 1, 0),
        deny,
        stmt(LOAD, 16),
        eq(1, 1, 0),
        eq(2, 0, 1),
        allow,
        deny,
        // sendmsg: only the private setup channel, which must close before exec.
        eq(libc::SYS_sendmsg as u32, 0, 7),
        stmt(LOAD, 20),
        eq(0, 1, 0),
        deny,
        stmt(LOAD, 16),
        eq(transfer_fd, 0, 1),
        allow,
        deny,
        deny,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    #[test]
    fn unsupported_platform_has_no_permissive_fallback() {
        assert!(unsafe { install(3) }.unwrap_err().contains("unavailable"));
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn kernel_denies_unlisted_syscalls_and_exec_without_a_broker() {
        use std::os::fd::{AsFd, AsRawFd};
        let mut sockets = [-1; 2];
        assert_eq!(
            unsafe {
                libc::socketpair(
                    libc::AF_UNIX,
                    libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC,
                    0,
                    sockets.as_mut_ptr(),
                )
            },
            0
        );
        let child = unsafe { libc::fork() };
        assert!(child >= 0);
        if child == 0 {
            unsafe {
                if install(2).is_ok() {
                    libc::_exit(10);
                }
                let listener = match install(sockets[0]) {
                    Ok(v) => v,
                    Err(_) => libc::_exit(11),
                };
                // No broker exists. Closing the sole listener must make exec
                // notification fail ENOSYS before argument validation/execution.
                libc::close(listener.as_fd().as_raw_fd());
                for number in [
                    libc::SYS_socket,
                    libc::SYS_clone,
                    libc::SYS_fork,
                    libc::SYS_execve,
                    libc::SYS_openat,
                    libc::SYS_mmap,
                    libc::SYS_ptrace,
                    libc::SYS_process_vm_writev,
                    libc::SYS_io_uring_setup,
                    libc::SYS_memfd_create,
                    libc::SYS_dup,
                    libc::SYS_prlimit64,
                    libc::SYS_seccomp,
                    libc::SYS_sendmsg,
                ] {
                    if libc::syscall(number, -1i64, 0i64, 0i64, 0i64, 0i64, 0i64) != -1
                        || *libc::__errno_location() != libc::EPERM
                    {
                        libc::_exit(12);
                    }
                }
                for (number, fd) in [
                    (libc::SYS_read, 1u64),
                    (libc::SYS_write, 0),
                    (libc::SYS_write, (1u64 << 32) | 1),
                    (libc::SYS_sendmsg, (1u64 << 32) | sockets[0] as u64),
                ] {
                    if libc::syscall(number, fd, std::ptr::null::<u8>(), 0) != -1
                        || *libc::__errno_location() != libc::EPERM
                    {
                        libc::_exit(13);
                    }
                }
                if libc::syscall(
                    libc::SYS_execveat,
                    -1,
                    c"".as_ptr(),
                    std::ptr::null::<u8>(),
                    std::ptr::null::<u8>(),
                    libc::AT_EMPTY_PATH,
                ) != -1
                    || *libc::__errno_location() != libc::ENOSYS
                {
                    libc::_exit(14);
                }
                if libc::syscall(libc::SYS_sendmsg, sockets[0], std::ptr::null::<u8>(), 0) != -1
                    || *libc::__errno_location() != libc::EFAULT
                {
                    libc::_exit(16);
                }
                // Zero-length writes must reach the kernel rather than EPERM.
                if libc::syscall(libc::SYS_write, 1u64, c"".as_ptr(), 0) != 0 {
                    libc::_exit(15);
                }
                libc::_exit(0);
            }
        }
        let status = wait_bounded(child);
        unsafe {
            libc::close(sockets[0]);
            libc::close(sockets[1]);
        }
        assert!(
            libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
            "child status {status}"
        );
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn alternate_syscall_abis_are_killed() {
        for x32 in [false, true] {
            let mut sockets = [-1; 2];
            assert_eq!(
                unsafe {
                    libc::socketpair(
                        libc::AF_UNIX,
                        libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC,
                        0,
                        sockets.as_mut_ptr(),
                    )
                },
                0
            );
            let child = unsafe { libc::fork() };
            assert!(child >= 0);
            if child == 0 {
                unsafe {
                    let zero = libc::rlimit {
                        rlim_cur: 0,
                        rlim_max: 0,
                    };
                    if libc::setrlimit(libc::RLIMIT_CORE, &zero) != 0 {
                        libc::_exit(10);
                    }
                    let _listener = match install(sockets[0]) {
                        Ok(v) => v,
                        Err(_) => libc::_exit(11),
                    };
                    if x32 {
                        libc::syscall(0x40000000 | libc::SYS_getpid);
                    } else {
                        // Legacy i386 getpid must fail the architecture gate,
                        // even though this process itself is an x86-64 binary.
                        std::arch::asm!("int 0x80", inlateout("eax") 20u32 => _, clobber_abi("C"), options(nostack));
                    }
                    libc::_exit(12);
                }
            }
            let status = wait_bounded(child);
            unsafe {
                libc::close(sockets[0]);
                libc::close(sockets[1]);
            }
            assert!(libc::WIFSIGNALED(status), "child status {status}");
            assert_eq!(libc::WTERMSIG(status), libc::SIGSYS);
        }
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn wait_bounded(child: libc::pid_t) -> i32 {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
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
                unsafe {
                    libc::kill(child, libc::SIGKILL);
                }
                while unsafe { libc::waitpid(child, &mut status, 0) } == -1
                    && std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR)
                {
                }
                panic!("syscall-gate test child stalled");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}
