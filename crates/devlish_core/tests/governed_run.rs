use devlish_core::{
    compile_source_to_json,
    governed_run::{Completion, GovernedRun, RunError},
    CompileOptions,
};
use devlish_vm::{
    policy::{EffectPolicy, PolicyRecorder},
    HostEffects,
};
use serde_json::{json, Value};
const PRIVATE: &str = "SYNTHETIC_PRIVATE_DATA";
fn compile(source: &str) -> Value {
    serde_json::from_str(
        &compile_source_to_json(
            source,
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .unwrap(),
    )
    .unwrap()
}
fn run(source: &str, allow: bool, effects: &[&str], limit: u64) -> GovernedRun {
    let policy = EffectPolicy::new(compile(&format!("Rule:\n  id: test.runner\n  version: 1.0.0\nRespond with record with {allow} as allow and \"{PRIVATE}\" as reason"))).unwrap();
    GovernedRun::new(
        compile(source),
        json!({"secret":PRIVATE}),
        policy,
        limit,
        effects.iter().map(|s| s.to_string()).collect(),
    )
    .unwrap()
}
#[derive(Default)]
struct Host {
    responses: Vec<Value>,
    reads: Vec<Value>,
    csv: Option<String>,
    events: usize,
    fail: bool,
}
impl HostEffects for Host {
    fn emit_event(&mut self, _: &Value) {
        self.events += 1;
    }
    fn write_file(&mut self, _: &Value) -> Result<(), String> {
        panic!("unexpected write")
    }
    fn read_file(&mut self, request: &Value) -> Result<Value, String> {
        self.reads.push(request.clone());
        Ok(json!(self.csv.as_ref().expect("unexpected read")))
    }
    fn respond(&mut self, value: &Value) -> Result<(), String> {
        self.responses.push(value.clone());
        if self.fail {
            Err(PRIVATE.into())
        } else {
            Ok(())
        }
    }
}
#[derive(Default)]
struct Recorder {
    values: Vec<Value>,
    fail_at: Option<usize>,
}
impl PolicyRecorder for Recorder {
    fn record(&mut self, value: &Value) -> Result<(), String> {
        if self.fail_at == Some(self.values.len()) {
            return Err(PRIVATE.into());
        }
        self.values.push(value.clone());
        Ok(())
    }
}

#[test]
fn unicode_concatenation_response_remains_governed_and_recorded() {
    let source = "html equals \"界🙂\" plus \" · Items: \" plus count of list of 1, 2\nRespond with html";
    for (allow, effects, approved) in [
        (true, vec!["respond"], true),
        (false, vec!["respond"], false),
        (true, vec![], false),
    ] {
        let mut host = Host::default();
        let mut recorder = Recorder::default();
        let result = run(source, allow, &effects, 1000).run(&mut host, &mut recorder);
        assert_eq!(result.is_ok(), approved);
        assert_eq!(recorder.values[0]["effect"], "respond");
        assert_eq!(recorder.values[0]["allow"], approved);
        assert_eq!(recorder.values.last().unwrap()["success"], approved);
        if approved {
            assert_eq!(host.responses, vec![json!("界🙂 · Items: 2")]);
            assert_eq!(recorder.values[1]["outcome"]["status"], "succeeded");
        } else {
            assert!(host.responses.is_empty());
        }
    }
}

#[test]
fn multiline_csv_reads_remain_governed_and_journaled() {
    let source = "Permissions:\n  Read files from \"/inbox/\"\nRead CSV from \"/inbox/invoices.csv\" as rows\nRespond with rows";
    let csv = "id,notes\r\n1,\"first\n\nlast \"\"quote\"\"\"\r\n2,\"CRLF\r\npreserved\"";
    for (allow, effects, admitted) in [
        (true, vec!["read_file", "respond"], true),
        (false, vec!["read_file", "respond"], false),
        (true, vec!["respond"], false),
    ] {
        let mut host = Host {
            csv: Some(csv.into()),
            ..Host::default()
        };
        let mut recorder = Recorder::default();
        let result = run(source, allow, &effects, 1000).run(&mut host, &mut recorder);
        assert_eq!(result.is_ok(), admitted);
        assert_eq!(host.reads.len(), usize::from(admitted));
        assert_eq!(recorder.values[0]["effect"], "read_file");
        assert_eq!(recorder.values[0]["allow"], admitted);
        assert_eq!(recorder.values.last().unwrap()["success"], admitted);
        if admitted {
            assert_eq!(
                host.reads[0],
                json!({"path":"/inbox/invoices.csv", "format":"csv"})
            );
            assert_eq!(
                host.responses,
                vec![json!([
                    {"id":"1", "notes":"first\n\nlast \"quote\""},
                    {"id":"2", "notes":"CRLF\r\npreserved"}
                ])]
            );
            assert_eq!(recorder.values[1]["outcome"]["status"], "succeeded");
            assert_eq!(recorder.values[2]["effect"], "respond");
        } else {
            assert!(host.responses.is_empty());
        }
    }
}

#[test]
fn loop_control_closes_exited_recovery_blocks() {
    let mut mismatches = Vec::new();
    for loop_header in [
        "While n is less than 2",
        "Until n is 2",
        "For each item in list of 1, 2",
    ] {
        for control in ["n equals n", "Break", "Continue"] {
            for nested in [false, true] {
                let body = if nested {
                    format!("    Try:\n      {control}\n    Otherwise:\n      Respond with \"stale inner\"")
                } else {
                    format!("    {control}")
                };
                let source = format!(
                    "n equals 0\n{loop_header}:\n  n equals n plus 1\n  Try:\n{body}\n  Otherwise:\n    Respond with \"stale outer\"\nFail with \"outside recovery\""
                );
                let mut host = Host::default();
                let mut recorder = Recorder::default();
                let result = run(&source, true, &["respond"], 500).run(&mut host, &mut recorder);
                if result != Err(RunError::Execution)
                    || !host.responses.is_empty()
                    || recorder.values.len() != 1
                    || recorder.values[0]["success"] != false
                {
                    mismatches.push(format!(
                        "{loop_header}, {control}, nested={nested}: {result:?}, responses={:?}, logs={:?}",
                        host.responses, recorder.values
                    ));
                }
                assert_eq!(host.events, 0);
            }
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn loop_control_preserves_enclosing_recovery_and_handles_otherwise() {
    for loop_header in [
        "While n is less than 2",
        "Until n is 2",
        "For each item in list of 1, 2",
    ] {
        for control in ["Break", "Continue"] {
            for from_recovery in [false, true] {
                let body = if from_recovery {
                    format!("      Try:\n        Fail with \"inner\"\n      Otherwise:\n        {control}")
                } else {
                    // This loop is inside the inner Try; its control must retain that handler.
                    format!("      For each token in list of 1, 2:\n        {control}\n      Fail with \"inner\"")
                };
                let source = format!(
                    "n equals 0\nTry:\n  {loop_header}:\n    n equals n plus 1\n    Try:\n{body}\n    Otherwise:\n      recovered equals last_error\n  Fail with \"outer\"\nOtherwise:\n  Respond with last_error"
                );
                let mut host = Host::default();
                let mut recorder = Recorder::default();
                run(&source, true, &["respond"], 1000)
                    .run(&mut host, &mut recorder)
                    .unwrap();
                assert_eq!(host.responses, vec![json!({"message":"outer"})]);
                assert_eq!(recorder.values[0]["effect"], "respond");
                assert_eq!(recorder.values[1]["outcome"]["status"], "succeeded");
                assert_eq!(recorder.values.last().unwrap()["success"], true);
            }
        }
    }
}

#[test]
fn recovery_loop_control_keeps_instruction_and_effect_limits() {
    let source = "For each item in list of 1:\n  Try:\n    Continue\n  Otherwise:\n    Fail with \"unexpected recovery\"\nRespond with \"done\"";
    // One-item For each costs 22 instructions, plus TRY_BEGIN and the closing TRY_END.
    for (allow, effects, limit, success) in [
        (true, vec!["respond"], 24, true),
        (true, vec!["respond"], 23, false),
        (false, vec!["respond"], 24, false),
        (true, vec![], 24, false),
    ] {
        let mut host = Host::default();
        let mut recorder = Recorder::default();
        let result = run(source, allow, &effects, limit).run(&mut host, &mut recorder);
        assert_eq!(result.is_ok(), success);
        assert_eq!(host.responses.len(), usize::from(success));
        assert_eq!(recorder.values.last().unwrap()["success"], success);
    }
    for loop_header in ["While true", "Until false"] {
        let source = format!("{loop_header}:\n  Try:\n    Continue\n  Otherwise:\n    Respond with \"unexpected recovery\"");
        let mut host = Host::default();
        let mut recorder = Recorder::default();
        assert_eq!(
            run(&source, true, &["respond"], 30).run(&mut host, &mut recorder),
            Err(RunError::Execution)
        );
        assert!(host.responses.is_empty());
        assert_eq!(recorder.values.last().unwrap()["success"], false);
    }
    let source = "Permissions:\n  Read files from \"/inbox/\"\nFor each item in list of 1:\n  Try:\n    Read CSV from \"/inbox/data.csv\" as rows\n    Continue\n  Otherwise:\n    Fail with \"unexpected recovery\"\nRespond with \"done\"";
    for total in [1, 2] {
        let mut host = Host {
            csv: Some("id\n1".into()),
            ..Host::default()
        };
        let mut recorder = Recorder::default();
        let runner = run(source, true, &["read_file", "respond"], 100)
            .with_effect_budget(
                devlish_vm::effect_budget::EffectBudget::parse(
                    &json!({"total":total,"per_effect":{}}),
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(runner.run(&mut host, &mut recorder).is_ok(), total == 2);
        assert_eq!(host.reads.len(), 1);
        assert_eq!(host.responses.len(), usize::from(total == 2));
        assert_eq!(recorder.values[2]["effect"], "respond");
        assert_eq!(recorder.values[2]["allow"], total == 2);
        assert_eq!(recorder.values.last().unwrap()["success"], total == 2);
    }
}

#[test]
fn for_each_continue_advances_first_middle_and_last_items() {
    for skip in [1, 2, 3] {
        let source = format!(
            "visited equals 0\nkept equals 0\nFor each item in list of 1, 2, 3:\n  visited equals visited plus 1\n  If item is {skip}:\n    Continue\n  kept equals kept plus item\nRespond with record with visited as visited and kept as kept"
        );
        let mut host = Host::default();
        let mut recorder = Recorder::default();
        run(&source, true, &["respond"], 300)
            .run(&mut host, &mut recorder)
            .unwrap();
        assert_eq!(host.responses, vec![json!({"visited":3,"kept":6-skip})]);
        assert_eq!(recorder.values[0]["effect"], "respond");
        assert_eq!(recorder.values[1]["outcome"]["status"], "succeeded");
        assert_eq!(recorder.values.last().unwrap()["success"], true);
        assert_eq!(host.events, 0);
    }
}

#[test]
fn nested_for_each_continue_and_break_use_nearest_loop() {
    // The outer Continue follows completed inner loops, as in the header-row failure.
    let source = "pairs equals 0\nkept equals 0\nFor each row in list of 1, 2, 3:\n  For each token in list of 1, 2, 3:\n    If token is 1:\n      Continue\n    If token is 3:\n      Break\n    pairs equals pairs plus 1\n  If row is 1:\n    For each column in list of 1, 2:\n      Continue\n    Continue\n  If row is 3:\n    Break\n  kept equals kept plus row\nRespond with record with pairs as pairs and kept as kept";
    let mut host = Host::default();
    let mut recorder = Recorder::default();
    run(source, true, &["respond"], 500)
        .run(&mut host, &mut recorder)
        .unwrap();
    assert_eq!(host.responses, vec![json!({"pairs":3,"kept":2})]);
    assert_eq!(recorder.values.last().unwrap()["success"], true);
}

#[test]
fn while_and_until_continue_recheck_conditions_and_preserve_break() {
    for condition in ["While n is less than 3", "Until n is 3"] {
        for stop in ["", "  If n is 2:\n    Break\n"] {
            let source = format!(
                "n equals 0\nkept equals 0\n{condition}:\n  n equals n plus 1\n  For each token in list of 1, 2:\n    Continue\n{stop}  If n is 1:\n    Continue\n  kept equals kept plus n\nRespond with record with n as n and kept as kept"
            );
            let mut host = Host::default();
            let mut recorder = Recorder::default();
            run(&source, true, &["respond"], 300)
                .run(&mut host, &mut recorder)
                .unwrap();
            let expected = if stop.is_empty() {
                json!({"n":3,"kept":5})
            } else {
                json!({"n":2,"kept":0})
            };
            assert_eq!(host.responses, vec![expected]);
            assert_eq!(recorder.values.last().unwrap()["success"], true);
        }
    }
}

#[test]
fn for_each_continue_keeps_instruction_and_response_governance() {
    let source = "For each item in list of 1, 2, 3:\n  Continue\nRespond with \"done\"";
    // 7 setup + 3 * (6 item selection + 1 Continue + 5 increment/back edge)
    // + 3 final condition + 2 response instructions. No extra VM instructions.
    for (allow, effects, limit, success) in [
        (true, vec!["respond"], 48, true),
        (true, vec!["respond"], 47, false),
        (false, vec!["respond"], 48, false),
        (true, vec![], 48, false),
    ] {
        let mut host = Host::default();
        let mut recorder = Recorder::default();
        let result = run(source, allow, &effects, limit).run(&mut host, &mut recorder);
        if success {
            assert!(result.is_ok());
            assert_eq!(host.responses, vec![json!("done")]);
        } else {
            assert_eq!(result, Err(RunError::Execution));
            assert!(host.responses.is_empty());
        }
        assert_eq!(recorder.values.last().unwrap()["success"], success);
    }
    for source in ["While true:\n  Continue", "Until false:\n  Continue"] {
        let mut host = Host::default();
        let mut recorder = Recorder::default();
        assert_eq!(
            run(source, true, &[], 48).run(&mut host, &mut recorder),
            Err(RunError::Execution)
        );
        assert_eq!(recorder.values.last().unwrap()["success"], false);
    }
}

#[test]
fn only_approved_responses_leave_private_vm_and_no_debug_events_escape() {
    for source in [
        "Ask \"Secret?\" as secret\nx equals secret",
        "Ask \"Secret?\" as secret\nRespond with \"Public response\"",
    ] {
        // Arrange: private input remains in the VM context.
        let mut host = Host::default();
        let mut recorder = Recorder::default();
        // Act: use the same runner as the verified CLI.
        let completion = run(source, true, &["respond"], 1000)
            .run(&mut host, &mut recorder)
            .unwrap();
        // Assert: returned metadata and ordinary logs cannot contain the private envelope/reason.
        assert_eq!(completion.response_emitted, !host.responses.is_empty());
        assert!(!completion.paused);
        assert_eq!(host.events, 0);
        assert!(!serde_json::to_string(&completion)
            .unwrap()
            .contains(PRIVATE));
        assert!(!serde_json::to_string(&recorder.values)
            .unwrap()
            .contains(PRIVATE));
        assert!(!serde_json::to_string(&host.responses)
            .unwrap()
            .contains(PRIVATE));
        assert_eq!(recorder.values.last().unwrap()["success"], true);
    }
}
#[test]
fn denial_and_permission_intersection_never_reach_response_sink() {
    for (allow, effects) in [(false, vec!["respond"]), (true, vec![])] {
        let mut host = Host::default();
        let mut recorder = Recorder::default();
        let error = run("Respond with \"private\"", allow, &effects, 1000)
            .run(&mut host, &mut recorder)
            .unwrap_err();
        assert_eq!(error, RunError::Execution);
        assert!(!error.to_string().contains(PRIVATE));
        assert!(host.responses.is_empty());
        assert_eq!(recorder.values[0]["allow"], false);
        assert_eq!(recorder.values.last().unwrap()["success"], false);
    }
}
#[test]
fn decision_outcome_and_terminal_record_failures_never_report_success() {
    for fail in 0..3 {
        let mut host = Host::default();
        let mut recorder = Recorder {
            fail_at: Some(fail),
            ..Default::default()
        };
        let error = run("Respond with \"public\"", true, &["respond"], 1000)
            .run(&mut host, &mut recorder)
            .unwrap_err();
        assert_eq!(error, RunError::Recording);
        assert!(!error.to_string().contains(PRIVATE));
        assert_eq!(host.responses.len(), usize::from(fail > 0));
        assert!(!recorder
            .values
            .iter()
            .any(|v| v["type"] == "policy_run_finished"));
    }
}
#[test]
fn errors_and_checkpoint_contents_are_withheld() {
    for source in [
        "Fail with \"SYNTHETIC_PRIVATE_DATA\"",
        "Respond with \"public\"",
    ] {
        let mut host = Host {
            fail: true,
            ..Default::default()
        };
        let mut recorder = Recorder::default();
        let error = run(source, true, &["respond"], 1000)
            .run(&mut host, &mut recorder)
            .unwrap_err();
        assert_eq!(error, RunError::Execution);
        assert!(!error.to_string().contains(PRIVATE));
        assert!(!serde_json::to_string(&recorder.values)
            .unwrap()
            .contains(PRIVATE));
    }
    let mut host = Host::default();
    let mut recorder = Recorder::default();
    let result = run(
        "Ask \"Secret?\" as secret\nCheckpoint \"SYNTHETIC_PRIVATE_DATA\" saving context as state",
        true,
        &[],
        1000,
    )
    .run(&mut host, &mut recorder)
    .unwrap();
    assert_eq!(
        result,
        Completion {
            response_emitted: false,
            paused: true
        }
    );
    assert_eq!(recorder.values.last().unwrap()["paused"], true);
    assert!(!serde_json::to_string(&recorder.values)
        .unwrap()
        .contains(PRIVATE));
}
#[test]
fn bounded_execution_and_initialization_fail_closed() {
    let mut host = Host::default();
    let mut recorder = Recorder::default();
    let error = run("While true:\n  x equals 1", true, &[], 20)
        .run(&mut host, &mut recorder)
        .unwrap_err();
    assert_eq!(error, RunError::Execution);
    assert_eq!(recorder.values.last().unwrap()["success"], false);
    let policy = EffectPolicy::new(compile("Rule:\n  id: test.runner\n  version: 1.0.0\nRespond with record with true as allow and \"ok\" as reason")).unwrap();
    for limit in [0, 10_000_001] {
        assert!(matches!(
            GovernedRun::new(
                compile("x equals 1"),
                json!({}),
                policy.clone(),
                limit,
                Default::default()
            ),
            Err(RunError::InvalidControls)
        ));
    }
    assert!(matches!(
        GovernedRun::new(
            json!({"format":PRIVATE}),
            json!({}),
            policy,
            100,
            Default::default()
        ),
        Err(RunError::Initialization)
    ));
}
#[test]
fn raw_evidence_requires_explicit_trusted_opt_in() {
    for raw in [false, true] {
        let mut host = Host::default();
        let mut recorder = Recorder::default();
        let runner = run("Respond with \"public\"", true, &["respond"], 1000);
        let runner = if raw {
            runner.with_replay_evidence()
        } else {
            runner
        };
        runner.run(&mut host, &mut recorder).unwrap();
        assert_eq!(recorder.values[0].get("request").is_some(), raw);
        assert_eq!(recorder.values[1].get("exchange").is_some(), raw);
        // Even replay opt-in cannot copy the policy's raw denial/allow reason.
        assert!(!serde_json::to_string(&recorder.values)
            .unwrap()
            .contains(PRIVATE));
    }
}

#[test]
fn caught_recorder_failure_still_blocks_later_effects_and_completion() {
    let source = "Try:\n  Respond with \"first\"\nOtherwise:\n  x equals 1\nTry:\n  Respond with \"second\"\nOtherwise:\n  x equals 2";
    for fail in [0, 1] {
        let mut host = Host::default();
        let mut recorder = Recorder {
            fail_at: Some(fail),
            ..Default::default()
        };
        let result = run(source, true, &["respond"], 1000).run(&mut host, &mut recorder);
        assert_eq!(result, Err(RunError::Recording));
        assert_eq!(host.responses.len(), fail);
        assert!(!recorder
            .values
            .iter()
            .any(|v| v["type"] == "policy_run_finished"));
    }
}

#[test]
fn caught_host_failure_cannot_refund_effect_budget() {
    let source = "Try:\n  Respond with \"first\"\nOtherwise:\n  Respond with \"retry\"";
    let mut host = Host {
        fail: true,
        ..Host::default()
    };
    let mut recorder = Recorder::default();
    let runner = run(source, true, &["respond"], 1000)
        .with_effect_budget(
            devlish_vm::effect_budget::EffectBudget::parse(
                &json!({"total":1,"per_effect":{"respond":1}}),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        runner.run(&mut host, &mut recorder),
        Err(RunError::Execution)
    );
    assert_eq!(host.responses, vec![json!("first")]);
    let decisions: Vec<_> = recorder
        .values
        .iter()
        .filter(|r| r["type"] == "effect_decision")
        .collect();
    assert_eq!(decisions.len(), 2);
    assert_eq!(decisions[0]["allow"], true);
    assert_eq!(decisions[1]["allow"], false);
}
