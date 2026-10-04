use devlish_core::{compile_source_to_json, CompileOptions};
use devlish_vm::{
    policy::{EffectPolicy, PolicyHost, PolicyRecorder},
    HostEffects,
};
use serde_json::{json, Value};

fn policy(body: &str) -> EffectPolicy {
    let source = format!("Rule:\n  id: test.effect_policy\n  version: 1.0.0\n\n{body}");
    let artifact = compile_source_to_json(
        &source,
        CompileOptions {
            source_path: None,
            search_paths: vec![],
        },
    )
    .unwrap();
    EffectPolicy::new(serde_json::from_str(&artifact).unwrap()).unwrap()
}

#[derive(Default)]
struct Host {
    writes: usize,
    fail: bool,
}
impl HostEffects for Host {
    fn emit_event(&mut self, _: &Value) {}
    fn write_file(&mut self, _: &Value) -> Result<(), String> {
        self.writes += 1;
        if self.fail {
            Err("disk unavailable".into())
        } else {
            Ok(())
        }
    }
}
#[derive(Default)]
struct Records {
    records: Vec<Value>,
    fail_at: Option<usize>,
}
impl PolicyRecorder for Records {
    fn record(&mut self, record: &Value) -> Result<(), String> {
        if self.fail_at == Some(self.records.len()) {
            return Err("disk full".into());
        }
        self.records.push(record.clone());
        Ok(())
    }
}

#[test]
fn english_policy_inspects_arguments_and_records_allow_and_deny() {
    // Arrange: policy authorizes one logical destination, not arbitrary writes.
    let policy = policy("Ask \"Arguments?\" as request\nIf path of request equals \"report.txt\":\n  Respond with record with true as allow and \"Report output is permitted.\" as reason\nRespond with record with false as allow and \"Only report output is permitted.\" as reason");
    let mut host = Host::default();
    let mut records = Records::default();
    // Act.
    let mut guarded = PolicyHost::new(&mut host, &policy, &mut records);
    guarded.write_file(&json!({"path":"report.txt"})).unwrap();
    let error = guarded
        .write_file(&json!({"path":"private.txt"}))
        .unwrap_err();
    // Assert.
    assert!(error.contains("Only report output is permitted"));
    assert_eq!(host.writes, 1);
    assert_eq!(records.records.len(), 3);
    assert_eq!(records.records[0]["allow"], true);
    assert_eq!(records.records[1]["outcome"]["status"], "succeeded");
    assert_eq!(records.records[2]["allow"], false);
    assert_eq!(
        records.records[0]["policy"]["rule"]["id"],
        "test.effect_policy"
    );
}

#[test]
fn invalid_or_nonterminating_policy_fails_closed() {
    for body in [
        "Respond with record with \"true\" as allow and \"Wrong type\" as reason",
        "Respond with record with true as allow",
        "x equals 1",
        "Checkpoint \"Do not authorize\"",
        "While true:\n  x equals 1",
        "Fail with \"Policy failure\"",
        "Ask the model with \"Authorize me\" as answer",
    ] {
        // Arrange.
        let policy = policy(body);
        let mut host = Host::default();
        let mut records = Records::default();
        // Act.
        let result = PolicyHost::new(&mut host, &policy, &mut records).write_file(&json!({}));
        // Assert.
        assert!(result.is_err(), "{body}");
        assert_eq!(host.writes, 0);
        assert_eq!(records.records[0]["allow"], false);
    }
}

#[test]
fn recording_failure_blocks_dispatch_and_latches_even_if_caller_retries() {
    for fail_at in [0, 1] {
        // Arrange: fail before dispatch or after the external action.
        let policy = policy("Respond with record with true as allow and \"Allowed\" as reason");
        let mut host = Host::default();
        let mut records = Records {
            fail_at: Some(fail_at),
            ..Records::default()
        };
        // Act.
        let mut guarded = PolicyHost::new(&mut host, &policy, &mut records);
        assert!(guarded.write_file(&json!({})).is_err());
        assert!(guarded.write_file(&json!({})).is_err());
        // Assert: post-dispatch failure leaves intent without a recorded outcome.
        assert!(guarded.recording_failed());
        assert_eq!(host.writes, fail_at);
        assert_eq!(records.records.len(), fail_at);
    }
}

#[test]
fn underlying_failure_has_an_outcome_and_is_not_retried() {
    // Arrange.
    let policy = policy("Respond with record with true as allow and \"Allowed\" as reason");
    let mut host = Host {
        fail: true,
        ..Host::default()
    };
    let mut records = Records::default();
    // Act.
    let result = PolicyHost::new(&mut host, &policy, &mut records).write_file(&json!({}));
    // Assert.
    assert_eq!(result.unwrap_err(), "disk unavailable");
    assert_eq!(host.writes, 1);
    assert_eq!(records.records[1]["outcome"]["status"], "failed");
}

