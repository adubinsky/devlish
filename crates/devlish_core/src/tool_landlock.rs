//! One filesystem restriction layer for a future Linux child launcher.
//! This is not a complete sandbox and is not wired into tool dispatch.

#[derive(Debug)]
pub struct LandlockRuleset {
    #[cfg(target_os = "linux")]
    fd: std::os::fd::OwnedFd,
}

impl LandlockRuleset {
    /// Create an empty allowlist handling every filesystem right in Landlock
    /// ABI 3. New file reads/writes, directory reads, exec, creation/removal and truncation
    /// are denied after restriction. ABI support is mandatory, never downgraded.
    pub fn deny_file_access() -> Result<Self, &'static str> {
        #[cfg(target_os = "linux")]
        {
            use std::os::fd::FromRawFd;
            #[repr(C)]
            struct Attributes {
                handled_access_fs: u64,
            }
            // Query only; no policy is applied to the calling thread here.
            let abi = unsafe {
                libc::syscall(
                    libc::SYS_landlock_create_ruleset,
                    std::ptr::null::<u8>(),
                    0,
                    1,
                )
            };
            if abi < 3 {
                return Err("Landlock ABI 3 file restrictions are unavailable");
            }
            // EXECUTE through TRUNCATE, bits 0..14, are the ABI 3 filesystem
            // rights. Later rights are not silently claimed by this profile.
            let attributes = Attributes {
                handled_access_fs: (1 << 15) - 1,
            };
            // SAFETY: live, correctly sized C-layout attributes; kernel copies
            // them during this syscall. A successful result is a new owned fd.
            let fd = unsafe {
                libc::syscall(
                    libc::SYS_landlock_create_ruleset,
                    &attributes,
                    std::mem::size_of::<Attributes>(),
                    0,
                )
            };
            if fd < 0 {
                return Err("Landlock ruleset creation failed");
            }
            Ok(Self {
                fd: unsafe { std::os::fd::OwnedFd::from_raw_fd(fd as i32) },
            })
        }
        #[cfg(not(target_os = "linux"))]
        Err("Landlock file restrictions are unavailable on this platform")
    }

    /// Irreversibly restrict THIS thread and future descendants. A launcher
    /// must call this only in a fresh child and terminate that child on failure:
    /// no_new_privs may already be set. Other threads and inherited descriptors
    /// are not protected by this operation. No allocations occur on this path.
    pub fn restrict_current_thread(&self) -> Result<(), &'static str> {
        #[cfg(target_os = "linux")]
        {
            use std::os::fd::AsRawFd;
            // SAFETY: integer-only prctl arguments and a live owned ruleset fd.
            if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
                return Err("cannot prevent privilege gain before Landlock restriction");
            }
            if unsafe { libc::syscall(libc::SYS_landlock_restrict_self, self.fd.as_raw_fd(), 0) }
                != 0
            {
                return Err("Landlock thread restriction failed");
            }
            Ok(())
        }
        #[cfg(not(target_os = "linux"))]
        Err("Landlock file restrictions are unavailable on this platform")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(target_os = "linux"))]
    #[test]
    fn unsupported_platform_never_falls_back() {
        assert!(LandlockRuleset::deny_file_access()
            .unwrap_err()
            .contains("unavailable"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn kernel_denies_new_file_access_but_inherited_descriptors_remain_a_separate_gate() {
        use std::{
            ffi::CString,
            fs,
            os::fd::AsRawFd,
            time::{SystemTime, UNIX_EPOCH},
        };
        let dir = std::env::temp_dir().join(format!(
            "devlish-landlock-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("synthetic-data");
        fs::write(&path, b"public test fixture").unwrap();
        let inherited = fs::File::open(&path).unwrap();
        let raw = CString::new(path.to_str().unwrap()).unwrap();
        let new = CString::new(dir.join("new-file").to_str().unwrap()).unwrap();
        let directory = CString::new(dir.to_str().unwrap()).unwrap();
        let rules = LandlockRuleset::deny_file_access()
            .expect("Linux qualification requires enabled ABI >= 3");
        // The test runner itself must never be restricted. Child path below uses
        // only syscalls, fixed stack data and static errors: no Rust allocator,
        // unwinding, inherited mutexes, stdio or destructors after fork.
        let child = unsafe { libc::fork() };
        assert!(child >= 0);
        if child == 0 {
            unsafe {
                if rules.restrict_current_thread().is_err() {
                    libc::_exit(10);
                }
                // The applied policy survives closing the setup descriptor.
                if libc::close(rules.fd.as_raw_fd()) != 0 {
                    libc::_exit(16);
                }
                for flags in [
                    libc::O_RDONLY,
                    libc::O_WRONLY,
                    libc::O_RDONLY | libc::O_TRUNC,
                ] {
                    if libc::open(raw.as_ptr(), flags) != -1
                        || *libc::__errno_location() != libc::EACCES
                    {
                        libc::_exit(11);
                    }
                }
                if libc::open(directory.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY) != -1
                    || *libc::__errno_location() != libc::EACCES
                {
                    libc::_exit(12);
                }
                if libc::open(new.as_ptr(), libc::O_WRONLY | libc::O_CREAT, 0o600) != -1
                    || *libc::__errno_location() != libc::EACCES
                {
                    libc::_exit(13);
                }
                if libc::unlink(raw.as_ptr()) != -1 || *libc::__errno_location() != libc::EACCES {
                    libc::_exit(14);
                }
                // Deliberately demonstrate why a launcher must close inherited
                // descriptors: Landlock does not revoke previously granted read.
                let mut byte = 0u8;
                if libc::read(inherited.as_raw_fd(), (&mut byte as *mut u8).cast(), 1) != 1
                    || byte != b'p'
                {
                    libc::_exit(15);
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
        assert_eq!(fs::read(&path).unwrap(), b"public test fixture");
        assert!(!dir.join("new-file").exists());
        drop(inherited);
        fs::remove_dir_all(dir).unwrap();
    }
}
