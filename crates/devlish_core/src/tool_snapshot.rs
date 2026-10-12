//! Linux byte-snapshot primitive for a future authorized tool launcher.
//!
//! This does not authorize, parse or execute a program. The expected digest must
//! come from protected operator configuration. Verified native tool dispatch
//! remains unavailable until catalog and platform containment checks exist.
use std::path::Path;

/// Exact checked bytes retained in a sealed Linux memfd. No writable mappings
/// or unsealed descriptors are exposed during construction.
#[derive(Debug)]
pub struct SealedToolSnapshot {
    #[cfg(target_os = "linux")]
    file: std::fs::File,
    digest: String,
    size: u64,
}

impl SealedToolSnapshot {
    /// Copy a bounded regular file through a non-symlink absolute path, seal the
    /// copy, then hash the sealed object itself. Later source changes cannot
    /// alter the returned bytes. Errors disclose no source path or file content.
    pub fn from_path(path: &Path, expected_sha256: &str) -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        {
            linux::load(path, expected_sha256)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (path, expected_sha256);
            Err("sealed tool snapshots are unavailable on this platform".into())
        }
    }

    pub fn sha256(&self) -> &str {
        &self.digest
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn read_sealed_bytes(&self) -> Result<Vec<u8>, String> {
        use std::os::unix::fs::FileExt;
        let mut bytes = vec![0; self.size as usize];
        // Positional reads cannot be redirected by another descriptor's cursor.
        self.file
            .read_exact_at(&mut bytes, 0)
            .map_err(|_| "sealed tool snapshot read failed")?;
        Ok(bytes)
    }
}

#[cfg(target_os = "linux")]
impl std::os::fd::AsFd for SealedToolSnapshot {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        std::os::fd::AsFd::as_fd(&self.file)
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::{
        ffi::CString,
        fs::{File, OpenOptions},
        io::{Read, Seek},
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{ffi::OsStrExt, fs::OpenOptionsExt},
        },
    };
    const MAX_BYTES: u64 = devlish_audit::MAX_ARTIFACT_BYTES;
    const SEALS: i32 =
        libc::F_SEAL_WRITE | libc::F_SEAL_GROW | libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL;

    pub(super) fn load(path: &Path, expected: &str) -> Result<SealedToolSnapshot, String> {
        if expected.len() != 64
            || !expected
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err("tool digest must be lowercase SHA-256 hex".into());
        }
        let source = open_source(path)?;
        let metadata = source.metadata().map_err(|_| "tool metadata unavailable")?;
        if !metadata.is_file() || metadata.len() > MAX_BYTES {
            return Err("tool source must be a bounded regular file".into());
        }
        // SAFETY: the static NUL-terminated name is valid for the duration of
        // the call; the successful fd is immediately transferred to File.
        let fd = unsafe {
            libc::memfd_create(
                c"devlish-tool-snapshot".as_ptr(),
                libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
            )
        };
        if fd < 0 {
            return Err("sealed tool snapshot creation unavailable".into());
        }
        let mut file = unsafe { File::from_raw_fd(fd) };
        let copied = std::io::copy(&mut source.take(MAX_BYTES + 1), &mut file)
            .map_err(|_| "tool snapshot copy failed")?;
        if copied > MAX_BYTES {
            return Err("tool snapshot exceeds size limit".into());
        }
        // SEAL_WRITE, not FUTURE_WRITE: no existing shared writable mapping may
        // survive this check. Every seal applies to the object and all aliases.
        if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_ADD_SEALS, SEALS) } < 0 {
            return Err("tool snapshot sealing failed".into());
        }
        let actual_seals = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GET_SEALS) };
        if actual_seals < 0 || actual_seals & SEALS != SEALS {
            return Err("tool snapshot seals could not be verified".into());
        }
        file.rewind().map_err(|_| "tool snapshot read failed")?;
        let mut bytes = Vec::new();
        (&mut file)
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "tool snapshot read failed")?;
        if bytes.len() as u64 != copied || crate::sha256_hex(&bytes) != expected {
            return Err("sealed tool snapshot digest mismatch".into());
        }
        file.rewind().map_err(|_| "tool snapshot read failed")?;
        Ok(SealedToolSnapshot {
            file,
            digest: expected.to_string(),
            size: copied,
        })
    }

    fn open_source(path: &Path) -> Result<File, String> {
        let raw = path.as_os_str().as_bytes();
        if !raw.starts_with(b"/") || raw.len() > 4096 || raw.contains(&0) {
            return Err("tool path must be a bounded absolute path".into());
        }
        let parts: Vec<_> = raw[1..].split(|b| *b == b'/').collect();
        if parts
            .iter()
            .any(|p| p.is_empty() || *p == b"." || *p == b"..")
        {
            return Err("tool path must contain only normal components".into());
        }
        let mut current = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open("/")
            .map_err(|_| "tool path unavailable")?;
        for (index, part) in parts.iter().enumerate() {
            let name = CString::new(*part).map_err(|_| "invalid tool path")?;
            let directory = if index + 1 == parts.len() {
                0
            } else {
                libc::O_DIRECTORY
            };
            // SAFETY: current owns its live directory descriptor; name stays
            // alive for the call. No path component can redirect through a link.
            let fd = unsafe {
                libc::openat(
                    current.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY
                        | libc::O_NOFOLLOW
                        | libc::O_NONBLOCK
                        | libc::O_CLOEXEC
                        | directory,
                )
            };
            if fd < 0 {
                return Err("tool path unavailable or contains a symlink".into());
            }
            current = unsafe { File::from_raw_fd(fd) };
        }
        Ok(current)
    }
}

