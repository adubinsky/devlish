#![cfg(unix)]
use devlish_core::local_tools::LocalTools;
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "devlish-local-tools-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn script(&self, name: &str, body: &str) {
        fs::write(self.0.join(name), format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(self.0.join(name), fs::Permissions::from_mode(0o700)).unwrap();
    }
    fn cli(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_devlish-core"))
            .current_dir(&self.0)
            .env("PATH", "/usr/bin:/bin")
            .args(args)
            .output()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn actual_local_and_path_programs_run_with_literal_arguments_and_clean_environment() {
    let root = Fixture::new();
    let path = Fixture::new();
    root.script(
        "localtool",
        "printf '%s' \"$1\"; printf 'diagnostic' >&2; test -z \"$HOME\"",
    );
    path.script("pathtool.sh", "printf 'from path'");
    let tools = LocalTools::new(&root.0, path.0.as_os_str()).unwrap();
    let result = tools
        .run(&json!({"tool_id":"localtool","arguments":["$(touch escaped); literal"]}))
        .unwrap();
    assert_eq!(result["stdout"], "$(touch escaped); literal");
    assert_eq!(result["stderr"], "diagnostic");
    assert_eq!(result["exit_code"], 0);
    assert_eq!(result["executable"]["immutable_execution_verified"], false);
    assert!(!root.0.join("escaped").exists());
    assert_eq!(
        tools
            .run(&json!({"tool_id":"pathtool.sh","arguments":[]}))
            .unwrap()["stdout"],
        "from path"
    );
}

#[test]
fn outside_symlinks_path_overrides_and_excessive_capture_are_rejected() {
    let root = Fixture::new();
    let outside = Fixture::new();
    outside.script("forbidden", "printf 'outside'");
    symlink(outside.0.join("forbidden"), root.0.join("escape")).unwrap();
    let tools = LocalTools::new(&root.0, std::ffi::OsStr::new("")).unwrap();
    for request in [
        json!({"tool_id":"escape","arguments":[]}),
        json!({"tool_id":"missing","arguments":[]}),
        json!({"tool_id":"../forbidden","arguments":[]}),
        json!({"tool_id":"escape","arguments":[],"cwd":"/"}),
    ] {
        assert!(tools.run(&request).is_err());
    }
    root.script("overflow", "while :; do printf '0123456789012345678901234567890123456789012345678901234567890123456789'; done");
    assert!(tools
        .run(&json!({"tool_id":"overflow","arguments":[]}))
        .unwrap_err()
        .contains("64 KiB"));
    root.script("timeout", "while :; do :; done");
    assert!(tools
        .run(&json!({"tool_id":"timeout","arguments":[]}))
        .unwrap_err()
        .contains("five seconds"));
}

#[test]
fn cli_posture_denials_and_replay_use_real_tools_without_reexecuting_them() {
    let root = Fixture::new();
    root.script("localtool", "printf 'public output'; printf x >> marker");
    fs::write(root.0.join("agent.dvl"), "Permissions:\n  Run catalog tool \"localtool\"\n\nrequest equals record with \"localtool\" as tool_id and list of \"\" as arguments\nRun catalog tool request as result\nRespond with \"Done.\"\n").unwrap();
    let policy = "Rule:\n  id: local.policy\n  version: 1.0.0\n\nAsk \"Effect?\" as effect\nIf effect equals \"respond\":\n  Respond with record with true as allow and \"Fixed acknowledgement allowed.\" as reason\nRespond with record with \"abstain\" as decision and \"No tool rule matches.\" as reason\n";
    fs::write(root.0.join("policy.dvl"), policy).unwrap();
    for (source, output) in [("agent.dvl", "agent.json"), ("policy.dvl", "policy.json")] {
        assert!(root
            .cli(&["compile", source, "--output", output])
            .status
            .success());
    }
    let runtime = PathBuf::from(env!("CARGO_BIN_EXE_devlish-core"));
    let files: Vec<Value> = [("runtime", runtime), ("program", root.0.join("agent.json")), ("policy", root.0.join("policy.json"))]
        .into_iter().map(|(role,path)| json!({"id":role,"role":role,"sha256":devlish_core::sha256_hex(&fs::read(&path).unwrap()),"path":path})).collect();
    fs::write(
        root.0.join("manifest.json"),
        serde_json::to_vec(
            &json!({"format":"devlish-application-manifest","format_version":1,"files":files}),
        )
        .unwrap(),
    )
    .unwrap();
    fs::write(root.0.join("cases.json"), serde_json::to_vec(&json!([{"name":"acknowledgement","input":{"effect":"respond","request":{}},"expected":{"allow":true,"reason":"Fixed acknowledgement allowed."}}])).unwrap()).unwrap();
    assert!(root
        .cli(&[
            "report",
            "application",
            "manifest.json",
            "--output",
            "application-report.json"
        ])
        .status
        .success());
    assert!(root
        .cli(&[
            "report",
            "policy",
            "policy.json",
            "cases.json",
            "--output",
            "policy-report.json",
            "--default-authorization",
            "allow-unless-forbidden"
        ])
        .status
        .success());
    let result = root.cli(&[
        "run",
        "agent.dvl",
        "--quiet",
        "--policy",
        "policy.dvl",
        "--policy-log",
        "allow.jsonl",
        "--policy-evidence",
        "--default-authorization",
        "allow-unless-forbidden",
    ]);
    assert!(
        result.status.success(),
        "{} {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fs::read(root.0.join("marker")).unwrap(), b"x");
    fs::write(root.0.join("input.json"), "{}").unwrap();
    let report = root.cli(&[
        "report",
        "process",
        "agent.json",
        "policy.json",
        "input.json",
        "allow.jsonl",
        "application-report.json",
        "policy-report.json",
    ]);
    assert!(
        report.status.success(),
        "{} {}",
        String::from_utf8_lossy(&report.stdout),
        String::from_utf8_lossy(&report.stderr)
    );
    assert_eq!(fs::read(root.0.join("marker")).unwrap(), b"x");
    assert!(root
        .cli(&[
            "report",
            "policy",
            "policy.json",
            "cases.json",
            "--output",
            "wrong-posture.json",
            "--default-authorization",
            "deny-unless-allowed"
        ])
        .status
        .success());
    let mismatch = root.cli(&[
        "report",
        "process",
        "agent.json",
        "policy.json",
        "input.json",
        "allow.jsonl",
        "application-report.json",
        "wrong-posture.json",
    ]);
    assert!(!mismatch.status.success());
    let log = fs::read_to_string(root.0.join("allow.jsonl")).unwrap();
    let records: Vec<Value> = log
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap()["record"].clone())
        .collect();
    assert!(records
        .iter()
        .any(|r| r["effect"] == "run_tool" && r["decision_origin"] == "default_posture"));
    let denied = root.cli(&[
        "run",
        "agent.dvl",
        "--quiet",
        "--policy",
        "policy.dvl",
        "--policy-log",
        "deny.jsonl",
        "--default-authorization",
        "deny-unless-allowed",
    ]);
    assert!(!denied.status.success());
    fs::write(
        root.0.join("policy.dvl"),
        policy.replace("\"abstain\" as decision", "\"deny\" as decision"),
    )
    .unwrap();
    let explicit = root.cli(&[
        "run",
        "agent.dvl",
        "--quiet",
        "--policy",
        "policy.dvl",
        "--policy-log",
        "explicit.jsonl",
        "--default-authorization",
        "allow-unless-forbidden",
    ]);
    assert!(!explicit.status.success());
    assert_eq!(fs::read(root.0.join("marker")).unwrap(), b"x");
}
