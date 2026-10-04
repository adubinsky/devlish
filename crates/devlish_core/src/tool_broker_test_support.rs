//! Synthetic Linux test transport/supervision only. Not a production broker.
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

// Linux UAPI include/uapi/linux/seccomp.h, notification ioctl definitions.
// libc 0.2.186 has the structures/encoders but not these named constants.
const NOTIF_RECV: libc::Ioctl = libc::_IOWR::<libc::seccomp_notif>(b'!' as u32, 0);
const NOTIF_SEND: libc::Ioctl = libc::_IOWR::<libc::seccomp_notif_resp>(b'!' as u32, 1);
const NOTIF_ID_VALID: libc::Ioctl = libc::_IOW::<u64>(b'!' as u32, 2);

pub(super) struct Child(pub Option<libc::pid_t>);
impl Child {
    pub fn wait(&mut self) -> i32 {
        let pid = self.0.unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let mut status = 0;
            let result = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
            if result == pid {
                self.0 = None;
                return status;
            }
            if result < 0 {
                assert_eq!(
                    std::io::Error::last_os_error().raw_os_error(),
                    Some(libc::EINTR)
                );
            }
            assert!(std::time::Instant::now() < deadline, "broker child stalled");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        if let Some(pid) = self.0 {
            // This guard is the sole reaper. An unreaped child PID cannot be
            // reused; failed tests kill and reap their own disposable child.
            unsafe {
                libc::kill(pid, libc::SIGKILL);
            }
            let mut status = 0;
            while unsafe { libc::waitpid(pid, &mut status, 0) } < 0
                && std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR)
            {}
        }
    }
}

pub(super) fn socket_pair() -> [OwnedFd; 2] {
    let mut raw = [-1; 2];
    assert_eq!(
        unsafe {
            libc::socketpair(
                libc::AF_UNIX,
                libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC,
                0,
                raw.as_mut_ptr(),
            )
        },
        0
    );
    unsafe { [OwnedFd::from_raw_fd(raw[0]), OwnedFd::from_raw_fd(raw[1])] }
}

/// Child-only, allocation-free SCM_RIGHTS send. Caller never unwinds/returns.
pub(super) unsafe fn send_listener(socket: i32, listener: i32) -> bool {
    let mut marker = 1u8;
    let mut iov = libc::iovec {
        iov_base: (&mut marker as *mut u8).cast(),
        iov_len: 1,
    };
    let mut control = [0usize; 4]; // aligned, at least CMSG_SPACE(sizeof(int))
    let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    message.msg_control = control.as_mut_ptr().cast();
    message.msg_controllen =
        unsafe { libc::CMSG_SPACE(std::mem::size_of::<i32>() as u32) } as usize;
    let header = unsafe { libc::CMSG_FIRSTHDR(&message) };
    if header.is_null() {
        return false;
    }
    unsafe {
        (*header).cmsg_level = libc::SOL_SOCKET;
        (*header).cmsg_type = libc::SCM_RIGHTS;
        (*header).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<i32>() as u32) as usize;
        std::ptr::write_unaligned(libc::CMSG_DATA(header).cast::<i32>(), listener);
        libc::sendmsg(socket, &message, libc::MSG_NOSIGNAL) == 1
    }
}

fn readable(fd: i32) {
    let mut descriptor = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    assert_eq!(
        unsafe { libc::poll(&mut descriptor, 1, 5000) },
        1,
        "test transport stalled"
    );
    assert_ne!(descriptor.revents & libc::POLLIN, 0);
}

pub(super) fn receive_listener(socket: &OwnedFd) -> OwnedFd {
    readable(socket.as_raw_fd());
    let mut marker = 0u8;
    let mut iov = libc::iovec {
        iov_base: (&mut marker as *mut u8).cast(),
        iov_len: 1,
    };
    let mut control = [0usize; 4];
    let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    message.msg_control = control.as_mut_ptr().cast();
    message.msg_controllen = std::mem::size_of_val(&control);
    assert_eq!(
        unsafe { libc::recvmsg(socket.as_raw_fd(), &mut message, libc::MSG_CMSG_CLOEXEC) },
        1
    );
    assert_eq!(marker, 1);
    assert_eq!(message.msg_flags & (libc::MSG_CTRUNC | libc::MSG_TRUNC), 0);
    let header = unsafe { libc::CMSG_FIRSTHDR(&message) };
    assert!(!header.is_null());
    unsafe {
        assert_eq!((*header).cmsg_level, libc::SOL_SOCKET);
        assert_eq!((*header).cmsg_type, libc::SCM_RIGHTS);
        assert_eq!(
            (*header).cmsg_len,
            libc::CMSG_LEN(std::mem::size_of::<i32>() as u32) as usize
        );
        let fd = OwnedFd::from_raw_fd(std::ptr::read_unaligned(
            libc::CMSG_DATA(header).cast::<i32>(),
        ));
        assert!(libc::CMSG_NXTHDR(&message, header).is_null());
        assert_ne!(
            libc::fcntl(fd.as_raw_fd(), libc::F_GETFD) & libc::FD_CLOEXEC,
            0
        );
        fd
    }
}

pub(super) fn check_notification_sizes() {
    let mut sizes: libc::seccomp_notif_sizes = unsafe { std::mem::zeroed() };
    assert_eq!(
        unsafe {
            libc::syscall(
                libc::SYS_seccomp,
                libc::SECCOMP_GET_NOTIF_SIZES,
                0,
                &mut sizes,
            )
        },
        0
    );
    // Refuse unknown ABI extensions instead of letting the kernel write beyond
    // the test's fixed structures. A production broker needs version handling.
    assert_eq!(
        sizes.seccomp_notif as usize,
        std::mem::size_of::<libc::seccomp_notif>()
    );
    assert_eq!(
        sizes.seccomp_notif_resp as usize,
        std::mem::size_of::<libc::seccomp_notif_resp>()
    );
    assert_eq!(
        sizes.seccomp_data as usize,
        std::mem::size_of::<libc::seccomp_data>()
    );
}

pub(super) fn notification(listener: &OwnedFd) -> libc::seccomp_notif {
    readable(listener.as_raw_fd());
    let mut request: libc::seccomp_notif = unsafe { std::mem::zeroed() };
    assert_eq!(
        unsafe { libc::ioctl(listener.as_raw_fd(), NOTIF_RECV, &mut request,) },
        0
    );
    assert_eq!(
        unsafe { libc::ioctl(listener.as_raw_fd(), NOTIF_ID_VALID, &request.id,) },
        0
    );
    request
}

pub(super) fn reply(listener: &OwnedFd, id: u64, first_exec: bool) {
    let response = libc::seccomp_notif_resp {
        id,
        val: 0,
        error: if first_exec { 0 } else { -libc::EPERM },
        flags: if first_exec {
            libc::SECCOMP_USER_NOTIF_FLAG_CONTINUE as u32
        } else {
            0
        },
    };
    assert_eq!(
        unsafe { libc::ioctl(listener.as_raw_fd(), NOTIF_SEND, &response,) },
        0
    );
}
