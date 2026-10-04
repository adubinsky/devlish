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
    let start = json!({"type":"policy_run_started","format_version":3,"session_id":"test-session","runtime_file_sha256":sha256(b"runtime"),
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
    let prepared = release
        .prepare_receipt(
            &log,
            "test-session",
            devlish_audit::receipt::ReceiptKind::Terminal,
        )
        .unwrap();
    let prepared_value: Value = serde_json::from_slice(&prepared).unwrap();
    assert_eq!(prepared_value["log_file_sha256"], sha256(&log));
    assert_eq!(prepared_value["record_count"], 2);
    assert_eq!(prepared_value["release_manifest_sha256"], digest);
    assert_eq!(
        prepared,
        release
            .prepare_receipt(
                &log,
                "test-session",
                devlish_audit::receipt::ReceiptKind::Terminal
            )
            .unwrap()
    );
    let prefix = &log[..=log.iter().position(|b| *b == b'\n').unwrap()];
    assert!(release
        .prepare_receipt(
            prefix,
            "test-session",
            devlish_audit::receipt::ReceiptKind::Checkpoint
        )
        .is_ok());
    assert!(release
        .prepare_receipt(
            prefix,
            "test-session",
            devlish_audit::receipt::ReceiptKind::Terminal
        )
        .is_err());
    assert!(release
        .prepare_receipt(
            &log,
            "wrong-session",
            devlish_audit::receipt::ReceiptKind::Terminal
        )
        .is_err());

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
        ("log.jsonl", log.clone()),
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
    fs::write(dir.join("prepare.json"),bytes(&json!({"log":"log.jsonl","session_id":"test-session","kind":"terminal","output":"unsigned-receipt.json"}))).unwrap();
    let prepare = || {
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
                "--prepare-receipt",
                "prepare.json",
            ])
            .output()
            .unwrap()
    };
    let output = prepare();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let output: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(output["receipt_signed"], false);
    assert_eq!(output["signer_authorized"], false);
    assert_eq!(
        fs::read(dir.join("unsigned-receipt.json")).unwrap(),
        prepared
    );
    assert!(!prepare().status.success(), "must not overwrite receipt");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let receipt_key = devlish_audit::issuance::ReceiptKey {
            id: "release".into(),
            public_key_sha256: sha256(f.key.public_key().as_ref()),
        };
        let authority_dir = dir.join("authority");
        fs::create_dir(&authority_dir).unwrap();
        fs::set_permissions(&authority_dir, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(release
            .reserve_terminal_receipt(
                &authority_dir,
                "tenant",
                "test-session",
                &receipt_key,
                b"invalid log"
            )
            .is_err());
        assert_eq!(fs::read_dir(&authority_dir).unwrap().count(), 0);
        let reservation = release
            .reserve_terminal_receipt(&authority_dir, "tenant", "test-session", &receipt_key, &log)
            .unwrap();
        assert_eq!(reservation.receipt(), prepared);
        assert_eq!(reservation.receipt_sha256(), sha256(&prepared));
        assert_eq!(reservation.key_id(), "release");
        let signature = bytes(
            &json!({"format":"devlish-detached-signature","format_version":1,"algorithm":"ed25519","key_id":"release","purpose":"audit-receipt",
            "signature_hex":hex(f.key.sign(&signing_message(Purpose::AuditReceipt,reservation.receipt())).as_ref())}),
        );
        reservation.complete(&signature, &bytes(&f.trust)).unwrap();
        assert!(release
            .reserve_terminal_receipt(&authority_dir, "tenant", "test-session", &receipt_key, &log)
            .is_err());
    }
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

