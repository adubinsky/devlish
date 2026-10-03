use devlish_core::{compile_source_to_json, CompileOptions};
use devlish_vm::{
    policy::{EffectPolicy, PolicyHost, PolicyRecorder},
    HostEffects, Vm,
};
use serde_json::{json, Value};

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
        self.dispatched += 1;
        Ok(())
    }
    fn call_service(&mut self, _: &Value) -> Result<Value, String> {
        self.dispatched += 1;
        Ok(json!({}))
    }
    fn llm_complete(&mut self, _: &Value) -> Result<Value, String> {
        self.dispatched += 1;
        Ok(json!({}))
    }
    fn file_copy(&mut self, _: &str, _: &str) -> Result<(), String> {
        self.dispatched += 1;
        Ok(())
    }
    fn http_request(&mut self, _: &str, _: &str, _: &Value, _: &Value) -> Result<Value, String> {
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
fn disclosure_examples_enforce_every_golden_case_at_the_host_boundary() {
    for name in ["nppi", "company_ip"] {
        let base =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/data_protection");
        let source = std::fs::read_to_string(base.join(format!("{name}.dvl"))).unwrap();
        let artifact: Value = serde_json::from_str(
            &compile_source_to_json(
                &source,
                CompileOptions {
                    source_path: None,
                    search_paths: vec![],
                },
            )
            .unwrap(),
        )
        .unwrap();
        let policy = EffectPolicy::new(artifact.clone()).unwrap();
        let cases: Vec<Value> = serde_json::from_str(
            &std::fs::read_to_string(base.join(format!("{name}.cases.json"))).unwrap(),
        )
        .unwrap();
        for case in cases {
            // Arrange: all values are synthetic; no external hosts are contacted.
            let mut host = Host::default();
            let mut records = Records::default();
            let mut vm = Vm::new(artifact.clone(), case["input"].clone()).unwrap();
            let decision = vm.run(&mut Host::default()).unwrap();
            assert_eq!(
                decision["response"], case["expected"],
                "{name}: {}",
                case["name"]
            );
            let mut guarded = PolicyHost::new(&mut host, &policy, &mut records);
            let request = &case["input"]["request"];
            // Act: exercise the real interception wrapper, not just policy evaluation.
            let result = match case["input"]["effect"].as_str().unwrap() {
                "call_service" => guarded.call_service(request).map(|_| ()),
                "llm_complete" => guarded.llm_complete(request).map(|_| ()),
                "write_file" => guarded.write_file(request),
                "respond" => guarded.respond(&request["value"]),
                "file_copy" => guarded.file_copy(
                    request["source"].as_str().unwrap(),
                    request["destination"].as_str().unwrap(),
                ),
                "http_request" => guarded
                    .http_request(
                        request["method"].as_str().unwrap(),
                        request["url"].as_str().unwrap(),
                        &request["body"],
                        &request["headers"],
                    )
                    .map(|_| ()),
                effect => panic!("unhandled fixture effect: {effect}"),
            };
            // Assert: a denial must never reach any external implementation.
            let allow = case["expected"]["allow"].as_bool().unwrap();
            assert_eq!(
                result.is_ok(),
                allow,
                "{name}: {}: {result:?}",
                case["name"]
            );
            assert_eq!(host.dispatched, usize::from(allow));
            assert_eq!(records.0[0]["allow"], allow);
            assert_eq!(records.0[0]["reason"], case["expected"]["reason"]);
            let serialized = serde_json::to_string(&records.0).unwrap();
            for sensitive in ["000-00-0000", "0000123456", "private_algorithm", "MDAwLTAw"] {
                assert!(!serialized.contains(sensitive));
            }
        }
    }
}

#[test]
fn audit_explanations_preserve_quoted_separators_and_unicode() {
    // Arrange: policy text must survive compilation verbatim.
    let source = r#"Respond with record with true as allow and "NPPI, IP and café as text: \"review and approve\"." as reason"#;
    let package = serde_json::from_str(
        &compile_source_to_json(
            source,
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .unwrap(),
    )
    .unwrap();
    // Act.
    let result = Vm::new(package, json!({}))
        .unwrap()
        .run(&mut Host::default())
        .unwrap();
    // Assert.
    assert_eq!(
        result["response"],
        json!({"allow":true,"reason":"NPPI, IP and café as text: \"review and approve\"."})
    );
}