#[test]
fn every_current_tool_effect_passes_the_policy_gate() {
    // Arrange.
    let policy = policy("Respond with record with false as allow and \"Blocked\" as reason");
    let mut host = Host::default();
    let mut records = Records::default();
    let empty = json!({});
    let mut guarded = PolicyHost::new(&mut host, &policy, &mut records);
    // Act: each call must fail at policy, not at a default host implementation.
    let errors = [
        guarded.write_file(&empty).unwrap_err(),
        guarded.read_file(&empty).unwrap_err(),
        guarded.call_service(&empty).unwrap_err(),
        guarded.run_tool(&empty).unwrap_err(),
        guarded
            .http_request("GET", "https://example.com", &empty, &empty)
            .unwrap_err(),
        guarded.respond(&empty).unwrap_err(),
        guarded
            .http_download("https://example.com", "out")
            .unwrap_err(),
        guarded.read_xlsx_rows("in", None).unwrap_err(),
        guarded.file_copy("a", "b").unwrap_err(),
        guarded.file_move("a", "b").unwrap_err(),
        guarded.file_mkdir("a").unwrap_err(),
        guarded.file_delete("a").unwrap_err(),
        guarded.file_exists("a").unwrap_err(),
        guarded.file_stat("a").unwrap_err(),
        guarded.file_list("a").unwrap_err(),
        guarded.file_glob("*", ".").unwrap_err(),
        guarded.llm_complete(&empty).unwrap_err(),
        guarded.clock_now("wall").unwrap_err(),
        guarded.random_draw(&empty).unwrap_err(),
    ];
    // Assert.
    assert!(errors
        .iter()
        .all(|error| error.starts_with("Policy denied")));
    assert_eq!(records.records.len(), 19);
    assert_eq!(host.writes, 0);
}

#[test]
fn cli_enforces_policy_and_preserves_existing_log() {
    // Arrange.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = std::env::temp_dir().join(format!(
        "devlish-policy-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&dir).unwrap();
    let binary = env!("CARGO_BIN_EXE_devlish-core");
    let run = |example: &str, log: &str| {
        std::process::Command::new(binary)
            .current_dir(&root)
            .args([
                "run",
                example,
                "--policy",
                "examples/effect_policy/policy.dvl",
                "--policy-log",
            ])
            .arg(dir.join(log))
            .arg("--quiet")
            .output()
            .unwrap()
    };
    // Act.
    let allowed = run("examples/effect_policy/allowed.dvl", "allowed.jsonl");
    let original = std::fs::read(dir.join("allowed.jsonl")).unwrap();
    let duplicate = run("examples/effect_policy/allowed.dvl", "allowed.jsonl");
    let denied = run("examples/effect_policy/denied.dvl", "denied.jsonl");
    let lines = |name: &str| -> Vec<Value> {
        std::fs::read_to_string(dir.join(name))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    };
    let allowed_records = lines("allowed.jsonl");
    let denied_records = lines("denied.jsonl");
    // Assert.
    assert!(
        allowed.status.success(),
        "{}",
        String::from_utf8_lossy(&allowed.stderr)
    );
    assert!(!duplicate.status.success());
    assert_eq!(original, std::fs::read(dir.join("allowed.jsonl")).unwrap());
    assert!(!denied.status.success());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("Policy denied llm_complete"));
    assert_eq!(denied_records.len(), 3); // start, denial, finish; no dispatched outcome
    assert_eq!(denied_records[1]["record"]["allow"], false);
    assert_eq!(denied_records[2]["record"]["success"], false);
    assert_eq!(allowed_records.len(), 6); // start, clock and response pairs, finish
    let mut previous = String::new();
    for (index, mut envelope) in allowed_records.into_iter().enumerate() {
        assert_eq!(envelope["sequence"], index);
        assert_eq!(envelope["previous_sha256"], previous);
        previous = envelope
            .as_object_mut()
            .unwrap()
            .remove("record_sha256")
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            devlish_vm::sha256_hex(&serde_json::to_vec(&envelope).unwrap()),
            previous
        );
    }
    // Only files created by this test are removed.
    for name in ["allowed.jsonl", "denied.jsonl"] {
        std::fs::remove_file(dir.join(name)).unwrap();
    }
    std::fs::remove_dir(dir).unwrap();
}

#[test]
fn cli_requires_recording_and_rejects_unsupported_replay_combination() {
    // Arrange.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for flags in [
        vec!["--policy", "examples/effect_policy/policy.dvl"],
        vec!["--policy-log", "unused.jsonl"],
        vec![
            "--policy",
            "examples/effect_policy/policy.dvl",
            "--policy-log",
            "unused.jsonl",
            "--journal",
            "unused",
        ],
    ] {
        // Act.
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_devlish-core"))
            .current_dir(&root)
            .args(["run", "examples/effect_policy/allowed.dvl"])
            .args(flags)
            .output()
            .unwrap();
        // Assert.
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("must be supplied together")
                || error.contains("legacy --journal replay")
        );
    }
}