#[test]
fn recorded_controls_match_signed_permissions_and_counts_not_execution() {
    let mut f = Fixture::new();
    let policy = br#"{"rule":"nppi"}"#.to_vec();
    let program = br#"{"rule":"agent"}"#.to_vec();
    let permissions = bytes(
        &json!({"format":"devlish-runtime-permissions","format_version":1,"allowed_effects":["respond","clock_now"],"instruction_limit":1000,"effect_budget":{"total":3,"per_effect":{"clock_now":1}}}),
    );
    let mut snapshots = std::collections::BTreeMap::new();
    for artifact in f.manifest["artifacts"].as_array_mut().unwrap() {
        let id = artifact["id"].as_str().unwrap().to_owned();
        let data = match id.as_str() {
            "policy" => policy.clone(),
            "permissions" => permissions.clone(),
            _ => id.as_bytes().to_vec(),
        };
        artifact["sha256"] = json!(sha256(&data));
        snapshots.insert(id, data);
    }
    snapshots.insert("program".into(), program.clone());
    f.manifest["artifacts"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"program","role":"program","sha256":sha256(&program)}));
    f.trust["keys"][0]["purposes"] = json!(["release-manifest", "audit-receipt"]);
    let release = verify_release(
        &bytes(&f.manifest),
        &f.signature(),
        &bytes(&f.trust),
        &bytes(&f.requirements),
        |id| Ok(snapshots[id].clone()),
    )
    .unwrap();
    let canonical = |b: &[u8]| {
        sha256(&serde_json::to_vec_pretty(&serde_json::from_slice::<Value>(b).unwrap()).unwrap())
    };
    let identity = json!({"artifact_sha256":canonical(&policy)});
    let start = json!({"type":"policy_run_started","format_version":3,"session_id":"controls","runtime_file_sha256":sha256(b"runtime"),"policy":identity,"program_sha256":canonical(&program),"verified_release":{
        "session_id":"controls","release_manifest_sha256":sha256(&bytes(&f.manifest)),"runtime_file_sha256":sha256(b"runtime"),"permissions_sha256":sha256(&permissions),"catalog_sha256":sha256(b"tool-catalog"),"containment_sha256":sha256(b"containment"),"allowed_effects":["clock_now","respond"],"instruction_limit":1000,"effect_budget":{"total":3,"per_effect":{"clock_now":1}}
    }});
    let make_log = |start: Value, effects: &[(&str, bool)]| {
        let mut records = vec![start];
        for (index, (kind, allow)) in effects.iter().enumerate() {
            records.push(json!({"type":"effect_decision","effect_id":index+1,"effect":kind,"allow":allow,"policy":identity}));
            if *allow {
                records.push(json!({"type":"effect_outcome","effect_id":index+1,"effect":kind,"outcome":{"status":"succeeded"}}));
            }
        }
        records.push(json!({"type":"policy_run_finished","success":true,"paused":false}));
        let mut previous = String::new();
        let mut log = vec![];
        for (sequence, record) in records.into_iter().enumerate() {
            let mut envelope =
                json!({"sequence":sequence,"previous_sha256":previous,"record":record});
            previous = sha256(&bytes(&envelope));
            envelope["record_sha256"] = json!(previous);
            log.extend(bytes(&envelope));
            log.push(b'\n');
        }
        log
    };
    let log = make_log(
        start.clone(),
        &[("clock_now", true), ("clock_now", false), ("respond", true)],
    );
    let receipt = release
        .prepare_receipt(
            &log,
            "controls",
            devlish_audit::receipt::ReceiptKind::Terminal,
        )
        .unwrap();
    let signature = bytes(
        &json!({"format":"devlish-detached-signature","format_version":1,"algorithm":"ed25519","key_id":"release","purpose":"audit-receipt","signature_hex":hex(f.key.sign(&signing_message(Purpose::AuditReceipt,&receipt)).as_ref())}),
    );
    let result = release
        .bind_receipt(
            &log,
            &receipt,
            &signature,
            &bytes(&f.trust),
            &sha256(&receipt),
            "controls",
        )
        .unwrap();
    assert!(result.recorded_controls_match_release);
    assert!(
        !result.execution_origin_verified
            && !result.policy_enforcement_verified
            && !result.replay_verified
    );
    for (field, replacement) in [
        ("permissions_sha256", json!("a".repeat(64))),
        ("catalog_sha256", json!("a".repeat(64))),
        ("containment_sha256", json!("a".repeat(64))),
        ("runtime_file_sha256", json!("a".repeat(64))),
        ("instruction_limit", json!(1001)),
        (
            "allowed_effects",
            json!(["respond", "clock_now", "write_file"]),
        ),
        (
            "allowed_effects",
            json!(["respond", "clock_now", "respond"]),
        ),
        (
            "effect_budget",
            json!({"total":4,"per_effect":{"clock_now":1}}),
        ),
        ("effect_budget", Value::Null),
    ] {
        let mut changed = start.clone();
        changed["verified_release"][field] = replacement;
        assert!(
            release
                .prepare_receipt(
                    &make_log(changed, &[]),
                    "controls",
                    devlish_audit::receipt::ReceiptKind::Terminal
                )
                .is_err(),
            "{field}"
        );
    }
    for effects in [
        vec![("clock_now", true), ("clock_now", true)],
        vec![("clock_now", false), ("clock_now", true)],
        vec![("write_file", true)],
        vec![
            ("respond", false),
            ("respond", false),
            ("respond", false),
            ("respond", true),
        ],
    ] {
        assert!(
            release
                .prepare_receipt(
                    &make_log(start.clone(), &effects),
                    "controls",
                    devlish_audit::receipt::ReceiptKind::Terminal
                )
                .is_err(),
            "{effects:?}"
        );
        // Even a correctly signed fabricated receipt cannot promote these assertions.
        let bad_log = make_log(start.clone(), &effects);
        let lines: Vec<_> = bad_log
            .split(|b| *b == b'\n')
            .filter(|line| !line.is_empty())
            .collect();
        let tail: Value = serde_json::from_slice(lines.last().unwrap()).unwrap();
        let receipt = bytes(
            &json!({"format":"devlish-audit-receipt","format_version":1,"kind":"terminal","session_id":"controls","release_manifest_sha256":sha256(&bytes(&f.manifest)),"log_file_sha256":sha256(&bad_log),"log_head_sha256":tail["record_sha256"],"record_count":lines.len()}),
        );
        let signature = bytes(
            &json!({"format":"devlish-detached-signature","format_version":1,"algorithm":"ed25519","key_id":"release","purpose":"audit-receipt","signature_hex":hex(f.key.sign(&signing_message(Purpose::AuditReceipt,&receipt)).as_ref())}),
        );
        assert!(release
            .bind_receipt(
                &bad_log,
                &receipt,
                &signature,
                &bytes(&f.trust),
                &sha256(&receipt),
                "controls"
            )
            .is_err());
    }
    // An older log without claimed controls can bind identities but gets no control assurance.
    let mut legacy = start;
    legacy.as_object_mut().unwrap().remove("verified_release");
    let legacy = make_log(legacy, &[]);
    let receipt = release
        .prepare_receipt(
            &legacy,
            "controls",
            devlish_audit::receipt::ReceiptKind::Terminal,
        )
        .unwrap();
    let signature = bytes(
        &json!({"format":"devlish-detached-signature","format_version":1,"algorithm":"ed25519","key_id":"release","purpose":"audit-receipt","signature_hex":hex(f.key.sign(&signing_message(Purpose::AuditReceipt,&receipt)).as_ref())}),
    );
    assert!(
        !release
            .bind_receipt(
                &legacy,
                &receipt,
                &signature,
                &bytes(&f.trust),
                &sha256(&receipt),
                "controls"
            )
            .unwrap()
            .recorded_controls_match_release
    );
    // Operator requirements are independently trusted; log metadata cannot relax them.
    f.requirements["require_recorded_controls"] = json!(true);
    let strict = verify_release(
        &bytes(&f.manifest),
        &f.signature(),
        &bytes(&f.trust),
        &bytes(&f.requirements),
        |id| Ok(snapshots[id].clone()),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&strict).unwrap()["recorded_controls_required"],
        true
    );
    assert!(strict
        .prepare_receipt(
            &legacy,
            "controls",
            devlish_audit::receipt::ReceiptKind::Terminal
        )
        .is_err());
    assert!(strict
        .bind_receipt(
            &legacy,
            &receipt,
            &signature,
            &bytes(&f.trust),
            &sha256(&receipt),
            "controls"
        )
        .is_err());
    assert!(strict
        .prepare_receipt(
            &log,
            "controls",
            devlish_audit::receipt::ReceiptKind::Terminal
        )
        .is_ok());
}

