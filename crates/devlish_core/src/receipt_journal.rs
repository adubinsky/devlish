//! Durable receipt-issuer evidence and offline Devlish decision replay.
//! A retained digest detects changes; it does not authenticate the original host.
use devlish_audit::{issuance::terminal_operation_id, sha256, MAX_ARTIFACT_BYTES};
use devlish_vm::policy::{EffectPolicy, PolicyRecorder};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
};
const MAX_RECORD_BYTES: usize = 1024 * 1024;
pub const MAX_JOURNAL_BYTES: u64 = (4 * MAX_RECORD_BYTES) as u64;

pub struct ReceiptJournal {
    file: File,
    sequence: u64,
    previous: String,
    failed: bool,
    next: &'static str,
}
impl ReceiptJournal {
    /// Create one new journal per issuance attempt in operator-controlled storage.
    /// Existing files are never reused or overwritten. Raw capture is separately
    /// enabled on ReceiptIssuer, not through this recorder or caller payloads.
    pub fn create(path: &Path) -> Result<Self, String> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(path)
            .map_err(|_| "cannot create a new receipt journal")?;
        let mut journal = Self {
            file,
            sequence: 0,
            previous: String::new(),
            failed: false,
            next: "start",
        };
        journal.record(&json!({"type":"receipt_issuance_started","format_version":1}))?;
        #[cfg(unix)]
        File::open(
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )
        .and_then(|directory| directory.sync_all())
        .map_err(|_| "cannot persist receipt journal directory")?;
        Ok(journal)
    }
}
impl PolicyRecorder for ReceiptJournal {
    fn record(&mut self, record: &Value) -> Result<(), String> {
        if self.failed {
            return Err("receipt journal previously failed".into());
        }
        // Poison first; success alone re-enables appends after persistence.
        self.failed = true;
        let next = match self.next {
            "start" if *record == json!({"type":"receipt_issuance_started","format_version":1}) => {
                "preflight"
            }
            "preflight" | "final"
                if record["type"] == "receipt_authorization"
                    && record["phase"]
                        == if self.next == "preflight" {
                            "prepare_audit_receipt"
                        } else {
                            "issue_audit_receipt"
                        }
                    && record["allow"].is_boolean() =>
            {
                if record["allow"] == false {
                    "closed"
                } else if self.next == "preflight" {
                    "final"
                } else {
                    "outcome"
                }
            }
            "outcome"
                if record["type"] == "receipt_signing_outcome"
                    && (record["status"] == "completed" || record["status"] == "uncertain") =>
            {
                "closed"
            }
            _ => {
                return Err("receipt journal does not permit this record or another attempt".into())
            }
        };
        if self.sequence >= 4 {
            return Err("one receipt journal may contain only one attempt".into());
        }
        let mut envelope =
            json!({"sequence":self.sequence,"previous_sha256":self.previous,"record":record});
        let hash = sha256(&serde_json::to_vec(&envelope).map_err(|_| "invalid journal record")?);
        envelope["record_sha256"] = json!(hash);
        let mut bytes = serde_json::to_vec(&envelope).map_err(|_| "invalid journal record")?;
        bytes.push(b'\n');
        if bytes.len() > MAX_RECORD_BYTES {
            return Err("receipt journal record exceeds size limit".into());
        }
        self.file
            .write_all(&bytes)
            .and_then(|_| self.file.sync_all())
            .map_err(|_| "cannot persist receipt journal record")?;
        self.previous = hash;
        self.sequence += 1;
        self.next = next;
        self.failed = false;
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    sequence: u64,
    previous_sha256: String,
    record: Value,
    record_sha256: String,
}
fn digest(value: &Value) -> String {
    sha256(&serde_json::to_vec(value).expect("JSON serializes"))
}
fn is_digest(value: &Value) -> bool {
    value.as_str().is_some_and(|v| {
        v.len() == 64
            && v.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn replay_decision(
    policy: &EffectPolicy,
    record: &Value,
    phase: &str,
    operation: &str,
) -> Result<bool, String> {
    let request = record
        .get("request")
        .ok_or("replay requires operator-enabled raw receipt evidence")?;
    let authority = record
        .get("authority")
        .ok_or("replay requires separate recorded authority")?;
    let (allow, reason) = policy
        .evaluate_with_authority(phase, request, authority)
        .unwrap_or_else(|error| (false, error));
    let expected = json!({"type":"receipt_authorization","phase":phase,"operation_id":operation,
        "policy":policy.identity(),"allow":allow,"reason_sha256":sha256(reason.as_bytes()),
        "request_sha256":digest(request),"authority_sha256":digest(authority),"request":request,"authority":authority});
    if *record != expected {
        return Err("recorded receipt authorization does not reproduce".into());
    }
    Ok(allow)
}

/// Reproduce one completed, denied or explicitly uncertain attempt without any
/// live backend calls. Missing outcomes stay unresolved and cannot pass replay.
/// The anchor must be retained independently of the candidate journal.
pub fn replay(policy_bytes: &[u8], journal: &[u8], retained_digest: &str) -> Result<Value, String> {
    if policy_bytes.len() as u64 > MAX_ARTIFACT_BYTES || journal.len() as u64 > MAX_JOURNAL_BYTES {
        return Err("receipt replay input exceeds size limit".into());
    }
    if !journal.ends_with(b"\n") {
        return Err("receipt journal has an incomplete final record".into());
    }
    if !is_digest(&json!(retained_digest)) || sha256(journal) != retained_digest {
        return Err("receipt journal differs from independently retained digest".into());
    }
    let mut policy = EffectPolicy::new(
        serde_json::from_slice(policy_bytes).map_err(|_| "invalid compiled receipt policy")?,
    )?;
    policy.set_file_digest(sha256(policy_bytes));
    let mut records = Vec::new();
    let mut previous = String::new();
    for (index, line) in std::str::from_utf8(journal)
        .map_err(|_| "invalid journal encoding")?
        .lines()
        .enumerate()
    {
        if index >= 4 || line.len() + 1 > MAX_RECORD_BYTES {
            return Err("receipt journal exceeds one bounded attempt".into());
        }
        let envelope: Envelope =
            serde_json::from_str(line).map_err(|_| "invalid receipt journal envelope")?;
        if envelope.sequence != index as u64
            || envelope.previous_sha256 != previous
            || envelope.record_sha256
                != digest(
                    &json!({"sequence":envelope.sequence,"previous_sha256":envelope.previous_sha256,"record":envelope.record}),
                )
        {
            return Err("receipt journal chain or sequence mismatch".into());
        }
        previous = envelope.record_sha256;
        records.push(envelope.record);
    }
    if records.first() != Some(&json!({"type":"receipt_issuance_started","format_version":1})) {
        return Err("missing receipt journal start".into());
    }
    let preflight = records.get(1).ok_or("no recorded receipt authorization")?;
    let authority = preflight
        .get("authority")
        .ok_or("replay requires operator-enabled raw receipt evidence")?;
    let tenant = authority["tenant_id"]
        .as_str()
        .ok_or("missing authority tenant")?;
    let session = authority["session_id"]
        .as_str()
        .ok_or("missing authority session")?;
    let operation = terminal_operation_id(tenant, session)?;
    if authority["reservation_state"] != "prepared" {
        return Err("preflight authority was not prepared".into());
    }
    let mut decisions = 1;
    let status;
    if !replay_decision(&policy, preflight, "prepare_audit_receipt", &operation)? {
        if records.len() != 2 {
            return Err("records follow denied preflight".into());
        }
        status = "denied";
    } else {
        let final_decision = records
            .get(2)
            .ok_or("unresolved preflight: final authorization missing")?;
        let mut reserved = authority.clone();
        reserved["reservation_state"] = json!("reserved");
        if final_decision["authority"] != reserved
            || final_decision["request"] != preflight["request"]
        {
            return Err("request or authority changed between receipt authorizations".into());
        }
        decisions += 1;
        if !replay_decision(&policy, final_decision, "issue_audit_receipt", &operation)? {
            if records.len() != 3 {
                return Err("records follow denied final authorization".into());
            }
            status = "denied";
        } else {
            let outcome = records
                .get(3)
                .ok_or("signing outcome unresolved; reconciliation required")?;
            status = outcome["status"].as_str().ok_or("missing signing status")?;
            let expected = match status {
                "completed"
                    if is_digest(&outcome["signature_sha256"])
                        && is_digest(&authority["receipt_sha256"]) =>
                {
                    json!({"type":"receipt_signing_outcome","operation_id":operation,"status":"completed",
                        "receipt_sha256":authority["receipt_sha256"],"signature_sha256":outcome["signature_sha256"],
                        "execution_origin_verified":false,"policy_enforcement_verified":false})
                }
                "uncertain" if is_digest(&outcome["diagnostic_sha256"]) => {
                    json!({"type":"receipt_signing_outcome","operation_id":operation,"status":"uncertain","diagnostic_sha256":outcome["diagnostic_sha256"]})
                }
                _ => return Err("unsupported or malformed signing outcome".into()),
            };
            if *outcome != expected {
                return Err("signing outcome differs from authorized operation".into());
            }
        }
    }
    Ok(
        json!({"scope":"offline reproduction of recorded receipt authorization decisions",
        "policy":policy.identity(),"journal_sha256":sha256(journal),"log_head_sha256":previous,
        "operation_id":operation,"decision_count":decisions,"recorded_signing_status":status,
        "retained_digest_matched":true,"log_chain_verified":true,"policy_decisions_reproduced":true,
        "authority_authenticated":false,"receipt_signature_verified":false,"protected_signer_verified":false,
        "execution_origin_verified":false,"policy_enforcement_verified":false}),
    )
}
