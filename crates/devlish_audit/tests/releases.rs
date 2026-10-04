use devlish_audit::{hex, release::verify_release, sha256, signing_message, Purpose};
use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use serde_json::{json, Value};

struct Fixture {
    key: Ed25519KeyPair,
    manifest: Value,
    requirements: Value,
    trust: Value,
}
fn bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}
impl Fixture {
    fn new() -> Self {
        let key = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let key = Ed25519KeyPair::from_pkcs8(key.as_ref()).unwrap();
        let artifacts: Vec<_> = [
            "runtime",
            "compiler",
            "policy",
            "tool-catalog",
            "permissions",
            "containment",
            "source-closure",
            "build-attestation",
            "tool",
        ]
        .iter()
        .map(|role| json!({"id": role, "role":role,"sha256":sha256(role.as_bytes())}))
        .collect();
        let manifest = json!({"format":"devlish-release-manifest","format_version":1,
            "release_id":"synthetic-v2", "environment":"test", "target":"test-target", "sequence":2,
            "valid_from":100,"valid_until":200,"repository":"synthetic-repo","commit":"synthetic-commit",
            "workflow":"release", "policy_id":"nppi", "policy_version":"1", "artifacts":artifacts});
        let requirements = json!({"format":"devlish-release-requirements","format_version":1,
            "environment":"test","target":"test-target","repository":"synthetic-repo","commit":"synthetic-commit",
            "workflow":"release","policy_id":"nppi","policy_version":"1","minimum_sequence":2,
            "evaluated_at":150,"revocations_valid_from":100,"revocations_valid_until":200,
            "revoked_manifest_sha256":[],"authorized_release_keys":["release"]});
        let trust = json!({"format":"devlish-audit-trust","format_version":1,"keys":[{
            "id":"release","public_key_hex":hex(key.public_key().as_ref()),"purposes":["release-manifest"],"revoked":false}]});
        Self {
            key,
            manifest,
            requirements,
            trust,
        }
    }
    fn signature(&self) -> Vec<u8> {
        bytes(
            &json!({"format":"devlish-detached-signature","format_version":1,"algorithm":"ed25519",
            "key_id":"release","purpose":"release-manifest",
            "signature_hex":hex(self.key.sign(&signing_message(Purpose::ReleaseManifest, &bytes(&self.manifest))).as_ref())}),
        )
    }
    fn check(&self) -> Result<devlish_audit::release::ReleaseVerification, String> {
        verify_release(
            &bytes(&self.manifest),
            &self.signature(),
            &bytes(&self.trust),
            &bytes(&self.requirements),
            |id| Ok(id.as_bytes().to_vec()),
        )
    }
}
#[test]
fn repeatable_approval_does_not_claim_build_or_execution_verification() {
    let f = Fixture::new();
    let report = f.check().unwrap();
    assert!(report.release_requirements_verified && report.artifact_snapshots_verified);
    assert!(
        !report.build_provenance_verified
            && !report.execution_origin_verified
            && !report.policy_enforcement_verified
    );
    assert_eq!(
        serde_json::to_value(report).unwrap(),
        serde_json::to_value(f.check().unwrap()).unwrap()
    );
}
#[test]
fn signed_wrong_scope_is_rejected_before_reading_artifacts() {
    for field in [
        "environment",
        "target",
        "repository",
        "commit",
        "workflow",
        "policy_id",
        "policy_version",
    ] {
        let mut f = Fixture::new();
        f.manifest[field] = json!("wrong");
        let err = verify_release(
            &bytes(&f.manifest),
            &f.signature(),
            &bytes(&f.trust),
            &bytes(&f.requirements),
            |_| panic!("must not read artifacts"),
        )
        .unwrap_err();
        assert!(err.contains("scope"), "{field}: {err}");
    }
}
#[test]
fn expiry_future_release_stale_revocations_and_rollback_fail() {
    for (field, value) in [
        ("evaluated_at", 99),
        ("evaluated_at", 200),
        ("minimum_sequence", 3),
        ("revocations_valid_from", 151),
        ("revocations_valid_until", 150),
    ] {
        let mut f = Fixture::new();
        f.requirements[field] = json!(value);
        assert!(f.check().is_err(), "{field}");
    }
    let mut f = Fixture::new();
    f.requirements["evaluated_at"] = json!(100);
    assert!(f.check().is_ok());
    f.manifest["valid_until"] = json!(100);
    assert!(f.check().is_err());
}
#[test]
fn revoked_release_key_and_non_release_authority_fail() {
    let mut f = Fixture::new();
    f.requirements["revoked_manifest_sha256"] = json!([sha256(&bytes(&f.manifest))]);
    assert!(f.check().unwrap_err().contains("revoked"));
    f.requirements["revoked_manifest_sha256"] = json!([]);
    f.trust["keys"][0]["revoked"] = json!(true);
    assert!(f.check().unwrap_err().contains("revoked"));
    f.trust["keys"][0]["revoked"] = json!(false);
    f.requirements["authorized_release_keys"] = json!(["builder"]);
    assert!(f.check().unwrap_err().contains("release authority"));
}
#[test]
fn missing_duplicate_and_invalid_artifacts_fail() {
    let mut f = Fixture::new();
    f.manifest["artifacts"].as_array_mut().unwrap().remove(0);
    assert!(f.check().unwrap_err().contains("missing required"));
    let mut f = Fixture::new();
    f.manifest["artifacts"][1]["id"] = json!("runtime");
    assert!(f.check().unwrap_err().contains("duplicate"));
    f.manifest["artifacts"][1]["id"] = json!("../compiler");
    assert!(f.check().unwrap_err().contains("artifact ID"));
    let mut f = Fixture::new();
    f.manifest["artifacts"][0]["sha256"] = json!("0".repeat(64));
    assert!(f.check().unwrap_err().contains("digest"));
}
#[test]
fn modified_manifest_and_replaced_artifact_fail() {
    let f = Fixture::new();
    let mut altered = bytes(&f.manifest);
    altered.push(b' ');
    assert!(verify_release(
        &altered,
        &f.signature(),
        &bytes(&f.trust),
        &bytes(&f.requirements),
        |_| panic!("unauthenticated reads")
    )
    .is_err());
    assert!(verify_release(
        &bytes(&f.manifest),
        &f.signature(),
        &bytes(&f.trust),
        &bytes(&f.requirements),
        |_| Ok(b"replacement".to_vec())
    )
    .unwrap_err()
    .contains("digest"));
}
#[test]
fn cli_reads_operator_mapping_and_rejects_replaced_tool() {
    use std::{fs, process::Command};
    let f = Fixture::new();
    let dir = std::env::temp_dir().join(format!(
        "devlish-release-{}-{}",
        std::process::id(),
        hex(f.key.public_key().as_ref())
    ));
    fs::create_dir(&dir).unwrap();
    for (name, data) in [
        ("manifest.json", bytes(&f.manifest)),
        ("requirements.json", bytes(&f.requirements)),
        ("trust.json", bytes(&f.trust)),
        ("signature.json", f.signature()),
    ] {
        fs::write(dir.join(name), data).unwrap();
    }
    let mut mapping = Vec::new();
    for artifact in f.manifest["artifacts"].as_array().unwrap() {
        let id = artifact["id"].as_str().unwrap();
        fs::write(dir.join(id), id).unwrap();
        mapping.push(json!({"id":id,"path":id}));
    }
    fs::write(dir.join("artifacts.json"), bytes(&json!(mapping))).unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_devlish-audit"))
            .current_dir(&dir)
            .args([
                "verify-release",
                "manifest.json",
                "--signature",
                "signature.json",
                "--trust",
                "trust.json",
                "--requirements",
                "requirements.json",
                "--artifacts",
                "artifacts.json",
            ])
            .output()
            .unwrap()
    };
    let result = run();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    fs::write(dir.join("tool"), "modified external program").unwrap();
    let result = run();
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stdout).contains("artifact digest or size mismatch: tool")
    );
    fs::remove_dir_all(dir).unwrap();
}