#[test]
fn recorded_control_requirement_is_explicit_boolean_operator_configuration() {
    let mut f = Fixture::new();
    assert_eq!(
        serde_json::to_value(f.check().unwrap()).unwrap()["recorded_controls_required"],
        false
    );
    for invalid in [Value::Null, json!("true"), json!(1), json!({})] {
        f.requirements["require_recorded_controls"] = invalid;
        assert!(f.check().is_err());
    }
    f.requirements["require_recorded_controls"] = json!(true);
    assert_eq!(
        serde_json::to_value(f.check().unwrap()).unwrap()["recorded_controls_required"],
        true
    );
}


fn tool_catalog_fixture() -> (Fixture, Value) {
    let mut fixture = Fixture::new();
    fixture.manifest["target"] = json!(devlish_audit::tool_catalog::STATIC_TARGET);
    fixture.requirements["target"] = fixture.manifest["target"].clone();
    let catalog = json!({"format":"devlish-external-tool-catalog","format_version":1,
    "target":devlish_audit::tool_catalog::STATIC_TARGET,"tools":[{
        "id":"public-grep","artifact_id":"tool","path":"/opt/devlish/tools/grep",
        "image_profile":devlish_audit::tool_catalog::STATIC_PROFILE,
        "containment_id":"containment",
        "allowed_arguments":[["--fixed-strings","--","published","/work/public.txt"]]
    }]});
    (fixture, catalog)
}
fn verify_with_catalog(
    f: &mut Fixture,
    catalog: &Value,
) -> devlish_audit::release::ReleaseVerification {
    let snapshot = bytes(catalog);
    f.manifest["artifacts"][3]["sha256"] = json!(sha256(&snapshot));
    verify_release(
        &bytes(&f.manifest),
        &f.signature(),
        &bytes(&f.trust),
        &bytes(&f.requirements),
        |id| {
            Ok(if id == "tool-catalog" {
                snapshot.clone()
            } else {
                id.as_bytes().to_vec()
            })
        },
    )
    .unwrap()
}
#[test]
fn signed_catalog_selects_exact_arguments_and_retains_release_commitments() {
    let (mut f, catalog) = tool_catalog_fixture();
    let release = verify_with_catalog(&mut f, &catalog);
    let verified = release
        .tool_catalog("tool-catalog", &bytes(&catalog))
        .unwrap();
    let arguments: Vec<String> = ["--fixed-strings", "--", "published", "/work/public.txt"]
        .map(str::to_string)
        .into();
    let selection = verified.select("public-grep", &arguments, 150).unwrap();
    assert_eq!(selection.id(), "public-grep");
    assert_eq!(selection.path(), "/opt/devlish/tools/grep");
    assert_eq!(selection.arguments(), arguments);
    assert_eq!(selection.artifact_id(), "tool");
    assert_eq!(
        selection.image_profile(),
        devlish_audit::tool_catalog::STATIC_PROFILE
    );
    assert_eq!(selection.containment_id(), "containment");
    assert_eq!(selection.tool_sha256(), sha256(b"tool"));
    assert_eq!(selection.containment_sha256(), sha256(b"containment"));
    assert_eq!(selection.catalog_sha256(), sha256(&bytes(&catalog)));
    assert_eq!(selection.manifest_sha256(), sha256(&bytes(&f.manifest)));
    assert_eq!(selection.valid_until(), 200);
    assert!(!release.execution_origin_verified && !release.policy_enforcement_verified);
    for id in ["Public-grep", "grep", "/usr/bin/grep", ""] {
        assert!(verified.select(id, &arguments, 150).is_err());
    }
    for args in [
        vec!["-R", "/work/private"],
        vec!["--fixed-strings", "--", "published", "/work/nppi.csv"],
        vec![
            "--fixed-strings",
            "--",
            "published",
            "/work/company-secret.txt",
        ],
        vec!["$(cat /work/private)"],
        vec![],
    ] {
        assert!(verified
            .select(
                "public-grep",
                &args.into_iter().map(str::to_string).collect::<Vec<_>>(),
                150
            )
            .is_err());
    }
}
#[test]
fn catalog_uses_private_release_state_not_mutable_report_fields() {
    let (mut f, catalog) = tool_catalog_fixture();
    f.requirements["revocations_valid_until"] = json!(180);
    let mut release = verify_with_catalog(&mut f, &catalog);
    let digest = sha256(&bytes(&f.manifest));
    release.evaluated_at = 0;
    release.manifest_signature.artifact_sha256 = "f".repeat(64);
    release.release_requirements_verified = false;
    let verified = release
        .tool_catalog("tool-catalog", &bytes(&catalog))
        .unwrap();
    let args = catalog["tools"][0]["allowed_arguments"][0]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert!(verified.select("public-grep", &args, 149).is_err());
    assert!(verified.select("public-grep", &args, 180).is_err());
    let selected = verified.select("public-grep", &args, 179).unwrap();
    assert_eq!(selected.valid_until(), 180);
    assert_eq!(selected.manifest_sha256(), digest);
}
#[test]
fn replaced_catalog_wrong_id_role_or_release_is_rejected() {
    let (mut f, mut catalog) = tool_catalog_fixture();
    let release = verify_with_catalog(&mut f, &catalog);
    assert!(release.tool_catalog("tool", &bytes(&catalog)).is_err());
    assert!(release.tool_catalog("missing", &bytes(&catalog)).is_err());
    catalog["tools"][0]["path"] = json!("/tmp/attacker");
    assert!(release
        .tool_catalog("tool-catalog", &bytes(&catalog))
        .is_err());
    let other = verify_with_catalog(&mut f, &catalog);
    assert!(other
        .tool_catalog("tool-catalog", &vec![b' '; 65537])
        .is_err());
}
#[test]
fn signed_catalog_rejects_unsafe_paths_profiles_and_artifact_roles() {
    let (mut f, baseline) = tool_catalog_fixture();
    for (field, bad) in [
        ("id", "../grep"),
        ("id", ""),
        ("artifact_id", "runtime"),
        ("artifact_id", "missing"),
        ("containment_id", "tool"),
        ("containment_id", "missing"),
        ("path", "grep"),
        ("path", "/"),
        ("path", "/tmp//grep"),
        ("path", "/tmp/../grep"),
        ("path", "/tmp/./grep"),
        ("path", "/tmp/grep/"),
        ("path", "/tmp/grep\0"),
        ("image_profile", "dynamic"),
        ("environment", "LD_PRELOAD=attacker"),
    ] {
        let mut catalog = baseline.clone();
        catalog["tools"][0][field] = json!(bad);
        let release = verify_with_catalog(&mut f, &catalog);
        assert!(
            release
                .tool_catalog("tool-catalog", &bytes(&catalog))
                .is_err(),
            "{field} {bad:?}"
        );
    }
    for (field, value) in [
        ("target", json!("aarch64-unknown-linux-gnu")),
        ("format_version", json!(2)),
        ("format", json!("devlish-tool-catalog")),
        ("tools", json!([])),
    ] {
        let mut catalog = baseline.clone();
        catalog[field] = value;
        let release = verify_with_catalog(&mut f, &catalog);
        assert!(release
            .tool_catalog("tool-catalog", &bytes(&catalog))
            .is_err());
    }
}
#[test]
fn signed_catalog_rejects_duplicate_ids_arguments_and_transport_overflow() {
    let (mut f, baseline) = tool_catalog_fixture();
    let mut duplicate = baseline.clone();
    duplicate["tools"]
        .as_array_mut()
        .unwrap()
        .push(baseline["tools"][0].clone());
    let release = verify_with_catalog(&mut f, &duplicate);
    assert!(release
        .tool_catalog("tool-catalog", &bytes(&duplicate))
        .is_err());
    for arguments in [
        json!([]),
        json!([["x"], ["x"]]),
        json!([["\0"]]),
        json!([["x".repeat(4097)]]),
        json!([vec!["x"; 65]]),
        json!([vec!["x".repeat(4096); 5]]),
        json!([null]),
        json!([[1]]),
    ] {
        let mut catalog = baseline.clone();
        catalog["tools"][0]["allowed_arguments"] = arguments;
        let release = verify_with_catalog(&mut f, &catalog);
        assert!(release
            .tool_catalog("tool-catalog", &bytes(&catalog))
            .is_err());
    }
    // The maximum accepted argument bytes are UTF-8 bytes, not characters.
    let mut catalog = baseline;
    catalog["tools"][0]["allowed_arguments"] = json!([
        vec!["é".repeat(2048); 4],
        vec![""; 64],
        Vec::<String>::new()
    ]);
    let release = verify_with_catalog(&mut f, &catalog);
    let verified = release
        .tool_catalog("tool-catalog", &bytes(&catalog))
        .unwrap();
    assert!(verified.select("public-grep", &[], 150).is_ok());
}

