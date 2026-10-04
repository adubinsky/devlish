#![cfg(all(feature = "native", unix))]
use devlish_audit::{
    hex,
    issuance::ReceiptKey,
    receipt::{verify_receipt, ExpectedReceipt, ReceiptKind},
    release::{verify_release, ReleaseVerification},
    sha256, signing_message, Purpose,
};
use devlish_core::{
    compile_source_to_json,
    receipt_issuer::{ReceiptIssuer, ReceiptSigningBackend},
    CompileOptions,
};
use devlish_vm::policy::PolicyRecorder;
use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}
fn key() -> Ed25519KeyPair {
    let data = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    Ed25519KeyPair::from_pkcs8(data.as_ref()).unwrap()
}
fn sign(key: &Ed25519KeyPair, id: &str, purpose: Purpose, data: &[u8]) -> Vec<u8> {
    bytes(
        &json!({"format":"devlish-detached-signature","format_version":1,"algorithm":"ed25519","key_id":id,"purpose":purpose,
        "signature_hex":hex(key.sign(&signing_message(purpose,data)).as_ref())}),
    )
}
struct Backend {
    key: Ed25519KeyPair,
    calls: usize,
    available: bool,
    mode: &'static str,
}
impl ReceiptSigningBackend for Backend {
    fn available(&self) -> bool {
        self.available
    }
    fn sign(&mut self, id: &str, message: &[u8]) -> Result<[u8; 64], String> {
        self.calls += 1;
        assert_eq!(id, "receipt");
        match self.mode {
            "uncertain" => Err("SYNTHETIC_PRIVATE_BACKEND_DIAGNOSTIC".into()),
            "invalid" => Ok([0; 64]),
            _ => Ok(self.key.sign(message).as_ref().try_into().unwrap()),
        }
    }
}
#[derive(Default)]
struct Records {
    values: Vec<Value>,
    calls: usize,
    fail_at: Option<usize>,
}
impl PolicyRecorder for Records {
    fn record(&mut self, value: &Value) -> Result<(), String> {
        let index = self.calls;
        self.calls += 1;
        if self.fail_at == Some(index) {
            return Err("SYNTHETIC_PRIVATE_RECORDER_DIAGNOSTIC".into());
        }
        self.values.push(value.clone());
        Ok(())
    }
}
struct Fixture {
    directory: PathBuf,
    release: ReleaseVerification,
    log: Vec<u8>,
    trust: Value,
    request: Value,
    backend: Backend,
    policy: Vec<u8>,
    release_digest: String,
}
impl Fixture {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "devlish-issuer-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        let release_key = key();
        let receipt_key = key();
        let trust = json!({"format":"devlish-audit-trust","format_version":1,"keys":[
            {"id":"release","public_key_hex":hex(release_key.public_key().as_ref()),"purposes":["release-manifest"],"revoked":false},
            {"id":"receipt","public_key_hex":hex(receipt_key.public_key().as_ref()),"purposes":["audit-receipt"],"revoked":false}]});
        let mut artifacts: BTreeMap<String, Vec<u8>> = [
            "runtime",
            "compiler",
            "tool-catalog",
            "permissions",
            "containment",
            "source-closure",
            "build-attestation",
        ]
        .into_iter()
        .map(|id| (id.into(), id.as_bytes().to_vec()))
        .collect();
        artifacts.insert(
            "policy".into(),
            bytes(&json!({"synthetic":"execution-policy"})),
        );
        artifacts.insert("program".into(), bytes(&json!({"synthetic":"program"})));
        let manifest = json!({"format":"devlish-release-manifest","format_version":1,"release_id":"synthetic","environment":"test","target":"test-target","sequence":1,
            "valid_from":100,"valid_until":200,"repository":"synthetic-repo","commit":"synthetic-commit","workflow":"release","policy_id":"synthetic-policy","policy_version":"1",
            "artifacts":artifacts.iter().map(|(id,data)|json!({"id":id,"role":id,"sha256":sha256(data)})).collect::<Vec<_>>()});
        let requirements = json!({"format":"devlish-release-requirements","format_version":1,"environment":"test","target":"test-target","repository":"synthetic-repo","commit":"synthetic-commit",
            "workflow":"release","policy_id":"synthetic-policy","policy_version":"1","minimum_sequence":1,"evaluated_at":150,"revocations_valid_from":100,"revocations_valid_until":200,
            "revoked_manifest_sha256":[],"authorized_release_keys":["release"]});
        let release_digest = sha256(&bytes(&manifest));
        let release = verify_release(
            &bytes(&manifest),
            &sign(
                &release_key,
                "release",
                Purpose::ReleaseManifest,
                &bytes(&manifest),
            ),
            &bytes(&trust),
            &bytes(&requirements),
            |id| Ok(artifacts[id].clone()),
        )
        .unwrap();
        let canonical = |id: &str| {
            sha256(
                &serde_json::to_vec_pretty(
                    &serde_json::from_slice::<Value>(&artifacts[id]).unwrap(),
                )
                .unwrap(),
            )
        };
        let start = json!({"type":"policy_run_started","format_version":3,"session_id":"session","runtime_file_sha256":sha256(&artifacts["runtime"]),"program_sha256":canonical("program"),"policy":{"artifact_sha256":canonical("policy")}});
        let mut log = Vec::new();
        let mut previous = String::new();
        for (sequence, record) in [
            start,
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
        let receipt = release
            .prepare_receipt(&log, "session", ReceiptKind::Terminal)
            .unwrap();
        let request = json!({"tenant_id":"tenant","session_id":"session","release_sha256":release_digest,"receipt_sha256":sha256(&receipt),"key_id":"receipt","purpose":"audit-receipt","kind":"terminal"});
        let policy = compile_source_to_json(
            include_str!("../../../examples/receipt_authority/authorize.dvl"),
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .unwrap()
        .into_bytes();
        Self {
            directory,
            release,
            log,
            trust,
            request,
            backend: Backend {
                key: receipt_key,
                calls: 0,
                available: true,
                mode: "valid",
            },
            policy,
            release_digest,
        }
    }
    fn issuer(&self) -> ReceiptIssuer {
        ReceiptIssuer::new(
            &self.policy,
            &sha256(&self.policy),
            self.directory.clone(),
            "tenant".into(),
            ReceiptKey::from_trust("receipt", &bytes(&self.trust)).unwrap(),
        )
        .unwrap()
    }
    fn files(&self) -> usize {
        fs::read_dir(&self.directory).unwrap().count()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.directory).unwrap();
    }
}

