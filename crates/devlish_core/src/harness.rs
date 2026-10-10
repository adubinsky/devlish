//! `devlish harness` — outbound LLM-driven sessions.

use crate::devlish_search_paths_for;
use devlish_core::service::{service_run, RunRequest};
use devlish_llm::LlmConfig;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn run_harness(args: Vec<String>) -> Result<(), String> {
    if args.len() < 2 {
        return Err(harness_usage());
    }
    match args[1].as_str() {
        "run" => harness_run(args),
        "generate" => harness_generate(args),
        "resume" => harness_resume(args),
        "init-config" => {
            let path = LlmConfig::ensure_default_file()?;
            println!("wrote {}", path.display());
            Ok(())
        }
        "--help" | "-h" | "help" => {
            println!("{}", harness_usage());
            Ok(())
        }
        other => Err(format!(
            "unknown harness subcommand: {other}\n\n{}",
            harness_usage()
        )),
    }
}

fn harness_usage() -> String {
    "Usage:\n  devlish harness generate <contract.txt> --output-dir DIR --policy-log FILE [--provider NAME] [--model NAME]\n  \
     devlish harness run <file.dvl> [--provider NAME] [--model NAME] [--input JSON] [--env KEY=VALUE]\n             \
     [--policy FILE --policy-log FILE] [--default-authorization deny-unless-allowed|allow-unless-forbidden]\n  \
     devlish harness resume <session.json> [--input JSON]\n             \
     [--policy FILE --policy-log FILE] [--default-authorization deny-unless-allowed|allow-unless-forbidden]\n  \
     devlish harness init-config\n"
        .to_string()
}

/// Authoring is itself a governed workflow: model output is data, never executed.
/// Only this fixed workflow can write the two requested artifact paths.
fn generation_sources(program: &str, policy: &str) -> (String, String) {
    let program = serde_json::to_string(program).unwrap();
    let policy = serde_json::to_string(policy).unwrap();
    let workflow = format!(
        "Permissions:\n  Call language models\n  Write files to {program}\n  Write files to {policy}\n\nAsk the model with prompt expecting json as draft\npayload equals draft\nRequire payload has fields program, policy\nartifact shape equals record with \"text\" as program and \"text\" as policy\nRequire payload matches shape artifact shape\nWrite program of payload to {program}\nWrite policy of payload to {policy}\nRespond with payload\n"
    );
    let guard = format!(
        "Rule:\n  id: harness.authoring\n  version: 1.0.0\n\nAsk \"Effect?\" as effect\nAsk \"Request?\" as request\nIf effect equals \"llm_complete\" or effect equals \"respond\":\n  Respond with record with true as allow and \"Harness authoring effect.\" as reason\nIf effect equals \"write_file\":\n  If path of request equals {program} or path of request equals {policy}:\n    Respond with record with true as allow and \"Harness artifact output.\" as reason\nRespond with record with false as allow and \"Outside harness authoring contract.\" as reason\n"
    );
    (workflow, guard)
}

