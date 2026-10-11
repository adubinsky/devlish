//! Controlled streams and bounded collection for a future Linux launcher.
//! No process is spawned or authorized here; disclosure policy remains external.

#[cfg(not(target_os = "linux"))]
#[derive(Debug)]
pub struct ToolStreams;
#[cfg(not(target_os = "linux"))]
impl ToolStreams {
    pub fn new(_: &[u8]) -> Result<Self, &'static str> {
        Err("tool stream supervision is unavailable on this platform")
    }
}
#[cfg(target_os = "linux")]
pub use linux::{CapturedOutput, ParentStreams, ToolStreams};

#[cfg(target_os = "linux")]
mod linux {
    use devlish_audit::tool_containment::{STREAM_BYTES, WALL_TIMEOUT_MS};
    use std::{
        os::fd::{AsRawFd, FromRawFd, OwnedFd},
        time::{Duration, Instant},
    };

    pub struct ToolStreams {
        input: Vec<u8>,
        parent_input: OwnedFd,
        child_input: OwnedFd,
        parent_output: OwnedFd,
        child_output: OwnedFd,
        parent_error: OwnedFd,
        child_error: OwnedFd,
        deadline: Instant,
    }
    pub struct ParentStreams {
        input: Vec<u8>,
        stdin: Option<OwnedFd>,
        stdout: Option<OwnedFd>,
        stderr: Option<OwnedFd>,
        deadline: Instant,
    }
    /// Private buffers are released only after normal exit and complete EOF.
    /// These bytes remain untrusted and require separate disclosure approval.
    pub struct CapturedOutput {
        exit_code: i32,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    }
    impl CapturedOutput {
        pub fn exit_code(&self) -> i32 {
            self.exit_code
        }
        pub fn stdout(&self) -> &[u8] {
            &self.stdout
        }
        pub fn stderr(&self) -> &[u8] {
            &self.stderr
        }
    }

    impl ToolStreams {
        /// Prepare private streams before fork. Construction starts the fixed
        /// wall budget; the broker must check this deadline before granting exec.
        pub fn new(input: &[u8]) -> Result<Self, &'static str> {
            if input.len() as u64 > STREAM_BYTES {
                return Err("tool input exceeds stream limit");
            }
            let deadline = Instant::now() + Duration::from_millis(WALL_TIMEOUT_MS);
            let [parent_input, child_input] = pair(true)?;
            // A private one-way local socket permits MSG_NOSIGNAL sends without
            // modifying the host process's global SIGPIPE disposition.
            if unsafe { libc::shutdown(parent_input.as_raw_fd(), libc::SHUT_RD) } != 0
                || unsafe { libc::shutdown(child_input.as_raw_fd(), libc::SHUT_WR) } != 0
            {
                return Err("cannot configure tool input channel");
            }
            let [parent_output, child_output] = pair(false)?;
            let [parent_error, child_error] = pair(false)?;
            for fd in [&parent_input, &parent_output, &parent_error] {
                nonblocking(fd)?;
            }
            Ok(Self {
                input: input.to_vec(),
                parent_input,
                child_input,
                parent_output,
                child_output,
                parent_error,
                child_error,
                deadline,
            })
        }
        pub fn deadline(&self) -> Instant {
            self.deadline
        }