#[cfg(all(test, not(target_os = "linux")))]
mod unsupported_tests {
    use super::*;
    #[test]
    fn unsupported_platform_has_no_filesystem_fallback() {
        let error = SealedToolSnapshot::from_path(Path::new("/does-not-exist"), &"0".repeat(64))
            .unwrap_err();
        assert_eq!(
            error,
            "sealed tool snapshots are unavailable on this platform"
        );
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{
        fs,
        os::{
            fd::{AsFd, AsRawFd},
            unix::fs::{symlink, FileExt, PermissionsExt},
        },
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    const BYTES: &[u8] = b"synthetic tool bytes: this fixture is never executed";
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let directory = std::env::temp_dir().join(format!(
                "devlish-sealed-tool-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&directory).unwrap();
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
            fs::write(directory.join("tool"), BYTES).unwrap();
            Self(directory)
        }
        fn load(&self) -> SealedToolSnapshot {
            SealedToolSnapshot::from_path(&self.0.join("tool"), &crate::sha256_hex(BYTES)).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn retained_bytes(snapshot: &SealedToolSnapshot) -> Vec<u8> {
        let mut bytes = vec![0; snapshot.size() as usize];
        snapshot.file.read_exact_at(&mut bytes, 0).unwrap();
        bytes
    }

    #[test]
    fn sealed_bytes_survive_source_modification_replacement_and_removal() {
        let f = Fixture::new();
        let snapshot = f.load();
        fs::write(f.0.join("tool"), b"modified in place").unwrap();
        fs::write(f.0.join("replacement"), b"replacement").unwrap();
        fs::rename(f.0.join("replacement"), f.0.join("tool")).unwrap();
        fs::remove_file(f.0.join("tool")).unwrap();
        assert_eq!(snapshot.sha256(), crate::sha256_hex(BYTES));
        assert_eq!(retained_bytes(&snapshot), BYTES);
        let fd = snapshot.as_fd().as_raw_fd();
        assert_ne!(
            unsafe { libc::fcntl(fd, libc::F_GETFD) } & libc::FD_CLOEXEC,
            0
        );
    }

    #[test]
    fn every_descriptor_alias_is_denied_writes_growth_and_shrinkage() {
        let f = Fixture::new();
        let snapshot = f.load();
        let alias = snapshot.file.try_clone().unwrap();
        for file in [&snapshot.file, &alias] {
            assert_eq!(
                file.write_at(b"attack", 0).unwrap_err().raw_os_error(),
                Some(libc::EPERM)
            );
            for size in [0, BYTES.len() as u64 + 1] {
                assert_eq!(
                    file.set_len(size).unwrap_err().raw_os_error(),
                    Some(libc::EPERM)
                );
            }
        }
        // A shared writable mapping would bypass ordinary write(2), so verify
        // the kernel rejects that path too. A private COW mapping is harmless.
        let mapping = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                BYTES.len(),
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                snapshot.file.as_raw_fd(),
                0,
            )
        };
        if mapping != libc::MAP_FAILED {
            unsafe {
                libc::munmap(mapping, BYTES.len());
            }
            panic!("sealed object accepted a shared writable mapping");
        }
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::EPERM)
        );
        assert_eq!(retained_bytes(&snapshot), BYTES);
    }

    #[test]
    fn changed_or_malformed_digest_never_yields_a_snapshot() {
        let f = Fixture::new();
        for digest in [
            "".into(),
            "F".repeat(64),
            "0".repeat(64),
            crate::sha256_hex(b"other"),
        ] {
            assert!(SealedToolSnapshot::from_path(&f.0.join("tool"), &digest).is_err());
        }
    }

    #[test]
    fn symlinks_at_any_depth_and_non_normal_paths_are_rejected() {
        let f = Fixture::new();
        symlink(f.0.join("tool"), f.0.join("link")).unwrap();
        symlink(&f.0, f.0.join("directory-link")).unwrap();
        for path in [
            f.0.join("link"),
            f.0.join("directory-link/tool"),
            f.0.join("directory-link/./tool"),
            f.0.join("./tool"),
            f.0.join("../tool"),
            PathBuf::from("tool"),
        ] {
            assert!(
                SealedToolSnapshot::from_path(&path, &crate::sha256_hex(BYTES)).is_err(),
                "{}",
                path.display()
            );
        }
    }

    #[test]
    fn non_regular_and_oversized_sources_fail_without_waiting_for_a_writer() {
        let f = Fixture::new();
        let fifo = f.0.join("fifo");
        let name = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        let large = f.0.join("large");
        fs::File::create(&large)
            .unwrap()
            .set_len(devlish_audit::MAX_ARTIFACT_BYTES + 1)
            .unwrap();
        for path in [&f.0, &fifo, &large] {
            assert!(SealedToolSnapshot::from_path(path, &crate::sha256_hex(BYTES)).is_err());
        }
    }
}
