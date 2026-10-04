//! Repeatable, explicitly scoped tamper-evidence reports. No live replay effects.
use devlish_core::{integrity::read_regular_file, sha256_hex};
use devlish_vm::{
    policy::{EffectPolicy, PolicyHost, PolicyRecorder},
    HostEffects, Vm,
};
use serde_json::{json, Value};
use std::collections::{BTreeSet, VecDeque};
use std::io::Write;
use std::path::{Path, PathBuf};

fn hash(value: &Value) -> String {
    sha256_hex(&serde_json::to_vec(value).expect("JSON serializes"))
}
fn artifact_hash(value: &Value) -> String {
    sha256_hex(&serde_json::to_vec_pretty(value).expect("JSON serializes"))
}
fn read_json(path: &str) -> Result<(Vec<u8>, Value), String> {
    let bytes = read_regular_file(Path::new(path))?;
    let value =
        serde_json::from_slice(&bytes).map_err(|e| format!("invalid JSON in {path}: {e}"))?;
    Ok((bytes, value))
}
fn seal(kind: &str, passed: bool, details: Value) -> Value {
    let mut report = json!({"format":"devlish-compliance-report", "format_version":1,
        "kind":kind, "passed":passed, "assurance":"tamper-evidence",
        "signature_verified":false, "details":details});
    report["report_sha256"] = json!(hash(&report));
    report
}
fn verify(report: &Value, expected_kind: Option<&str>) -> Result<String, String> {
    if report["format"] != "devlish-compliance-report"
        || report["format_version"] != 1
        || report["passed"].as_bool().is_none()
        || ![
            "application",
            "policy",
            "process",
            "receipt-issuer",
            "verification",
        ]
        .iter()
        .any(|kind| report["kind"] == *kind)
        || expected_kind.is_some_and(|kind| report["kind"] != kind)
    {
        return Err("unsupported report format or kind".into());
    }
    let mut body = report.clone();
    let digest = body
        .as_object_mut()
        .ok_or("report must be an object")?
        .remove("report_sha256")
        .and_then(|v| v.as_str().map(str::to_owned))
        .ok_or("report missing report_sha256")?;
    if hash(&body) != digest {
        return Err("report digest mismatch: report was altered".into());
    }
    Ok(digest)
}