#[test]
fn tool_request_report_is_repeatable_redacted_and_cannot_invent_execution() {
    let (mut f, catalog) = tool_catalog_fixture();
    let mut release = verify_with_catalog(&mut f, &catalog);
    let request = bytes(
        &json!({"tool_id":"public-grep","arguments":catalog["tools"][0]["allowed_arguments"][0]}),
    );
    let first = release
        .verify_tool_request("tool-catalog", &bytes(&catalog), &request)
        .unwrap();
    release.evaluated_at = 199;
    release.execution_origin_verified = true;
    release.policy_enforcement_verified = true;
    let second = release
        .verify_tool_request("tool-catalog", &bytes(&catalog), &request)
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(first["catalog_membership_verified"], true);
    assert_eq!(first["tool_artifact_snapshot_verified"], true);
    assert_eq!(first["evaluated_at"], 150);
    assert_eq!(first["request_sha256"], sha256(&request));
    for key in [
        "tool_image_profile_verified",
        "containment_enforcement_verified",
        "execution_origin_verified",
        "policy_enforcement_verified",
    ] {
        assert_eq!(first[key], false);
    }
    let output = first.to_string();
    assert!(
        !output.contains("/work/public.txt")
            && !output.contains("--fixed-strings")
            && !output.contains("/opt/devlish")
    );
    for request in [
        r#"{"tool_id":"public-grep","tool_id":"public-grep","arguments":[]}"#,
        r#"{"tool_id":"public-grep","arguments":[],"catalog_id":"other"}"#,
        r#"{"tool_id":"public-grep","arguments":[],"evaluated_at":150}"#,
        r#"{"tool_id":"public-grep","arguments":["--nppi"]}"#,
    ] {
        assert!(release
            .verify_tool_request("tool-catalog", &bytes(&catalog), request.as_bytes())
            .is_err());
    }
    assert!(release
        .verify_tool_request("tool-catalog", &bytes(&catalog), &vec![b' '; 65537])
        .is_err());
}