        /// Install controlled blocking standard streams without allocating.
        /// # Safety
        /// Call only in a disposable single-threaded child. Replaced standard
        /// descriptors may have Rust owners which must never run or be dropped.
        /// On failure `_exit`; on success close unlisted descriptors, finish
        /// trusted setup and exec/_exit without unwinding. This doesn't close
        /// the original endpoints or apply the other containment controls.
        pub unsafe fn install_child_stdio(&self) -> Result<(), &'static str> {
            for (source, target) in [
                (&self.child_input, 0),
                (&self.child_output, 1),
                (&self.child_error, 2),
            ] {
                // All original endpoints are above stderr, so no dup clobbers
                // a source for another standard stream. dup2 clears CLOEXEC.
                if unsafe { libc::dup2(source.as_raw_fd(), target) } != target {
                    return Err("cannot install controlled tool standard stream");
                }
            }
            Ok(())
        }

        /// Parent only, after fork: drop the child's original endpoints so EOF
        /// reflects its lifetime. The child's copies require separate cleanup.
        pub fn into_parent(self) -> ParentStreams {
            ParentStreams {
                input: self.input,
                stdin: Some(self.parent_input),
                stdout: Some(self.parent_output),
                stderr: Some(self.parent_error),
                deadline: self.deadline,
            }
        }
    }

    impl ParentStreams {
        pub fn deadline(&self) -> Instant {
            self.deadline
        }

        /// Collect under the fixed profile. Errors discard all captured bytes.
        /// # Safety
        /// `pid` must be the caller's live, exclusively owned and unreaped child;
        /// no other thread/handler may reap it. The caller must retain permission
        /// to kill it, and the child must not be able to change credentials.
        /// This call becomes its sole reaper
        /// and kills/reaps on errors or unwind. The child must be unable to fork,
        /// and its only remaining output writers must be controlled stdio. This
        /// is not process-tree containment. The broker must already have granted
        /// the initial exec and made all later exec requests fail closed.
        pub unsafe fn collect(self, pid: libc::pid_t) -> Result<CapturedOutput, &'static str> {
            unsafe { self.collect_with_clock(pid, Instant::now) }
        }

        unsafe fn collect_with_clock(
            mut self,
            pid: libc::pid_t,
            mut now: impl FnMut() -> Instant,
        ) -> Result<CapturedOutput, &'static str> {
            if pid <= 0 {
                return Err("invalid owned tool child");
            }
            let mut child = OwnedChild(Some(pid));
            let mut sent = 0;
            let mut stdout = Vec::with_capacity(STREAM_BYTES as usize);
            let mut stderr = Vec::with_capacity(STREAM_BYTES as usize);
            let mut status = None;
            loop {
                if now() >= self.deadline {
                    return Err("tool wall deadline exceeded");
                }
                if sent == self.input.len() {
                    self.stdin.take();
                }
                if let Some(fd) = &self.stdin {
                    let remaining = &self.input[sent..];
                    let result = unsafe {
                        libc::send(
                            fd.as_raw_fd(),
                            remaining.as_ptr().cast(),
                            remaining.len(),
                            libc::MSG_NOSIGNAL,
                        )
                    };
                    if result > 0 {
                        sent += result as usize;
                    } else if result == 0 || !retryable() {
                        return Err("tool input delivery failed");
                    }
                }
                drain(&mut self.stdout, &mut stdout)?;
                drain(&mut self.stderr, &mut stderr)?;
                if child.0.is_some() {
                    let mut observed = 0;
                    let result = unsafe { libc::waitpid(pid, &mut observed, libc::WNOHANG) };
                    if result == pid {
                        child.0 = None;
                        status = Some(observed);
                    } else if result < 0 {
                        // ECHILD violates the ownership contract. Do not signal
                        // a PID that another reaper may already have released.
                        if errno() == libc::ECHILD {
                            child.0 = None;
                        }
                        if errno() != libc::EINTR {
                            return Err("cannot observe owned tool child");
                        }
                    }
                }
                if let Some(status) = status {
                    if self.stdout.is_none() && self.stderr.is_none() {
                        if sent != self.input.len() {
                            return Err("tool ended before input delivery completed");
                        }
                        if !libc::WIFEXITED(status) {
                            return Err("tool ended without ordinary exit");
                        }
                        // EOF and exit can be observed after descheduling in
                        // this iteration. Recheck at the release boundary.
                        if now() >= self.deadline {
                            return Err("tool wall deadline exceeded");
                        }
                        return Ok(CapturedOutput {
                            exit_code: libc::WEXITSTATUS(status),
                            stdout,
                            stderr,
                        });
                    }
                }
                let mut polls = [
                    poll_fd(&self.stdin, libc::POLLOUT),
                    poll_fd(&self.stdout, libc::POLLIN),
                    poll_fd(&self.stderr, libc::POLLIN),
                ];
                let remaining = self.deadline.saturating_duration_since(now());
                let timeout = remaining.as_millis().min(50) as i32;
                let ready =
                    unsafe { libc::poll(polls.as_mut_ptr(), polls.len() as libc::nfds_t, timeout) };
                if ready < 0 && errno() != libc::EINTR {
                    return Err("tool stream polling failed");
                }
                if polls.iter().any(|p| p.revents & libc::POLLNVAL != 0) {
                    return Err("tool stream descriptor became invalid");
                }
            }
        }
    }

    struct OwnedChild(Option<libc::pid_t>);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            if let Some(pid) = self.0 {
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                }
                let mut status = 0;
                while unsafe { libc::waitpid(pid, &mut status, 0) } < 0 && errno() == libc::EINTR {}
            }
        }
    }
    fn errno() -> i32 {
        std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
    }
    fn retryable() -> bool {
        matches!(errno(), libc::EAGAIN | libc::EINTR)
    }
    fn poll_fd(fd: &Option<OwnedFd>, events: i16) -> libc::pollfd {
        libc::pollfd {
            fd: fd.as_ref().map_or(-1, AsRawFd::as_raw_fd),
            events,
            revents: 0,
        }
    }
    fn drain(fd: &mut Option<OwnedFd>, output: &mut Vec<u8>) -> Result<(), &'static str> {
        let Some(open) = fd.as_ref() else {
            return Ok(());
        };
        // A bounded number of successful reads prevents one ready stream from
        // starving the other stream or the wall-clock check.
        for _ in 0..4 {
            let mut buffer = [0u8; 4096];
            let read =
                unsafe { libc::read(open.as_raw_fd(), buffer.as_mut_ptr().cast(), buffer.len()) };
            if read == 0 {
                fd.take();
                return Ok(());
            }
            if read < 0 {
                return if retryable() {
                    Ok(())
                } else {
                    Err("tool output read failed")
                };
            }
            let read = read as usize;
            if output.len() + read > STREAM_BYTES as usize {
                return Err("tool output exceeds stream limit");
            }
            output.extend_from_slice(&buffer[..read]);
        }
        Ok(())
    }
    fn nonblocking(fd: &OwnedFd) -> Result<(), &'static str> {
        let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) };
        if flags < 0
            || unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } != 0
        {
            return Err("cannot configure nonblocking tool stream");
        }
        Ok(())
    }
    fn pair(socket: bool) -> Result<[OwnedFd; 2], &'static str> {
        let mut raw = [-1; 2];
        let result = unsafe {
            if socket {
                libc::socketpair(
                    libc::AF_UNIX,
                    libc::SOCK_STREAM | libc::SOCK_CLOEXEC,
                    0,
                    raw.as_mut_ptr(),
                )
            } else {
                libc::pipe2(raw.as_mut_ptr(), libc::O_CLOEXEC)
            }
        };
        if result != 0 {
            return Err("cannot create private tool streams");
        }
        let mut owned = unsafe { [OwnedFd::from_raw_fd(raw[0]), OwnedFd::from_raw_fd(raw[1])] };
        for fd in &mut owned {
            if fd.as_raw_fd() <= 2 {
                let higher = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 3) };
                if higher < 0 {
                    return Err("cannot isolate tool stream descriptor numbers");
                }
                *fd = unsafe { OwnedFd::from_raw_fd(higher) };
            }
        }
        Ok(owned)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn small_input_buffer(streams: &ToolStreams) {
            let bytes: libc::c_int = 4096;
            assert_eq!(
                unsafe {
                    libc::setsockopt(
                        streams.parent_input.as_raw_fd(),
                        libc::SOL_SOCKET,
                        libc::SO_SNDBUF,
                        (&bytes as *const libc::c_int).cast(),
                        std::mem::size_of_val(&bytes) as libc::socklen_t,
                    )
                },
                0
            );
        }
        unsafe fn child_setup(streams: &ToolStreams) {
            unsafe {
                if streams.install_child_stdio().is_err()
                    || crate::tool_descriptors::close_unlisted(&[]).is_err()
                {
                    libc::_exit(10);
                }
            }
        }
        unsafe fn write_bytes(fd: i32, byte: u8, mut count: usize) {
            let buffer = [byte; 4096];
            while count > 0 {
                let amount = count.min(buffer.len());
                let wrote = unsafe { libc::write(fd, buffer.as_ptr().cast(), amount) };
                if wrote < 0 && unsafe { *libc::__errno_location() } == libc::EINTR {
                    continue;
                }
                if wrote <= 0 {
                    unsafe {
                        libc::_exit(11);
                    }
                }
                count -= wrote as usize;
            }
        }
        fn assert_reaped(pid: libc::pid_t) {
            let mut status = 0;
            assert_eq!(
                unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) },
                -1
            );
            assert_eq!(errno(), libc::ECHILD);
        }

        #[test]
        fn full_duplex_streams_handle_backpressure_and_exact_caps_before_release() {
            let input = vec![b'i'; STREAM_BYTES as usize];
            let streams = ToolStreams::new(&input).unwrap();
            small_input_buffer(&streams);
            let pid = unsafe { libc::fork() };
            assert!(pid >= 0);
            if pid == 0 {
                unsafe {
                    child_setup(&streams);
                    // Output exceeds pipe capacity before this child reads any
                    // input. A sequential write-all/read-all parent would stall.
                    write_bytes(1, b'o', STREAM_BYTES as usize);
                    write_bytes(2, b'e', STREAM_BYTES as usize);
                    let mut total = 0usize;
                    loop {
                        let mut buffer = [0u8; 4096];
                        let read = libc::read(0, buffer.as_mut_ptr().cast(), buffer.len());
                        if read < 0 && *libc::__errno_location() == libc::EINTR {
                            continue;
                        }
                        if read < 0 {
                            libc::_exit(12);
                        }
                        if read == 0 {
                            break;
                        }
                        for byte in &buffer[..read as usize] {
                            if *byte != b'i' {
                                libc::_exit(13);
                            }
                        }
                        total += read as usize;
                    }
                    libc::_exit(if total == STREAM_BYTES as usize {
                        7
                    } else {
                        14
                    });
                }
            }
            let captured = unsafe { streams.into_parent().collect(pid) }.unwrap();
            assert_eq!(captured.exit_code(), 7);
            assert_eq!(captured.stdout(), vec![b'o'; STREAM_BYTES as usize]);
            assert_eq!(captured.stderr(), vec![b'e'; STREAM_BYTES as usize]);
            assert_reaped(pid);
        }

        #[test]
        fn deadline_crossed_during_final_collection_withholds_capture() {
            let streams = ToolStreams::new(b"").unwrap();
            let deadline = streams.deadline();
            let pid = unsafe { libc::fork() };
            assert!(pid >= 0);
            if pid == 0 {
                unsafe {
                    child_setup(&streams);
                    libc::_exit(0);
                }
            }
            // Wait for exit without reaping, so collection's first iteration
            // sees EOF and ordinary exit. Its entry clock is within budget;
            // every later observation is at the deadline.
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            assert_eq!(
                unsafe {
                    libc::waitid(
                        libc::P_PID,
                        pid as u32,
                        &mut info,
                        libc::WEXITED | libc::WNOWAIT,
                    )
                },
                0
            );
            let mut first = true;
            let captured = unsafe {
                streams.into_parent().collect_with_clock(pid, || {
                    if first {
                        first = false;
                        deadline - Duration::from_millis(1)
                    } else {
                        deadline
                    }
                })
            };
            assert_eq!(captured.err(), Some("tool wall deadline exceeded"));
            assert_reaped(pid);
        }

        #[test]
        fn either_output_overrun_discards_capture_and_reaps_child() {
            for fd in [1, 2] {
                let streams = ToolStreams::new(b"").unwrap();
                let pid = unsafe { libc::fork() };
                assert!(pid >= 0);
                if pid == 0 {
                    unsafe {
                        child_setup(&streams);
                        write_bytes(fd, b'x', STREAM_BYTES as usize + 1);
                        loop {
                            libc::pause();
                        }
                    }
                }
                assert_eq!(
                    unsafe { streams.into_parent().collect(pid) }.err().unwrap(),
                    "tool output exceeds stream limit"
                );
                assert_reaped(pid);
            }
        }

        #[test]
        fn wall_deadline_kills_and_reaps_a_blocked_child_without_partial_output() {
            let streams = ToolStreams::new(b"").unwrap();
            let pid = unsafe { libc::fork() };
            assert!(pid >= 0);
            if pid == 0 {
                unsafe {
                    child_setup(&streams);
                    write_bytes(1, b'x', 1);
                    loop {
                        libc::pause();
                    }
                }
            }
            assert_eq!(
                unsafe { streams.into_parent().collect(pid) }.err().unwrap(),
                "tool wall deadline exceeded"
            );
            assert_reaped(pid);
        }

        #[test]
        fn oversized_input_refuses_before_setup_and_early_close_fails_delivery() {
            assert_eq!(
                ToolStreams::new(&vec![0; STREAM_BYTES as usize + 1])
                    .err()
                    .unwrap(),
                "tool input exceeds stream limit"
            );
            let streams = ToolStreams::new(&vec![0; STREAM_BYTES as usize]).unwrap();
            small_input_buffer(&streams);
            let pid = unsafe { libc::fork() };
            assert!(pid >= 0);
            if pid == 0 {
                unsafe {
                    child_setup(&streams);
                    libc::close(0);
                    loop {
                        libc::pause();
                    }
                }
            }
            assert_eq!(
                unsafe { streams.into_parent().collect(pid) }.err().unwrap(),
                "tool input delivery failed"
            );
            assert_reaped(pid);
        }
    }
}

#[cfg(all(test, not(target_os = "linux")))]
mod tests {
    use super::*;
    #[cfg(not(target_os = "linux"))]
    #[test]
    fn unsupported_platform_has_no_supervision_fallback() {
        assert!(ToolStreams::new(b"").unwrap_err().contains("unavailable"));
    }
}