pub fn run(mut args: Vec<String>) -> Result<(), String> {
    const USAGE: &str = "Usage: devlish report application <manifest.json> | policy <compiled-policy.json> <cases.json> | process <compiled-program.json> <compiled-policy.json> <input.json> <evidence.jsonl> <application-report.json> <policy-report.json> | receipt-issuer <compiled-policy.json> <journal.jsonl> --sha256 <retained-journal-digest> | explain <report.json> | verify <report.json> [--sha256 <trusted-report-digest>] [--output <new-file>]";
    let mut output = None;
    if let Some(index) = args.iter().position(|s| s == "--output") {
        output = Some(PathBuf::from(
            args.get(index + 1).ok_or("--output requires a path")?,
        ));
        args.drain(index..=index + 1);
    }
    if args.get(1).map(String::as_str) == Some("explain") {
        if args.len() != 3 || output.is_some() {
            return Err("Usage: devlish report explain <report.json>".into());
        }
        let (_, report) = read_json(&args[2])?;
        verify(&report, None)?;
        let source = include_str!("../../../examples/reports/explain.dvl");
        let compiled = devlish_core::compile_source_to_json(
            source,
            devlish_core::CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .map_err(|e| e.to_string())?;
        let package = serde_json::from_str(&compiled).map_err(|e| e.to_string())?;
        let summary = super::run_case_capture(&package, &json!({"report":report}))?;
        println!(
            "{}",
            summary
                .as_str()
                .ok_or("report explanation did not return text")?
        );
        return if report["passed"] == true {
            Ok(())
        } else {
            Err("report checks failed".into())
        };
    }
    let mut report = match args.get(1).map(String::as_str) {
        Some("application") if args.len() == 3 => application(&args[2])?,
        Some("policy") if args.len() == 4 => policy(&args[2], &args[3])?,
        Some("process") if args.len() == 8 => process(&args[2..])?,
        #[cfg(feature = "native")]
        Some("receipt-issuer") if args.len() == 6 && args[4] == "--sha256" => {
            let policy = devlish_audit::read_bounded(
                Path::new(&args[2]),
                devlish_audit::MAX_ARTIFACT_BYTES,
            )?;
            let journal = devlish_audit::read_bounded(
                Path::new(&args[3]),
                devlish_core::receipt_journal::MAX_JOURNAL_BYTES,
            )?;
            let details = devlish_core::receipt_journal::replay(&policy, &journal, &args[5])?;
            seal("receipt-issuer", true, details)
        }
        Some("verify") if args.len() == 3 || (args.len() == 5 && args[3] == "--sha256") => {
            let (_, report) = read_json(&args[2])?;
            let digest = verify(&report, None)?;
            if args.len() == 5 && !digest.eq_ignore_ascii_case(&args[4]) {
                return Err("report does not match the independently supplied anchor".into());
            }
            seal(
                "verification",
                true,
                json!({"verified_report_sha256":digest,
                "anchor_checked":args.len() == 5, "checks_reexecuted":false,
                "reported_checks_passed":report["passed"]}),
            )
        }
        _ => return Err(USAGE.into()),
    };
    // Identify the verifier separately from the runtime claimed by the run log.
    let verifier = std::env::current_exe().map_err(|e| e.to_string())?;
    report["verifier_file_sha256"] = json!(sha256_hex(&read_regular_file(&verifier)?));
    report
        .as_object_mut()
        .ok_or("report must be an object")?
        .remove("report_sha256");
    report["report_sha256"] = json!(hash(&report));
    let bytes = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
    if let Some(path) = output {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&path)
            .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        file.write_all(&bytes)
            .and_then(|_| file.write_all(b"\n"))
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
    } else {
        println!("{}", String::from_utf8(bytes).map_err(|e| e.to_string())?);
    }
    if report["passed"] != true {
        return Err("report checks failed".into());
    }
    Ok(())
}

fn application(path: &str) -> Result<Value, String> {
    let (bytes, manifest) = read_json(path)?;
    if manifest["format"] != "devlish-application-manifest" || manifest["format_version"] != 1 {
        return Err("unsupported application manifest".into());
    }
    let files = manifest["files"]
        .as_array()
        .filter(|v| !v.is_empty())
        .ok_or("manifest needs files")?;
    let base = Path::new(path).parent().unwrap_or(Path::new("."));
    let mut ids = BTreeSet::new();
    let mut results = Vec::new();
    for file in files {
        let id = file["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or("file needs id")?;
        if !ids.insert(id) {
            return Err(format!("duplicate file id: {id}"));
        }
        let role = file["role"].as_str().ok_or("file needs role")?;
        if ![
            "runtime",
            "program",
            "policy",
            "tool",
            "source",
            "configuration",
        ]
        .contains(&role)
        {
            return Err(format!("unknown file role: {role}"));
        }
        let expected = file["sha256"]
            .as_str()
            .filter(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()))
            .ok_or("file needs a SHA-256 digest")?
            .to_ascii_lowercase();
        let path = base.join(file["path"].as_str().ok_or("file needs path")?);
        let measurement = read_regular_file(&path).map(|bytes| sha256_hex(&bytes));
        let actual = measurement.as_ref().ok();
        results.push(json!({"id":id, "role":role, "expected_sha256":expected,
            "actual_sha256":actual, "passed":actual.is_some_and(|a| a == &expected),
            "readable":measurement.is_ok()}));
    }
    if !results.iter().any(|r| r["role"] == "runtime") {
        return Err("manifest needs a runtime entry".into());
    }
    let passed = results.iter().all(|r| r["passed"] == true);
    Ok(seal(
        "application",
        passed,
        json!({"scope":"declared application file integrity",
        "manifest_file_sha256":sha256_hex(&bytes), "files":results,
        "build_provenance_verified":false, "baseline_authenticated":false}),
    ))
}

