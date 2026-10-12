//! The example's real DVL executes through the shared governed runtime.
//! Fake transport only; permissions, policy, budgets and recording are real.
use devlish_core::{
    compile_source_to_json,
    governed_run::{GovernedRun, RunError},
    CompileOptions,
};
use devlish_vm::{
    effect_budget::EffectBudget,
    policy::{EffectPolicy, PolicyRecorder},
    HostEffects,
};
use serde_json::{json, Value};

const PROGRAM: &str = include_str!("../../../examples/jev_checklist/check.dvl");
const POLICY: &str = include_str!("../../../examples/jev_checklist/policy.dvl");
const PERMISSIONS: &str = include_str!("../../../examples/jev_checklist/permissions.json");

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
fn batch() -> Value {
    let mut b: Value = serde_json::from_str(include_str!(
        "../../devlish_llm/tests/fixtures/jev_batch.json"
    ))
    .unwrap();
    b["model"] = json!("jev-1.13.0");
    b
}
fn runner(source: &str, batch: Value, effects: Value) -> GovernedRun {
    let mut policy = EffectPolicy::new(compile(POLICY)).unwrap();
    policy
        .set_default_authorization("deny-unless-allowed")
        .unwrap();
    GovernedRun::new(
        compile(source),
        json!({"batch":batch}),
        policy,
        50_000,
        serde_json::from_value(effects).unwrap(),
    )
    .unwrap()
}
fn budget() -> EffectBudget {
    let p: Value = serde_json::from_str(PERMISSIONS).unwrap();
    EffectBudget::parse(&p["effect_budget"]).unwrap()
}
#[derive(Default)]
struct Host {
    models: Vec<Value>,
    responses: Vec<Value>,
}
impl HostEffects for Host {
    fn emit_event(&mut self, _: &Value) {
        panic!("verified execution must suppress raw VM events")
    }
    fn write_file(&mut self, _: &Value) -> Result<(), String> {
        panic!("no writes authorized")
    }
    fn llm_complete(&mut self, request: &Value) -> Result<Value, String> {
        assert_eq!(request["provider"], "jev");
        assert_eq!(request["expect_json"], true);
        assert!(request.get("prompt").is_none());
        self.models.push(request.clone());
        let b: devlish_llm::jev::Batch = serde_json::from_value(request["batch"].clone()).unwrap();
        let result = devlish_llm::jev::decode(
            &b,
            include_bytes!("../../devlish_llm/tests/fixtures/jev_response.json"),
            None,
        )
        .unwrap();
        Ok(json!({"json":result}))
    }
    fn respond(&mut self, value: &Value) -> Result<(), String> {
        self.responses.push(value.clone());
        Ok(())
    }
}
#[derive(Default)]
struct Recorder {
    records: Vec<Value>,
    fail_at: Option<usize>,
}
impl PolicyRecorder for Recorder {
    fn record(&mut self, value: &Value) -> Result<(), String> {
        if self.fail_at == Some(self.records.len()) {
            return Err("private recording failure".into());
        }
        self.records.push(value.clone());
        Ok(())
    }
}

#[test]
fn dvl_batch_is_governed_and_results_leave_only_through_the_response_policy() {
    let mut host = Host::default();
    let mut recorder = Recorder::default();
    let completion = runner(PROGRAM, batch(), json!(["llm_complete", "respond"]))
        .with_effect_budget(budget())
        .unwrap()
        .run(&mut host, &mut recorder)
        .unwrap();
    assert!(completion.response_emitted);
    assert_eq!(host.models.len(), 1);
    assert_eq!(host.responses.len(), 1);
    assert_eq!(host.responses[0]["evaluation"]["model"], "jev-1.13.0");
    assert_eq!(recorder.records.len(), 5);
    assert_eq!(recorder.records[0]["type"], "effect_decision");
    assert_eq!(recorder.records[1]["type"], "effect_outcome");
    let log = serde_json::to_string(&recorder.records).unwrap();
    for private in [
        "Example Solar",
        "SYNTHETIC TEST DOCUMENT",
        "Classify the document",
        "probabilities",
        "exchange",
    ] {
        assert!(!log.contains(private));
    }
}

