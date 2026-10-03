use devlish_audit::{hex, sha256, signing_message, verify, Purpose, MAX_METADATA_BYTES};
use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use serde_json::{json, Value};

struct Fixture {
    key: Ed25519KeyPair,
    trust: Value,
    bytes: Vec<u8>,
    envelope: Value,
}
impl Fixture {
    fn new() -> Self {
        let key = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let key = Ed25519KeyPair::from_pkcs8(key.as_ref()).unwrap();
        let bytes = br#"{"claimed_execution":true,"policy_enforced":true}"#.to_vec();
        let trust = json!({"format":"devlish-audit-trust","format_version":1,"keys":[{
            "id":"test-release","public_key_hex":hex(key.public_key().as_ref()),
            "purposes":["release-artifact","audit-evidence"],"revoked":false}]});
        let signature = key.sign(&signing_message(Purpose::AuditEvidence, &bytes));
        let envelope = json!({"format":"devlish-detached-signature","format_version":1,
            "algorithm":"ed25519","key_id":"test-release","purpose":"audit-evidence",
            "signature_hex":hex(signature.as_ref())});
        Self {
            key,
            trust,
            bytes,
            envelope,
        }
    }
    fn check(&self) -> Result<devlish_audit::Verification, String> {
        verify(
            &self.bytes,
            &serde_json::to_vec(&self.envelope).unwrap(),
            &serde_json::to_vec(&self.trust).unwrap(),
            Purpose::AuditEvidence,
        )
    }
}
#[test]
fn valid_signature_never_promotes_fabricated_claims_to_execution_assurance() {
    let f = Fixture::new();
    let report = f.check().unwrap();
    assert!(report.signature_verified);
    assert_eq!(report.artifact_sha256, sha256(&f.bytes));
    assert!(!report.execution_origin_verified);
    assert!(!report.policy_enforcement_verified);
    assert!(!report.log_chain_verified);
    assert!(!report.external_anchor_verified);
    assert!(!report.replay_verified);
    assert_eq!(
        serde_json::to_value(&report).unwrap(),
        serde_json::to_value(f.check().unwrap()).unwrap()
    );
}
#[test]
fn changed_bytes_and_replacement_signer_fail() {
    let mut f = Fixture::new();
    f.bytes.push(b' ');
    assert!(f
        .check()
        .unwrap_err()
        .contains("signature verification failed"));
    let other = Fixture::new();
    f.bytes.pop();
    f.envelope["signature_hex"] = other.envelope["signature_hex"].clone();
    assert!(f.check().is_err());
}
#[test]
fn unknown_revoked_and_unauthorized_keys_fail() {
    let mut f = Fixture::new();
    f.envelope["key_id"] = json!("unknown");
    assert!(f.check().unwrap_err().contains("not trusted"));
    f.envelope["key_id"] = json!("test-release");
    f.trust["keys"][0]["revoked"] = json!(true);
    assert!(f.check().unwrap_err().contains("revoked"));
    f.trust["keys"][0]["revoked"] = json!(false);
    f.trust["keys"][0]["purposes"] = json!(["release-artifact"]);
    assert!(f.check().unwrap_err().contains("not authorized"));
}

#[test]
fn signatures_cannot_be_reused_across_purposes() {
    let mut f = Fixture::new();
    // Relabeling a release signature as evidence does not change signed context.
    f.envelope["signature_hex"] = json!(hex(f
        .key
        .sign(&signing_message(Purpose::ReleaseArtifact, &f.bytes))
        .as_ref()));
    assert!(f.check().is_err());
    f.envelope["purpose"] = json!("release-artifact");
    assert!(f.check().unwrap_err().contains("purpose"));
}
#[test]
fn trust_cannot_be_injected_in_an_envelope() {
    let mut f = Fixture::new();
    f.envelope["public_key_hex"] = f.trust["keys"][0]["public_key_hex"].clone();
    assert!(f.check().unwrap_err().contains("unknown field"));
}
#[test]
fn ambiguous_trust_and_malformed_metadata_fail_closed() {
    let mut f = Fixture::new();
    let duplicate = f.trust["keys"][0].clone();
    f.trust["keys"].as_array_mut().unwrap().push(duplicate);
    assert!(f.check().unwrap_err().contains("duplicate"));
    f.trust["keys"].as_array_mut().unwrap().pop();
    f.envelope["algorithm"] = json!("none");
    assert!(f.check().is_err());
    f.envelope["algorithm"] = json!("ed25519");
    f.envelope["signature_hex"] = json!("é".repeat(64));
    assert!(f.check().is_err());
    let mut trust = serde_json::to_vec(&f.trust).unwrap();
    trust.resize(MAX_METADATA_BYTES as usize + 1, b' ');
    assert!(verify(&f.bytes, b"{}", &trust, Purpose::AuditEvidence)
        .unwrap_err()
        .contains("size"));
}
#[test]
fn duplicate_json_fields_are_rejected() {
    let f = Fixture::new();
    let envelope = serde_json::to_string(&f.envelope).unwrap();
    let envelope = envelope.replacen('{', "{\"key_id\":\"other\",", 1);
    assert!(verify(
        &f.bytes,
        envelope.as_bytes(),
        &serde_json::to_vec(&f.trust).unwrap(),
        Purpose::AuditEvidence
    )
    .unwrap_err()
    .contains("duplicate field"));
}
#[test]
fn cli_verifies_without_running_candidate_and_rejects_bad_arguments() {
    use std::{fs, process::Command};
    let f = Fixture::new();
    let dir = std::env::temp_dir().join(format!(
        "devlish-audit-{}-{}",
        std::process::id(),
        hex(f.key.public_key().as_ref())
    ));
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("candidate"), &f.bytes).unwrap();
    fs::write(
        dir.join("signature.json"),
        serde_json::to_vec(&f.envelope).unwrap(),
    )
    .unwrap();
    fs::write(
        dir.join("trust.json"),
        serde_json::to_vec(&f.trust).unwrap(),
    )
    .unwrap();
    let args = [
        "verify",
        "candidate",
        "--signature",
        "signature.json",
        "--trust",
        "trust.json",
        "--purpose",
        "audit-evidence",
    ];
    let output = Command::new(env!("CARGO_BIN_EXE_devlish-audit"))
        .current_dir(&dir)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["signature_verified"], true);
    assert_eq!(report["execution_origin_verified"], false);
    fs::write(dir.join("candidate"), b"changed").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_devlish-audit"))
        .current_dir(&dir)
        .args(args)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["signature_verified"], false);
    let output = Command::new(env!("CARGO_BIN_EXE_devlish-audit"))
        .args([
            "verify",
            "candidate",
            "--trust",
            "a",
            "--trust",
            "b",
            "--purpose",
            "audit-evidence",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["error"]
            .as_str()
            .unwrap()
            .contains("Usage")
    );
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn file_reader_rejects_directories_and_oversized_files() {
    let dir = std::env::temp_dir();
    assert!(devlish_audit::read_bounded(&dir, 1024).is_err());
    let executable = std::env::current_exe().unwrap();
    assert!(devlish_audit::read_bounded(&executable, 1).is_err());
}