fn policy(path: &str, cases_path: &str) -> Result<Value, String> {
    let (bytes, package) = read_json(path)?;
    let policy = EffectPolicy::new(package)?;
    let (case_bytes, cases) = read_json(cases_path)?;
    let cases = cases
        .as_array()
        .filter(|c| !c.is_empty())
        .ok_or("policy report requires nonempty cases")?;
    let mut names = BTreeSet::new();
    let mut checks = Vec::new();
    for case in cases {
        let name = case["name"].as_str().ok_or("case needs name")?;
        if !names.insert(name) {
            return Err(format!("duplicate case: {name}"));
        }
        let input = case.get("input").ok_or("case needs input")?;
        let effect = input["effect"].as_str().ok_or("case input needs effect")?;
        let request = input.get("request").ok_or("case input needs request")?;
        let expected = case
            .get("expected")
            .filter(|e| {
                e["allow"].as_bool().is_some()
                    && e["reason"].as_str().is_some_and(|r| !r.trim().is_empty())
            })
            .ok_or("case expected must be a policy decision")?;
        // Fixture authority is an explicit offline input, never authentication
        // or a way to populate authority in ordinary live PolicyHost calls.
        let authority = input.get("authority").unwrap_or(&Value::Null);
        let actual = policy
            .evaluate_with_authority(effect, request, authority)
            .map(|(allow, reason)| json!({"allow":allow,"reason":reason}));
        checks.push(
            json!({"name":name, "input_sha256":hash(input), "expected_sha256":hash(expected),
            "actual_sha256":actual.as_ref().ok().map(hash), "evaluation_completed":actual.is_ok(),
            "passed":actual.as_ref().is_ok_and(|a| a == expected)}),
        );
    }
    Ok(seal(
        "policy",
        checks.iter().all(|c| c["passed"] == true),
        json!({
        "scope":"deterministic evaluation of supplied golden cases", "policy":policy.identity(),
        "policy_file_sha256":sha256_hex(&bytes), "cases_file_sha256":sha256_hex(&case_bytes), "cases":checks,
        "coverage_exhaustive":false, "authority_authenticated":false}),
    ))
}

#[derive(Default)]
struct Records(Vec<Value>);
impl PolicyRecorder for Records {
    fn record(&mut self, record: &Value) -> Result<(), String> {
        self.0.push(record.clone());
        Ok(())
    }
}
struct Exchange {
    kind: String,
    request: Value,
    result: Value,
}
struct ReplayHost {
    exchanges: VecDeque<Exchange>,
    mismatch: bool,
}
impl ReplayHost {
    fn invoke(&mut self, kind: &str, request: Value) -> Result<Value, String> {
        let next = self.exchanges.pop_front();
        let Some(next) = next.filter(|e| e.kind == kind && e.request == request) else {
            self.mismatch = true;
            return Err("replay effect mismatch".into());
        };
        if let Some(value) = next.result.get("ok") {
            return Ok(value.clone());
        }
        Err(next.result["err"]
            .as_str()
            .ok_or("invalid recorded error")?
            .to_owned())
    }
}