fn harness_generate(args: Vec<String>) -> Result<(), String> {
    let mut contract = None;
    let mut output = None;
    let mut log = None;
    let mut provider = None;
    let mut model = None;
    let mut index = 2;
    while index < args.len() {
        match args[index].as_str() {
            "--output-dir" => {
                output = Some(PathBuf::from(take_value(
                    &args,
                    &mut index,
                    "--output-dir",
                )?))
            }
            "--policy-log" => {
                log = Some(PathBuf::from(take_value(
                    &args,
                    &mut index,
                    "--policy-log",
                )?))
            }
            "--provider" => provider = Some(take_value(&args, &mut index, "--provider")?),
            "--model" => model = Some(take_value(&args, &mut index, "--model")?),
            flag if flag.starts_with('-') => return Err(format!("unknown option: {flag}")),
            path => {
                if contract.replace(PathBuf::from(path)).is_some() {
                    return Err("expected one contract file".into());
                }
            }
        }
        index += 1;
    }
    let contract = contract.ok_or_else(harness_usage)?;
    let output = output.ok_or("--output-dir is required")?;
    let log = log.ok_or("--policy-log is required")?;
    // Canonicalize an existing operator-selected directory; never take paths from the model.
    let output = output
        .canonicalize()
        .map_err(|e| format!("output directory: {e}"))?;
    let program = output.join("program.dvl");
    let policy = output.join("policy.dvl");
    let log_parent = log
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let log_path = log_parent
        .canonicalize()
        .map_err(|e| format!("policy log directory: {e}"))?
        .join(log.file_name().ok_or("policy log must name a file")?);
    if log_path == program || log_path == policy {
        return Err("policy log must be separate from generated artifacts".into());
    }
    for path in [&program, &policy] {
        if path.symlink_metadata().is_ok() {
            return Err(format!("refusing to replace {}", path.display()));
        }
    }
    let contract = fs::read_to_string(contract).map_err(|e| format!("contract: {e}"))?;
    let prompt = format!(
        "Author a Devlish program and an independent deny-by-default effect policy for this contract. Return only a JSON object with string fields program and policy. Do not run the artifacts. Use only the documented language. Policy input variables are effect and request; respond with a record containing boolean allow and a fixed nonempty reason.\n\nCONTRACT:\n{contract}\n\nLANGUAGE REFERENCE:\n{}\n\nGRAMMAR:\n{}\n\nEFFECT POLICY:\n{}",
        include_str!("../../../docs/LANGUAGE_REFERENCE.md"),
        include_str!("../../../docs/LANGUAGE_GRAMMAR.ebnf"),
        include_str!("../../../docs/EFFECT_POLICY.md"),
    );
    let (source, guard) = generation_sources(
        program.to_str().ok_or("output path must be UTF-8")?,
        policy.to_str().ok_or("output path must be UTF-8")?,
    );
    let session_dir = sessions_dir()?.join(new_session_id());
    fs::create_dir(&session_dir)
        .or_else(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                fs::create_dir_all(session_dir.parent().unwrap())?;
                fs::create_dir(&session_dir)
            } else {
                Err(e)
            }
        })
        .map_err(|e| format!("session directory: {e}"))?;
    let guard_path = session_dir.join("authoring-policy.dvl");
    fs::write(&guard_path, guard).map_err(|e| format!("authoring policy: {e}"))?;
    let result = service_run(RunRequest {
        source: source.clone(),
        source_path: Some(
            output
                .join("harness-authoring.dvl")
                .to_string_lossy()
                .into_owned(),
        ),
        input: json!({"prompt": prompt}),
        provider,
        model,
        policy_path: Some(guard_path),
        policy_log: Some(log),
        default_authorization: Some("deny-unless-allowed".into()),
        ..RunRequest::default()
    });
    let session_path = session_dir.join("session.json");
    let session = json!({"kind": "harness-generation", "source": source,
        "program_path": program, "policy_path": policy, "ok": result.ok, "result": result.value});
    fs::write(
        &session_path,
        serde_json::to_vec_pretty(&session).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("generation session: {e}"))?;
    println!(
        "{}",
        json!({"ok": result.ok, "session_path": session_path, "program_path": program, "policy_path": policy})
    );
    if !result.ok {
        return Err("harness generation failed; inspect the local session and policy log".into());
    }
    Ok(())
}

#[derive(Default)]
struct PolicyArgs {
    policy_path: Option<PathBuf>,
    policy_log: Option<PathBuf>,
    default_authorization: Option<String>,
}

