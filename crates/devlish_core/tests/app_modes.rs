//! Public entry points: prompt, file, and long-lived service.
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "devlish-modes-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join(".devlish")).unwrap();
        Self(root)
    }
    fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_devlish-core"));
        cmd.current_dir(&self.0)
            .env_remove("DEVLISH_VERIFIED_PROFILE")
            .env_remove("DEVLISH_AUDIT_LOG")
            .env_remove("DEVLISH_DEFAULT_AUTHORIZATION")
            .env("HOME", &self.0)
            .env("DEVLISH_CONFIG", self.0.join("config.toml"));
        cmd
    }
    fn prompt(&self, input: &str) -> std::process::Output {
        let mut child = self
            .command()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn no_arguments_opens_model_prompt_and_eof_or_exit_closes_it() {
    let f = Fixture::new();
    for input in ["", "/exit\n", "/clear\n/exit\n"] {
        let out = f.prompt(input);
        assert!(out.status.success());
        let text = String::from_utf8(out.stdout).unwrap();
        assert!(text.contains("devlish> "));
        assert!(!text.contains("Type Devlish statements"));
        assert!(!f.0.join(".devlish/sessions").exists());
    }
}

#[test]
fn file_modes_compile_in_memory_and_preserve_input_and_failures() {
    let f = Fixture::new();
    fs::write(
        f.0.join("workflow.dvl"),
        "Ask \"Name?\" as name\nRespond with name\n",
    )
    .unwrap();
    for mode in ["--run", "-r"] {
        let out = f
            .command()
            .args([
                mode,
                "workflow.dvl",
                "--quiet",
                "--input",
                "{\"name\":\"World\"}",
            ])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(String::from_utf8_lossy(&out.stdout).contains("World"));
        assert!(!f.0.join("workflow.dvlc.json").exists());
        assert!(!f
            .command()
            .args([mode, "missing.dvl"])
            .output()
            .unwrap()
            .status
            .success());
    }
}

#[test]
fn prompt_turns_use_real_devlish_model_effects_conversation_and_policy_logs() {
    let f = Fixture::new();
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    fs::write(f.0.join("config.toml"), format!("default_provider = \"ollama\"\ndefault_model = \"test-model\"\n[ollama]\nbase_url = \"http://{}\"\n", server.server_addr())).unwrap();
    let model = std::thread::spawn(move || {
        let mut prompts = Vec::new();
        for index in 0..3 {
            let mut request = server
                .recv_timeout(Duration::from_secs(15))
                .unwrap()
                .unwrap();
            assert_eq!(request.url(), "/chat/completions");
            let mut body = String::new();
            request.as_reader().read_to_string(&mut body).unwrap();
            let body: Value = serde_json::from_str(&body).unwrap();
            prompts.push(body["messages"][0]["content"].as_str().unwrap().to_string());
            request
                .respond(
                    tiny_http::Response::from_string(
                        json!({"choices":[{"message":{"content":json!({"steps":[{"action":"respond","payload":format!("answer {index}")}]}).to_string()}}]})
                            .to_string(),
                    )
                    .with_header(
                        tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap(),
                    ),
                )
                .unwrap();
        }
        prompts
    });
    let out = f.prompt("First request\nSecond request\n/clear\nFresh request\n/exit\n");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains("answer 0") && text.contains("answer 1"),
        "{text}"
    );
    let prompts = model.join().unwrap();
    assert!(
        prompts[1].contains("First request")
            && prompts[1].contains("answer 0")
            && prompts[1].contains("Second request")
    );
    assert!(prompts[2].contains("Fresh request"));
    assert!(!prompts[2].contains("First request") && !prompts[2].contains("answer 0"));
    let logs: Vec<_> = fs::read_dir(f.0.join(".devlish/sessions"))
        .unwrap()
        .collect();
    assert_eq!(logs.len(), 3);
    for log in logs {
        let log = fs::read_to_string(log.unwrap().path()).unwrap();
        assert!(log.contains("policy_run_finished"));
        assert!(log.contains("llm_complete") && log.contains("respond"));
        assert!(!log.contains("First request"));
    }
}