#[test]
fn devlish_approvals_guard_backend_and_result_verifies_independently() {
    // Arrange: only the test backend owns an ephemeral signing key.
    let mut f = Fixture::new();
    let issuer = f.issuer();
    let mut records = Records::default();
    // Act: the real orchestration performs preflight, reservation and final approval.
    let result = issuer
        .issue(
            &f.release,
            "session",
            &f.log,
            &f.request,
            &bytes(&f.trust),
            &mut f.backend,
            &mut records,
        )
        .unwrap();
    // Assert: returned bytes verify independently; replay cannot sign again.
    let report = verify_receipt(
        &f.log,
        &result.receipt,
        &result.signature,
        &bytes(&f.trust),
        ExpectedReceipt {
            sha256: &sha256(&result.receipt),
            session_id: "session",
            release_sha256: &f.release_digest,
        },
    )
    .unwrap();
    assert!(!report.execution_origin_verified && !report.policy_enforcement_verified);
    assert_eq!(f.backend.calls, 1);
    assert_eq!(f.files(), 2);
    assert_eq!(records.values[0]["phase"], "prepare_audit_receipt");
    assert_eq!(records.values[1]["phase"], "issue_audit_receipt");
    assert_eq!(records.values[2]["status"], "completed");
    assert!(issuer
        .issue(
            &f.release,
            "session",
            &f.log,
            &f.request,
            &bytes(&f.trust),
            &mut f.backend,
            &mut records
        )
        .is_err());
    assert_eq!(f.backend.calls, 1);
}

#[test]
fn untrusted_request_denials_neither_sign_nor_consume_the_slot() {
    let mut f = Fixture::new();
    let issuer = f.issuer();
    for field in [
        "tenant_id",
        "session_id",
        "release_sha256",
        "receipt_sha256",
        "key_id",
        "purpose",
        "kind",
        "authority",
    ] {
        let mut request = f.request.clone();
        request[field] = json!("SYNTHETIC_PRIVATE_CALLER_DATA");
        let mut records = Records::default();
        let error = issuer
            .issue(
                &f.release,
                "session",
                &f.log,
                &request,
                &bytes(&f.trust),
                &mut f.backend,
                &mut records,
            )
            .err()
            .unwrap();
        assert_eq!(f.backend.calls, 0);
        assert_eq!(f.files(), 0);
        assert_eq!(records.values[0]["allow"], false);
        assert!(!error.contains("SYNTHETIC_PRIVATE"));
        assert!(!serde_json::to_string(&records.values)
            .unwrap()
            .contains("SYNTHETIC_PRIVATE"));
    }
    f.backend.available = false;
    assert!(issuer
        .issue(
            &f.release,
            "session",
            &f.log,
            &f.request,
            &bytes(&f.trust),
            &mut f.backend,
            &mut Records::default()
        )
        .is_err());
    assert_eq!(f.backend.calls, 0);
    assert_eq!(f.files(), 0);
}