fn take_value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn harness_run(args: Vec<String>) -> Result<(), String> {
    let mut file: Option<PathBuf> = None;
    let mut provider: Option<String> = None;
    let mut model: Option<String> = None;
    let mut input = json!({});
    let mut env = Vec::new();
    let mut policy = PolicyArgs::default();
    let mut index = 2usize;
    while index < args.len() {
        match args[index].as_str() {
            "--provider" => {
                provider = Some(take_value(&args, &mut index, "--provider")?);
            }
            "--model" => {
                model = Some(take_value(&args, &mut index, "--model")?);
            }
            "--input" => {
                let raw = take_value(&args, &mut index, "--input")?;
                input =
                    serde_json::from_str(&raw).map_err(|e| format!("invalid --input JSON: {e}"))?;
            }
            "--env" => {
                let raw = take_value(&args, &mut index, "--env")?;
                let (k, v) = raw
                    .split_once('=')
                    .ok_or_else(|| format!("invalid --env {raw}"))?;
                env.push((k.to_string(), v.to_string()));
            }
            "--policy" => {
                policy.policy_path =
                    Some(PathBuf::from(take_value(&args, &mut index, "--policy")?));
            }
            "--policy-log" => {
                policy.policy_log = Some(PathBuf::from(take_value(
                    &args,
                    &mut index,
                    "--policy-log",
                )?));
            }
            "--default-authorization" => {
                policy.default_authorization =
                    Some(take_value(&args, &mut index, "--default-authorization")?);
            }
            value if value.starts_with('-') => return Err(format!("unknown option: {value}")),
            value => {
                if file.is_some() {
                    return Err(format!("unexpected argument: {value}"));
                }
                file = Some(PathBuf::from(value));
            }
        }
        index += 1;
    }
    let file = file.ok_or_else(harness_usage)?;
    let source =
        fs::read_to_string(&file).map_err(|e| format!("failed to read {}: {e}", file.display()))?;
    let source_path = file.to_string_lossy().to_string();
    let result = service_run(RunRequest {
        source,
        source_path: Some(source_path.clone()),
        input: input.clone(),
        env,
        provider,
        model,
        search_paths: devlish_search_paths_for(Some(file.as_path())),
        policy_path: policy.policy_path,
        policy_log: policy.policy_log,
        default_authorization: policy.default_authorization,
    });

    let session_id = new_session_id();
    let session_dir = sessions_dir()?;
    fs::create_dir_all(&session_dir)
        .map_err(|e| format!("failed to create {}: {e}", session_dir.display()))?;
    let session_path = session_dir.join(format!("{session_id}.json"));
    let session = json!({
        "id": session_id,
        "source_path": source_path,
        "input": input,
        "result": result.value,
        "ok": result.ok,
        "is_checkpoint": result.value.get("is_checkpoint").and_then(Value::as_bool).unwrap_or(false),
    });
    fs::write(
        &session_path,
        serde_json::to_string_pretty(&session).unwrap_or_default(),
    )
    .map_err(|e| format!("failed to write session: {e}"))?;

    println!(
        "{}",
        serde_json::to_string_pretty(&session).unwrap_or_default()
    );
    if !result.ok {
        return Err("harness run failed".to_string());
    }
    Ok(())
}

fn harness_resume(args: Vec<String>) -> Result<(), String> {
    let mut session_file: Option<PathBuf> = None;
    let mut extra_input = json!({});
    let mut policy = PolicyArgs::default();
    let mut index = 2usize;
    while index < args.len() {
        match args[index].as_str() {
            "--input" => {
                let raw = take_value(&args, &mut index, "--input")?;
                extra_input =
                    serde_json::from_str(&raw).map_err(|e| format!("invalid --input JSON: {e}"))?;
            }
            "--policy" => {
                policy.policy_path =
                    Some(PathBuf::from(take_value(&args, &mut index, "--policy")?));
            }
            "--policy-log" => {
                policy.policy_log = Some(PathBuf::from(take_value(
                    &args,
                    &mut index,
                    "--policy-log",
                )?));
            }
            "--default-authorization" => {
                policy.default_authorization =
                    Some(take_value(&args, &mut index, "--default-authorization")?);
            }
            value if value.starts_with('-') => return Err(format!("unknown option: {value}")),
            value => {
                if session_file.is_some() {
                    return Err(format!("unexpected argument: {value}"));
                }
                session_file = Some(PathBuf::from(value));
            }
        }
        index += 1;
    }
    let session_file = session_file.ok_or_else(harness_usage)?;
    let session: Value = serde_json::from_str(
        &fs::read_to_string(&session_file)
            .map_err(|e| format!("failed to read {}: {e}", session_file.display()))?,
    )
    .map_err(|e| format!("invalid session JSON: {e}"))?;

    let source_path = session
        .get("source_path")
        .and_then(Value::as_str)
        .ok_or_else(|| "session missing source_path".to_string())?;
    let source = fs::read_to_string(source_path)
        .map_err(|e| format!("failed to read {source_path}: {e}"))?;

    let mut input = session.get("input").cloned().unwrap_or(json!({}));
    if let (Some(base), Some(extra)) = (input.as_object_mut(), extra_input.as_object()) {
        for (k, v) in extra {
            base.insert(k.clone(), v.clone());
        }
    }

    if let Some(checkpoint) = session
        .pointer("/result/checkpoint")
        .or_else(|| session.pointer("/result"))
    {
        if checkpoint.get("is_checkpoint").and_then(Value::as_bool) == Some(true) {
            if let Some(ctx) = checkpoint.as_object() {
                if let Some(base) = input.as_object_mut() {
                    for (k, v) in ctx {
                        if k != "success" && k != "is_checkpoint" && k != "events" {
                            base.entry(k.clone()).or_insert_with(|| v.clone());
                        }
                    }
                }
            }
        }
    }

    let result = service_run(RunRequest {
        source,
        source_path: Some(source_path.to_string()),
        input,
        env: vec![],
        provider: None,
        model: None,
        search_paths: devlish_search_paths_for(Some(Path::new(source_path))),
        policy_path: policy.policy_path,
        policy_log: policy.policy_log,
        default_authorization: policy.default_authorization,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "resumed_from": session_file.display().to_string(),
            "ok": result.ok,
            "result": result.value
        }))
        .unwrap_or_default()
    );
    if !result.ok {
        return Err("harness resume failed".to_string());
    }
    Ok(())
}

