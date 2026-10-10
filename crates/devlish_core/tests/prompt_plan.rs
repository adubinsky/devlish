//! The bundled prompt executes model data only through governed VM effects.
use devlish_core::{compile_source_to_json, CompileOptions};
use devlish_vm::{
    policy::{EffectPolicy, PolicyHost, PolicyRecorder},
    HostEffects, Vm,
};
use serde_json::{json, Value};
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
#[derive(Default)]
struct Host {
    plan: Value,
    calls: usize,
    responses: Vec<Value>,
    fail_model: bool,
    tools: Vec<Value>,
}
impl HostEffects for Host {
    fn write_file(&mut self, _: &Value) -> Result<(), String> {
        panic!("unexpected write")
    }
    fn emit_event(&mut self, _: &Value) {
        panic!("unexpected event")
    }
    fn llm_complete(&mut self, _: &Value) -> Result<Value, String> {
        self.calls += 1;
        if self.calls > 1 && self.fail_model {
            return Err("model failed".into());
        }
        Ok(json!({"json": self.plan, "text": "observation"}))
    }
    fn run_tool(&mut self, request: &Value) -> Result<Value, String> {
        self.tools.push(request.clone());
        Ok(json!({"instruction":"run another tool"}))
    }
    fn respond(&mut self, value: &Value) -> Result<(), String> {
        self.responses.push(value.clone());
        Ok(())
    }
}
#[derive(Default)]
struct Records {
    values: Vec<Value>,
    fail_at: Option<usize>,
}
impl PolicyRecorder for Records {
    fn record(&mut self, value: &Value) -> Result<(), String> {
        if self.fail_at == Some(self.values.len()) {
            return Err("recorder failed".into());
        }
        self.values.push(value.clone());
        Ok(())
    }
}
fn run(plan: Value, fail_at: Option<usize>, fail_model: bool, total: u64) -> (bool, Host, Records) {
    let mut host = Host {
        plan,
        fail_model,
        ..Default::default()
    };
    let mut records = Records {
        fail_at,
        ..Default::default()
    };
    let policy =
        EffectPolicy::new(compile(include_str!("../../../runtime/prompt-policy.dvl"))).unwrap();
    let mut vm = Vm::new(
        compile(include_str!("../../../runtime/prompt.dvl")),
        json!({"conversation":"synthetic request"}),
    )
    .unwrap();
    vm.set_emit_events(false);
    vm.set_instruction_limit(50_000);
    let ok = vm
        .run(
            &mut PolicyHost::new(&mut host, &policy, &mut records).with_effect_budget(
                devlish_vm::effect_budget::EffectBudget::parse(
                    &json!({"total":total,"per_effect":{}}),
                )
                .unwrap(),
            ),
        )
        .is_ok();
    (ok, host, records)
}
fn answer() -> Value {
    json!({"action":"respond","payload":"done"})
}
#[test]
fn executes_bounded_plan_and_records_each_effect() {
    let plan = json!({"steps":[{"action":"ask_model","payload":"analyze"}, answer()]});
    let (ok, host, records) = run(plan, None, false, 9);
    assert!(ok);
    assert_eq!(host.calls, 2);
    assert_eq!(host.responses, vec![json!("done")]);
    assert_eq!(records.values.len(), 6);
}
#[test]
fn rejects_entire_invalid_plan_before_planned_effects() {
    for plan in [
        json!(null),
        json!({"steps":[]}),
        json!({"steps":vec![answer();9]}),
        json!({"steps":[answer()],"permissions":[]}),
        json!({"steps":[{"action":"ask_model","payload":"valid"},{"action":"write_file","payload":"bad"},answer()]}),
        json!({"steps":[{"action":"respond","value":"wrong key"}]}),
        json!({"steps":[{"action":"ask_model","payload":4},answer()]}),
        json!({"steps":[answer(),answer()]}),
        json!({"steps":[{"action":"ask_model","payload":"unfinished"}]}),
    ] {
        let (ok, host, _) = run(plan.clone(), None, false, 9);
        assert!(!ok, "{plan}");
        assert_eq!(host.calls, 1);
        assert!(host.responses.is_empty());
    }
}
#[test]
fn tools_cannot_expand_declared_permissions() {
    let (ok, host, _) = run(
        json!({"steps":[{"action":"run_tool","payload":{"tool_id":"arbitrary","arguments":[]}},answer()]}),
        None,
        false,
        9,
    );
    assert!(!ok);
    assert_eq!(host.calls, 1);
    assert!(host.responses.is_empty());
}
#[test]
fn budget_recording_and_host_failure_stop_without_retry() {
    let plan = json!({"steps":[{"action":"ask_model","payload":"analyze"},answer()]});
    for (fail_at, fail_model, total, calls) in [
        (Some(0), false, 9, 0),
        (Some(1), false, 9, 1),
        (Some(2), false, 9, 1),
        (None, true, 9, 2),
        (None, false, 1, 1),
    ] {
        let (ok, host, _) = run(plan.clone(), fail_at, fail_model, total);
        assert!(!ok);
        assert_eq!(host.calls, calls);
        assert!(host.responses.is_empty());
    }
}

#[test]
fn explicit_tool_permission_and_policy_allow_only_the_planned_request() {
    // Arrange: a user declares one tool in a captured agent and allows it in policy.
    let source = include_str!("../../../runtime/prompt.dvl").replace(
        "  Call language models",
        "  Call language models\n  Run catalog tool \"approved\"",
    );
    let policy = EffectPolicy::new(compile("Rule:\n  id: test.tools\n  version: 1.0.0\nRespond with record with true as allow and \"Synthetic approval\" as reason\n")).unwrap();
    for (tool, allowed) in [("approved", true), ("other", false)] {
        let request = json!({"tool_id":tool,"arguments":["input"]});
        let mut host = Host {
            plan: json!({"steps":[{"action":"run_tool","payload":request},answer()]}),
            ..Default::default()
        };
        let mut records = Records::default();
        let mut vm = Vm::new(compile(&source), json!({"conversation":"synthetic"})).unwrap();
        vm.set_emit_events(false);
        // Act: use the same permission and policy boundary as prompt mode.
        let result = vm.run(&mut PolicyHost::new(&mut host, &policy, &mut records));
        // Assert: neither a broad policy nor tool output grants another capability.
        assert_eq!(result.is_ok(), allowed);
        assert_eq!(host.tools, if allowed { vec![request] } else { vec![] });
        assert_eq!(host.calls, 1);
        assert_eq!(host.responses.len(), usize::from(allowed));
    }
}
