//! Terminal UI only; every turn's model calls and effects execute in Devlish.
use super::{compile_source_to_json, execute_config, load_package, CompileOptions, RunConfig};
use serde_json::{json, Value};
use std::{
    fs,
    io::{self, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

const PROGRAM: &str = include_str!("../../../runtime/prompt.dvl");
const POLICY: &str = include_str!("../../../runtime/prompt-policy.dvl");
const MAX_CONVERSATION: usize = 128 * 1024;

fn package(path: &Path, fallback: &str) -> Result<Value, String> {
    match fs::symlink_metadata(path) {
        Ok(_) => return load_package(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("cannot inspect {}: {error}", path.display())),
    }
    let compiled = compile_source_to_json(
        fallback,
        CompileOptions {
            source_path: None,
            search_paths: vec![],
        },
    )
    .map_err(|e| e.to_string())?;
    serde_json::from_str(&compiled).map_err(|e| e.to_string())
}

pub fn run() -> Result<(), String> {
    let program_path = Path::new(".devlish/agent.dvl");
    let policy_path = Path::new(".devlish/policy.dvl");
    // Snapshot and compile once; changing files cannot switch policy mid-session.
    let program = package(program_path, PROGRAM)?;
    let policy = package(policy_path, POLICY)?;
    devlish_vm::policy::EffectPolicy::new(policy.clone())?;
    let posture = std::env::var("DEVLISH_DEFAULT_AUTHORIZATION")
        .unwrap_or_else(|_| "allow-unless-forbidden".into());
    if !["allow-unless-forbidden", "deny-unless-allowed"].contains(&posture.as_str()) {
        return Err(
            "DEVLISH_DEFAULT_AUTHORIZATION must be allow-unless-forbidden or deny-unless-allowed"
                .into(),
        );
    }
    let mut history = Vec::<Value>::new();
    println!("Devlish {}", super::VERSION);
    println!("Enter a request. /clear starts a fresh conversation; /exit exits.");
    println!(
        "Each turn runs a Devlish program with enforced policy. Audit logs: .devlish/sessions/"
    );
    loop {
        print!("devlish> ");
        io::stdout().flush().map_err(|e| e.to_string())?;
        let mut line = String::new();
        if io::stdin()
            .read_line(&mut line)
            .map_err(|e| e.to_string())?
            == 0
        {
            break;
        }
        let text = line.trim();
        match text {
            "/exit" | "/quit" => break,
            "/clear" => {
                history.clear();
                continue;
            }
            "" => continue,
            _ => {}
        }
        let mut pending = history.clone();
        pending.push(json!({"role":"user","content":text}));
        let conversation = format!(
            "Continue this conversation. Reply to the latest user message.\n{}",
            serde_json::to_string(&pending).map_err(|e| e.to_string())?
        );
        if conversation.len() > MAX_CONVERSATION {
            eprintln!("Conversation exceeds 128 KiB. Use /clear to start a fresh conversation.");
            continue;
        }
        fs::create_dir_all(".devlish/sessions")
            .map_err(|e| format!("cannot create audit directory: {e}"))?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let log = format!(".devlish/sessions/{}-{nonce}.jsonl", std::process::id());
        let config = RunConfig::parse(vec![
            "run".into(),
            program_path.display().to_string(),
            "--quiet".into(),
            "--input".into(),
            json!({"conversation":conversation}).to_string(),
            "--policy".into(),
            policy_path.display().to_string(),
            "--policy-log".into(),
            log,
            "--default-authorization".into(),
            posture.clone(),
        ])?;
        match execute_config(config, Some(program.clone()), Some(policy.clone())) {
            Ok(result) => {
                if let Some(response) = result.get("response") {
                    pending.push(json!({"role":"assistant","content":response}));
                    history = pending;
                }
            }
            Err(error) => eprintln!("Turn failed: {error}"),
        }
    }
    Ok(())
}