fn sessions_dir() -> Result<PathBuf, String> {
    if let Some(home) = std::env::var_os("HOME") {
        return Ok(PathBuf::from(home).join(".devlish").join("sessions"));
    }
    Ok(PathBuf::from(".devlish").join("sessions"))
}

fn new_session_id() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("sess-{secs}")
}

#[cfg(test)]
mod generation_tests {
    use super::*;
    use devlish_core::service::service_compile;
    use devlish_vm::{
        policy::{EffectPolicy, PolicyHost, PolicyRecorder},
        HostEffects, Vm,
    };

    #[derive(Default)]
    struct TestHost {
        writes: Vec<Value>,
        payload: Value,
    }
    impl HostEffects for TestHost {
        fn emit_event(&mut self, _: &Value) {}
        fn llm_complete(&mut self, _: &Value) -> Result<Value, String> {
            Ok(json!({"json": self.payload}))
        }
        fn write_file(&mut self, request: &Value) -> Result<(), String> {
            self.writes.push(request.clone());
            Ok(())
        }
        fn respond(&mut self, _: &Value) -> Result<(), String> {
            Ok(())
        }
    }
    #[derive(Default)]
    struct Recorder(Vec<Value>);
    impl PolicyRecorder for Recorder {
        fn record(&mut self, record: &Value) -> Result<(), String> {
            self.0.push(record.clone());
            Ok(())
        }
    }

    #[test]
    fn generation_uses_governed_effects_and_preserves_model_text() {
        let (source, guard) = generation_sources("/challenge/program.dvl", "/challenge/policy.dvl");
        let compiled = service_compile(&source, None, vec![]);
        assert!(compiled.ok, "{}", compiled.value);
        let compiled_guard = service_compile(&guard, None, vec![]);
        assert!(compiled_guard.ok, "{}", compiled_guard.value);
        let policy = EffectPolicy::new(compiled_guard.value).unwrap();
        let payload = json!({"program": "Respond with \"ok\"\n", "policy": "model policy text\n"});
        let mut host = TestHost {
            payload: payload.clone(),
            ..TestHost::default()
        };
        let mut recorder = Recorder::default();
        let result = Vm::new(compiled.value, json!({"prompt": "contract"}))
            .unwrap()
            .run(&mut PolicyHost::new(&mut host, &policy, &mut recorder))
            .unwrap();
        assert_eq!(result["response"], payload);
        assert_eq!(host.writes.len(), 2);
        assert_eq!(host.writes[0]["content"], payload["program"]);
        assert_eq!(host.writes[1]["content"], payload["policy"]);
        assert!(
            !policy
                .evaluate("write_file", &json!({"path": "/other"}))
                .unwrap()
                .0
        );
        assert!(
            !policy
                .evaluate("read_file", &json!({"path": "/challenge/fixture.txt"}))
                .unwrap()
                .0
        );
        assert!(!policy.evaluate("run_tool", &json!({})).unwrap().0);
        assert!(!policy.evaluate("http_request", &json!({})).unwrap().0);
        assert_eq!(
            recorder
                .0
                .iter()
                .filter(|r| r["type"] == "effect_outcome" && r["outcome"]["status"] == "succeeded")
                .count(),
            4
        );
    }

    #[test]
    fn generation_rejects_malformed_output_before_writing() {
        let (source, guard) = generation_sources("/challenge/program.dvl", "/challenge/policy.dvl");
        let compiled = service_compile(&source, None, vec![]);
        assert!(compiled.ok, "{}", compiled.value);
        let policy = EffectPolicy::new(service_compile(&guard, None, vec![]).value).unwrap();
        for payload in [
            json!({"program": "text"}),
            json!({"program": 42, "policy": "text"}),
        ] {
            let mut host = TestHost {
                payload,
                ..TestHost::default()
            };
            let mut recorder = Recorder::default();
            let result = Vm::new(compiled.value.clone(), json!({"prompt": "contract"}))
                .unwrap()
                .run(&mut PolicyHost::new(&mut host, &policy, &mut recorder));
            assert!(result.is_err());
            assert!(host.writes.is_empty());
        }
    }
}
