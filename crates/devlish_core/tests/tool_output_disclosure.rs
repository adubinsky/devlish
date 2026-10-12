use devlish_core::{compile_source_to_json, CompileOptions};
use devlish_vm::policy::EffectPolicy;
use serde_json::Value;

fn policy() -> EffectPolicy {
    let compiled = compile_source_to_json(
        include_str!("../../../examples/tool_output_disclosure/authorize.dvl"),
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
        "../../../examples/tool_output_disclosure/cases.json"
    ))
    .unwrap()
}

#[test]
fn public_capture_disclosure_and_private_data_denials_are_repeatable() {
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
            first.0,
            case["expected"]["allow"].as_bool().unwrap(),
            "{}: {}",
            case["name"],
            first.1
        );
        assert_eq!(first.1, case["expected"]["reason"].as_str().unwrap());
        assert_eq!(first, evaluate());
        // Reasons are fixed policy text, never a copy of either captured stream.
        for key in ["stdout", "stderr"] {
            if let Some(value) = input["request"][key].as_str().filter(|s| !s.is_empty()) {
                assert!(
                    !first.1.contains(value),
                    "reason disclosed captured content"
                );
            }
        }
    }
}

#[test]
fn ordinary_callers_cannot_provide_disclosure_authority() {
    let policy = policy();
    let case = cases().remove(0);
    let input = &case["input"];
    assert!(
        !policy
            .evaluate("disclose_tool_output", &input["request"])
            .unwrap()
            .0
    );
    let mut request = input["request"].clone();
    request["authority"] = input["authority"].clone();
    assert!(!policy.evaluate("disclose_tool_output", &request).unwrap().0);
    assert!(
        !policy
            .evaluate_with_authority("disclose_tool_output", &request, &input["authority"])
            .unwrap()
            .0
    );
}

#[test]
fn neither_stream_can_be_relabeled_by_the_proposed_request() {
    let policy = policy();
    let case = cases().remove(0);
    let input = &case["input"];
    for field in ["stdout_classification", "stderr_classification"] {
        for label in ["nppi", "company-ip", "unknown", "", "PUBLIC"] {
            let mut authority = input["authority"].clone();
            authority[field] = label.into();
            assert!(
                !policy
                    .evaluate_with_authority("disclose_tool_output", &input["request"], &authority)
                    .unwrap()
                    .0
            );
            let mut request = input["request"].clone();
            request[field] = "public".into();
            assert!(
                !policy
                    .evaluate_with_authority("disclose_tool_output", &request, &authority)
                    .unwrap()
                    .0
            );
        }
    }
}
