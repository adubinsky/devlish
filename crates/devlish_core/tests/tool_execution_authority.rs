use devlish_core::{compile_source_to_json, CompileOptions};
use devlish_vm::policy::EffectPolicy;
use serde_json::{json, Value};

fn policy() -> EffectPolicy {
    let compiled = compile_source_to_json(
        include_str!("../../../examples/tool_execution_authority/authorize.dvl"),
        CompileOptions {
            source_path: None,
            search_paths: vec![],
        },
    )
    .unwrap();
    EffectPolicy::new(serde_json::from_str(&compiled).unwrap()).unwrap()
}
fn cases() -> Vec<Value> {
    serde_json::from_str(include_str!(
        "../../../examples/tool_execution_authority/cases.json"
    ))
    .unwrap()
}

#[test]
fn tool_execution_authority_cases_are_repeatable_and_explain_decisions() {
    let policy = policy();
    for case in cases() {
        let input = &case["input"];
        let evaluate = || {
            policy
                .evaluate_with_authority(
                    input["effect"].as_str().unwrap(),
                    &input["request"],
                    &input["authority"],
                )
                .unwrap()
        };
        let first = evaluate();
        assert_eq!(
            json!({"allow":first.0,"reason":first.1}),
            case["expected"],
            "{}",
            case["name"]
        );
        assert_eq!(first, evaluate());
    }
}

#[test]
fn caller_cannot_supply_the_separate_authority_channel() {
    let policy = policy();
    let case = cases().remove(0);
    let input = &case["input"];
    let mut request = input["request"].clone();
    assert!(
        !policy
            .evaluate("continue_initial_tool_exec", &request)
            .unwrap()
            .0
    );
    request["authority"] = input["authority"].clone();
    assert!(
        !policy
            .evaluate("continue_initial_tool_exec", &request)
            .unwrap()
            .0
    );
    assert!(
        !policy
            .evaluate_with_authority("continue_initial_tool_exec", &request, &input["authority"])
            .unwrap()
            .0
    );
}

#[test]
fn consumed_or_uncertain_host_grant_cannot_authorize_the_same_request_again() {
    let policy = policy();
    let case = cases().remove(0);
    let input = &case["input"];
    let request = &input["request"];
    let mut authority = input["authority"].clone();
    assert!(
        policy
            .evaluate_with_authority("continue_initial_tool_exec", request, &authority)
            .unwrap()
            .0
    );
    // The protected host owns this transition. This pure policy does not create
    // an atomic reservation and must never accept this state from a caller.
    for state in ["consumed", "uncertain"] {
        authority["reservation_state"] = json!(state);
        assert!(
            !policy
                .evaluate_with_authority("continue_initial_tool_exec", request, &authority)
                .unwrap()
                .0
        );
    }
}
