//! CLI adapter for the shared admitted session.
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub(super) fn run(args: Vec<String>) -> Result<(), String> {
    devlish_core::logutil::set_level(devlish_core::logutil::LogLevel::Error);
    const USAGE: &str = "Usage: DEVLISH_VERIFIED_PROFILE=<operator-profile.json> devlish-core run-verified --policy-log <new-path> --session-id <id> [--input <json>] [--policy-evidence]";
    let mut options = BTreeMap::new();
    let mut evidence = false;
    let mut index = 1;
    while index < args.len() {
        if args[index] == "--policy-evidence" && !evidence {
            evidence = true;
            index += 1;
            continue;
        }
        if !["--policy-log", "--session-id", "--input"].contains(&args[index].as_str())
            || index + 1 >= args.len()
            || options
                .insert(args[index].as_str(), args[index + 1].as_str())
                .is_some()
        {
            return Err(USAGE.into());
        }
        index += 2;
    }
    let log = options.get("--policy-log").ok_or(USAGE)?;
    let session = options.get("--session-id").ok_or(USAGE)?;
    let input = options.get("--input").copied().unwrap_or("{}");
    let input: Value =
        serde_json::from_str(input).map_err(|e| format!("invalid input JSON: {e}"))?;
    let path = std::env::var_os("DEVLISH_VERIFIED_PROFILE")
        .map(PathBuf::from)
        .ok_or("run-verified requires an operator-configured DEVLISH_VERIFIED_PROFILE")?;
    let admitted =
        devlish_core::verified_session::VerifiedSession::admit(&path, session, evidence)?;
    let mut host = super::NativeHost::new(
        super::CredentialStore::new(&[], Some(admitted.program_path())),
        None,
    );
    host.verified_model_route = Some(admitted.model_route());
    let completion = admitted.execute(input, Path::new(log), &mut host)?;
    if !completion.response_emitted {
        println!(
            "{}",
            json!({"success":true,"response_emitted":false,"paused":completion.paused})
        );
    }
    Ok(())
}
