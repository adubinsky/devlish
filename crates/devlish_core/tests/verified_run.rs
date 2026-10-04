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

fn model_route() -> Value {
    json!({"format":"devlish-model-route","format_version":1,"provider":"openrouter",
        "model":"synthetic/approved-model","credential":"SYNTHETIC_MODEL_KEY",
        "timeout_seconds":1,"max_request_bytes":4096,"max_response_bytes":4096,"max_tokens":100})
}
fn permit_model(f: &mut Fixture, route: Option<Value>) {
    let mut catalog =
        json!({"format":"devlish-tool-catalog","format_version":1,"host_effects":["llm_complete"]});
    if let Some(route) = route {
        catalog["llm_route"] = route;
    }
    replace_json(f, "tool-catalog", catalog);
    replace_json(
        f,
        "permissions",
        json!({"format":"devlish-runtime-permissions","format_version":1,"allowed_effects":["llm_complete"],"instruction_limit":1000}),
    );
}
#[test]
fn model_permission_requires_an_approved_signed_route_before_admission() {
    for route in [
        None,
        Some(json!({"provider":"openrouter","base_url":"http://unapproved.invalid"})),
    ] {
        let mut f = Fixture::new();
        permit_model(&mut f, route);
        let result = f.run();
        assert!(!result.status.success());
        f.assert_no_dispatch();
    }
}
#[test]
fn verified_model_calls_cannot_fall_back_to_mutable_user_routing() {
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut f = Fixture::new();
    permit_model(&mut f, Some(model_route()));
    replace_compiled(&mut f,"program","Permissions:\n  Call language models\nAsk the model \"synthetic/other-model\" with \"public\" expecting json as plan");
    f.profile["allow_raw_evidence"] = json!(true);
    f.save();
    fs::write(f.dir.join("untrusted.toml"),format!("default_provider = \"ollama\"\ndefault_model = \"synthetic/other-model\"\n[ollama]\nbase_url = \"http://{}\"\n",listener.local_addr().unwrap())).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_devlish-core"))
        .current_dir(&f.dir)
        .env("DEVLISH_VERIFIED_PROFILE", f.dir.join("profile.json"))
        .env("DEVLISH_CONFIG", f.dir.join("untrusted.toml"))
        .args([
            "run-verified",
            "--policy-log",
            "run.jsonl",
            "--session-id",
            "test-session",
            "--policy-evidence",
        ])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    let records: Vec<Value> = fs::read_to_string(f.dir.join("run.jsonl"))
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(records[1]["record"]["allow"], true);
    assert_eq!(
        records[2]["record"]["exchange"]["err"],
        "model request conflicts with approved route"
    );
    assert_eq!(records.last().unwrap()["record"]["success"], false);
}

