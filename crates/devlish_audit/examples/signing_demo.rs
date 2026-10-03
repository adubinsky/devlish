//! Synthetic demonstration only. Creates an ephemeral key, never exports it.
use devlish_audit::{hex, signing_message, Purpose};
use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use serde_json::json;
use std::{fs, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("supply a new demo directory")?,
    );
    fs::create_dir(&dir)?;
    let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
        .map_err(|_| "demo key generation failed")?;
    let key = Ed25519KeyPair::from_pkcs8(document.as_ref()).map_err(|_| "demo key rejected")?;
    let bytes = b"Synthetic evidence: a signature authenticates bytes, not execution.\n";
    let signature = key.sign(&signing_message(Purpose::AuditEvidence, bytes));
    fs::write(dir.join("evidence.txt"), bytes)?;
    fs::write(
        dir.join("signature.json"),
        serde_json::to_vec_pretty(&json!({
            "format":"devlish-detached-signature","format_version":1,"algorithm":"ed25519",
            "key_id":"ephemeral-demo","purpose":"audit-evidence","signature_hex":hex(signature.as_ref())
        }))?,
    )?;
    fs::write(
        dir.join("trust.json"),
        serde_json::to_vec_pretty(&json!({
            "format":"devlish-audit-trust","format_version":1,"keys":[{
                "id":"ephemeral-demo","public_key_hex":hex(key.public_key().as_ref()),
                "purposes":["audit-evidence"],"revoked":false
            }]
        }))?,
    )?;
    println!("Created synthetic evidence, signature and demo trust configuration in {}. Do not use this demo trust configuration for real evidence.",dir.display());
    Ok(())
}