#[test]
fn explicit_prompt_policy_denial_blocks_model_before_credentials_or_network() {
    let f = Fixture::new();
    fs::write(f.0.join(".devlish/policy.dvl"), "Rule:\n  id: prompt.denial\n  version: 1.0.0\n\nRespond with record with false as allow and \"Company policy prohibits outbound messages.\" as reason\n").unwrap();
    let out = f.prompt("A prohibited message\n/exit\n");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("Company policy prohibits outbound messages."),
        "{text}"
    );
    assert!(!text.contains("missing API key"));
    let log = fs::read_dir(f.0.join(".devlish/sessions"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .unwrap();
    let log = fs::read_to_string(log).unwrap();
    let records: Vec<Value> = log
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(records
        .iter()
        .any(|r| r["record"]["type"] == "effect_decision" && r["record"]["allow"] == false));
    assert!(!records
        .iter()
        .any(|r| r["record"]["type"] == "effect_outcome"));
}

#[test]
fn server_modes_stay_running_and_answer_health_requests() {
    let f = Fixture::new();
    for mode in ["--server", "-s"] {
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = socket.local_addr().unwrap();
        drop(socket);
        let mut child = f
            .command()
            .args([mode, "--bind", &address.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let url = format!("http://{address}/v1/health");
        let mut healthy = false;
        for _ in 0..100 {
            if let Ok(response) = ureq::get(&url).timeout(Duration::from_millis(100)).call() {
                let body: Value = response.into_json().unwrap();
                healthy = body["ok"] == true;
                break;
            }
            if child.try_wait().unwrap().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let running = child.try_wait().unwrap().is_none();
        let _ = child.kill();
        let _ = child.wait();
        assert!(
            healthy && running,
            "{mode} should host a long-lived service"
        );
    }
}

#[test]
fn installing_produces_a_standalone_devlish_entry_point() {
    let f = Fixture::new();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    // Exercise the real install target using the already compiled test binary.
    let out = Command::new("make")
        .current_dir(repo)
        .arg("install")
        .arg("CARGO=true")
        .arg(format!("BINARY={}", env!("CARGO_BIN_EXE_devlish-core")))
        .arg(format!("PREFIX={}", f.0.join("installed").display()))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let executable = f.0.join("installed/bin/devlish");
    let out = Command::new(executable)
        .current_dir(&f.0)
        .env_remove("DEVLISH_VERIFIED_PROFILE")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("devlish> "));
}

#[test]
fn invalid_custom_policy_stops_startup_instead_of_using_the_builtin_policy() {
    let f = Fixture::new();
    fs::write(f.0.join(".devlish/policy.dvl"), "Respond with true\n").unwrap();
    let out = f.prompt("Never send this\n");
    assert!(!out.status.success());
    assert!(!String::from_utf8_lossy(&out.stdout).contains("devlish> "));
    assert!(!f.0.join(".devlish/sessions").exists());
}

#[cfg(unix)]
#[test]
fn custom_prompt_program_uses_the_same_local_tools_and_operator_posture() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    fs::write(f.0.join("localtool"), "#!/bin/sh\nprintf x >> marker\n").unwrap();
    fs::set_permissions(f.0.join("localtool"), fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(f.0.join(".devlish/agent.dvl"), "Permissions:\n  Run catalog tool \"localtool\"\n\nrequest equals record with \"localtool\" as tool_id and list of \"\" as arguments\nRun catalog tool request as result\nRespond with \"Done.\"\n").unwrap();
    fs::write(f.0.join(".devlish/policy.dvl"), "Rule:\n  id: prompt.local_policy\n  version: 1.0.0\n\nAsk \"Effect?\" as effect\nIf effect equals \"respond\":\n  Respond with record with true as allow and \"Acknowledgement permitted.\" as reason\nRespond with record with \"abstain\" as decision and \"Use the operator default.\" as reason\n").unwrap();
    for (posture, allowed) in [
        ("deny-unless-allowed", false),
        ("allow-unless-forbidden", true),
    ] {
        let mut child = f
            .command()
            .env("DEVLISH_DEFAULT_AUTHORIZATION", posture)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"Run the workflow\n/exit\n")
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success());
        assert_eq!(f.0.join("marker").exists(), allowed);
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).contains("Done."),
            allowed
        );
    }
    assert_eq!(fs::read_to_string(f.0.join("marker")).unwrap(), "x");
}

