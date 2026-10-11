use devlish_audit::{
    hex,
    issuance::{terminal_operation_id, verify_issuance, IssuanceVerification},
    sha256, signing_message, Purpose,
};
use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use serde_json::{json, Value};

fn bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}
struct Fixture {
    log: Vec<u8>,
    pending: Value,
    completed: Value,
    trust: Value,
    expected: Value,
}
impl Fixture {
    fn new(kind: &str) -> Self {
        let secret = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let key = Ed25519KeyPair::from_pkcs8(secret.as_ref()).unwrap();
        let mut log = Vec::new();
        let mut previous = String::new();
        for (sequence, record) in [
            json!({"type":"policy_run_started","format_version":3,"session_id":"session"}),
            json!({"type":"policy_run_finished","success":true,"paused":false}),
        ]
        .into_iter()
        .enumerate()
        {
            let mut envelope =
                json!({"sequence":sequence,"previous_sha256":previous,"record":record});
            previous = sha256(&bytes(&envelope));
            envelope["record_sha256"] = json!(previous);
            log.extend(bytes(&envelope));
            log.push(b'\n');
        }
        let receipt = bytes(&json!({"format":"devlish-audit-receipt","format_version":1,
            "kind":kind,"session_id":"session","release_manifest_sha256":"a".repeat(64),
            "log_file_sha256":sha256(&log),"log_head_sha256":previous,"record_count":2}));
        let signature = json!({"format":"devlish-detached-signature","format_version":1,
            "algorithm":"ed25519","key_id":"key","purpose":"audit-receipt",
            "signature_hex":hex(key.sign(&signing_message(Purpose::AuditReceipt,&receipt)).as_ref())});
        let fingerprint = sha256(key.public_key().as_ref());
        let pending = json!({"format":"devlish-receipt-reservation","format_version":1,
            "tenant_id":"tenant","session_id":"session","key_id":"key",
            "key_public_sha256":fingerprint,"receipt_sha256":sha256(&receipt),
            "receipt":String::from_utf8(receipt.clone()).unwrap()});
        let completed = json!({"format":"devlish-receipt-issuance","format_version":1,
            "operation_id":terminal_operation_id("tenant","session").unwrap(),
            "receipt_sha256":sha256(&receipt),"key_id":"key","signature":signature,
            "signature_verification":{"signature_verified":true,"execution_origin_verified":true},
            "signature_verified":true,"signer_authorization_verified":true,
            "execution_origin_verified":true,"policy_enforcement_verified":true});
        let trust = json!({"format":"devlish-audit-trust","format_version":1,"keys":[
            {"id":"key","public_key_hex":hex(key.public_key().as_ref()),"purposes":["audit-receipt"],"revoked":false}]});
        let expected = json!({"tenant_id":"tenant","session_id":"session","key_id":"key",
            "key_public_sha256":fingerprint,"receipt_sha256":sha256(&receipt),
            "release_manifest_sha256":"a".repeat(64)});
        Self {
            log,
            pending,
            completed,
            trust,
            expected,
        }
    }
    fn check(&self) -> Result<IssuanceVerification, String> {
        verify_issuance(
            &self.log,
            &bytes(&self.pending),
            &bytes(&self.completed),
            &bytes(&self.trust),
            &bytes(&self.expected),
        )
    }
}
#[test]
fn repeats_fresh_verification_without_promoting_saved_claims() {
    // Arrange: candidate completion deliberately claims stronger assurance.
    let f = Fixture::new("terminal");
    // Act: independent checking repeats from original bytes and fresh trust.
    let first = f.check().unwrap();
    let second = f.check().unwrap();
    // Assert: neither saved verification nor the unsigned tenant is authenticated.
    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&second).unwrap()
    );
    assert!(first.issuance_records_consistent && first.receipt.terminal_receipt_verified);
    assert!(!first.stored_verification_trusted && !first.tenant_binding_authenticated);
    assert!(!first.signer_authorization_verified && !first.issuer_history_replayed);
    assert!(!first.execution_origin_verified && !first.policy_enforcement_verified);
    assert!(!first.receipt.release_manifest_verified);
}
#[test]
fn saved_success_cannot_bypass_revocation_signature_or_log_checks() {
    for mode in [
        "revoked",
        "wrong-purpose",
        "invalid-signature",
        "alternate-key-id",
        "rebound-pin",
        "truncated-log",
        "altered-receipt",
        "checkpoint",
    ] {
        let mut f = Fixture::new(if mode == "checkpoint" {
            "checkpoint"
        } else {
            "terminal"
        });
        match mode {
            "revoked" => f.trust["keys"][0]["revoked"] = json!(true),
            "wrong-purpose" => f.trust["keys"][0]["purposes"] = json!(["release-artifact"]),
            "invalid-signature" => {
                f.completed["signature"]["signature_hex"] = json!("0".repeat(128))
            }
            "alternate-key-id" => {
                f.trust["keys"][0]["id"] = json!("other");
                f.completed["signature"]["key_id"] = json!("other");
            }
            "rebound-pin" => {
                f.pending["key_public_sha256"] = json!("b".repeat(64));
                f.expected["key_public_sha256"] = json!("b".repeat(64));
            }
            "truncated-log" => f.log.truncate(f.log.len() / 2),
            "altered-receipt" => {
                let altered = format!("{} ", f.pending["receipt"].as_str().unwrap());
                let digest = sha256(altered.as_bytes());
                f.pending["receipt"] = json!(altered);
                f.pending["receipt_sha256"] = json!(digest);
                f.completed["receipt_sha256"] = json!(digest);
            }
            _ => {}
        }
        assert!(f.check().is_err(), "{mode}");
    }
}
#[test]
fn mismatched_metadata_and_independent_expectations_fail_closed() {
    let mut f = Fixture::new("terminal");
    for field in [
        "tenant_id",
        "session_id",
        "key_id",
        "key_public_sha256",
        "receipt_sha256",
        "release_manifest_sha256",
    ] {
        let original = f.expected[field].clone();
        f.expected[field] = json!(if field.ends_with("sha256") {
            "b".repeat(64)
        } else {
            "other".into()
        });
        assert!(f.check().is_err(), "expectation {field}");
        f.expected[field] = original;
    }
    for field in [
        "tenant_id",
        "session_id",
        "key_id",
        "key_public_sha256",
        "receipt_sha256",
        "format",
    ] {
        let original = f.pending[field].clone();
        f.pending[field] = json!("changed");
        assert!(f.check().is_err(), "pending {field}");
        f.pending[field] = original;
    }
    for field in ["operation_id", "key_id", "receipt_sha256", "format"] {
        let original = f.completed[field].clone();
        f.completed[field] = json!("changed");
        assert!(f.check().is_err(), "completed {field}");
        f.completed[field] = original;
    }
    f.completed["unknown"] = json!(true);
    assert!(f.check().is_err());
    f.completed.as_object_mut().unwrap().remove("unknown");
    let duplicate = format!(
        "{{\"key_id\":\"key\",{}",
        &String::from_utf8(bytes(&f.pending)).unwrap()[1..]
    );
    assert!(verify_issuance(
        &f.log,
        duplicate.as_bytes(),
        &bytes(&f.completed),
        &bytes(&f.trust),
        &bytes(&f.expected)
    )
    .is_err());
    assert!(verify_issuance(
        &f.log,
        &bytes(&f.pending),
        &[],
        &bytes(&f.trust),
        &bytes(&f.expected)
    )
    .is_err());
    assert!(verify_issuance(
        &f.log,
        &vec![b' '; 65_537],
        &bytes(&f.completed),
        &bytes(&f.trust),
        &bytes(&f.expected)
    )
    .is_err());
}
#[test]
fn cli_requires_independent_expectations_and_never_writes_candidate_records() {
    // Arrange: unique, synthetic inputs, no production key material.
    let f = Fixture::new("terminal");
    let dir = std::env::temp_dir().join(format!(
        "devlish-issuance-cli-{}-{}",
        std::process::id(),
        sha256(&bytes(&f.trust))
    ));
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("log"), &f.log).unwrap();
    for (name, value) in [
        ("pending", &f.pending),
        ("completed", &f.completed),
        ("trust", &f.trust),
        ("expected", &f.expected),
    ] {
        std::fs::write(dir.join(name), bytes(value)).unwrap();
    }
    let run = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_devlish-audit"))
            .current_dir(&dir)
            .args(args)
            .output()
            .unwrap()
    };
    let args = [
        "verify-issuance",
        "log",
        "--pending",
        "pending",
        "--completed",
        "completed",
        "--trust",
        "trust",
        "--expectations",
        "expected",
    ];
    // Act and assert: deterministic success, missing/duplicate options fail.
    let first = run(&args);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stdout)
    );
    assert_eq!(first.stdout, run(&args).stdout);
    assert!(!run(&args[..8]).status.success());
    let mut duplicate = args;
    duplicate[8] = "--trust";
    assert!(!run(&duplicate).status.success());
    assert_eq!(
        std::fs::read(dir.join("pending")).unwrap(),
        bytes(&f.pending)
    );
    assert_eq!(
        std::fs::read(dir.join("completed")).unwrap(),
        bytes(&f.completed)
    );
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 5);
    std::fs::remove_dir_all(dir).unwrap();
}
