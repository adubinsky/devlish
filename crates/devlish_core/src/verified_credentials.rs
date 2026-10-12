//! Explicit operator credential sources for verified adapters. No dotenv lookup.
use std::{fs::File, io::Read, path::Path};

pub(super) enum VerifiedCredentials {
    Environment,
    #[cfg(unix)]
    Directory(File),
}
impl VerifiedCredentials {
    pub(super) fn new(directory: Option<&Path>) -> Result<Self, String> {
        let Some(path) = directory else {
            return Ok(Self::Environment);
        };
        if !path.is_absolute() {
            return Err("operator credential directory must be absolute".into());
        }
        if path
            .components()
            .any(|part| part == std::path::Component::ParentDir)
        {
            return Err("operator credential directory must not contain parent traversal".into());
        }
        // Strip trailing separators/dots lexically, never by resolving links.
        // Otherwise `link/` or `link/.` can bypass final-component O_NOFOLLOW.
        let path: std::path::PathBuf = path.components().collect();
        #[cfg(not(unix))]
        {
            let _ = path;
            Err("mounted verified credentials require Unix permissions".into())
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
            let file = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(
                    libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
                )
                .open(&path)
                .map_err(|_| "operator credential directory unavailable")?;
            let metadata = file
                .metadata()
                .map_err(|_| "operator credential directory unavailable")?;
            if !metadata.is_dir() || metadata.mode() & 0o077 != 0 || !trusted_owner(metadata.uid())
            {
                return Err(
                    "operator credential directory must be private and operator-owned".into(),
                );
            }
            // Retain the descriptor. Replacing the pathname cannot redirect reads.
            Ok(Self::Directory(file))
        }
    }
    pub(super) fn resolve(&self, key: &str) -> Option<String> {
        if key.is_empty()
            || key.len() > 128
            || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return None;
        }
        match self {
            Self::Environment => std::env::var(key).ok().and_then(clean_secret),
            #[cfg(unix)]
            Self::Directory(directory) => read_secret(directory, key).ok(),
        }
    }
}
fn clean_secret(mut value: String) -> Option<String> {
    if value.ends_with('\n') {
        value.pop();
        if value.ends_with('\r') {
            value.pop();
        }
    }
    if value.is_empty() || value.len() > 8192 || value.contains(['\0', '\r', '\n']) {
        None
    } else {
        Some(value)
    }
}
#[cfg(unix)]
fn trusted_owner(uid: u32) -> bool {
    uid == 0 || uid == unsafe { libc::geteuid() }
}
#[cfg(unix)]
fn read_secret(directory: &File, key: &str) -> Result<String, String> {
    use std::os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::MetadataExt,
    };
    let name = std::ffi::CString::new(key).map_err(|_| "invalid credential name")?;
    // Names are validated identifiers, never paths. O_NONBLOCK prevents FIFO hangs.
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err("operator credential unavailable".into());
    }
    let file = unsafe { File::from_raw_fd(fd) };
    let metadata = file
        .metadata()
        .map_err(|_| "operator credential unavailable")?;
    if !metadata.is_file()
        || metadata.mode() & 0o077 != 0
        || !trusted_owner(metadata.uid())
        || metadata.nlink() != 1
        || metadata.len() > 8192
    {
        return Err("operator credential has unsafe metadata".into());
    }
    let mut bytes = Vec::new();
    file.take(8193)
        .read_to_end(&mut bytes)
        .map_err(|_| "operator credential unavailable")?;
    if bytes.len() > 8192 {
        return Err("operator credential exceeds size limit".into());
    }
    String::from_utf8(bytes)
        .ok()
        .and_then(clean_secret)
        .ok_or_else(|| "operator credential has invalid content".into())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{
        os::unix::fs::{symlink, PermissionsExt},
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "devlish-credentials-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }
        fn secret(&self, name: &str, value: &[u8]) {
            let path = self.0.join(name);
            std::fs::write(&path, value).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn private_mounted_credentials_are_bounded_and_do_not_follow_links() {
        let f = Fixture::new();
        f.secret("KEY", b"synthetic-secret\n");
        let store = VerifiedCredentials::new(Some(&f.0)).unwrap();
        assert_eq!(store.resolve("KEY").as_deref(), Some("synthetic-secret"));
        for key in ["../KEY", "KEY/child", "", "unknown"] {
            assert!(store.resolve(key).is_none());
        }
        symlink(f.0.join("KEY"), f.0.join("LINK")).unwrap();
        assert!(store.resolve("LINK").is_none());
        std::fs::hard_link(f.0.join("KEY"), f.0.join("HARDLINK")).unwrap();
        assert!(store.resolve("KEY").is_none());
        f.secret("PUBLIC", b"synthetic-public");
        std::fs::set_permissions(f.0.join("PUBLIC"), std::fs::Permissions::from_mode(0o644))
            .unwrap();
        assert!(store.resolve("PUBLIC").is_none());
        for (key, value) in [
            ("LARGE", vec![b'x'; 8193]),
            ("EMPTY", vec![]),
            ("MULTILINE", b"first\nsecond".to_vec()),
            ("INVALID_UTF8", vec![255]),
        ] {
            f.secret(key, &value);
            assert!(store.resolve(key).is_none());
        }
        let fifo = std::ffi::CString::new(f.0.join("FIFO").as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        assert!(store.resolve("FIFO").is_none());
    }
    #[test]
    fn directory_symlink_spelling_cannot_bypass_source_validation() {
        let f = Fixture::new();
        let link = f.0.join("link");
        symlink(&f.0, &link).unwrap();
        for suffix in ["", "/", "/.", "//./"] {
            let spelling = format!("{}{suffix}", link.display());
            assert!(
                VerifiedCredentials::new(Some(Path::new(&spelling))).is_err(),
                "accepted {spelling}"
            );
            let ordinary = format!("{}{suffix}", f.0.display());
            assert!(VerifiedCredentials::new(Some(Path::new(&ordinary))).is_ok());
        }
        assert!(
            VerifiedCredentials::new(Some(&f.0.join("../").join(f.0.file_name().unwrap())))
                .is_err()
        );
    }
    #[test]
    fn mounted_directory_is_retained_and_unsafe_sources_fail_closed() {
        assert!(VerifiedCredentials::new(Some(Path::new("relative"))).is_err());
        let f = Fixture::new();
        let original = f.0.join("original");
        std::fs::create_dir(&original).unwrap();
        std::fs::set_permissions(&original, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(original.join("KEY"), b"approved").unwrap();
        std::fs::set_permissions(original.join("KEY"), std::fs::Permissions::from_mode(0o600))
            .unwrap();
        let store = VerifiedCredentials::new(Some(&original)).unwrap();
        std::fs::rename(&original, f.0.join("retained")).unwrap();
        std::fs::create_dir(&original).unwrap();
        std::fs::write(original.join("KEY"), b"replaced").unwrap();
        std::fs::set_permissions(&original, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(store.resolve("KEY").as_deref(), Some("approved"));
        assert!(VerifiedCredentials::new(Some(&original)).is_err());
        symlink(f.0.join("retained"), f.0.join("link")).unwrap();
        assert!(VerifiedCredentials::new(Some(&f.0.join("link"))).is_err());
        assert!(VerifiedCredentials::new(Some(&f.0.join("missing"))).is_err());
    }
    #[test]
    fn verified_store_ignores_dotenv_and_mounted_mode_never_falls_back_to_environment() {
        const CHILD: &str = "DEVLISH_TEST_VERIFIED_CREDENTIALS";
        if std::env::var_os(CHILD).is_some() {
            let store = super::super::CredentialStore::verified().unwrap();
            assert_eq!(
                store.resolve("SYNTHETIC_OPERATOR_KEY").as_deref(),
                Some("operator-value")
            );
            let empty = Fixture::new();
            let mounted = VerifiedCredentials::new(Some(&empty.0)).unwrap();
            assert!(mounted.resolve("SYNTHETIC_OPERATOR_KEY").is_none());
            return;
        }
        let f = Fixture::new();
        std::fs::create_dir(f.0.join(".devlish")).unwrap();
        std::fs::write(
            f.0.join(".devlish/.env"),
            "SYNTHETIC_OPERATOR_KEY=untrusted-home",
        )
        .unwrap();
        std::fs::write(f.0.join(".env"), "SYNTHETIC_OPERATOR_KEY=untrusted-project").unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact","verified_credentials::tests::verified_store_ignores_dotenv_and_mounted_mode_never_falls_back_to_environment"])
            .current_dir(&f.0).env("HOME",&f.0).env(CHILD,"1").env_remove("DEVLISH_CREDENTIALS_DIR")
            .env("SYNTHETIC_OPERATOR_KEY","operator-value").output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}
