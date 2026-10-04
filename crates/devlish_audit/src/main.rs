use devlish_audit::{read_bounded, verify, Purpose, MAX_ARTIFACT_BYTES, MAX_METADATA_BYTES};
use serde_json::json;
use std::{collections::BTreeMap, path::Path};
const USAGE: &str = "Usage: devlish-audit init-admission <new-state.json> --requirements <operator-requirements.json>\n       devlish-audit verify-release <manifest.json> --signature <signature.json> --trust <operator-trust.json> --requirements <operator-requirements.json> --artifacts <operator-artifacts.json> [--evidence <evidence.json> | --prepare-receipt <request.json>]\n       devlish-audit verify <file> --signature <signature.json> --trust <operator-trust.json> --purpose <release-manifest|release-artifact|audit-evidence|audit-receipt>\n       devlish-audit verify-log <log.jsonl> --receipt <receipt.json> --signature <signature.json> --trust <operator-trust.json> --receipt-sha256 <retained-digest> --session-id <expected-session> --release-sha256 <expected-release-digest>";
fn run(args: &[String]) -> Result<serde_json::Value, String> {
    if args.first().map(String::as_str) == Some("init-admission") {
        if args.len() != 4 || args[2] != "--requirements" {
            return Err(USAGE.into());
        }
        devlish_audit::admission::initialize(
            Path::new(&args[1]),
            &read_bounded(Path::new(&args[3]), MAX_METADATA_BYTES)?,
        )?;
        return Ok(
            json!({"format":"devlish-admission-initialized","format_version":1,"initialized":true}),
        );
    }
    if args.first().map(String::as_str) == Some("verify-release") {
        if ![10, 12].contains(&args.len()) {
            return Err(USAGE.into());
        }
        let mut options = BTreeMap::new();
        for pair in args[2..].chunks_exact(2) {
            if ![
                "--signature",
                "--trust",
                "--requirements",
                "--artifacts",
                "--evidence",
                "--prepare-receipt",
            ]
            .contains(&pair[0].as_str())
                || options.insert(pair[0].as_str(), pair[1].as_str()).is_some()
            {
                return Err(USAGE.into());
            }
        }
        if ["--signature", "--trust", "--requirements", "--artifacts"]
            .iter()
            .any(|key| !options.contains_key(key))
        {
            return Err(USAGE.into());
        }
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct LocalArtifact {
            id: String,
            path: std::path::PathBuf,
        }
        let index_path = Path::new(options["--artifacts"]);
        let index: Vec<LocalArtifact> =
            serde_json::from_slice(&read_bounded(index_path, MAX_METADATA_BYTES)?)
                .map_err(|e| format!("invalid artifact mapping: {e}"))?;
        let mut paths = BTreeMap::new();
        for entry in index {
            if paths.insert(entry.id, entry.path).is_some() {
                return Err("duplicate artifact mapping".into());
            }
        }
        let report = devlish_audit::release::verify_release(
            &read_bounded(Path::new(&args[1]), MAX_METADATA_BYTES)?,
            &read_bounded(Path::new(options["--signature"]), MAX_METADATA_BYTES)?,
            &read_bounded(Path::new(options["--trust"]), MAX_METADATA_BYTES)?,
            &read_bounded(Path::new(options["--requirements"]), MAX_METADATA_BYTES)?,
            |id| {
                let path = paths
                    .get(id)
                    .ok_or_else(|| format!("missing artifact mapping: {id}"))?;
                read_bounded(
                    &index_path.parent().unwrap_or(Path::new(".")).join(path),
                    MAX_ARTIFACT_BYTES,
                )
            },
        )?;
        if let Some(request_path) = options.get("--prepare-receipt") {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Request {
                log: String,
                session_id: String,
                kind: devlish_audit::receipt::ReceiptKind,
                output: String,
            }
            let path = Path::new(request_path);
            let request: Request = serde_json::from_slice(&read_bounded(path, MAX_METADATA_BYTES)?)
                .map_err(|e| format!("invalid receipt preparation request: {e}"))?;
            let base = path.parent().unwrap_or(Path::new("."));
            let log = read_bounded(&base.join(&request.log), MAX_ARTIFACT_BYTES)?;
            let receipt = report.prepare_receipt(&log, &request.session_id, request.kind)?;
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let output = base.join(&request.output);
            let mut file = options
                .open(&output)
                .map_err(|e| format!("cannot create new unsigned receipt: {e}"))?;
            use std::io::Write;
            file.write_all(&receipt)
                .and_then(|_| file.sync_all())
                .map_err(|e| e.to_string())?;
            #[cfg(unix)]
            std::fs::File::open(
                output
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new(".")),
            )
            .and_then(|dir| dir.sync_all())
            .map_err(|e| e.to_string())?;
            return Ok(
                json!({"release":report,"receipt_sha256":devlish_audit::sha256(&receipt),
                "receipt_signed":false,"signer_authorized":false,"execution_origin_verified":false,
                "explanation":"Unsigned receipt prepared from checked recorded history. A separately authorized signer and independent retention are still required."}),
            );
        }
        if let Some(evidence_path) = options.get("--evidence") {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Evidence {
                application_report: String,
                policy_report: String,
                log: String,
                receipt: String,
                signature: String,
                trust: String,
                retained_receipt_sha256: String,
                session_id: String,
            }
            let path = Path::new(evidence_path);
            let evidence: Evidence =
                serde_json::from_slice(&read_bounded(path, MAX_METADATA_BYTES)?)
                    .map_err(|e| format!("invalid evidence mapping: {e}"))?;
            let read = |name: &str, limit| {
                read_bounded(&path.parent().unwrap_or(Path::new(".")).join(name), limit)
            };
            let log = read(&evidence.log, MAX_ARTIFACT_BYTES)?;
            let receipt = report.bind_receipt(
                &log,
                &read(&evidence.receipt, MAX_METADATA_BYTES)?,
                &read(&evidence.signature, MAX_METADATA_BYTES)?,
                &read(&evidence.trust, MAX_METADATA_BYTES)?,
                &evidence.retained_receipt_sha256,
                &evidence.session_id,
            )?;
            let reports = report.bind_run_reports(
                &read(&evidence.application_report, MAX_ARTIFACT_BYTES)?,
                &read(&evidence.policy_report, MAX_ARTIFACT_BYTES)?,
                &log,
            )?;
            return Ok(json!({"release":report,"reports":reports,"receipt":receipt}));
        }
        return serde_json::to_value(report).map_err(|e| e.to_string());
    }
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
