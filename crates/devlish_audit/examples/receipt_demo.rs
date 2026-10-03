//! Local synthetic signer for demonstrating receipt verification, NOT custody.
use devlish_audit::{hex, read_bounded, sha256, signing_message, Purpose, MAX_ARTIFACT_BYTES};
use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("Usage: receipt_demo <existing-log.jsonl> <new-demo-directory>".into());
    }
    let log = read_bounded(&PathBuf::from(&args[0]), MAX_ARTIFACT_BYTES)?;
    let lines: Vec<Value> = std::str::from_utf8(&log)?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let last = lines.last().ok_or("empty log")?;
    let kind =
        if last["record"]["type"] == "policy_run_finished" && last["record"]["paused"] == false {
            "terminal"
        } else {
            "checkpoint"
        };
    let dir = PathBuf::from(&args[1]);
    fs::create_dir(&dir)?;
    let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
        .map_err(|_| "demo key generation failed")?;
    let key = Ed25519KeyPair::from_pkcs8(document.as_ref()).map_err(|_| "demo key rejected")?;
    let receipt = serde_json::to_vec_pretty(
        &json!({"format":"devlish-audit-receipt","format_version":1,"kind":kind,"session_id":"synthetic-demo-session","release_manifest_sha256":"0".repeat(64),"log_file_sha256":sha256(&log),"log_head_sha256":last["record_sha256"],"record_count":lines.len()}),
    )?;
    fs::write(dir.join("receipt.json"), &receipt)?;
    fs::write(
        dir.join("signature.json"),
        serde_json::to_vec_pretty(
            &json!({"format":"devlish-detached-signature","format_version":1,"algorithm":"ed25519","key_id":"ephemeral-demo","purpose":"audit-receipt","signature_hex":hex(key.sign(&signing_message(Purpose::AuditReceipt,&receipt)).as_ref())}),
        )?,
    )?;
    fs::write(
        dir.join("trust.json"),
        serde_json::to_vec_pretty(
            &json!({"format":"devlish-audit-trust","format_version":1,"keys":[{"id":"ephemeral-demo","public_key_hex":hex(key.public_key().as_ref()),"purposes":["audit-receipt"],"revoked":false}]}),
        )?,
    )?;
    println!(
        "{}",
        json!({"receipt_sha256":sha256(&receipt),"session_id":"synthetic-demo-session","release_sha256":"0".repeat(64),"warning":"Synthetic local signer only. This is not independent retention, approved release identity, or proof of execution."})
    );
    Ok(())
}