fn process(args: &[String]) -> Result<Value, String> {
    let (program_bytes, program) = read_json(&args[0])?;
    let (policy_bytes, policy_package) = read_json(&args[1])?;
    let (_, input) = read_json(&args[2])?;
    let log = read_regular_file(Path::new(&args[3]))?;
    let (_, application_report) = read_json(&args[4])?;
    let (_, policy_report) = read_json(&args[5])?;
    let application_anchor = verify(&application_report, Some("application"))?;
    let policy_anchor = verify(&policy_report, Some("policy"))?;
    if application_report["passed"] != true || policy_report["passed"] != true {
        return Err("process report requires passing application and policy reports".into());
    }
    let text = std::str::from_utf8(&log).map_err(|e| e.to_string())?;
    let mut previous = String::new();
    let mut records = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let mut envelope: Value = serde_json::from_str(line)
            .map_err(|e| format!("invalid log line {}: {e}", index + 1))?;
        if envelope["sequence"] != json!(index) || envelope["previous_sha256"] != previous {
            return Err(format!("broken log sequence at line {}", index + 1));
        }
        let claimed = envelope
            .as_object_mut()
            .ok_or("log envelope must be an object")?
            .remove("record_sha256")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or("missing record hash")?;
        if hash(&envelope) != claimed {
            return Err(format!("altered log record at line {}", index + 1));
        }
        previous = claimed;
        records.push(envelope["record"].clone());
    }
    let start = records.first().ok_or("empty evidence log")?;
    let finish = records.last().ok_or("empty evidence log")?;
    if start["type"] != "policy_run_started"
        || start["format_version"] != 3
        || start["capture_evidence"] != true
    {
        return Err(
            "process replay requires a version 3 log recorded with --policy-evidence".into(),
        );
    }
    if finish["type"] != "policy_run_finished" || records.len() < 2 {
        return Err("incomplete run: no completion record; effects may be unresolved".into());
    }
    let mut policy = EffectPolicy::new(policy_package)?;
    if let Some(expected) = start
        .pointer("/policy/verified_file_sha256")
        .and_then(Value::as_str)
    {
        if sha256_hex(&policy_bytes) != expected {
            return Err("policy file pin mismatch".into());
        }
        policy.set_file_digest(expected.to_owned());
    }
    if start["policy"] != *policy.identity()
        || start["program_sha256"] != artifact_hash(&program)
        || start["input_sha256"] != hash(&input)
    {
        return Err("program, policy, or input differs from recorded run".into());
    }
    if policy_report["details"]["policy_file_sha256"] != sha256_hex(&policy_bytes)
        || policy_report["details"]["policy"]["artifact_sha256"]
            != policy.identity()["artifact_sha256"]
    {
        return Err("policy report belongs to a different policy".into());
    }
    let files = application_report["details"]["files"]
        .as_array()
        .ok_or("application report missing files")?;
    for (role, digest) in [
        ("runtime", start["runtime_file_sha256"].clone()),
        ("program", json!(sha256_hex(&program_bytes))),
        ("policy", json!(sha256_hex(&policy_bytes))),
    ] {
        if !digest.is_string()
            || !files
                .iter()
                .any(|f| f["role"] == role && f["passed"] == true && f["actual_sha256"] == digest)
        {
            return Err(format!(
                "{role} is not bound to the passing application report"
            ));
        }
    }
    let mut exchanges = VecDeque::new();
    let mut expected_records = Vec::new();
    let mut index = 1;
    let mut next_id = 1;
    let mut denied = 0;
    while index < records.len() - 1 {
        let decision = &records[index];
        if decision["type"] != "effect_decision" || decision["effect_id"] != next_id {
            return Err("unexpected effect decision or id".into());
        }
        let request = decision
            .get("request")
            .ok_or("effect request evidence missing")?;
        if decision["request_sha256"] != hash(request) {
            return Err("effect request hash mismatch".into());
        }
        let kind = decision["effect"].as_str().ok_or("effect kind missing")?;
        let allow = decision["allow"]
            .as_bool()
            .ok_or("decision missing allow")?;
        expected_records.push(decision.clone());
        index += 1;
        if allow {
            let outcome = records.get(index).ok_or("effect outcome missing")?;
            if outcome["type"] != "effect_outcome"
                || outcome["effect_id"] != next_id
                || outcome["effect"] != kind
            {
                return Err(
                    "allowed effect has no matching outcome; execution may be uncertain".into(),
                );
            }
            let exchange = outcome
                .get("exchange")
                .ok_or("effect response evidence missing")?;
            let expected_outcome = if let Some(value) = exchange.get("ok") {
                if exchange.as_object().is_none_or(|o| o.len() != 1) {
                    return Err("invalid exchange".into());
                }
                json!({"status":"succeeded", "result_sha256":hash(value)})
            } else if exchange["err"].is_string()
                && exchange.as_object().is_some_and(|o| o.len() == 1)
            {
                json!({"status":"failed", "error_sha256":hash(&exchange["err"])})
            } else {
                return Err("invalid exchange".into());
            };
            if outcome["outcome"] != expected_outcome {
                return Err("effect response hash mismatch".into());
            }
            expected_records.push(outcome.clone());
            exchanges.push_back(Exchange {
                kind: kind.to_owned(),
                request: request.clone(),
                result: exchange.clone(),
            });
            index += 1;
        } else {
            denied += 1;
        }
        next_id += 1;
    }
    let mut replay_host = ReplayHost {
        exchanges,
        mismatch: false,
    };
    let mut replay_records = Records::default();
    let mut vm = Vm::new(program, input).map_err(|e| e.message)?;
    vm.set_emit_events(start["emit_events"].as_bool().ok_or("missing event mode")?);
    let guarded = PolicyHost::new(&mut replay_host, &policy, &mut replay_records).with_evidence();
    let mut guarded = if start["redact_diagnostics"] == true {
        guarded.with_redacted_diagnostics()
    } else {
        guarded
    };
    if let Some(binding) = start.get("verified_release") {
        let effects: std::collections::BTreeSet<String> =
            serde_json::from_value(binding["allowed_effects"].clone())
                .map_err(|e| format!("invalid recorded permissions: {e}"))?;
        let limit = binding["instruction_limit"]
            .as_u64()
            .filter(|limit| *limit > 0 && *limit <= 10_000_000)
            .ok_or("invalid recorded instruction limit")?;
        vm.set_instruction_limit(limit);
        guarded = guarded.with_allowed_effects(effects);
    }
    let outcome = vm.run(&mut guarded);
    let result = match &outcome {
        Ok(value) => json!({"ok":value}),
        Err(error) => json!({"err":error.message}),
    };
    let paused = result
        .pointer("/ok/is_checkpoint")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let matches = !replay_host.mismatch
        && replay_host.exchanges.is_empty()
        && replay_records.0 == expected_records
        && finish["result_sha256"] == hash(&result)
        && finish["success"] == outcome.is_ok()
        && finish["paused"] == paused;
    Ok(seal(
        "process",
        matches,
        json!({"scope":"offline reproduction of recorded policy-controlled execution",
        "application_report_sha256":application_anchor, "policy_report_sha256":policy_anchor,
        "log_file_sha256":sha256_hex(&log), "log_head_sha256":previous, "log_record_count":records.len(),
        "program_artifact_sha256":start["program_sha256"], "policy":policy.identity(),
        "input_sha256":start["input_sha256"], "replay_matches":matches,
        "execution_succeeded":outcome.is_ok(), "execution_completed":outcome.is_ok() && !paused, "task_success_verified":false,
        "denied_effects":denied, "live_effects_performed":false, "runtime_identity_independently_attested":false}),
    ))
}

