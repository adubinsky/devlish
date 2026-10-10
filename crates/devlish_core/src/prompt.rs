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

fn parse_limits(value: &Value) -> Result<(u64, devlish_vm::effect_budget::EffectBudget), String> {
    let object = value.as_object().ok_or("prompt limits must be an object")?;
    if object.len() != 2
        || !object.contains_key("instruction_limit")
        || !object.contains_key("effect_budget")
    {
        return Err("prompt limits require only instruction_limit and effect_budget".into());
    }
    let instructions = value["instruction_limit"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= 10_000_000)
        .ok_or("prompt instruction_limit must be 1 through 10000000")?;
    Ok((
        instructions,
        devlish_vm::effect_budget::EffectBudget::parse(&value["effect_budget"])?,
    ))
}

fn limits() -> Result<(u64, devlish_vm::effect_budget::EffectBudget), String> {
    let path = Path::new(".devlish/limits.json");
    let value = match fs::symlink_metadata(path) {
        Ok(_) => serde_json::from_slice(
            &fs::read(path).map_err(|e| format!("cannot read prompt limits: {e}"))?,
        )
        .map_err(|e| format!("invalid prompt limits JSON: {e}"))?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => json!({
            "instruction_limit": 50_000,
            "effect_budget": {"total": 9, "per_effect": {"llm_complete": 8, "run_tool": 7, "respond": 1}}
        }),
        Err(error) => return Err(format!("cannot inspect prompt limits: {error}")),
    };
    parse_limits(&value)
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
    let (instruction_limit, budget) = limits()?;
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
        let mut config = RunConfig::parse(vec![
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
        config.prompt_instruction_limit = Some(instruction_limit);
        config.prompt_budget = Some(budget.clone());
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn operator_prompt_limits_are_strict_and_bounded() {
        let value = json!({"instruction_limit":50_000,"effect_budget":{"total":16,"per_effect":{"llm_complete":1,"http_request":1}}});
        let (instructions, budget) = parse_limits(&value).unwrap();
        assert_eq!(instructions, 50_000);
        assert_eq!(budget.to_value(), value["effect_budget"]);
        for invalid in [
            Value::Null,
            json!({}),
            json!({"instruction_limit":0,"effect_budget":value["effect_budget"]}),
            json!({"instruction_limit":10_000_001,"effect_budget":value["effect_budget"]}),
            json!({"instruction_limit":50_000,"effect_budget":{"total":0,"per_effect":{}}}),
            json!({"instruction_limit":50_000,"effect_budget":value["effect_budget"],"permissions":[]}),
        ] {
            assert!(parse_limits(&invalid).is_err());
        }
    }
}