#[test]
fn cli_tool_report_reverifies_bytes_trust_and_options_without_executing() {
    use std::{fs, process::Command};
    let (mut f, catalog) = tool_catalog_fixture();
    verify_with_catalog(&mut f, &catalog);
    let dir = std::env::temp_dir().join(format!(
        "devlish-catalog-cli-{}",
        hex(f.key.public_key().as_ref())
    ));
    fs::create_dir(&dir).unwrap();
    for (name, data) in [
        ("manifest.json", bytes(&f.manifest)),
        ("signature.json", f.signature()),
        ("trust.json", bytes(&f.trust)),
        ("requirements.json", bytes(&f.requirements)),
        (
            "request.json",
            bytes(
                &json!({"tool_id":"public-grep","arguments":catalog["tools"][0]["allowed_arguments"][0]}),
            ),
        ),
    ] {
        fs::write(dir.join(name), data).unwrap();
    }
    let mut mapping = Vec::new();
    for artifact in f.manifest["artifacts"].as_array().unwrap() {
        let id = artifact["id"].as_str().unwrap();
        fs::write(
            dir.join(id),
            if id == "tool-catalog" {
                bytes(&catalog)
            } else {
                id.as_bytes().to_vec()
            },
        )
        .unwrap();
        mapping.push(json!({"id":id,"path":id}));
    }
    fs::write(dir.join("artifacts.json"), bytes(&json!(mapping))).unwrap();
    let run = |text: bool, extra: &[&str]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_devlish-audit"));
        command.current_dir(&dir);
        if text {
            command.arg("--text");
        }
        command
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
            .args(extra)
            .output()
            .unwrap()
    };
    let options = [
        "--tool-catalog",
        "tool-catalog",
        "--tool-request",
        "request.json",
    ];
    let first = run(false, &options);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stdout)
    );
    assert_eq!(first.stdout, run(false, &options).stdout);
    let report: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(
        report["tool_selection"]["tool_image_profile_verified"],
        false
    );
    let english = run(true, &options);
    assert!(english.status.success());
    let english = String::from_utf8(english.stdout).unwrap();
    assert!(english.contains("Catalog membership verified: Yes"));
    assert!(english.contains("Actual policy enforcement independently established: No"));
    assert!(!english.contains("/work/public.txt"));
    for extra in [
        vec!["--tool-catalog", "tool-catalog"],
        vec!["--tool-request", "request.json"],
        vec!["--evidence", "absent", "--prepare-receipt", "absent"],
        vec![
            "--tool-catalog",
            "tool-catalog",
            "--tool-request",
            "request.json",
            "--evidence",
            "absent",
        ],
        vec![
            "--tool-catalog",
            "runtime",
            "--tool-request",
            "request.json",
        ],
        vec![
            "--tool-catalog",
            "missing",
            "--tool-request",
            "request.json",
        ],
    ] {
        assert!(!run(false, &extra).status.success(), "{extra:?}");
    }
    fs::write(dir.join("tool"), "replaced").unwrap();
    assert!(!run(false, &options).status.success());
    fs::write(dir.join("tool"), "tool").unwrap();
    f.trust["keys"][0]["revoked"] = json!(true);
    fs::write(dir.join("trust.json"), bytes(&f.trust)).unwrap();
    assert!(!run(false, &options).status.success());
    fs::remove_dir_all(dir).unwrap();
}