impl HostEffects for ReplayHost {
    fn emit_event(&mut self, _: &Value) {}
    fn write_file(&mut self, request: &Value) -> Result<(), String> {
        self.invoke("write_file", request.clone()).and_then(|v| {
            if v.is_null() {
                Ok(())
            } else {
                self.mismatch = true;
                Err("invalid recorded unit result".into())
            }
        })
    }
    fn read_file(&mut self, request: &Value) -> Result<Value, String> {
        self.invoke("read_file", request.clone())
    }
    fn call_service(&mut self, request: &Value) -> Result<Value, String> {
        self.invoke("call_service", request.clone())
    }
    fn http_request(
        &mut self,
        method: &str,
        url: &str,
        body: &Value,
        headers: &Value,
    ) -> Result<Value, String> {
        self.invoke(
            "http_request",
            json!({"method":method,"url":url,"body":body,"headers":headers}),
        )
    }
    fn respond(&mut self, value: &Value) -> Result<(), String> {
        self.invoke("respond", json!({"value":value}))
            .and_then(|v| {
                if v.is_null() {
                    Ok(())
                } else {
                    self.mismatch = true;
                    Err("invalid recorded unit result".into())
                }
            })
    }
    fn http_download(&mut self, url: &str, path: &str) -> Result<(), String> {
        self.invoke("http_download", json!({"url":url,"path":path}))
            .and_then(|v| {
                if v.is_null() {
                    Ok(())
                } else {
                    self.mismatch = true;
                    Err("invalid recorded unit result".into())
                }
            })
    }
    fn read_xlsx_rows(&mut self, path: &str, sheet: Option<&str>) -> Result<Value, String> {
        self.invoke("read_xlsx_rows", json!({"path":path,"sheet":sheet}))
    }
    fn file_copy(&mut self, source: &str, destination: &str) -> Result<(), String> {
        self.invoke(
            "file_copy",
            json!({"source":source,"destination":destination}),
        )
        .and_then(|v| {
            if v.is_null() {
                Ok(())
            } else {
                self.mismatch = true;
                Err("invalid recorded unit result".into())
            }
        })
    }
    fn file_move(&mut self, source: &str, destination: &str) -> Result<(), String> {
        self.invoke(
            "file_move",
            json!({"source":source,"destination":destination}),
        )
        .and_then(|v| {
            if v.is_null() {
                Ok(())
            } else {
                self.mismatch = true;
                Err("invalid recorded unit result".into())
            }
        })
    }
    fn file_mkdir(&mut self, path: &str) -> Result<(), String> {
        self.invoke("file_mkdir", json!({"path":path}))
            .and_then(|v| {
                if v.is_null() {
                    Ok(())
                } else {
                    self.mismatch = true;
                    Err("invalid recorded unit result".into())
                }
            })
    }
    fn file_delete(&mut self, path: &str) -> Result<(), String> {
        self.invoke("file_delete", json!({"path":path}))
            .and_then(|v| {
                if v.is_null() {
                    Ok(())
                } else {
                    self.mismatch = true;
                    Err("invalid recorded unit result".into())
                }
            })
    }
    fn file_exists(&mut self, path: &str) -> Result<bool, String> {
        self.invoke("file_exists", json!({"path":path}))
            .and_then(|v| {
                v.as_bool().ok_or_else(|| {
                    self.mismatch = true;
                    "invalid recorded boolean result".into()
                })
            })
    }
    fn file_stat(&mut self, path: &str) -> Result<Value, String> {
        self.invoke("file_stat", json!({"path":path}))
    }
    fn file_list(&mut self, path: &str) -> Result<Value, String> {
        self.invoke("file_list", json!({"path":path}))
    }
    fn file_glob(&mut self, pattern: &str, directory: &str) -> Result<Value, String> {
        self.invoke(
            "file_glob",
            json!({"pattern":pattern,"directory":directory}),
        )
    }
    fn llm_complete(&mut self, request: &Value) -> Result<Value, String> {
        self.invoke("llm_complete", request.clone())
    }
    fn clock_now(&mut self, kind: &str) -> Result<Value, String> {
        self.invoke("clock_now", json!({"kind":kind}))
    }
    fn random_draw(&mut self, request: &Value) -> Result<Value, String> {
        self.invoke("random_draw", request.clone())
    }
}
