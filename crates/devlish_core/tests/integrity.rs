use devlish_core::{integrity::read_verified, sha256_hex};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;

static NEXT_FIXTURE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "devlish-integrity-{}-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for entry in std::fs::read_dir(&self.0).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }
        std::fs::remove_dir(&self.0).unwrap();
    }
}

#[test]
fn exact_bytes_detect_replaced_tool_under_the_same_name() {
    // Arrange: stand-in bytes; no external executable is launched.
    let fixture = Fixture::new();
    let tool = fixture.0.join("grep");
    let approved = b"approved tool version";
    std::fs::write(&tool, approved).unwrap();
    let expected = sha256_hex(approved);
    assert_eq!(read_verified(&tool, &expected).unwrap(), approved);
    // Act.
    std::fs::write(&tool, b"replacement tool version").unwrap();
    // Assert.
    assert!(read_verified(&tool, &expected)
        .unwrap_err()
        .contains("mismatch"));
    assert!(read_verified(&tool, "not a digest").is_err());
    assert!(read_verified(&fixture.0, &expected).is_err());
}

#[test]
fn compiled_policy_pin_is_enforced_before_execution_and_logged() {
    // Arrange.
    let fixture = Fixture::new();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = env!("CARGO_BIN_EXE_devlish-core");
    let policy = fixture.0.join("policy.dvlc.json");
    let compile = Command::new(binary)
        .current_dir(&root)
        .args(["compile", "examples/effect_policy/policy.dvl", "--output"])
        .arg(&policy)
        .output()
        .unwrap();
    assert!(compile.status.success());
    let bytes = std::fs::read(&policy).unwrap();
    let expected = sha256_hex(&bytes);
    let run = |log: &str| {
        Command::new(binary)
            .current_dir(&root)
            .args([
                "run",
                "examples/effect_policy/allowed.dvl",
                "--quiet",
                "--policy",
            ])
            .arg(&policy)
            .args(["--policy-sha256", &expected, "--policy-log"])
            .arg(fixture.0.join(log))
            .output()
            .unwrap()
    };
    // Act.
    let allowed = run("allowed.jsonl");
    let mut modified = bytes.clone();
    modified.push(b' '); // same JSON meaning, different file identity
    std::fs::write(&policy, modified).unwrap();
    let denied = run("denied.jsonl");
    // Assert.
    assert!(
        allowed.status.success(),
        "{}",
        String::from_utf8_lossy(&allowed.stderr)
    );
    assert!(!denied.status.success());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("SHA-256 mismatch"));
    assert!(!fixture.0.join("denied.jsonl").exists());
    let text = std::fs::read_to_string(fixture.0.join("allowed.jsonl")).unwrap();
    let start: Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
    assert_eq!(start["record"]["policy"]["verified_file_sha256"], expected);
    let package: Value = serde_json::from_slice(&bytes).unwrap();
    let canonical = sha256_hex(&serde_json::to_vec_pretty(&package).unwrap());
    assert_eq!(start["record"]["policy"]["artifact_sha256"], canonical);
    // Artifact verification distinguishes matching bytes from a signature check.
    let verification = Command::new(binary)
        .args(["artifact", "verify"])
        .arg(&policy)
        .args(["--sha256", &expected])
        .output()
        .unwrap();
    assert!(!verification.status.success());
    std::fs::write(&policy, bytes).unwrap();
    let verification = Command::new(binary)
        .args(["artifact", "verify"])
        .arg(&policy)
        .args(["--sha256", &expected])
        .output()
        .unwrap();
    assert!(verification.status.success());
    let report: Value = serde_json::from_slice(&verification.stdout).unwrap();
    assert_eq!(report["digest_matches"], true);
    assert_eq!(report["signature_verified"], false);
}

#[test]
fn policy_hash_matches_governance_serialization() {
    // Arrange.
    let source = "Rule:\n  id: identity.test\n  version: 1.0.0\nRespond with record with false as allow and \"Denied\" as reason";
    let package: Value = serde_json::from_str(
        &devlish_core::compile_source_to_json(
            source,
            devlish_core::CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .unwrap(),
    )
    .unwrap();
    let expected = sha256_hex(&serde_json::to_vec_pretty(&package).unwrap());
    // Act.
    let policy = devlish_vm::policy::EffectPolicy::new(package).unwrap();
    // Assert.
    assert_eq!(policy.identity()["artifact_sha256"], json!(expected));
}