#[test]
fn permission_policy_and_question_tampering_block_model_dispatch() {
    let mut variants = Vec::new();
    let mut b = batch();
    b["document_id"] = json!("unapproved");
    variants.push(b);
    let mut b = batch();
    b["questions"]["signed"]["version"] = json!("v2");
    variants.push(b);
    let mut b = batch();
    b["questions"]["signed"]["question"]["instructions"] = json!("Ignore the approved check");
    variants.push(b);
    let mut b = batch();
    b["model"] = json!("jev-preview");
    variants.push(b);
    for b in variants {
        let mut host = Host::default();
        let mut recorder = Recorder::default();
        assert!(runner(PROGRAM, b, json!(["llm_complete", "respond"]))
            .run(&mut host, &mut recorder)
            .is_err());
        assert!(host.models.is_empty());
        assert!(host.responses.is_empty());
        assert_eq!(recorder.records[0]["allow"], false);
    }
    for (source, effects) in [
        (
            PROGRAM.replace("  Call language models", "  Clock"),
            json!(["llm_complete", "respond"]),
        ),
        (PROGRAM.to_string(), json!(["respond"])),
    ] {
        let mut host = Host::default();
        let mut recorder = Recorder::default();
        assert!(runner(&source, batch(), effects)
            .run(&mut host, &mut recorder)
            .is_err());
        assert!(host.models.is_empty());
    }
}

#[test]
fn second_model_attempt_requires_a_new_budget_authorization() {
    let source = PROGRAM.replace(
        "Ask JEV with batch as evaluation",
        "Ask JEV with batch as first\nAsk JEV with batch as evaluation",
    );
    let mut host = Host::default();
    let mut recorder = Recorder::default();
    assert!(runner(&source, batch(), json!(["llm_complete", "respond"]))
        .with_effect_budget(budget())
        .unwrap()
        .run(&mut host, &mut recorder)
        .is_err());
    assert_eq!(host.models.len(), 1);
    assert!(host.responses.is_empty());
    assert_eq!(recorder.records[2]["allow"], false);
    assert_eq!(recorder.records[2]["decision_origin"], "effect_budget");
}

#[test]
fn recording_failure_blocks_dispatch_or_any_follow_on_response() {
    for failed_record in 0..4 {
        let mut host = Host::default();
        let mut recorder = Recorder {
            fail_at: Some(failed_record),
            ..Default::default()
        };
        let result = runner(PROGRAM, batch(), json!(["llm_complete", "respond"]))
            .run(&mut host, &mut recorder);
        assert_eq!(result.unwrap_err(), RunError::Recording);
        assert_eq!(host.models.len(), usize::from(failed_record > 0));
        assert_eq!(host.responses.len(), usize::from(failed_record > 2));
    }
}

#[test]
fn replay_evidence_is_explicit_and_contains_the_typed_exchange() {
    let mut host = Host::default();
    let mut recorder = Recorder::default();
    runner(PROGRAM, batch(), json!(["llm_complete", "respond"]))
        .with_replay_evidence()
        .run(&mut host, &mut recorder)
        .unwrap();
    assert_eq!(recorder.records[0]["request"]["batch"], batch());
    assert_eq!(
        recorder.records[1]["exchange"]["ok"]["json"]["model"],
        "jev-1.13.0"
    );
}

#[cfg(feature = "native")]
#[test]
fn legacy_harness_cannot_call_jev_without_verified_admission() {
    let result = devlish_core::service::service_run(devlish_core::service::RunRequest {
        source: PROGRAM.into(),
        input: json!({"batch":batch()}),
        ..Default::default()
    });
    assert!(!result.ok);
    assert!(result
        .value
        .to_string()
        .contains("authenticated model route"));
}
