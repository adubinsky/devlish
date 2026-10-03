use devlish_audit::{read_bounded, verify, Purpose, MAX_ARTIFACT_BYTES, MAX_METADATA_BYTES};
use serde_json::json;
use std::{collections::BTreeMap, path::Path};
const USAGE: &str = "Usage: devlish-audit verify <file> --signature <signature.json> --trust <operator-trust.json> --purpose <release-artifact|audit-evidence|audit-receipt>\n       devlish-audit verify-log <log.jsonl> --receipt <receipt.json> --signature <signature.json> --trust <operator-trust.json> --receipt-sha256 <retained-digest> --session-id <expected-session> --release-sha256 <expected-release-digest>";
fn run(args: &[String]) -> Result<serde_json::Value, String> {
    if args.first().map(String::as_str) == Some("verify-log") {
        if args.len() != 14 {
            return Err(USAGE.into());
        }
        let mut options = BTreeMap::new();
        for pair in args[2..].chunks_exact(2) {
            if ![
                "--receipt",
                "--signature",
                "--trust",
                "--receipt-sha256",
                "--session-id",
                "--release-sha256",
            ]
            .contains(&pair[0].as_str())
                || options.insert(pair[0].as_str(), pair[1].as_str()).is_some()
            {
                return Err(USAGE.into());
            }
        }
        let trust = read_bounded(Path::new(options["--trust"]), MAX_METADATA_BYTES)?;
        let signature = read_bounded(Path::new(options["--signature"]), MAX_METADATA_BYTES)?;
        let receipt = read_bounded(Path::new(options["--receipt"]), MAX_METADATA_BYTES)?;
        let log = read_bounded(Path::new(&args[1]), MAX_ARTIFACT_BYTES)?;
        let report = devlish_audit::receipt::verify_receipt(
            &log,
            &receipt,
            &signature,
            &trust,
            devlish_audit::receipt::ExpectedReceipt {
                sha256: options["--receipt-sha256"],
                session_id: options["--session-id"],
                release_sha256: options["--release-sha256"],
            },
        )?;
        return serde_json::to_value(report).map_err(|e| e.to_string());
    }
    if args.len() != 8 || args[0] != "verify" {
        return Err(USAGE.into());
    }
    let mut options = BTreeMap::new();
    for pair in args[2..].chunks_exact(2) {
        if !["--signature", "--trust", "--purpose"].contains(&pair[0].as_str())
            || options.insert(pair[0].as_str(), pair[1].as_str()).is_some()
        {
            return Err(USAGE.into());
        }
    }
    let purpose = Purpose::parse(options["--purpose"])?;
    let trust = read_bounded(Path::new(options["--trust"]), MAX_METADATA_BYTES)?;
    let signature = read_bounded(Path::new(options["--signature"]), MAX_METADATA_BYTES)?;
    let bytes = read_bounded(Path::new(&args[1]), MAX_ARTIFACT_BYTES)?;
    serde_json::to_value(verify(&bytes, &signature, &trust, purpose)?).map_err(|e| e.to_string())
}
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() == 1 && ["--help", "-h"].contains(&args[0].as_str()) {
        println!("{USAGE}");
        return;
    }
    match run(&args) {
        Ok(report) => println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("report serializes")
        ),
        Err(error) => {
            println!(
                "{}",
                json!({"format":"devlish-independent-verification", "format_version":1, "signature_verified":false, "assurance":"unverified", "error":error, "execution_origin_verified":false, "policy_enforcement_verified":false})
            );
            std::process::exit(1);
        }
    }
}