#[test]
fn prompt_operator_limits_cap_custom_workflow_attempts() {
    for (total, expected, success) in [(4, "xxx", true), (2, "xx", false)] {
        let f = Fixture::new();
        fs::write(f.0.join(".devlish/agent.dvl"), "Permissions:\n  Write files to \"marker\"\nAppend \"x\" to file \"marker\"\nAppend \"x\" to file \"marker\"\nAppend \"x\" to file \"marker\"\nRespond with \"Done.\"\n").unwrap();
        fs::write(f.0.join(".devlish/policy.dvl"), "Rule:\n  id: test.prompt_limits\n  version: 1.0.0\nRespond with record with true as allow and \"Synthetic fixture permitted.\" as reason\n").unwrap();
        fs::write(f.0.join(".devlish/limits.json"), json!({"instruction_limit":50000,"effect_budget":{"total":total,"per_effect":{"write_file":std::cmp::min(3,total),"respond":1}}}).to_string()).unwrap();
        let out = f.prompt("Run\n/exit\n");
        assert!(out.status.success());
        assert_eq!(fs::read_to_string(f.0.join("marker")).unwrap(), expected);
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).contains("Done."),
            success
        );
        let log = fs::read_dir(f.0.join(".devlish/sessions"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let records: Vec<Value> = fs::read_to_string(log)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap()["record"].clone())
            .collect();
        assert_eq!(records.last().unwrap()["success"], success);
        assert_eq!(records[1]["type"], "prompt_limits_captured");
        assert_eq!(records[1]["effect_budget"]["total"], total);
    }
}

#[test]
fn invalid_prompt_limits_stop_startup_before_any_effects() {
    let f = Fixture::new();
    fs::write(
        f.0.join(".devlish/limits.json"),
        "{\"instruction_limit\":50000,\"effect_budget\":{\"total\":0,\"per_effect\":{}}}",
    )
    .unwrap();
    let out = f.prompt("Run\n/exit\n");
    assert!(!out.status.success());
    assert!(!String::from_utf8_lossy(&out.stdout).contains("devlish> "));
    assert!(!f.0.join(".devlish/sessions").exists());
}

#[test]
fn harness_run_enforces_policy_and_writes_policy_log() {
    let f = Fixture::new();
    fs::write(
        f.0.join("workflow.dvl"),
        r#"Rule:
  id: test.harness.workflow
  version: 1.0.0

Permissions:
  Write files to "out.txt"

Export "secret" to "out.txt"
Respond with "done"
"#,
    )
    .unwrap();
    fs::write(
        f.0.join("policy.dvl"),
        r#"Rule:
  id: test.harness.policy
  version: 1.0.0

Ask "Which effect?" as effect
Ask "Request?" as request
If effect equals "respond":
  Respond with record with true as allow and "Response allowed." as reason
Respond with record with false as allow and "Denied by default." as reason
"#,
    )
    .unwrap();
    let log = f.0.join("policy.jsonl");
    let out = f
        .command()
        .args([
            "harness",
            "run",
            "workflow.dvl",
            "--policy",
            "policy.dvl",
            "--policy-log",
            log.to_str().unwrap(),
            "--default-authorization",
            "deny-unless-allowed",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "expected policy denial: {stdout}{stderr}"
    );
    assert!(log.exists(), "policy log missing: {stdout}{stderr}");
    let records: Vec<Value> = fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap()["record"].clone())
        .collect();
    assert!(records.iter().any(|r| r["type"] == "policy_run_started"));
    assert!(records.iter().any(|r| {
        r["type"] == "effect_decision" && r["effect"] == "write_file" && r["allow"] == false
    }));
    assert!(!f.0.join("out.txt").exists());
}

#[test]
fn harness_generation_requires_logging_and_preserves_existing_artifacts() {
    let f = Fixture::new();
    fs::write(f.0.join("contract.txt"), "Read fixture and respond.").unwrap();
    let missing_log = f
        .command()
        .args(["harness", "generate", "contract.txt", "--output-dir", "."])
        .output()
        .unwrap();
    assert!(!missing_log.status.success());
    assert!(String::from_utf8_lossy(&missing_log.stderr).contains("--policy-log is required"));
    let overlapping_log = f
        .command()
        .args([
            "harness",
            "generate",
            "contract.txt",
            "--output-dir",
            ".",
            "--policy-log",
            "program.dvl",
        ])
        .output()
        .unwrap();
    assert!(!overlapping_log.status.success());
    assert!(String::from_utf8_lossy(&overlapping_log.stderr)
        .contains("separate from generated artifacts"));
    assert!(!f.0.join("program.dvl").exists());
    fs::write(f.0.join("program.dvl"), "existing artifact").unwrap();
    let existing = f
        .command()
        .args([
            "harness",
            "generate",
            "contract.txt",
            "--output-dir",
            ".",
            "--policy-log",
            "generation.jsonl",
        ])
        .output()
        .unwrap();
    assert!(!existing.status.success());
    assert!(String::from_utf8_lossy(&existing.stderr).contains("refusing to replace"));
    assert_eq!(
        fs::read_to_string(f.0.join("program.dvl")).unwrap(),
        "existing artifact"
    );
    assert!(!f.0.join("generation.jsonl").exists());
    assert!(!f.0.join("policy.dvl").exists());
}