#[test]
fn policy_pin_and_current_key_trust_are_checked_before_backend() {
    let mut f = Fixture::new();
    assert!(ReceiptIssuer::new(
        &f.policy,
        &"0".repeat(64),
        f.directory.clone(),
        "tenant".into(),
        ReceiptKey::from_trust("receipt", &bytes(&f.trust)).unwrap()
    )
    .is_err());
    let issuer = f.issuer();
    for mode in ["revoked", "wrong-purpose", "rebound-id", "duplicate"] {
        let mut trust = f.trust.clone();
        match mode {
            "revoked" => trust["keys"][1]["revoked"] = json!(true),
            "wrong-purpose" => trust["keys"][1]["purposes"] = json!(["release-artifact"]),
            "rebound-id" => {
                trust["keys"][1]["public_key_hex"] = json!(hex(key().public_key().as_ref()))
            }
            _ => {
                let duplicate = trust["keys"][1].clone();
                trust["keys"].as_array_mut().unwrap().push(duplicate);
            }
        }
        assert!(
            issuer
                .issue(
                    &f.release,
                    "session",
                    &f.log,
                    &f.request,
                    &bytes(&trust),
                    &mut f.backend,
                    &mut Records::default()
                )
                .is_err(),
            "{mode}"
        );
        assert_eq!(f.backend.calls, 0);
        assert_eq!(f.files(), 0);
    }
}

#[test]
fn recorder_failures_block_signing_or_preserve_completed_evidence() {
    for phase in 0..3 {
        let mut f = Fixture::new();
        let issuer = f.issuer();
        let mut records = Records {
            fail_at: Some(phase),
            ..Default::default()
        };
        let error = issuer
            .issue(
                &f.release,
                "session",
                &f.log,
                &f.request,
                &bytes(&f.trust),
                &mut f.backend,
                &mut records,
            )
            .err()
            .unwrap();
        assert!(!error.contains("SYNTHETIC_PRIVATE"));
        assert_eq!(f.backend.calls, usize::from(phase == 2));
        assert_eq!(f.files(), phase);
        if phase > 0 {
            assert!(issuer
                .issue(
                    &f.release,
                    "session",
                    &f.log,
                    &f.request,
                    &bytes(&f.trust),
                    &mut f.backend,
                    &mut Records::default()
                )
                .is_err());
        }
        assert_eq!(f.backend.calls, usize::from(phase == 2));
    }
}

#[test]
fn backend_uncertainty_or_invalid_signature_never_reopens_issuance() {
    for mode in ["uncertain", "invalid"] {
        let mut f = Fixture::new();
        let issuer = f.issuer();
        f.backend.mode = mode;
        let mut records = Records::default();
        let error = issuer
            .issue(
                &f.release,
                "session",
                &f.log,
                &f.request,
                &bytes(&f.trust),
                &mut f.backend,
                &mut records,
            )
            .err()
            .unwrap();
        assert_eq!(f.backend.calls, 1);
        assert_eq!(f.files(), 1);
        assert!(!error.contains("SYNTHETIC_PRIVATE"));
        assert!(!serde_json::to_string(&records.values)
            .unwrap()
            .contains("SYNTHETIC_PRIVATE"));
        f.backend.mode = "valid";
        assert!(issuer
            .issue(
                &f.release,
                "session",
                &f.log,
                &f.request,
                &bytes(&f.trust),
                &mut f.backend,
                &mut Records::default()
            )
            .is_err());
        assert_eq!(f.backend.calls, 1);
    }
}

#[test]
fn final_devlish_denial_after_preflight_never_calls_backend() {
    // Arrange: operator policy accepts preflight but explicitly vetoes final signing.
    let mut f = Fixture::new();
    let source = include_str!("../../../examples/receipt_authority/authorize.dvl").replace(
        "# This policy never authenticates JSON.",
        "If effect equals \"issue_audit_receipt\":\n  Respond with record with false as allow and \"Operator final veto.\" as reason\n\n# This policy never authenticates JSON.",
    );
    f.policy = compile_source_to_json(
        &source,
        CompileOptions {
            source_path: None,
            search_paths: vec![],
        },
    )
    .unwrap()
    .into_bytes();
    let issuer = f.issuer();
    let mut records = Records::default();
    // Act.
    assert!(issuer
        .issue(
            &f.release,
            "session",
            &f.log,
            &f.request,
            &bytes(&f.trust),
            &mut f.backend,
            &mut records
        )
        .is_err());
    // Assert: successful preflight cannot substitute for final authorization.
    assert_eq!(records.values[0]["allow"], true);
    assert_eq!(records.values[1]["allow"], false);
    assert_eq!(f.backend.calls, 0);
    assert_eq!(f.files(), 1);
}
