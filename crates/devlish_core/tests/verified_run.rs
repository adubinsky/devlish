#![cfg(feature = "native")]

use devlish_audit::{hex, sha256, signing_message, Purpose};
use devlish_core::{compile_source_to_json, CompileOptions};
use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};
struct Fixture {
    dir: PathBuf,
    key: Ed25519KeyPair,
    manifest: Value,
    profile: Value,
    requirements: Value,
}
fn bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}
impl Fixture {
    fn new() -> Self {
        let key = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let key = Ed25519KeyPair::from_pkcs8(key.as_ref()).unwrap();
        let dir = std::env::temp_dir().join(format!(
            "devlish-verified-{}",
            hex(key.public_key().as_ref())
        ));
        fs::create_dir(&dir).unwrap();
        let compile = |source| {
            compile_source_to_json(
                source,
                CompileOptions {
                    source_path: None,
                    search_paths: vec![],
                },
            )
            .unwrap()
            .into_bytes()
        };
        let policy = compile("Rule:\n  id: test.verified\n  version: 1.0.0\nRespond with record with true as allow and \"Synthetic test permission\" as reason");
        let program = compile("Respond with \"verified success\"");
        fs::write(dir.join("policy"), &policy).unwrap();
        fs::write(dir.join("program"), &program).unwrap();
        let runtime = fs::read(env!("CARGO_BIN_EXE_devlish-core")).unwrap();
        let mut artifacts = vec![
            json!({"id":"runtime","role":"runtime","sha256":sha256(&runtime)}),
            json!({"id":"program","role":"program","sha256":sha256(&program)}),
            json!({"id":"policy","role":"policy","sha256":sha256(&policy)}),
        ];
        let mut mapping = vec![
            json!({"id":"program","path":"program"}),
            json!({"id":"policy","path":"policy"}),
        ];
        for role in [
            "compiler",
            "tool-catalog",
            "permissions",
            "containment",
            "source-closure",
            "build-attestation",
        ] {
            let data = match role {
                "permissions" => bytes(
                    &json!({"format":"devlish-runtime-permissions","format_version":1,"allowed_effects":["respond"],"instruction_limit":10000}),
                ),
                "tool-catalog" => bytes(
                    &json!({"format":"devlish-tool-catalog","format_version":1,"host_effects":["respond"]}),
                ),
                "containment" => bytes(
                    &json!({"format":"devlish-containment-profile","format_version":1,"mode":"in-process"}),
                ),
                _ => role.as_bytes().to_vec(),
            };
            fs::write(dir.join(role), &data).unwrap();
            artifacts.push(json!({"id":role,"role":role,"sha256":sha256(&data)}));
            mapping.push(json!({"id":role,"path":role}));
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let manifest = json!({"format":"devlish-release-manifest","format_version":1,"release_id":"synthetic-test", "environment":"test","target":"test-host","sequence":5,
            "valid_from":now-60,"valid_until":now+3600,"repository":"synthetic","commit":"test-commit","workflow":"test-release","policy_id":"test.verified","policy_version":"1.0.0","artifacts":artifacts});
        let requirements = json!({"format":"devlish-release-requirements","format_version":1,"environment":"test","target":"test-host","repository":"synthetic","commit":"test-commit","workflow":"test-release","policy_id":"test.verified","policy_version":"1.0.0","minimum_sequence":5,
            "evaluated_at":1,"revocations_valid_from":now-60,"revocations_valid_until":now+3600,"revoked_manifest_sha256":[],"authorized_release_keys":["test-release"]});
        let profile = json!({"format":"devlish-verified-profile","format_version":1,"manifest":"manifest.json","signature":"signature.json","trust":"trust.json","requirements":"requirements.json","admission_state":"admission.json","artifacts":mapping,"runtime_id":"runtime","program_id":"program","policy_id":"policy","permissions_id":"permissions","catalog_id":"tool-catalog","containment_id":"containment"});
        let trust = json!({"format":"devlish-audit-trust","format_version":1,"keys":[{"id":"test-release","public_key_hex":hex(key.public_key().as_ref()),"purposes":["release-manifest"],"revoked":false}]});
        fs::write(dir.join("trust.json"), bytes(&trust)).unwrap();
        let f = Self {
            dir,
            key,
            manifest,
            profile,
            requirements,
        };
        f.save();
        devlish_audit::admission::initialize(
            &f.dir.join("admission.json"),
            &bytes(&f.requirements),
        )
        .unwrap();
        f
    }
    fn save(&self) {
        let manifest = bytes(&self.manifest);
        let signature = json!({"format":"devlish-detached-signature","format_version":1,"algorithm":"ed25519","key_id":"test-release","purpose":"release-manifest","signature_hex":hex(self.key.sign(&signing_message(Purpose::ReleaseManifest,&manifest)).as_ref())});
        for (name, data) in [
            ("manifest.json", manifest),
            ("signature.json", bytes(&signature)),
            ("requirements.json", bytes(&self.requirements)),
            ("profile.json", bytes(&self.profile)),
        ] {
            fs::write(self.dir.join(name), data).unwrap();
        }
    }
    fn command(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_devlish-core"))
            .current_dir(&self.dir)
            .env("DEVLISH_VERIFIED_PROFILE", self.dir.join("profile.json"))
            .env_remove("DEVLISH_AUDIT_LOG")
            .args(args)
            .output()
            .unwrap()
    }
    fn run(&self) -> Output {
        self.command(&[
            "run-verified",
            "--policy-log",
            "run.jsonl",
            "--session-id",
            "test-session",
        ])
    }
    fn assert_no_dispatch(&self) {
        assert!(!self.dir.join("run.jsonl").exists());
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
#[test]
fn approved_snapshots_run_and_record_release_session_and_live_evaluation_time() {
    let f = Fixture::new();
    let result = f.run();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("verified success"));
    let log = fs::read_to_string(f.dir.join("run.jsonl")).unwrap();
    let start: Value = serde_json::from_str(log.lines().next().unwrap()).unwrap();
    assert_eq!(start["record"]["session_id"], "test-session");
    assert_eq!(
        start["record"]["verified_release"]["release_manifest_sha256"],
        sha256(&bytes(&f.manifest))
    );
    assert!(
        start["record"]["verified_release"]["evaluated_at"]
            .as_u64()
            .unwrap()
            > 1
    );
    assert!(
        !f.run().status.success(),
        "must not overwrite an existing audit log"
    );
}
#[test]
fn changed_artifact_and_wrong_runtime_fail_before_log_or_execution() {
    let f = Fixture::new();
    fs::write(f.dir.join("program"), b"changed").unwrap();
    assert!(!f.run().status.success());
    f.assert_no_dispatch();
    let mut f = Fixture::new();
    f.manifest["artifacts"][0]["sha256"] = json!("0".repeat(64));
    f.save();
    assert!(!f.run().status.success());
    f.assert_no_dispatch();
}
#[test]
fn caller_cannot_override_policy_or_use_unsupported_execution_route() {
    let f = Fixture::new();
    for args in [
        vec!["run", "program"],
        vec!["serve"],
        vec!["mcp"],
        vec!["harness", "resume"],
        vec!["repl"],
        vec![
            "run-verified",
            "--policy-log",
            "run.jsonl",
            "--session-id",
            "test",
            "--policy",
            "weaker.json",
        ],
    ] {
        assert!(!f.command(&args).status.success(), "{args:?}");
        f.assert_no_dispatch();
    }
}
#[test]
fn expired_release_cannot_replay_an_old_operator_evaluation_time() {
    let mut f = Fixture::new();
    f.manifest["valid_from"] = json!(1);
    f.manifest["valid_until"] = json!(10);
    f.requirements["evaluated_at"] = json!(2);
    f.save();
    assert!(!f.run().status.success());
    f.assert_no_dispatch();
}
#[test]
fn policy_metadata_must_match_compiled_snapshot() {
    let mut f = Fixture::new();
    f.manifest["policy_id"] = json!("other");
    f.requirements["policy_id"] = json!("other");
    f.save();
    let result = f.run();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("verified execution rejected"));
    f.assert_no_dispatch();
}

#[test]
fn persisted_floor_rejects_older_release_even_if_requirements_are_lowered() {
    let mut f = Fixture::new();
    assert!(f.run().status.success());
    f.manifest["sequence"] = json!(4);
    f.requirements["minimum_sequence"] = json!(1);
    f.save();
    let result = f.command(&[
        "run-verified",
        "--policy-log",
        "older.jsonl",
        "--session-id",
        "older",
    ]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("verified execution rejected"));
    assert!(!f.dir.join("older.jsonl").exists());
}
#[test]
fn missing_state_requires_explicit_operator_recovery() {
    let f = Fixture::new();
    fs::remove_file(f.dir.join("admission.json")).unwrap();
    assert!(!f.run().status.success());
    f.assert_no_dispatch();
}

fn replace_compiled(f: &mut Fixture, id: &str, source: &str) {
    let data = compile_source_to_json(
        source,
        CompileOptions {
            source_path: None,
            search_paths: vec![],
        },
    )
    .unwrap();
    fs::write(f.dir.join(id), &data).unwrap();
    for artifact in f.manifest["artifacts"].as_array_mut().unwrap() {
        if artifact["id"] == id {
            artifact["sha256"] = json!(sha256(data.as_bytes()));
        }
    }
    f.save();
}
#[test]
fn verified_failures_and_fallback_results_do_not_disclose_program_data() {
    let marker = "SYNTHETIC_NPPI_000_12_3456";
    for source in [
        format!("Fail with \"{marker}\""),
        format!("secret equals \"{marker}\""),
    ] {
        let mut f = Fixture::new();
        replace_compiled(&mut f, "program", &source);
        let result = f.run();
        assert!(!String::from_utf8_lossy(&result.stdout).contains(marker));
        assert!(!String::from_utf8_lossy(&result.stderr).contains(marker));
        let log = fs::read_to_string(f.dir.join("run.jsonl")).unwrap();
        assert!(!log.contains(marker));
    }
}
#[test]
fn policy_denial_reason_cannot_leak_synthetic_nppi_to_logs_or_cli() {
    let marker = "SYNTHETIC_NPPI_000_12_3456";
    let mut f = Fixture::new();
    replace_compiled(&mut f,"policy",&format!("Rule:\n  id: test.verified\n  version: 1.0.0\nRespond with record with false as allow and \"{marker}\" as reason"));
    replace_compiled(&mut f, "program", &format!("Respond with \"{marker}\""));
    let result = f.run();
    assert!(!result.status.success());
    assert!(!String::from_utf8_lossy(&result.stdout).contains(marker));
    assert!(!String::from_utf8_lossy(&result.stderr).contains(marker));
    let log = fs::read_to_string(f.dir.join("run.jsonl")).unwrap();
    assert!(!log.contains(marker));
    assert!(log.contains("reason_sha256"));
}

fn replace_json(f: &mut Fixture, id: &str, data: Value) {
    let data = bytes(&data);
    fs::write(f.dir.join(id), &data).unwrap();
    for artifact in f.manifest["artifacts"].as_array_mut().unwrap() {
        if artifact["id"] == id {
            artifact["sha256"] = json!(sha256(&data));
        }
    }
    f.save();
}
#[test]
fn signed_permissions_limit_effects_even_when_devlish_policy_allows_them() {
    let mut f = Fixture::new();
    replace_json(
        &mut f,
        "permissions",
        json!({"format":"devlish-runtime-permissions","format_version":1,"allowed_effects":[],"instruction_limit":1000}),
    );
    let result = f.run();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    let log = fs::read_to_string(f.dir.join("run.jsonl")).unwrap();
    let decision: Value = serde_json::from_str(log.lines().nth(1).unwrap()).unwrap();
    assert_eq!(decision["record"]["allow"], false);
    assert_eq!(decision["record"]["effect"], "respond");
    assert!(!log.contains("effect_outcome"));
}
#[test]
fn signed_instruction_budget_stops_nonterminating_program() {
    let mut f = Fixture::new();
    replace_compiled(&mut f, "program", "While true:\n  x equals 1");
    replace_json(
        &mut f,
        "permissions",
        json!({"format":"devlish-runtime-permissions","format_version":1,"allowed_effects":[],"instruction_limit":100}),
    );
    assert!(!f.run().status.success());
    let log = fs::read_to_string(f.dir.join("run.jsonl")).unwrap();
    let finish: Value = serde_json::from_str(log.lines().last().unwrap()).unwrap();
    assert_eq!(finish["record"]["success"], false);
}
#[test]
fn unsupported_containment_and_uncatalogued_effects_fail_before_execution() {
    let mut f = Fixture::new();
    replace_json(
        &mut f,
        "containment",
        json!({"format":"devlish-containment-profile","format_version":1,"mode":"hardware-isolated"}),
    );
    assert!(!f.run().status.success());
    f.assert_no_dispatch();
    let mut f = Fixture::new();
    replace_json(
        &mut f,
        "permissions",
        json!({"format":"devlish-runtime-permissions","format_version":1,"allowed_effects":["write_file"],"instruction_limit":1000}),
    );
    assert!(!f.run().status.success());
    f.assert_no_dispatch();
}

#[test]
fn raw_evidence_requires_operator_opt_in_before_dispatch() {
    let f = Fixture::new();
    let result = f.command(&[
        "run-verified",
        "--policy-log",
        "run.jsonl",
        "--session-id",
        "test-session",
        "--policy-evidence",
    ]);
    assert!(!result.status.success());
    f.assert_no_dispatch();
}

#[test]
fn verified_evidence_replays_with_release_permissions_and_redacted_diagnostics() {
    let mut f = Fixture::new();
    f.profile["allow_raw_evidence"] = json!(true);
    f.save();
    let result = f.command(&[
        "run-verified",
        "--policy-log",
        "run.jsonl",
        "--session-id",
        "test-session",
        "--policy-evidence",
    ]);
    assert!(result.status.success());
    let files: Vec<_> = ["runtime", "program", "policy"]
        .iter()
        .map(|id| {
            let path = if *id == "runtime" {
                PathBuf::from(env!("CARGO_BIN_EXE_devlish-core"))
            } else {
                f.dir.join(id)
            };
            json!({"id":id,"role":id,"path":path,"sha256":sha256(&fs::read(&path).unwrap())})
        })
        .collect();
    fs::write(
        f.dir.join("app-manifest.json"),
        bytes(&json!({"format":"devlish-application-manifest","format_version":1,"files":files})),
    )
    .unwrap();
    fs::write(f.dir.join("cases.json"),bytes(&json!([{"name":"response","input":{"effect":"respond","request":"verified success"},"expected":{"allow":true,"reason":"Synthetic test permission"}}]))).unwrap();
    fs::write(f.dir.join("input.json"), b"{}").unwrap();
    let audit = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_devlish-core"))
            .current_dir(&f.dir)
            .env_remove("DEVLISH_VERIFIED_PROFILE")
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    };
    audit(&[
        "report",
        "application",
        "app-manifest.json",
        "--output",
        "app-report.json",
    ]);
    audit(&[
        "report",
        "policy",
        "policy",
        "cases.json",
        "--output",
        "policy-report.json",
    ]);
    let args = [
        "report",
        "process",
        "program",
        "policy",
        "input.json",
        "run.jsonl",
        "app-report.json",
        "policy-report.json",
    ];
    let one = audit(&args);
    let two = audit(&args);
    assert_eq!(one.stdout, two.stdout);
    let report: Value = serde_json::from_slice(&one.stdout).unwrap();
    assert_eq!(report["passed"], true);
}
