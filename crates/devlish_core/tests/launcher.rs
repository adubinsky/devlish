//! The convenience wrapper cannot bootstrap verified execution safely.
#![cfg(unix)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "devlish-launcher-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        for dir in [
            "bin",
            "path",
            "crates/devlish_core/target/release",
            "crates/devlish_core/target/debug",
        ] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bin/devlish"),
            root.join("bin/devlish"),
        )
        .unwrap();
        Self(root)
    }
    fn candidate(&self, relative: &str, label: &str) {
        let path = self.0.join(relative);
        fs::write(&path, format!("#!/bin/sh\nprintf '%s\\n' '{label}' >> \"$MARKER\"\nprintf '%s\\n' \"$@\"\nexit 23\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    fn run(&self, args: &[&str], profile: Option<&str>) -> std::process::Output {
        let mut command = Command::new("/bin/bash");
        command
            .env_clear()
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.0.join("path").display()),
            )
            .env("MARKER", self.0.join("executed"))
            .arg(self.0.join("bin/devlish"))
            .args(args);
        if let Some(value) = profile {
            command.env("DEVLISH_VERIFIED_PROFILE", value);
        }
        command.output().unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn verified_requests_never_select_release_debug_or_path_candidates() {
    for candidate in [
        "crates/devlish_core/target/release/devlish-core",
        "crates/devlish_core/target/debug/devlish-core",
        "path/devlish-core",
    ] {
        let f = Fixture::new();
        f.candidate(candidate, "untrusted candidate");
        for (args, profile) in [
            (vec!["run-verified"], None),
            (vec!["serve-verified"], None),
            (vec!["run", "private.dvl"], Some("/operator/profile.json")),
            (vec!["--help"], Some("")),
            (vec![], Some("/operator/profile.json")),
        ] {
            let result = f.run(&args, profile);
            assert!(!result.status.success());
            assert!(result.stdout.is_empty());
            assert!(String::from_utf8_lossy(&result.stderr).contains("protected native executable"));
            assert!(!f.0.join("executed").exists());
        }
    }
}
#[test]
fn development_resolution_preserves_arguments_exit_status_and_priority() {
    let f = Fixture::new();
    for (candidate, label) in [
        ("path/devlish-core", "path"),
        ("crates/devlish_core/target/debug/devlish-core", "debug"),
        ("crates/devlish_core/target/release/devlish-core", "release"),
    ] {
        f.candidate(candidate, label);
        let result = f.run(&["run", "file with spaces.dvl", "literal;$(data)"], None);
        assert_eq!(result.status.code(), Some(23));
        assert_eq!(
            String::from_utf8(result.stdout).unwrap(),
            "run\nfile with spaces.dvl\nliteral;$(data)\n"
        );
        assert_eq!(
            fs::read_to_string(f.0.join("executed"))
                .unwrap()
                .lines()
                .last(),
            Some(label)
        );
    }
}
