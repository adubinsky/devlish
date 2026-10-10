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
        other => Err(format!("unknown harness subcommand: {other}\n\n{}", harness_usage())),
    }
}

fn harness_usage() -> String {
    "Usage:\n  \
     devlish harness run <file.dvl> [--provider NAME] [--model NAME] [--input JSON] [--env KEY=VALUE]\n             \
     [--policy FILE --policy-log FILE] [--default-authorization deny-unless-allowed|allow-unless-forbidden]\n  \
     devlish harness resume <session.json> [--input JSON]\n             \
     [--policy FILE --policy-log FILE] [--default-authorization deny-unless-allowed|allow-unless-forbidden]\n  \
     devlish harness init-config\n"
        .to_string()
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
                input = serde_json::from_str(&raw)
                    .map_err(|e| format!("invalid --input JSON: {e}"))?;
            }
            "--env" => {
                let raw = take_value(&args, &mut index, "--env")?;
                let (k, v) = raw
                    .split_once('=')
                    .ok_or_else(|| format!("invalid --env {raw}"))?;
                env.push((k.to_string(), v.to_string()));
            }
            "--policy" => {
                policy.policy_path = Some(PathBuf::from(take_value(&args, &mut index, "--policy")?));
            }
            "--policy-log" => {
                policy.policy_log =
                    Some(PathBuf::from(take_value(&args, &mut index, "--policy-log")?));
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

    println!("{}", serde_json::to_string_pretty(&session).unwrap_or_default());
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
                extra_input = serde_json::from_str(&raw)
                    .map_err(|e| format!("invalid --input JSON: {e}"))?;
            }
            "--policy" => {
                policy.policy_path = Some(PathBuf::from(take_value(&args, &mut index, "--policy")?));
            }
            "--policy-log" => {
                policy.policy_log =
                    Some(PathBuf::from(take_value(&args, &mut index, "--policy-log")?));
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
