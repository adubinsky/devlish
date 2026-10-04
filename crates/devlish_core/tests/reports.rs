use devlish_core::sha256_hex;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);
impl Fixture {
    fn new(denied: bool) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "devlish-reports-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&dir).unwrap();
        let f = Self(dir);
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let program = if denied {
            "examples/effect_policy/denied.dvl"
        } else {
            "examples/effect_policy/allowed.dvl"
        };
        for (source, output) in [
            (program, "program.json"),
            ("examples/effect_policy/policy.dvl", "policy.json"),
        ] {
            let compiled = Command::new(env!("CARGO_BIN_EXE_devlish-core"))
                .current_dir(&root)
                .args(["compile", source, "--output"])
                .arg(f.0.join(output))
                .output()
                .unwrap();
            assert!(compiled.status.success());
        }
        f.write("input.json", &json!({}));
        f.write("cases.json", &json!([
            {"name":"clock", "input":{"effect":"clock_now","request":{"kind":"wall"}},
             "expected":{"allow":true,"reason":"Reading the clock is permitted."}},
            {"name":"denial", "input":{"effect":"llm_complete","request":{"prompt":"test"}},
             "expected":{"allow":false,"reason":"This policy permits only clock reads and responses."}}
        ]));
        let runtime = PathBuf::from(env!("CARGO_BIN_EXE_devlish-core"));
        let files: Vec<Value> = [("runtime", runtime), ("program", f.0.join("program.json")), ("policy", f.0.join("policy.json"))]
            .into_iter().map(|(role,path)| json!({"id":role,"role":role,"sha256":sha256_hex(&std::fs::read(&path).unwrap()),"path":path})).collect();
        f.write(
            "manifest.json",
            &json!({"format":"devlish-application-manifest","format_version":1,"files":files}),
        );
        f.ok(&[
            "report",
            "application",
            "manifest.json",
            "--output",
            "application-report.json",
        ]);
        f.ok(&[
            "report",
            "policy",
            "policy.json",
            "cases.json",
            "--output",
            "policy-report.json",
        ]);
        let execution = f.command(&[
            "run",
            "program.json",
            "--input",
            "{}",
            "--policy",
            "policy.json",
            "--policy-log",
            "run.jsonl",
            "--policy-evidence",
            "--quiet",
        ]);
        assert_eq!(
            execution.status.success(),
            !denied,
            "{}",
            String::from_utf8_lossy(&execution.stderr)
        );
        f
    }
    fn command(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_devlish-core"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
    fn ok(&self, args: &[&str]) -> Output {
        let result = self.command(args);
        assert!(
            result.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        result
    }
    fn process(&self) -> Output {
        self.command(&[
            "report",
            "process",
            "program.json",
            "policy.json",
            "input.json",
            "run.jsonl",
            "application-report.json",
            "policy-report.json",
        ])
    }
    fn write(&self, path: &str, value: &Value) {
        std::fs::write(self.0.join(path), serde_json::to_vec_pretty(value).unwrap()).unwrap();
    }
    fn json(&self, path: &str) -> Value {
        serde_json::from_slice(&std::fs::read(self.0.join(path)).unwrap()).unwrap()
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
fn reports_are_repeatable_and_replay_clock_without_live_effects() {
    // Arrange.
    let f = Fixture::new(false);
    // Act.
    let first = f.process();
    let second = f.process();
    // Assert: recorded clock responses reproduce the same output and report hash.
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(first.stdout, second.stdout);
    let report: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(report["passed"], true);
    assert_eq!(report["details"]["replay_matches"], true);
    assert_eq!(report["details"]["live_effects_performed"], false);
    assert_eq!(report["details"]["execution_succeeded"], true);
    f.write("process-report.json", &report);
    let explanation = f.ok(&["report", "explain", "process-report.json"]);
    assert!(String::from_utf8_lossy(&explanation.stdout).contains("without live tool calls"));
    f.ok(&[
        "report",
        "verify",
        "process-report.json",
        "--sha256",
        report["report_sha256"].as_str().unwrap(),
    ]);
    let app = f.ok(&["report", "application", "manifest.json"]);
    assert_eq!(
        serde_json::from_slice::<Value>(&app.stdout).unwrap(),
        f.json("application-report.json")
    );
}

#[test]
fn policy_block_is_reproducible_without_claiming_task_success() {
    // Arrange.
    let f = Fixture::new(true);
    // Act.
    let result = f.process();
    // Assert.
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["passed"], true);
    assert_eq!(report["details"]["execution_succeeded"], false);
    assert_eq!(report["details"]["denied_effects"], 1);
}

#[test]
fn changed_application_and_failed_policy_case_produce_failing_reports() {
    // Arrange.
    let f = Fixture::new(false);
    let mut cases = f.json("cases.json");
    cases[0]["expected"]["allow"] = json!(false);
    f.write("cases.json", &cases);
    std::fs::write(f.0.join("program.json"), b"replaced executable artifact").unwrap();
    // Act.
    let app = f.command(&["report", "application", "manifest.json"]);
    let policy = f.command(&["report", "policy", "policy.json", "cases.json"]);
    // Assert.
    for result in [app, policy] {
        assert!(!result.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&result.stdout).unwrap()["passed"],
            false
        );
    }
}

#[test]
fn input_changes_log_tampering_and_truncation_are_rejected() {
    // Arrange.
    let f = Fixture::new(false);
    let original = std::fs::read_to_string(f.0.join("run.jsonl")).unwrap();
    f.write("input.json", &json!({"extra":"different"}));
    assert!(!f.process().status.success());
    f.write("input.json", &json!({}));
    // Act/Assert: mutate a hash, truncate completion, reorder, remove an interior record.
    let lines: Vec<_> = original.lines().collect();
    let mut reordered = lines.clone();
    reordered.swap(1, 2);
    let mut removed = lines.clone();
    removed.remove(1);
    for modified in [
        original.replacen("clock_now", "file_list", 1),
        lines[..lines.len() - 1].join("\n"),
        reordered.join("\n"),
        removed.join("\n"),
    ] {
        std::fs::write(f.0.join("run.jsonl"), modified).unwrap();
        assert!(!f.process().status.success());
    }
}

#[test]
fn resealed_changed_report_fails_the_original_external_anchor() {
    // Arrange.
    let f = Fixture::new(false);
    let mut report = f.json("application-report.json");
    let anchor = report["report_sha256"].as_str().unwrap().to_owned();
    report["details"]["manifest_file_sha256"] = json!("replacement");
    f.write("changed-report.json", &report);
    assert!(!f
        .command(&["report", "verify", "changed-report.json"])
        .status
        .success());
    // Act: attacker can recompute a plain self-hash.
    report.as_object_mut().unwrap().remove("report_sha256");
    report["report_sha256"] = json!(sha256_hex(&serde_json::to_vec(&report).unwrap()));
    f.write("changed-report.json", &report);
    // Assert: self-consistency passes, independently retained anchor does not.
    f.ok(&["report", "verify", "changed-report.json"]);
    assert!(!f
        .command(&[
            "report",
            "verify",
            "changed-report.json",
            "--sha256",
            &anchor
        ])
        .status
        .success());
}

#[test]
fn rehashed_tool_response_still_fails_execution_replay() {
    // Arrange.
    let f = Fixture::new(false);
    let mut log: Vec<Value> = std::fs::read_to_string(f.0.join("run.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    log[2]["record"]["exchange"]["ok"] = json!("altered clock result");
    log[2]["record"]["outcome"]["result_sha256"] = json!(sha256_hex(
        &serde_json::to_vec(&json!("altered clock result")).unwrap()
    ));
    let mut previous = String::new();
    for envelope in &mut log {
        envelope["previous_sha256"] = json!(previous);
        envelope.as_object_mut().unwrap().remove("record_sha256");
        previous = sha256_hex(&serde_json::to_vec(envelope).unwrap());
        envelope["record_sha256"] = json!(previous);
    }
    // Act.
    std::fs::write(
        f.0.join("run.jsonl"),
        log.iter()
            .map(|r| serde_json::to_string(r).unwrap())
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    let result = f.process();
    // Assert: valid chain does not imply execution agreement.
    assert!(!result.status.success());
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["details"]["replay_matches"], false);
}

#[test]
fn verbose_execution_replays_and_old_hash_only_logs_are_rejected() {
    // Arrange: event mode changes the VM result envelope and must be retained.
    let f = Fixture::new(false);
    std::fs::rename(f.0.join("run.jsonl"), f.0.join("original.jsonl")).unwrap();
    f.ok(&[
        "run",
        "program.json",
        "--policy",
        "policy.json",
        "--policy-log",
        "run.jsonl",
        "--policy-evidence",
    ]);
    // Act / Assert.
    let report = f.process();
    assert!(
        report.status.success(),
        "{}",
        String::from_utf8_lossy(&report.stderr)
    );
    std::fs::remove_file(f.0.join("run.jsonl")).unwrap();
    f.ok(&[
        "run",
        "program.json",
        "--policy",
        "policy.json",
        "--policy-log",
        "run.jsonl",
        "--quiet",
    ]);
    let report = f.process();
    assert!(!report.status.success());
    assert!(String::from_utf8_lossy(&report.stderr).contains("--policy-evidence"));
}

#[test]
fn authority_policy_reports_are_repeatable_and_do_not_authenticate_fixture_state() {
    // Arrange: synthetic fixture authority is supplied only to offline evaluation.
    let f = Fixture::new(false);
    let compiled = devlish_core::compile_source_to_json(
        include_str!("../../../examples/receipt_authority/authorize.dvl"),
        devlish_core::CompileOptions {
            source_path: None,
            search_paths: vec![],
        },
    )
    .unwrap();
    std::fs::write(f.0.join("receipt-policy.json"), compiled).unwrap();
    let mut cases: Value = serde_json::from_str(include_str!(
        "../../../examples/receipt_authority/cases.json"
    ))
    .unwrap();
    f.write("receipt-cases.json", &cases);
    // Act.
    let run = || {
        f.command(&[
            "report",
            "policy",
            "receipt-policy.json",
            "receipt-cases.json",
        ])
    };
    let first = run();
    let second = run();
    // Assert: stable passing report, with no authentication claim for case inputs.
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stdout)
    );
    assert_eq!(first.stdout, second.stdout);
    let report: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(report["details"]["authority_authenticated"], false);
    assert_eq!(
        report["details"]["cases"].as_array().unwrap().len(),
        cases.as_array().unwrap().len()
    );
    f.write("receipt-report.json", &report);
    let explanation = f.ok(&["report", "explain", "receipt-report.json"]);
    assert!(String::from_utf8_lossy(&explanation.stdout)
        .contains("does not authenticate supplied authority state"));
    f.ok(&[
        "report",
        "verify",
        "receipt-report.json",
        "--sha256",
        report["report_sha256"].as_str().unwrap(),
    ]);
    // A substituted authoritative candidate must change the decision and fail.
    cases[0]["input"]["authority"]["receipt_sha256"] = json!("f".repeat(64));
    f.write("receipt-cases.json", &cases);
    assert!(!run().status.success());
    // A nested caller field cannot fill the separate authority channel.
    let forged = cases[0]["input"]
        .as_object_mut()
        .unwrap()
        .remove("authority")
        .unwrap();
    cases[0]["input"]["request"]["authority"] = forged;
    f.write("receipt-cases.json", &cases);
    assert!(!run().status.success());
}

#[test]
fn application_report_measures_the_separately_distributed_audit_verifier() {
    let f = Fixture::new(false);
    let mut manifest = f.json("manifest.json");
    std::fs::write(f.0.join("audit-verifier"), b"synthetic verifier bytes").unwrap();
    manifest["files"].as_array_mut().unwrap().push(json!({"id":"verifier","role":"audit-verifier","path":"audit-verifier","sha256":sha256_hex(b"synthetic verifier bytes")}));
    f.write("manifest.json", &manifest);
    let accepted = f.ok(&["report", "application", "manifest.json"]);
    assert_eq!(
        serde_json::from_slice::<Value>(&accepted.stdout).unwrap()["passed"],
        true
    );
    std::fs::write(f.0.join("audit-verifier"), b"replaced").unwrap();
    let rejected = f.command(&["report", "application", "manifest.json"]);
    assert!(!rejected.status.success());
}

#[test]
fn tool_execution_policy_report_repeats_without_authenticating_launch_claims() {
    let fixture = Fixture::new(false);
    let compiled = devlish_core::compile_source_to_json(
        include_str!("../../../examples/tool_execution_authority/authorize.dvl"),
        devlish_core::CompileOptions {
            source_path: None,
            search_paths: vec![],
        },
    )
    .unwrap();
    std::fs::write(fixture.0.join("tool-policy.json"), compiled).unwrap();
    let cases: Value = serde_json::from_str(include_str!(
        "../../../examples/tool_execution_authority/cases.json"
    ))
    .unwrap();
    fixture.write("tool-cases.json", &cases);
    let run = || fixture.command(&["report", "policy", "tool-policy.json", "tool-cases.json"]);
    let first = run();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stdout)
    );
    let second = run();
    assert!(second.status.success());
    assert_eq!(first.stdout, second.stdout);
    let report: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(report["details"]["authority_authenticated"], false);
    assert_eq!(
        report["details"]["cases"].as_array().unwrap().len(),
        cases.as_array().unwrap().len()
    );
    fixture.write("tool-report.json", &report);
    fixture.ok(&[
        "report",
        "verify",
        "tool-report.json",
        "--sha256",
        report["report_sha256"].as_str().unwrap(),
    ]);
    let explanation = fixture.ok(&["report", "explain", "tool-report.json"]);
    assert!(String::from_utf8_lossy(&explanation.stdout)
        .contains("does not authenticate supplied authority state"));
}
