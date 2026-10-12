use devlish_core::{compile_source_to_json, CompileOptions};
use devlish_vm::{
    policy::{EffectPolicy, PolicyHost, PolicyRecorder},
    HostEffects,
};
use serde_json::{json, Value};

fn policy() -> EffectPolicy {
    let source = include_str!("../../../examples/receipt_authority/authorize.dvl");
    let compiled = compile_source_to_json(
        source,
        CompileOptions {
            source_path: None,
            search_paths: vec![],
        },
    )
    .unwrap();
    EffectPolicy::new(serde_json::from_str(&compiled).unwrap()).unwrap()
}

#[test]
fn receipt_authorization_rules_replay_all_synthetic_cases() {
    // Arrange: authority state is synthetic and supplied independently of requests.
    let policy = policy();
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../../../examples/receipt_authority/cases.json"
    ))
    .unwrap();
    for case in cases {
        let input = &case["input"];
        // Act: evaluate twice without any signing or external capability.
        let evaluate = || {
            policy
                .evaluate_with_authority(
                    input["effect"].as_str().unwrap(),
                    &input["request"],
                    &input["authority"],
                )
                .unwrap()
        };
        let (allow, reason) = evaluate();
        // Assert: English explanations are stable and never copy caller data.
        assert_eq!(
            json!({"allow":allow,"reason":reason}),
            case["expected"],
            "{}",
            case["name"]
        );
        assert_eq!((allow, reason), evaluate());
    }
}

#[derive(Default)]
struct Host {
    dispatched: usize,
}
impl HostEffects for Host {
    fn emit_event(&mut self, _: &Value) {}
    fn write_file(&mut self, _: &Value) -> Result<(), String> {
        self.dispatched += 1;
        Ok(())
    }
    fn respond(&mut self, _: &Value) -> Result<(), String> {
        Ok(())
    }
    fn call_service(&mut self, _: &Value) -> Result<Value, String> {
        self.dispatched += 1;
        Ok(json!({}))
    }
}
#[derive(Default)]
struct Records(Vec<Value>);
impl PolicyRecorder for Records {
    fn record(&mut self, record: &Value) -> Result<(), String> {
        self.0.push(record.clone());
        Ok(())
    }
}

#[test]
fn ordinary_host_cannot_gain_authority_through_caller_json() {
    // Arrange: even a copy of a passing authority descriptor is only caller data.
    let policy = policy();
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../../../examples/receipt_authority/cases.json"
    ))
    .unwrap();
    let mut request = cases[0]["input"]["request"].clone();
    let authority = cases[0]["input"]["authority"].clone();
    // Act and assert: the default evaluation path has no authority context.
    assert!(!policy.evaluate("issue_audit_receipt", &request).unwrap().0);
    request["authority"] = authority.clone();
    assert!(!policy.evaluate("issue_audit_receipt", &request).unwrap().0);
    let mut host = Host::default();
    let mut records = Records::default();
    let mut guarded = PolicyHost::new(&mut host, &policy, &mut records);
    assert!(guarded
        .call_service(&json!({
            "service":"ReceiptSigner","action":"issue_audit_receipt",
            "arguments":request,"authority":authority
        }))
        .is_err());
    assert_eq!(host.dispatched, 0);
    assert_eq!(records.0[0]["allow"], false);
}