#[test]
fn harness_requires_policy_and_enforces_recorded_budget_and_append() {
    let f = Fixture::new();
    fs::write(f.0.join("workflow.dvl"), "Permissions:\n  Write files to \"state.log\"\nExport \"started\\n\" to \"state.log\"\nAppend \"completed\\n\" to file \"state.log\"\nRespond with \"done\"\n").unwrap();
    let missing = f
        .command()
        .args(["harness", "run", "workflow.dvl"])
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(!f.0.join("state.log").exists());
    fs::create_dir_all(f.0.join(".devlish")).unwrap();
    fs::write(f.0.join(".devlish/policy.dvl"), "Rule:\n  id: test.harness.limits\n  version: 1.0.0\n\nAsk \"Effect?\" as effect\nIf effect equals \"write_file\" or effect equals \"respond\":\n  Respond with record with true as allow and \"Allowed fixture.\" as reason\nRespond with record with false as allow and \"Denied.\" as reason\n").unwrap();
    let limits = f.0.join(".devlish/limits.json");
    fs::write(&limits, r#"{"instruction_limit":50000,"effect_budget":{"total":3,"per_effect":{"write_file":2,"respond":1}}}"#).unwrap();
    let run = f
        .command()
        .args(["harness", "run", "workflow.dvl"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        fs::read_to_string(f.0.join("state.log")).unwrap(),
        "started\ncompleted\n"
    );
    let log = fs::read_dir(f.0.join(".devlish/sessions"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .unwrap();
    let records = fs::read_to_string(log).unwrap();
    assert!(records.contains("execution_limits_captured"));
    fs::write(&limits, r#"{"instruction_limit":50000,"effect_budget":{"total":3,"per_effect":{"write_file":1,"respond":1}}}"#).unwrap();
    let exhausted = f
        .command()
        .args(["harness", "run", "workflow.dvl"])
        .output()
        .unwrap();
    assert!(!exhausted.status.success());
    assert_eq!(
        fs::read_to_string(f.0.join("state.log")).unwrap(),
        "started\n"
    );
    fs::remove_file(f.0.join("state.log")).unwrap();
    fs::write(
        &limits,
        r#"{"instruction_limit":1,"effect_budget":{"total":3,"per_effect":{}}}"#,
    )
    .unwrap();
    assert!(!f
        .command()
        .args(["harness", "run", "workflow.dvl"])
        .output()
        .unwrap()
        .status
        .success());
    assert!(!f.0.join("state.log").exists());
    fs::write(
        &limits,
        r#"{"instruction_limit":0,"effect_budget":{"total":3,"per_effect":{}}}"#,
    )
    .unwrap();
    assert!(!f
        .command()
        .args(["harness", "run", "workflow.dvl"])
        .output()
        .unwrap()
        .status
        .success());
    assert!(!f.0.join("state.log").exists());
}

#[test]
fn harness_recorder_failure_prevents_effects() {
    let f = Fixture::new();
    fs::write(
        f.0.join("workflow.dvl"),
        "Permissions:\n  Write files to \"marker\"\nExport \"x\" to \"marker\"\n",
    )
    .unwrap();
    fs::write(f.0.join("policy.dvl"), "Rule:\n  id: test.harness.recorder\n  version: 1.0.0\n\nRespond with record with true as allow and \"Allowed.\" as reason\n").unwrap();
    let out = f
        .command()
        .args([
            "harness",
            "run",
            "workflow.dvl",
            "--policy-log",
            "missing/record.jsonl",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(!f.0.join("marker").exists());
}

#[test]
fn harness_policy_cannot_grant_undeclared_file_permissions() {
    let f = Fixture::new();
    fs::write(f.0.join("workflow.dvl"), "Export \"x\" to \"marker\"\n").unwrap();
    fs::write(f.0.join("policy.dvl"), "Rule:\n  id: test.allow\n  version: 1.0.0\nRespond with record with true as allow and \"Allowed.\" as reason\n").unwrap();
    let run = f
        .command()
        .args(["harness", "run", "workflow.dvl"])
        .output()
        .unwrap();
    assert!(!run.status.success());
    assert!(!f.0.join("marker").exists());
}

#[test]
fn harness_resume_requires_fresh_operator_policy_and_limits() {
    let f = Fixture::new();
    fs::write(
        f.0.join("workflow.dvl"),
        "Permissions:\n  Write files to \"marker\"\nExport \"x\" to \"marker\"\n",
    )
    .unwrap();
    fs::write(f.0.join("session.json"), r#"{"source_path":"workflow.dvl","input":{},"result":{"is_checkpoint":true,"policy_path":"allow.dvl","effect_budget":{"total":10000,"per_effect":{}}}}"#).unwrap();
    assert!(!f
        .command()
        .args(["harness", "resume", "session.json"])
        .output()
        .unwrap()
        .status
        .success());
    fs::write(f.0.join("policy.dvl"), "Rule:\n  id: test.resume\n  version: 1.0.0\nRespond with record with false as allow and \"Denied.\" as reason\n").unwrap();
    let denied = f
        .command()
        .args(["harness", "resume", "session.json"])
        .output()
        .unwrap();
    assert!(!denied.status.success());
    assert!(!f.0.join("marker").exists());
}

#[test]
fn harness_generation_rejects_entire_unsafe_pair_before_installation() {
    for program in [
        "import os\nos.chmod('x', 511)",
        "Permissions:\n  Run catalog tool \"chmod\"\nRespond with \"ok\"",
        "Respond with \"ok\"",
    ] {
        let f = Fixture::new();
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        fs::write(f.0.join("config.toml"), format!("default_provider = \"ollama\"\ndefault_model = \"test-model\"\n[ollama]\nbase_url = \"http://{}\"\n", server.server_addr())).unwrap();
        fs::write(f.0.join("contract.txt"), "Return a safe Devlish program.").unwrap();
        let payload = json!({"program":program,"policy":"print('not a Devlish policy')"});
        let worker = std::thread::spawn(move || {
            let request = server
                .recv_timeout(Duration::from_secs(10))
                .unwrap()
                .unwrap();
            request
                .respond(
                    tiny_http::Response::from_string(
                        json!({"choices":[{"message":{"content":payload.to_string()}}]})
                            .to_string(),
                    )
                    .with_header(
                        tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap(),
                    ),
                )
                .unwrap();
        });
        let out = f
            .command()
            .args([
                "harness",
                "generate",
                "contract.txt",
                "--output-dir",
                ".",
                "--policy-log",
                "generation.jsonl",
            ])
            .output()
            .unwrap();
        worker.join().unwrap();
        assert!(!out.status.success());
        assert!(!f.0.join("program.dvl").exists());
        assert!(!f.0.join("policy.dvl").exists());
        let records: Vec<Value> = fs::read_to_string(f.0.join("generation.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap()["record"].clone())
            .collect();
        assert!(records.iter().any(|r| r["type"] == "effect_outcome"
            && r["effect"] == "llm_complete"
            && r["outcome"]["status"] == "failed"));
        assert!(!records.iter().any(|r| r["effect"] == "write_file"));
    }
}

#[test]
fn harness_generation_installs_valid_pair_without_executable_permissions() {
    let f = Fixture::new();
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    fs::write(f.0.join("config.toml"), format!("default_provider = \"ollama\"\ndefault_model = \"test-model\"\n[ollama]\nbase_url = \"http://{}\"\n", server.server_addr())).unwrap();
    fs::write(f.0.join("contract.txt"), "Return a greeting.").unwrap();
    let program = "Respond with \"hello\"\n";
    let policy = "Rule:\n  id: test.generated\n  version: 1.0.0\nRespond with record with true as allow and \"Greeting response.\" as reason\n";
    let payload = json!({"program":program,"policy":policy});
    let worker = std::thread::spawn(move || {
        let request = server
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        request
            .respond(
                tiny_http::Response::from_string(
                    json!({"choices":[{"message":{"content":payload.to_string()}}]}).to_string(),
                )
                .with_header(
                    tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap(),
                ),
            )
            .unwrap();
    });
    let out = f
        .command()
        .args([
            "harness",
            "generate",
            "contract.txt",
            "--output-dir",
            ".",
            "--policy-log",
            "generation.jsonl",
        ])
        .output()
        .unwrap();
    worker.join().unwrap();
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        fs::read_to_string(f.0.join("program.dvl")).unwrap(),
        program
    );
    assert_eq!(fs::read_to_string(f.0.join("policy.dvl")).unwrap(), policy);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(f.0.join("program.dvl"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
