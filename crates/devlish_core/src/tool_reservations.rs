//! Durable one-attempt slots for a future governed tool launcher.
//! Storage bookkeeping is not policy authority or proof of execution.
#[cfg(target_os = "linux")]
pub use unix::RecordedCapture;
#[cfg(unix)]
pub use unix::{ConsumedLaunch, ReservedLaunch, ToolReservations};

#[cfg(not(unix))]
pub struct ToolReservations;
#[cfg(not(unix))]
impl ToolReservations {
    pub fn open(_: &std::path::Path) -> Result<Self, &'static str> {
        Err("tool reservations are unavailable on this platform")
    }
}

#[cfg(unix)]
mod unix {
    use devlish_audit::{sha256, tool_catalog::ToolSelection};
    use serde_json::{json, Value};
    use std::{
        ffi::CString,
        fs::{File, OpenOptions},
        io::Write,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{
                ffi::OsStrExt,
                fs::{MetadataExt, OpenOptionsExt},
            },
        },
        path::Path,
    };

    pub struct ToolReservations {
        directory: File,
    }
    /// Dropping a reservation leaves its slot occupied. There is no reopen,
    /// cancel, refund or automatic retry operation.
    #[must_use]
    pub struct ReservedLaunch {
        file: File,
        operation: String,
        reserved_bytes: Vec<u8>,
    }
    /// Evidence that this object persisted a consumption marker. This is not
    /// authorization to launch, nor evidence that a process actually executed.
    pub struct ConsumedLaunch {
        reservation: ReservedLaunch,
        evidence_sha256: String,
    }
    impl ConsumedLaunch {
        pub fn operation_id(&self) -> &str {
            &self.reservation.operation
        }
        pub fn evidence_sha256(&self) -> &str {
            &self.evidence_sha256
        }
    }

    /// Capture returned only after its terminal record has been synced. It
    /// still requires independent Devlish disclosure approval before any send.
    #[cfg(target_os = "linux")]
    pub struct RecordedCapture {
        capture: crate::tool_streams::CapturedOutput,
        evidence_sha256: String,
    }
    #[cfg(target_os = "linux")]
    impl RecordedCapture {
        pub fn capture(&self) -> &crate::tool_streams::CapturedOutput {
            &self.capture
        }
        pub fn evidence_sha256(&self) -> &str {
            &self.evidence_sha256
        }
    }

    impl ConsumedLaunch {
        /// The protected host must bind this capture to this operation's child.
        /// Ownership withholds and drops the capture on persistence failure.
        /// No return value proves execution origin or authorizes disclosure.
        #[cfg(target_os = "linux")]
        pub fn record_capture(
            self,
            capture: crate::tool_streams::CapturedOutput,
        ) -> Result<RecordedCapture, &'static str> {
            let digest =
                self.record_completed(capture.exit_code(), capture.stdout(), capture.stderr())?;
            Ok(RecordedCapture {
                capture,
                evidence_sha256: digest,
            })
        }

        #[cfg(any(target_os = "linux", test))]
        fn record_completed(
            mut self,
            exit_code: i32,
            stdout: &[u8],
            stderr: &[u8],
        ) -> Result<String, &'static str> {
            if !(0..=255).contains(&exit_code)
                || stdout.len() as u64 > devlish_audit::tool_containment::STREAM_BYTES
                || stderr.len() as u64 > devlish_audit::tool_containment::STREAM_BYTES
            {
                return Err("invalid bounded terminal capture; slot must not be retried");
            }
            let mut marker = serde_json::to_vec(&json!({
                "state":"completed", "operation_id":self.reservation.operation,
                "consumed_sha256":self.evidence_sha256,"exit_code":exit_code,
                "stdout_bytes":stdout.len(),"stdout_sha256":sha256(stdout),
                "stderr_bytes":stderr.len(),"stderr_sha256":sha256(stderr),
            }))
            .map_err(|_| "invalid completion marker")?;
            marker.push(b'\n');
            if self.reservation.reserved_bytes.len() + marker.len()
                > devlish_audit::tool_reservation::MAX_RESERVATION_BYTES as usize
            {
                return Err("terminal evidence exceeds size limit; slot must not be retried");
            }
            self.reservation
                .file
                .write_all(&marker)
                .and_then(|()| self.reservation.file.sync_all())
                .map_err(|_| {
                    "terminal outcome persistence uncertain; withhold output and do not retry"
                })?;
            self.reservation.reserved_bytes.extend_from_slice(&marker);
            Ok(sha256(&self.reservation.reserved_bytes))
        }
    }

    impl ToolReservations {
        /// Open an existing private operator-owned directory and pin its FD.
        /// The operator must protect its ancestors, stable identity and durable
        /// local filesystem across restarts. This does not resist administrators.
        pub fn open(path: &Path) -> Result<Self, &'static str> {
            let raw = path.as_os_str().as_bytes();
            if raw.len() > 4096
                || !raw.starts_with(b"/")
                || raw.contains(&0)
                || raw[1..]
                    .split(|b| *b == b'/')
                    .any(|p| p.is_empty() || p == b"." || p == b"..")
            {
                return Err("reservation directory requires a bounded normal absolute path");
            }
            let directory = OpenOptions::new()
                .read(true)
                .custom_flags(
                    libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
                )
                .open(path)
                .map_err(|_| "reservation directory unavailable")?;
            let metadata = directory
                .metadata()
                .map_err(|_| "reservation directory unavailable")?;
            if !metadata.is_dir()
                || metadata.mode() & 0o077 != 0
                || metadata.uid() != unsafe { libc::geteuid() }
            {
                return Err("reservation directory must be private and owned by this operator");
            }
            Ok(Self { directory })
        }

        /// Reserve a stable tenant/session/effect slot, independently of the
        /// selected release, tool or arguments. Changing them cannot reopen it.
        /// The selection is authenticated catalog membership, not admission or
        /// an effect-policy decision. Callers must supply protected identities
        /// and a non-reused effect number from their durable session state.
        pub fn reserve(
            &self,
            tenant: &str,
            session: &str,
            effect: u64,
            selection: &ToolSelection<'_>,
        ) -> Result<ReservedLaunch, &'static str> {
            let binding = json!({
                "release_sha256":selection.manifest_sha256(),
                "catalog_sha256":selection.catalog_sha256(),
                "tool_id":selection.id(), "tool_sha256":selection.tool_sha256(),
                "arguments_sha256":sha256(&serde_json::to_vec(selection.arguments()).map_err(|_| "invalid argument commitment")?),
                "containment_sha256":selection.containment_sha256(),
            });
            self.reserve_binding(tenant, session, effect, binding)
        }

        fn reserve_binding(
            &self,
            tenant: &str,
            session: &str,
            effect: u64,
            binding: Value,
        ) -> Result<ReservedLaunch, &'static str> {
            let operation = operation_id(tenant, session, effect)?;
            let mut bytes = serde_json::to_vec(&json!({
                "format":"devlish-tool-launch-reservation","format_version":1,
                "state":"reserved", "operation_id":operation,
                "tenant_id":tenant,"session_id":session,"effect_id":effect,
                "binding":binding,
            }))
            .map_err(|_| "invalid launch reservation")?;
            bytes.push(b'\n');
            if bytes.len() > devlish_audit::tool_reservation::MAX_RESERVED_RECORD_BYTES {
                return Err("launch reservation exceeds size limit");
            }
            let name =
                CString::new(format!("{operation}.jsonl")).map_err(|_| "invalid operation name")?;
            let fd = unsafe {
                libc::openat(
                    self.directory.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_WRONLY
                        | libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_NOFOLLOW
                        | libc::O_CLOEXEC,
                    0o600,
                )
            };
            if fd < 0 {
                return Err("launch slot already exists or cannot be reserved");
            }
            let mut file = unsafe { File::from_raw_fd(fd) };
            // Never remove even a partial file after failure. Its existence
            // blocks reuse; only completed persistence returns a live token.
            file.write_all(&bytes)
                .and_then(|()| file.sync_all())
                .and_then(|()| self.directory.sync_all())
                .map_err(|_| "launch reservation persistence failed; slot must not be retried")?;
            Ok(ReservedLaunch {
                file,
                operation,
                reserved_bytes: bytes,
            })
        }
    }

    impl ReservedLaunch {
        pub fn operation_id(&self) -> &str {
            &self.operation
        }

        /// Consume once, before any broker continuation. Ownership prevents
        /// another attempt through this token; errors deliberately burn it too.
        /// Persisting this marker is necessary but not sufficient to execute.
        pub fn consume(mut self) -> Result<ConsumedLaunch, &'static str> {
            let mut marker = serde_json::to_vec(&json!({
                "state":"consumed", "operation_id":self.operation,
                "reserved_sha256":sha256(&self.reserved_bytes),
            }))
            .map_err(|_| "invalid consumption marker")?;
            marker.push(b'\n');
            self.file
                .write_all(&marker)
                .and_then(|()| self.file.sync_all())
                .map_err(|_| "launch consumption uncertain; slot must not be retried")?;
            self.reserved_bytes.extend_from_slice(&marker);
            Ok(ConsumedLaunch {
                evidence_sha256: sha256(&self.reserved_bytes),
                reservation: self,
            })
        }
    }

    fn operation_id(tenant: &str, session: &str, effect: u64) -> Result<String, &'static str> {
        devlish_audit::tool_reservation::operation_id(tenant, session, effect)
            .map_err(|_| "invalid protected launch slot identity")
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::{
            fs,
            os::unix::fs::{symlink, PermissionsExt},
            path::PathBuf,
            sync::atomic::{AtomicU64, Ordering},
        };
        static NEXT: AtomicU64 = AtomicU64::new(0);
        struct Directory(PathBuf);
        impl Directory {
            fn new() -> Self {
                let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
                    "devlish-launch-slots-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
                Self(path)
            }
            fn open(&self) -> ToolReservations {
                ToolReservations::open(&self.0).unwrap()
            }
            fn slot(&self, effect: u64) -> PathBuf {
                self.0.join(format!(
                    "{}.jsonl",
                    operation_id("tenant", "session", effect).unwrap()
                ))
            }
        }
        impl Drop for Directory {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        fn reserve(
            store: &ToolReservations,
            effect: u64,
            binding: &str,
        ) -> Result<ReservedLaunch, &'static str> {
            store.reserve_binding(
                "tenant",
                "session",
                effect,
                json!({"synthetic_binding":binding}),
            )
        }

        #[test]
        fn abandoned_consumed_and_partial_slots_never_reopen_after_restart() {
            let directory = Directory::new();
            let store = directory.open();
            let pending = reserve(&store, 1, "original").unwrap();
            drop(pending);
            let consumed = reserve(&store, 2, "original").unwrap().consume().unwrap();
            let bytes = fs::read(directory.slot(2)).unwrap();
            assert_eq!(sha256(&bytes), consumed.evidence_sha256());
            let records: Vec<Value> = bytes
                .split(|b| *b == b'\n')
                .filter(|line| !line.is_empty())
                .map(|line| serde_json::from_slice(line).unwrap())
                .collect();
            assert_eq!(records.len(), 2);
            assert_eq!(records[0]["state"], "reserved");
            assert_eq!(records[1]["state"], "consumed");
            assert_eq!(records[1]["operation_id"], consumed.operation_id());
            fs::write(directory.slot(3), b"partial").unwrap();
            drop(store);
            let restarted = directory.open();
            for id in 1..=3 {
                assert!(reserve(&restarted, id, "changed-release-tool-or-arguments").is_err());
            }
        }

        #[test]
        fn terminal_record_commits_both_streams_without_raw_output_or_retry() {
            let directory = Directory::new();
            let store = directory.open();
            let consumed = reserve(&store, 1, "original").unwrap().consume().unwrap();
            let prior_digest = consumed.evidence_sha256().to_owned();
            let digest = consumed
                .record_completed(37, b"synthetic NPPI", b"synthetic company IP")
                .unwrap();
            let bytes = fs::read(directory.slot(1)).unwrap();
            assert_eq!(sha256(&bytes), digest);
            let text = String::from_utf8(bytes).unwrap();
            assert!(!text.contains("synthetic NPPI"));
            assert!(!text.contains("synthetic company IP"));
            let terminal: Value = serde_json::from_str(text.lines().nth(2).unwrap()).unwrap();
            assert_eq!(terminal["consumed_sha256"], prior_digest);
            assert_eq!(terminal["exit_code"], 37);
            assert_eq!(terminal["stdout_sha256"], sha256(b"synthetic NPPI"));
            assert_eq!(terminal["stderr_sha256"], sha256(b"synthetic company IP"));
            assert!(reserve(&directory.open(), 1, "retry").is_err());
        }

        #[test]
        fn terminal_write_failure_and_invalid_capture_burn_the_slot() {
            let directory = Directory::new();
            let store = directory.open();
            let mut consumed = reserve(&store, 1, "original").unwrap().consume().unwrap();
            consumed.reservation.file = File::open(directory.slot(1)).unwrap();
            assert!(consumed.record_completed(0, b"private", b"").is_err());
            assert_eq!(
                fs::read_to_string(directory.slot(1))
                    .unwrap()
                    .lines()
                    .count(),
                2
            );
            for (id, exit, out, err) in [
                (2, -1, vec![], vec![]),
                (3, 256, vec![], vec![]),
                (4, 0, vec![0; 65537], vec![]),
                (5, 0, vec![], vec![0; 65537]),
            ] {
                let consumed = reserve(&store, id, "original").unwrap().consume().unwrap();
                assert!(consumed.record_completed(exit, &out, &err).is_err());
                assert_eq!(
                    fs::read_to_string(directory.slot(id))
                        .unwrap()
                        .lines()
                        .count(),
                    2
                );
            }
            for id in 1..=5 {
                assert!(reserve(&directory.open(), id, "retry").is_err());
            }
        }

        #[test]
        fn concurrent_attempts_create_only_one_live_token() {
            let directory = Directory::new();
            let store = directory.open();
            let winners = std::thread::scope(|scope| {
                let handles: Vec<_> = (0..8)
                    .map(|_| scope.spawn(|| reserve(&store, 1, "same").is_ok()))
                    .collect();
                handles
                    .into_iter()
                    .map(|h| h.join().unwrap())
                    .filter(|won| *won)
                    .count()
            });
            assert_eq!(winners, 1);
        }

        #[test]
        fn consumption_io_failure_burns_the_slot() {
            let directory = Directory::new();
            let store = directory.open();
            let mut token = reserve(&store, 1, "original").unwrap();
            // Real EBADF write failure, not a backend reporting simulated success.
            token.file = File::open(directory.slot(1)).unwrap();
            assert!(token.consume().is_err());
            assert!(reserve(&directory.open(), 1, "retry").is_err());
            assert_eq!(
                fs::read_to_string(directory.slot(1))
                    .unwrap()
                    .lines()
                    .count(),
                1
            );
        }

        #[test]
        fn private_directory_is_pinned_and_slot_symlinks_are_not_followed() {
            let directory = Directory::new();
            let original = Directory::new();
            // Rename onto an empty owned directory; the store must retain its
            // original descriptor, even when the configured pathname changes.
            let store = directory.open();
            fs::rename(&directory.0, &original.0).unwrap();
            fs::create_dir(&directory.0).unwrap();
            fs::set_permissions(&directory.0, fs::Permissions::from_mode(0o700)).unwrap();
            drop(reserve(&store, 1, "pinned").unwrap());
            assert!(original.slot(1).exists());
            assert!(!directory.slot(1).exists());
            let target = original.0.join("sentinel");
            fs::write(&target, b"unchanged").unwrap();
            symlink(&target, original.slot(2)).unwrap();
            assert!(reserve(&store, 2, "symlink").is_err());
            assert_eq!(fs::read(target).unwrap(), b"unchanged");
        }

        #[test]
        fn unsafe_directories_and_invalid_slot_identities_fail_closed() {
            let directory = Directory::new();
            let store = directory.open();
            assert!(store
                .reserve_binding(
                    "tenant",
                    "session",
                    4,
                    json!({"too_large":"x".repeat(4096)})
                )
                .is_err());
            assert!(!directory.slot(4).exists());
            let link = Directory::new();
            symlink(&directory.0, link.0.join("alias")).unwrap();
            assert!(ToolReservations::open(&link.0.join("alias")).is_err());
            assert!(ToolReservations::open(Path::new("relative")).is_err());
            assert!(ToolReservations::open(&directory.0.join(".")).is_err());
            fs::set_permissions(&directory.0, fs::Permissions::from_mode(0o755)).unwrap();
            assert!(ToolReservations::open(&directory.0).is_err());
            for (tenant, session, effect) in [
                ("../escape", "session", 1),
                ("tenant", "", 1),
                ("tenant", "session", 0),
            ] {
                assert!(operation_id(tenant, session, effect).is_err());
            }
            assert_ne!(
                operation_id("ab", "c", 1).unwrap(),
                operation_id("a", "bc", 1).unwrap()
            );
        }
    }
}