fn seal_report(kind: &str, details: Value) -> Vec<u8> {
    let mut value = json!({"format":"devlish-compliance-report","format_version":1,"kind":kind,"passed":true,"details":details});
    value["report_sha256"] = json!(sha256(&bytes(&value)));
    bytes(&value)
}
#[test]
fn release_binding_checks_reports_receipts_and_recorded_identities() {
    let mut f = Fixture::new();
    let policy_bytes = br#"{"rule":"nppi"}"#;
    let program_bytes = br#"{"rule":"agent"}"#;
    f.manifest["artifacts"][2]["sha256"] = json!(sha256(policy_bytes));
    f.manifest["artifacts"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"program","role":"program","sha256":sha256(program_bytes)}));
    f.trust["keys"][0]["purposes"] = json!(["release-manifest", "audit-receipt"]);
    let release = verify_release(
        &bytes(&f.manifest),
        &f.signature(),
        &bytes(&f.trust),
        &bytes(&f.requirements),
        |id| {
            Ok(match id {
                "policy" => policy_bytes.to_vec(),
                "program" => program_bytes.to_vec(),
                _ => id.as_bytes().to_vec(),
            })
        },
    )
    .unwrap();
    let application = |runtime: &str| {
        seal_report(
            "application",
            json!({"files":[
        {"id":"runtime","role":"runtime","passed":true,"actual_sha256":runtime,"expected_sha256":runtime},
        {"id":"policy","role":"policy","passed":true,"actual_sha256":sha256(policy_bytes),"expected_sha256":sha256(policy_bytes)}]}),
        )
    };
    let policy = seal_report("policy", json!({"policy_file_sha256":sha256(policy_bytes)}));
    let app = application(&sha256(b"runtime"));
    let binding = release.bind_reports(&app, &policy).unwrap();
    assert_eq!(binding["reported_identities_match_release"], true);
    assert_eq!(binding["report_claims_independently_verified"], false);
    assert!(release
        .bind_reports(&application(&sha256(b"other runtime")), &policy)
        .is_err());
    let other_policy = seal_report(
        "policy",
        json!({"policy_file_sha256":sha256(b"other policy")}),
    );
    assert!(release.bind_reports(&app, &other_policy).is_err());
    let canonical = |data: &[u8]| {
        sha256(&serde_json::to_vec_pretty(&serde_json::from_slice::<Value>(data).unwrap()).unwrap())
    };
    let start = json!({"type":"policy_run_started","format_version":3,"runtime_file_sha256":sha256(b"runtime"),
        "policy":{"artifact_sha256":canonical(policy_bytes)},"program_sha256":canonical(program_bytes)});
    let sign_log = |start: Value, manifest_digest: String| {
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
        let receipt = bytes(
            &json!({"format":"devlish-audit-receipt","format_version":1,"kind":"terminal","session_id":"test-session",
            "release_manifest_sha256":manifest_digest,"log_file_sha256":sha256(&log),"log_head_sha256":previous,"record_count":2}),
        );
        let sig = bytes(
            &json!({"format":"devlish-detached-signature","format_version":1,"algorithm":"ed25519","key_id":"release","purpose":"audit-receipt",
            "signature_hex":hex(f.key.sign(&signing_message(Purpose::AuditReceipt,&receipt)).as_ref())}),
        );
        (log, receipt, sig)
    };
    let digest = sha256(&bytes(&f.manifest));
    let (log, receipt, sig) = sign_log(start.clone(), digest.clone());
    let result = release
        .bind_receipt(
            &log,
            &receipt,
            &sig,
            &bytes(&f.trust),
            &sha256(&receipt),
            "test-session",
        )
        .unwrap();
    assert!(release.bind_run_reports(&app, &policy, &log).is_ok());
    let mut wrong_start: Value =
        serde_json::from_slice(log.split(|b| *b == b'\n').next().unwrap()).unwrap();
    wrong_start["record"]["runtime_file_sha256"] = json!(sha256(b"other"));
    assert!(release
        .bind_run_reports(&app, &policy, &bytes(&wrong_start))
        .is_err());
    assert!(result.release_manifest_verified);
    assert!(!result.execution_origin_verified && !result.policy_enforcement_verified);
    for field in ["runtime_file_sha256", "program_sha256", "policy"] {
        let mut other = start.clone();
        other[field] = json!("0".repeat(64));
        let (log, receipt, sig) = sign_log(other, digest.clone());
        assert!(release
            .bind_receipt(
                &log,
                &receipt,
                &sig,
                &bytes(&f.trust),
                &sha256(&receipt),
                "test-session"
            )
            .unwrap_err()
            .contains("not bound"));
    }
    let (other_log, other_receipt, other_sig) = sign_log(start, "0".repeat(64));
    assert!(release
        .bind_receipt(
            &other_log,
            &other_receipt,
            &other_sig,
            &bytes(&f.trust),
            &sha256(&other_receipt),
            "test-session"
        )
        .is_err());

    // Exercise the public command with a complete evidence bundle.
    use std::{fs, process::Command};
    let dir = std::env::temp_dir().join(format!(
        "devlish-binding-{}",
        hex(f.key.public_key().as_ref())
    ));
    fs::create_dir(&dir).unwrap();
    let mut mapping = Vec::new();
    for artifact in f.manifest["artifacts"].as_array().unwrap() {
        let id = artifact["id"].as_str().unwrap();
        let data = match id {
            "policy" => policy_bytes.to_vec(),
            "program" => program_bytes.to_vec(),
            _ => id.as_bytes().to_vec(),
        };
        fs::write(dir.join(id), data).unwrap();
        mapping.push(json!({"id":id,"path":id}));
    }
    let evidence = json!({"application_report":"app.json","policy_report":"policy.json","log":"log.jsonl",
        "receipt":"receipt.json","signature":"receipt.sig.json","trust":"trust.json","retained_receipt_sha256":sha256(&receipt),"session_id":"test-session"});
    for (name, data) in [
        ("manifest.json", bytes(&f.manifest)),
        ("signature.json", f.signature()),
        ("trust.json", bytes(&f.trust)),
        ("requirements.json", bytes(&f.requirements)),
        ("artifacts.json", bytes(&json!(mapping))),
        ("evidence.json", bytes(&evidence)),
        ("app.json", app),
        ("policy.json", policy),
        ("log.jsonl", log),
        ("receipt.json", receipt),
        ("receipt.sig.json", sig),
    ] {
        fs::write(dir.join(name), data).unwrap();
    }
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_devlish-audit"))
            .current_dir(&dir)
            .args([
                "verify-release",
                "manifest.json",
                "--signature",
                "signature.json",
                "--trust",
                "trust.json",
                "--requirements",
                "requirements.json",
                "--artifacts",
                "artifacts.json",
                "--evidence",
                "evidence.json",
            ])
            .output()
            .unwrap()
    };
    let result = run();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    let result: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(result["receipt"]["release_manifest_verified"], true);
    fs::write(dir.join("app.json"), application(&sha256(b"different"))).unwrap();
    assert!(!run().status.success());
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn durable_admission_rejects_rollback_equivocation_and_concurrent_runs() {
    let mut f = Fixture::new();
    let dir = std::env::temp_dir().join(format!(
        "devlish-floor-{}",
        hex(f.key.public_key().as_ref())
    ));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("state.json");
    devlish_audit::admission::initialize(&path, &bytes(&f.requirements)).unwrap();
    assert!(devlish_audit::admission::initialize(&path, &bytes(&f.requirements)).is_err());
    let old = f.check().unwrap();
    let guard = old.admit(&path).unwrap();
    assert!(matches!(old.admit(&path), Err(e) if e.contains("locked")));
    drop(guard);
    f.manifest["sequence"] = json!(3);
    let newer = f.check().unwrap();
    drop(newer.admit(&path).unwrap());
    assert!(old.admit(&path).is_err());
    f.manifest["release_id"] = json!("different-same-sequence");
    let other = f.check().unwrap();
    assert!(other.admit(&path).is_err());
    // Caller mutation of public report fields cannot defeat private admission identity.
    let mut old = old;
    old.sequence = 100;
    assert!(old.admit(&path).is_err());
    drop(newer.admit(&path).unwrap());
    std::fs::write(&path, b"interrupted update").unwrap();
    assert!(newer.admit(&path).is_err());
    std::fs::remove_file(&path).unwrap();
    assert!(newer.admit(&path).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}
