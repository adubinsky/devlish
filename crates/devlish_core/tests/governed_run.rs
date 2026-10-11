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