fn library_fixture() -> Fixture {
    let mut f = Fixture::new();
    let runtime = fs::read(std::env::current_exe().unwrap()).unwrap();
    for artifact in f.manifest["artifacts"].as_array_mut().unwrap() {
        if artifact["id"] == "runtime" {
            artifact["sha256"] = json!(sha256(&runtime));
        }
    }
    f.save();
    f
}
#[derive(Default)]
struct LibraryHost {
    responses: Vec<Value>,
    events: usize,
}
impl devlish_vm::HostEffects for LibraryHost {
    fn emit_event(&mut self, _: &Value) {
        self.events += 1;
    }
    fn write_file(&mut self, _: &Value) -> Result<(), String> {
        panic!("unexpected write")
    }
    fn respond(&mut self, value: &Value) -> Result<(), String> {
        self.responses.push(value.clone());
        Ok(())
    }
}
#[test]
fn shared_admitted_session_executes_retained_snapshots_after_path_substitution() {
    let f = library_fixture();
    let session = devlish_core::verified_session::VerifiedSession::admit(
        &f.dir.join("profile.json"),
        "shared-session",
        false,
    )
    .unwrap();
    assert_eq!(session.program_path(), f.dir.join("program"));
    assert!(session.model_route().is_none());
    for path in ["program", "policy", "permissions", "tool-catalog"] {
        fs::write(f.dir.join(path), b"tampered after admission").unwrap();
    }
    let mut host = LibraryHost::default();
    let completion = session
        .execute(
            json!({"private":"SYNTHETIC_PRIVATE_INPUT"}),
            &f.dir.join("shared.jsonl"),
            &mut host,
        )
        .unwrap();
    assert!(completion.response_emitted);
    assert!(!completion.paused);
    assert_eq!(host.responses, vec![json!("verified success")]);
    assert_eq!(host.events, 0);
    let log = fs::read_to_string(f.dir.join("shared.jsonl")).unwrap();
    assert!(!log.contains("SYNTHETIC_PRIVATE_INPUT"));
    let start: Value = serde_json::from_str(log.lines().next().unwrap()).unwrap();
    assert_eq!(start["record"]["session_id"], "shared-session");
    assert_eq!(
        start["record"]["verified_release"]["release_manifest_sha256"],
        sha256(&bytes(&f.manifest))
    );
}
#[test]
fn shared_session_rejects_log_reuse_before_effects() {
    let f = library_fixture();
    fs::write(f.dir.join("occupied.jsonl"), b"existing evidence").unwrap();
    let session = devlish_core::verified_session::VerifiedSession::admit(
        &f.dir.join("profile.json"),
        "shared-session",
        false,
    )
    .unwrap();
    let mut host = LibraryHost::default();
    assert!(session
        .execute(json!({}), &f.dir.join("occupied.jsonl"), &mut host)
        .is_err());
    assert!(host.responses.is_empty());
    assert_eq!(
        fs::read(f.dir.join("occupied.jsonl")).unwrap(),
        b"existing evidence"
    );
}
#[test]
fn shared_session_validates_correlation_id_and_operator_evidence_consent() {
    let f = library_fixture();
    for id in ["", "../escape", "space id", "nonascii-é"] {
        assert!(devlish_core::verified_session::VerifiedSession::admit(
            &f.dir.join("profile.json"),
            id,
            false
        )
        .is_err());
    }
    assert!(devlish_core::verified_session::VerifiedSession::admit(
        &f.dir.join("profile.json"),
        "valid",
        true
    )
    .is_err());
    assert!(!f.dir.join("run.jsonl").exists());
}
#[cfg(unix)]
#[test]
fn shared_session_retains_rollback_lock_through_effect_dispatch() {
    use std::os::fd::AsRawFd;
    struct LockHost {
        state: fs::File,
        calls: usize,
    }
    impl devlish_vm::HostEffects for LockHost {
        fn emit_event(&mut self, _: &Value) {}
        fn write_file(&mut self, _: &Value) -> Result<(), String> {
            panic!("unexpected write")
        }
        fn respond(&mut self, _: &Value) -> Result<(), String> {
            assert_eq!(
                unsafe { libc::flock(self.state.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
                -1
            );
            assert_eq!(
                std::io::Error::last_os_error().kind(),
                std::io::ErrorKind::WouldBlock
            );
            self.calls += 1;
            Ok(())
        }
    }
    let f = library_fixture();
    let session = devlish_core::verified_session::VerifiedSession::admit(
        &f.dir.join("profile.json"),
        "shared-session",
        false,
    )
    .unwrap();
    let mut host = LockHost {
        state: fs::File::open(f.dir.join("admission.json")).unwrap(),
        calls: 0,
    };
    session
        .execute(json!({}), &f.dir.join("shared.jsonl"), &mut host)
        .unwrap();
    assert_eq!(host.calls, 1);
    assert_eq!(
        unsafe { libc::flock(host.state.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
}

#[test]
fn shared_session_anchors_relative_paths_before_working_directory_changes() {
    const CHILD: &str = "DEVLISH_TEST_RELATIVE_SESSION";
    if std::env::var_os(CHILD).is_some() {
        // Only this isolated child changes cwd; the parallel test runner does not.
        let original = std::env::current_dir().unwrap();
        let expected = fs::read(original.join("program")).unwrap();
        let session = devlish_core::verified_session::VerifiedSession::admit(
            std::path::Path::new("profile.json"),
            "relative-session",
            false,
        )
        .unwrap();
        std::env::set_current_dir(original.join("other")).unwrap();
        assert_eq!(
            fs::read(session.program_path()).unwrap(),
            expected,
            "credential lookup must remain anchored to the admitted program directory"
        );
        assert!(session.program_path().is_absolute());
        return;
    }
    let f = library_fixture();
    fs::create_dir(f.dir.join("other")).unwrap();
    fs::write(f.dir.join("other/program"), b"synthetic other directory").unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .current_dir(&f.dir)
        .env(CHILD, "1")
        .args([
            "--exact",
            "shared_session_anchors_relative_paths_before_working_directory_changes",
            "--nocapture",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
struct VerifiedServer {
    child: std::process::Child,
    url: String,
}
#[cfg(unix)]
impl VerifiedServer {
    fn start(f: &Fixture) -> Self {
        use std::{io::BufRead, os::unix::fs::PermissionsExt, process::Stdio};
        let logs = f.dir.join("http-logs");
        fs::create_dir(&logs).unwrap();
        fs::set_permissions(&logs, fs::Permissions::from_mode(0o700)).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_devlish-core"))
            .env("DEVLISH_VERIFIED_PROFILE", f.dir.join("profile.json"))
            .env("DEVLISH_SERVE_TOKEN", "a".repeat(64))
            .env_remove("DEVLISH_AUDIT_LOG")
            .args(["serve-verified", "--bind", "127.0.0.1:0", "--log-dir"])
            .arg(logs)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(stdout);
            let mut line = String::new();
            let _ = reader.read_line(&mut line);
            let _ = tx.send(line);
            // Drain until the owned child exits; private output is checked in logs/body.
            let _ = std::io::copy(&mut reader, &mut std::io::sink());
        });
        let mut server = Self {
            child,
            url: String::new(),
        };
        let line = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
        server.url = format!(
            "http://{}",
            line.trim()
                .strip_prefix("verified service listening on ")
                .expect("service must start")
        );
        server
    }
    fn request(&self, method: &str, path: &str, authorized: bool, body: &str) -> (u16, Value) {
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(5))
            .build();
        let mut request = agent.request(method, &format!("{}{path}", self.url));
        if authorized {
            request = request.set("Authorization", &format!("Bearer {}", "a".repeat(64)));
        }
        let result = if method == "GET" {
            request.call()
        } else {
            request.send_string(body)
        };
        let response = match result {
            Ok(r) | Err(ureq::Error::Status(_, r)) => r,
            Err(e) => panic!("local request failed: {e}"),
        };
        (
            response.status(),
            serde_json::from_str(&response.into_string().unwrap()).unwrap(),
        )
    }
}
#[cfg(unix)]
impl Drop for VerifiedServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(unix)]
#[test]
fn verified_http_authentication_schema_and_duplicate_sessions_fail_closed() {
    let f = Fixture::new();
    let server = VerifiedServer::start(&f);
    let private = "synthetic-private-input";
    let body = json!({"session_id":"http-test", "input":private}).to_string();
    assert_eq!(server.request("POST", "/v1/run", false, &body).0, 401);
    assert_eq!(server.request("GET", "/v1/health", false, "").0, 401);
    assert_eq!(
        server.request("GET", "/v1/health", true, "").1["execution_origin_verified"],
        false
    );
    assert_eq!(server.request("POST", "/v1/compile", true, &body).0, 404);
    assert_eq!(
        server
            .request(
                "POST",
                "/v1/run",
                true,
                &json!({"session_id":"bad", "input":null,"policy":"weaker"}).to_string()
            )
            .0,
        400
    );
    assert_eq!(
        server
            .request("POST", "/v1/run", true, &"x".repeat(65_537))
            .0,
        413
    );
    assert_eq!(
        server
            .request(
                "POST",
                "/v1/run",
                true,
                &json!({"session_id":"../escape", "input":null}).to_string()
            )
            .0,
        503
    );
    assert_eq!(fs::read_dir(f.dir.join("http-logs")).unwrap().count(), 0);
    let (status, result) = server.request("POST", "/v1/run", true, &body);
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["responses"], json!(["verified success"]));
    assert!(!result.to_string().contains(private));
    let path = f.dir.join("http-logs/http-test.jsonl");
    let log = fs::read(&path).unwrap();
    assert!(!String::from_utf8_lossy(&log).contains(private));
    assert_eq!(server.request("POST", "/v1/run", true, &body).0, 409);
    assert_eq!(fs::read(path).unwrap(), log);
    // Admission is repeated per request, not merely at daemon startup.
    fs::write(f.dir.join("program"), "tampered").unwrap();
    assert_eq!(
        server
            .request(
                "POST",
                "/v1/run",
                true,
                &json!({"session_id":"after-tamper", "input":null}).to_string()
            )
            .0,
        503
    );
    assert!(!f.dir.join("http-logs/after-tamper.jsonl").exists());
}

#[cfg(unix)]
#[test]
fn verified_http_denial_discards_buffered_output_and_private_errors() {
    let mut f = Fixture::new();
    replace_compiled(&mut f, "policy", "Rule:\n  id: test.verified\n  version: 1.0.0\nRespond with record with false as allow and \"synthetic-private-reason\" as reason");
    let server = VerifiedServer::start(&f);
    let (status, result) = server.request(
        "POST",
        "/v1/run",
        true,
        &json!({"session_id":"denied", "input":null}).to_string(),
    );
    assert_eq!(status, 500);
    assert!(!result.to_string().contains("synthetic-private-reason"));
    assert!(result.get("responses").is_none());
    assert!(f.dir.join("http-logs/denied.jsonl").exists());
}

#[cfg(unix)]
#[test]
fn verified_http_execution_failure_withholds_private_diagnostics() {
    let mut f = Fixture::new();
    replace_compiled(&mut f, "program", "Fail with \"synthetic-private-failure\"");
    let server = VerifiedServer::start(&f);
    let (status, result) = server.request(
        "POST",
        "/v1/run",
        true,
        &json!({"session_id":"failed", "input":null}).to_string(),
    );
    assert_eq!(status, 500);
    assert!(!result.to_string().contains("synthetic-private-failure"));
    assert!(result.get("responses").is_none());
    assert_eq!(
        server
            .request(
                "POST",
                "/v1/run",
                true,
                &json!({"session_id":"failed", "input":null}).to_string()
            )
            .0,
        409
    );
}

#[cfg(unix)]
#[test]
fn verified_http_startup_rejects_unsafe_binding_storage_and_missing_authentication() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let logs = f.dir.join("logs");
    fs::create_dir(&logs).unwrap();
    fs::set_permissions(&logs, fs::Permissions::from_mode(0o755)).unwrap();
    for (bind, token) in [
        ("0.0.0.0:0", Some("a".repeat(64))),
        ("127.0.0.1:0", None),
        ("127.0.0.1:0", Some("short".into())),
        ("127.0.0.1:0", Some("a".repeat(64))),
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_devlish-core"));
        command
            .env("DEVLISH_VERIFIED_PROFILE", f.dir.join("profile.json"))
            .env_remove("DEVLISH_SERVE_TOKEN");
        if let Some(token) = token {
            command.env("DEVLISH_SERVE_TOKEN", token);
        }
        let output = command
            .args(["serve-verified", "--bind", bind, "--log-dir"])
            .arg(&logs)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(!String::from_utf8_lossy(&output.stdout).contains("listening"));
    }
}