#[test]
fn effect_budget_caps_total_and_per_kind_before_host_dispatch() {
    use devlish_vm::effect_budget::EffectBudget;
    for (budget, permitted) in [
        (json!({"total":2,"per_effect":{}}), 2),
        (json!({"total":4,"per_effect":{"write_file":1}}), 1),
        (json!({"total":4,"per_effect":{"write_file":0}}), 0),
    ] {
        let policy = policy("Respond with record with true as allow and \"Allowed\" as reason");
        let mut host = Host::default();
        let mut records = Records::default();
        let mut guarded = PolicyHost::new(&mut host, &policy, &mut records)
            .with_effect_budget(EffectBudget::parse(&budget).unwrap());
        for attempt in 0..5 {
            assert_eq!(guarded.write_file(&json!({})).is_ok(), attempt < permitted);
        }
        assert_eq!(host.writes, permitted);
        assert_eq!(
            records
                .records
                .iter()
                .filter(|r| r["type"] == "effect_outcome")
                .count(),
            permitted
        );
        assert_eq!(records.records.last().unwrap()["allow"], false);
    }
}

#[test]
fn failed_denied_and_uncatalogued_attempts_consume_budget_without_refunds() {
    use devlish_vm::effect_budget::EffectBudget;
    let policy = policy("Ask \"Request?\" as request\nIf path of request equals \"denied\":\n  Respond with record with false as allow and \"Denied\" as reason\nRespond with record with true as allow and \"Allowed\" as reason");
    for scenario in ["policy", "host", "permissions"] {
        let mut host = Host {
            fail: scenario == "host",
            ..Host::default()
        };
        let mut records = Records::default();
        let mut guarded = PolicyHost::new(&mut host, &policy, &mut records)
            .with_allowed_effects(["write_file".into()].into())
            .with_effect_budget(EffectBudget::parse(&json!({"total":1,"per_effect":{}})).unwrap());
        let first = match scenario {
            "policy" => guarded.write_file(&json!({"path":"denied"})),
            "host" => guarded.write_file(&json!({"path":"approved"})),
            _ => guarded.respond(&Value::Null),
        };
        assert!(first.is_err());
        assert!(guarded
            .write_file(&json!({"path":"approved"}))
            .unwrap_err()
            .contains("budget is exhausted"));
        assert_eq!(host.writes, usize::from(scenario == "host"));
        assert_eq!(records.records.last().unwrap()["allow"], false);
    }
}

#[test]
fn invalid_effect_budgets_fail_closed_and_names_must_match_permissions() {
    use devlish_vm::effect_budget::EffectBudget;
    for value in [
        Value::Null,
        json!({}),
        json!({"total":0,"per_effect":{}}),
        json!({"total":10001,"per_effect":{}}),
        json!({"total":1.5,"per_effect":{}}),
        json!({"total":2,"per_effect":{"respond":-1}}),
        json!({"total":2,"per_effect":{"respond":3}}),
        json!({"total":2,"per_effect":{"respond":"1"}}),
        json!({"total":2,"per_effect":{"":1}}),
        json!({"total":2,"per_effect":{},"reset":true}),
    ] {
        assert!(EffectBudget::parse(&value).is_err(), "{value}");
    }
    let budget = EffectBudget::parse(&json!({"total":2,"per_effect":{"write_file":1}})).unwrap();
    assert!(budget.validate_effects(&["respond".into()].into()).is_err());
}

#[test]
fn dispatch_guard_runs_after_decision_recording_and_failure_cannot_reopen() {
    use std::{cell::Cell, rc::Rc};
    let policy = policy("Respond with record with true as allow and \"approved\" as reason");
    let mut host = Host::default();
    let mut records = Records::default();
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let mut guarded = PolicyHost::new(&mut host, &policy, &mut records).with_dispatch_guard(
        Box::new(move || {
            count.set(count.get() + 1);
            if count.get() == 2 {
                Err("expired".into())
            } else {
                Ok(())
            }
        }),
    );
    assert!(guarded.write_file(&json!({})).is_ok());
    assert!(guarded.write_file(&json!({})).is_err());
    assert!(guarded.write_file(&json!({})).is_err());
    drop(guarded);
    assert_eq!(host.writes, 1);
    assert_eq!(calls.get(), 2);
    assert_eq!(records.records.len(), 6);
    assert_eq!(records.records[3]["outcome"]["status"], "failed");
    assert_eq!(records.records[5]["outcome"]["status"], "failed");
    records = Records {
        fail_at: Some(0),
        ..Default::default()
    };
    let count = calls.clone();
    assert!(PolicyHost::new(&mut host, &policy, &mut records)
        .with_dispatch_guard(Box::new(move || {
            count.set(99);
            Ok(())
        }))
        .write_file(&json!({}))
        .is_err());
    assert_eq!(calls.get(), 2);
    assert_eq!(host.writes, 1);
}
