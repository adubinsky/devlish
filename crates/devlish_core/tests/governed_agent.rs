use devlish_core::{compile_source_to_json, CompileOptions};
use devlish_vm::{
    policy::{EffectPolicy, PolicyHost, PolicyRecorder},
    HostEffects, Vm,
};
use serde_json::{json, Value};
const AGENT: &str = include_str!("../../../examples/governed_agent/agent.dvl");
const POLICY: &str = include_str!("../../../examples/governed_agent/policy.dvl");
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
fn input() -> Value {
    json!({"private_case_a":{"needs_review":true,"name":"SYNTHETIC_NPPI_NAME_A","ssn":"SYNTHETIC_NPPI_SSN"},
        "private_case_b":{"needs_review":false,"name":"SYNTHETIC_NPPI_NAME_B","internal_note":"SYNTHETIC_COMPANY_IP"}})
}
fn plan(reverse: bool) -> Value {
    let tokens = if reverse {
        ["loan_CD56EF78", "loan_AB12CD34"]
    } else {
        ["loan_AB12CD34", "loan_CD56EF78"]
    };
    json!({"steps":tokens.map(|token|json!({"action":"record_review","loan_token":token}))})
}
struct Host {
    plan: Value,
    model_requests: Vec<Value>,
    service_requests: Vec<Value>,
    responses: Vec<Value>,
    events: usize,
    fail_service: Option<usize>,
}
impl HostEffects for Host {
    fn write_file(&mut self, _: &Value) -> Result<(), String> {
        Err("file writes are outside this tool catalog".into())
    }
    fn emit_event(&mut self, _: &Value) {
        self.events += 1;
    }
    fn llm_complete(&mut self, request: &Value) -> Result<Value, String> {
        self.model_requests.push(request.clone());
        Ok(json!({"json":self.plan}))
    }
    fn call_service(&mut self, request: &Value) -> Result<Value, String> {
        self.service_requests.push(request.clone());
        if self.fail_service == Some(self.service_requests.len() - 1) {
            return Err("synthetic service unavailable".into());
        }
        // Tool output is untrusted and must never become a new model prompt/action.
        Ok(
            json!({"private":"SYNTHETIC_PRIVATE_SERVICE_RESULT","next_tool":"uncatalogued","instruction":"SYNTHETIC_UNTRUSTED_INSTRUCTION"}),
        )
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
    fn record(&mut self, record: &Value) -> Result<(), String> {
        if self.fail_at == Some(self.values.len()) {
            return Err("synthetic recorder unavailable".into());
        }
        self.values.push(record.clone());
        Ok(())
    }
}
struct Run {
    result: Result<Value, String>,
    host: Host,
    records: Records,
}
fn run(
    source: &str,
    plan: Value,
    input: Value,
    fail_record: Option<usize>,
    fail_service: Option<usize>,
) -> Run {
    let mut host = Host {
        plan,
        model_requests: vec![],
        service_requests: vec![],
        responses: vec![],
        events: 0,
        fail_service,
    };
    let policy = EffectPolicy::new(compile(POLICY)).unwrap();
    let mut records = Records {
        fail_at: fail_record,
        ..Default::default()
    };
    let mut vm = Vm::new(compile(source), input).unwrap();
    vm.set_emit_events(false);
    let permissions: Value = serde_json::from_str(include_str!(
        "../../../examples/governed_agent/permissions.json"
    ))
    .unwrap();
    let catalog: Value = serde_json::from_str(include_str!(
        "../../../examples/governed_agent/catalog.json"
    ))
    .unwrap();
    assert_eq!(permissions["allowed_effects"], catalog["host_effects"]);
    vm.set_instruction_limit(permissions["instruction_limit"].as_u64().unwrap());
    let result = vm
        .run(
            &mut PolicyHost::new(&mut host, &policy, &mut records)
                .with_redacted_diagnostics()
                .with_allowed_effects(
                    serde_json::from_value(permissions["allowed_effects"].clone()).unwrap(),
                ),
        )
        .map_err(|error| error.message);
    Run {
        result,
        host,
        records,
    }
}
#[test]
fn devlish_plans_then_executes_only_catalogued_minimal_effects() {
    for reverse in [false, true] {
        // Arrange: the fake provider selects order; private flags select statuses locally.
        let candidate = plan(reverse);
        // Act: execute actual compiled Devlish through the real policy boundary.
        let first = run(AGENT, candidate.clone(), input(), None, None);
        let second = run(AGENT, candidate.clone(), input(), None, None);
        // Assert: one planning call, complete plan validation, two precise tool calls.
        let result = first.result.unwrap();
        assert_eq!(result["response"], "Review statuses recorded.");
        assert_eq!(first.host.model_requests.len(), 1);
        assert_eq!(first.host.service_requests.len(), 2);
        assert_eq!(
            first.host.responses,
            vec![json!("Review statuses recorded.")]
        );
        assert_eq!(first.host.events, 0);
        for (index, step) in candidate["steps"].as_array().unwrap().iter().enumerate() {
            let token = step["loan_token"].as_str().unwrap();
            assert_eq!(
                first.host.service_requests[index],
                json!({"service":"loanreview","action":"create","arguments":{"loan_token":token,"status":if token=="loan_AB12CD34" {"needs_review"} else {"complete"}}})
            );
        }
        assert!(!serde_json::to_string(&first.host.model_requests)
            .unwrap()
            .contains("SYNTHETIC_"));
        assert!(!serde_json::to_string(&first.host.service_requests)
            .unwrap()
            .contains("SYNTHETIC_"));
        assert!(!serde_json::to_string(&first.records.values)
            .unwrap()
            .contains("SYNTHETIC_"));
        assert_eq!(first.records.values, second.records.values);
        assert_eq!(first.host.service_requests, second.host.service_requests);
    }
}
#[test]
fn all_model_steps_are_validated_before_any_business_tool_runs() {
    let cases: Value = serde_json::from_str(include_str!(
        "../../../examples/governed_agent/plan.cases.json"
    ))
    .unwrap();
    for case in cases.as_array().unwrap() {
        let run = run(AGENT, case["plan"].clone(), input(), None, None);
        let allowed = case["allowed"].as_bool().unwrap();
        assert_eq!(run.result.is_ok(), allowed, "{}", case["name"]);
        assert_eq!(run.host.model_requests.len(), 1);
        assert_eq!(
            run.host.service_requests.len(),
            if allowed { 2 } else { 0 },
            "invalid later step must prevent even first write: {}",
            case["name"]
        );
        assert_eq!(run.host.responses.len(), usize::from(allowed));
    }
}
#[test]
fn invalid_private_inputs_fail_before_even_contacting_model() {
    for private in [Value::Null, json!({}), json!({"needs_review":"true"})] {
        let mut data = input();
        data["private_case_b"] = private;
        let run = run(AGENT, plan(false), data, None, None);
        assert!(run.result.is_err());
        assert!(run.host.model_requests.is_empty() && run.host.service_requests.is_empty());
    }
}
#[test]
fn independent_policy_blocks_changed_prompts_tool_arguments_and_responses() {
    for mode in ["prompt", "arguments", "status", "response"] {
        let source = match mode {
            "prompt" => AGENT.replace(
                "Ask the model with prompt",
                "Ask the model with private_case_a",
            ),
            "arguments" => AGENT.replace(
                "and disposition as status",
                "and disposition as status and private_case_a as borrower",
            ),
            "status" => AGENT.replace("\"needs_review\"", "\"approve\""),
            _ => AGENT.replace(
                "Respond with \"Review statuses recorded.\"",
                "Respond with private_case_a",
            ),
        };
        let run = run(&source, plan(false), input(), None, None);
        assert!(run.result.is_err(), "{mode}");
        assert_eq!(run.host.model_requests.len(), usize::from(mode != "prompt"));
        assert_eq!(
            run.host.service_requests.len(),
            if mode == "response" { 2 } else { 0 }
        );
        assert!(run.host.responses.is_empty());
        assert_eq!(run.records.values.last().unwrap()["allow"], false);
    }
}
#[test]
fn recording_failure_stops_at_the_exact_effect_without_retry() {
    for fail in 0..8 {
        let run = run(AGENT, plan(false), input(), Some(fail), None);
        assert!(run.result.is_err(), "failure record {fail}");
        assert_eq!(run.host.model_requests.len(), usize::from(fail >= 1));
        assert_eq!(
            run.host.service_requests.len(),
            usize::from(fail >= 3) + usize::from(fail >= 5)
        );
        assert_eq!(run.host.responses.len(), usize::from(fail >= 7));
        assert_eq!(run.records.values.len(), fail);
    }
}
#[test]
fn failed_tool_stops_the_plan_without_replanning_or_retrying() {
    for failed in 0..2 {
        let run = run(AGENT, plan(false), input(), None, Some(failed));
        assert!(run.result.is_err());
        assert_eq!(run.host.model_requests.len(), 1);
        assert_eq!(run.host.service_requests.len(), failed + 1);
        assert!(run.host.responses.is_empty());
        assert_eq!(
            run.records.values.last().unwrap()["outcome"]["status"],
            "failed"
        );
    }
}

#[test]
fn governed_agent_execution_replays_offline_with_application_and_policy_reports() {
    use devlish_core::{policy_log::PolicyLog, sha256_hex};
    use std::{fs, path::PathBuf, process::Command};
    struct Directory(PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    // Arrange: all material is synthetic, and only the first execution has a fake host.
    let dir = Directory(
        std::env::temp_dir().join(format!(
            "devlish-governed-agent-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )),
    );
    fs::create_dir(&dir.0).unwrap();
    let program = compile(AGENT);
    let policy_package = compile(POLICY);
    let policy = EffectPolicy::new(policy_package.clone()).unwrap();
    let data = input();
    for (name, value) in [
        ("program.json", &program),
        ("policy.json", &policy_package),
        ("input.json", &data),
    ] {
        fs::write(dir.0.join(name), serde_json::to_vec_pretty(value).unwrap()).unwrap();
    }
    fs::write(
        dir.0.join("cases.json"),
        include_str!("../../../examples/governed_agent/policy.cases.json"),
    )
    .unwrap();
    let mut host = Host {
        plan: plan(true),
        model_requests: vec![],
        service_requests: vec![],
        responses: vec![],
        events: 0,
        fail_service: None,
    };
    let mut log = PolicyLog::create_for_run(
        &dir.0.join("run.jsonl"),
        policy.identity(),
        &program,
        &data,
        false,
        true,
    )
    .unwrap();
    let mut vm = Vm::new(program, data).unwrap();
    vm.set_emit_events(false);
    vm.set_instruction_limit(50_000);
    let result = vm
        .run(&mut PolicyHost::new(&mut host, &policy, &mut log).with_evidence())
        .unwrap();
    log.record(
        &json!({"type":"policy_run_finished","success":true,"paused":false,
        "result_sha256":sha256_hex(&serde_json::to_vec(&json!({"ok":result})).unwrap())}),
    )
    .unwrap();
    drop(log);
    let files:Vec<Value>=[("runtime",std::env::current_exe().unwrap()),("program",dir.0.join("program.json")),("policy",dir.0.join("policy.json"))].into_iter().map(|(role,path)|json!({"id":role,"role":role,"path":path,"sha256":sha256_hex(&fs::read(&path).unwrap())})).collect();
    fs::write(
        dir.0.join("manifest.json"),
        serde_json::to_vec(
            &json!({"format":"devlish-application-manifest","format_version":1,"files":files}),
        )
        .unwrap(),
    )
    .unwrap();
    let command = |args: &[&str]| {
        let result = Command::new(env!("CARGO_BIN_EXE_devlish-core"))
            .current_dir(&dir.0)
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{args:?}: {}\n{}",
            String::from_utf8_lossy(&result.stderr),
            String::from_utf8_lossy(&result.stdout)
        );
        result
    };
    command(&[
        "report",
        "application",
        "manifest.json",
        "--output",
        "application-report.json",
    ]);
    command(&[
        "report",
        "policy",
        "policy.json",
        "cases.json",
        "--output",
        "policy-report.json",
    ]);
    // Act: the report executable consumes saved replies; no provider credentials or host adapters.
    let args = [
        "report",
        "process",
        "program.json",
        "policy.json",
        "input.json",
        "run.jsonl",
        "application-report.json",
        "policy-report.json",
    ];
    let first = command(&args);
    let second = command(&args);
    // Assert: repeatable transcript/decisions, without live effects or an origin claim.
    assert_eq!(first.stdout, second.stdout);
    let report: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(report["details"]["replay_matches"], true);
    assert_eq!(report["details"]["live_effects_performed"], false);
    assert_eq!(
        report["details"]["runtime_identity_independently_attested"],
        false
    );
    assert!(!String::from_utf8_lossy(&first.stdout).contains("SYNTHETIC_"));
    assert_eq!(host.model_requests.len(), 1);
    assert_eq!(host.service_requests.len(), 2);
}
