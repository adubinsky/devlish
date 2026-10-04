//! Verify an externally retained signed receipt against an existing format-3 log.
use crate::{
    decode, sha256, verify, Purpose, Verification, MAX_ARTIFACT_BYTES, MAX_METADATA_BYTES,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    format: String,
    format_version: u32,
    kind: ReceiptKind,
    session_id: String,
    release_manifest_sha256: String,
    log_file_sha256: String,
    log_head_sha256: String,
    record_count: u64,
}
#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReceiptKind {
    Checkpoint,
    Terminal,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    sequence: u64,
    previous_sha256: String,
    record: Value,
    record_sha256: String,
}
#[derive(Debug, Serialize)]
pub struct ReceiptVerification {
    pub format: &'static str,
    pub format_version: u32,
    pub receipt_signature: Verification,
    pub log_chain_verified: bool,
    pub retained_receipt_digest_matched: bool,
    pub receipt_sha256: String,
    pub session_id: String,
    pub session_binding_source: &'static str,
    pub release_manifest_sha256: String,
    pub release_manifest_verified: bool,
    pub log_head_sha256: String,
    pub record_count: u64,
    pub unmatched_intent_count: u64,
    pub terminal_receipt_verified: bool,
    pub recorded_execution_succeeded: Option<bool>,
    pub recorded_execution_completed: bool,
    pub replay_verified: bool,
    pub execution_origin_verified: bool,
    pub policy_enforcement_verified: bool,
    pub explanation: &'static str,
}

pub struct ExpectedReceipt<'a> {
    pub sha256: &'a str,
    pub session_id: &'a str,
    pub release_sha256: &'a str,
}

pub fn verify_receipt(
    log: &[u8],
    receipt: &[u8],
    signature: &[u8],
    trust: &[u8],
    expected: ExpectedReceipt<'_>,
) -> Result<ReceiptVerification, String> {
    if receipt.len() as u64 > MAX_METADATA_BYTES {
        return Err("receipt exceeds the size limit".into());
    }
    if log.len() as u64 > MAX_ARTIFACT_BYTES {
        return Err("log exceeds the size limit".into());
    }
    decode::<32>(expected.sha256)?;
    decode::<32>(expected.release_sha256)?;
    if !sha256(receipt).eq_ignore_ascii_case(expected.sha256) {
        return Err("receipt does not match independently retained digest".into());
    }
    let signature = verify(receipt, signature, trust, Purpose::AuditReceipt)?;
    let receipt_data: Receipt =
        serde_json::from_slice(receipt).map_err(|e| format!("invalid receipt: {e}"))?;
    if receipt_data.format != "devlish-audit-receipt"
        || receipt_data.format_version != 1
        || receipt_data.session_id.is_empty()
        || receipt_data.session_id.len() > 256
    {
        return Err("unsupported receipt format or session identity".into());
    }
    if receipt_data.session_id != expected.session_id
        || !receipt_data
            .release_manifest_sha256
            .eq_ignore_ascii_case(expected.release_sha256)
    {
        return Err("receipt belongs to a different session or release".into());
    }
    if receipt_data.log_file_sha256 != sha256(log) {
        return Err("log bytes do not match signed receipt".into());
    }
    let LogState {
        previous,
        count,
        pending,
        finish,
        host_session_bound,
    } = inspect_log(log, expected.session_id, expected.release_sha256)?;
    if count == 0 || count != receipt_data.record_count || previous != receipt_data.log_head_sha256
    {
        return Err("log count or head does not match signed receipt".into());
    }
    let terminal = receipt_data.kind == ReceiptKind::Terminal;
    if terminal && !matches!(finish, Some((_, false))) {
        return Err("terminal receipt requires a finished, unpaused run".into());
    }
    Ok(ReceiptVerification {
        format: "devlish-log-verification",
        format_version: 1,
        receipt_signature: signature,
        log_chain_verified: true,
        retained_receipt_digest_matched: true,
        receipt_sha256: sha256(receipt),
        session_id: receipt_data.session_id,
        session_binding_source: if host_session_bound {
            "receipt-signer-and-recorded-host-session"
        } else {
            "receipt-signer; format-3 log has no host session ID"
        },
        release_manifest_sha256: receipt_data.release_manifest_sha256,
        release_manifest_verified: false,
        log_head_sha256: previous,
        record_count: count,
        unmatched_intent_count: u64::from(pending.is_some()),
        terminal_receipt_verified: terminal,
        recorded_execution_succeeded: finish.map(|f| f.0),
        recorded_execution_completed: matches!(finish, Some((_, false))),
        replay_verified: false,
        execution_origin_verified: false,
        policy_enforcement_verified: false,
        explanation: concat!(
            "The signed receipt matches the supplied independent digest and this log snapshot. ",
            "This checks recorded history, not actual execution. Session and release association ",
            "are signer assertions; release approval, replay, custody of the retained digest, ",
            "and protected execution are not verified here."
        ),
    })
}

pub(crate) struct LogState {
    pub previous: String,
    pub count: u64,
    pub pending: Option<(u64, String)>,
    pub finish: Option<(bool, bool)>,
    pub host_session_bound: bool,
}
pub(crate) fn inspect_log(
    log: &[u8],
    session_id: &str,
    release_sha256: &str,
) -> Result<LogState, String> {
    if log.len() as u64 > MAX_ARTIFACT_BYTES {
        return Err("log exceeds size limit".into());
    }
    let text = std::str::from_utf8(log).map_err(|_| "log must be UTF-8")?;
    if !text.ends_with('\n') {
        return Err("log ends in an incomplete record".into());
    }
    let mut previous = String::new();
    let mut count = 0u64;
    let mut next_effect = 1u64;
    let mut pending: Option<(u64, String)> = None;
    let mut finish: Option<(bool, bool)> = None;
    let mut host_session_bound = false;
    for line in text.lines() {
        if line.len() > 1024 * 1024 {
            return Err("log record exceeds 1 MiB limit".into());
        }
        let envelope: Envelope =
            serde_json::from_str(line).map_err(|e| format!("invalid log record: {e}"))?;
        if envelope.sequence != count || envelope.previous_sha256 != previous {
            return Err("broken log sequence or previous hash".into());
        }
        let body = json!({"sequence":envelope.sequence,"previous_sha256":envelope.previous_sha256,"record":envelope.record});
        if sha256(&serde_json::to_vec(&body).map_err(|e| e.to_string())?) != envelope.record_sha256
        {
            return Err("log record digest mismatch".into());
        }
        if finish.is_some() {
            return Err("records follow the run finish".into());
        }
        let record = &envelope.record;
        match record["type"].as_str() {
            Some("policy_run_started") if count == 0 && record["format_version"] == 3 => {
                if let Some(session) = record.get("session_id") {
                    if session.as_str() != Some(session_id) {
                        return Err("log session differs from expected receipt session".into());
                    }
                    host_session_bound = true;
                }
                if let Some(binding) = record.get("verified_release") {
                    if !host_session_bound
                        || binding["session_id"] != record["session_id"]
                        || binding["release_manifest_sha256"].as_str() != Some(release_sha256)
                    {
                        return Err("log verified release differs from receipt".into());
                    }
                }
            }
            Some("effect_decision") if count > 0 => {
                if pending.is_some() {
                    return Err("new decision before prior effect outcome".into());
                }
                let id = record["effect_id"]
                    .as_u64()
                    .ok_or("decision needs effect ID")?;
                let effect = record["effect"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("decision needs effect name")?;
                let allow = record["allow"]
                    .as_bool()
                    .ok_or("decision needs boolean allow")?;
                if id != next_effect {
                    return Err("effect IDs are not sequential".into());
                }
                next_effect += 1;
                if allow {
                    pending = Some((id, effect.to_owned()));
                }
            }
            Some("effect_outcome") if count > 0 => {
                let id = record["effect_id"]
                    .as_u64()
                    .ok_or("outcome needs effect ID")?;
                let effect = record["effect"]
                    .as_str()
                    .ok_or("outcome needs effect name")?;
                if pending.as_ref().map(|(i, e)| (*i, e.as_str())) != Some((id, effect)) {
                    return Err("outcome does not match an authorized intent".into());
                }
                if ![Some("succeeded"), Some("failed")]
                    .contains(&record["outcome"]["status"].as_str())
                {
                    return Err("unsupported effect outcome".into());
                }
                pending = None;
            }
            Some("policy_run_finished") if count > 0 => {
                if pending.is_some() {
                    return Err("finished run has an unresolved effect intent".into());
                }
                finish = Some((
                    record["success"].as_bool().ok_or("finish needs success")?,
                    record["paused"].as_bool().ok_or("finish needs paused")?,
                ));
            }
            _ => return Err("unsupported log record or missing format-3 start".into()),
        }
        previous = envelope.record_sha256;
        count += 1;
    }

    if count == 0 {
        return Err("empty log".into());
    }
    Ok(LogState {
        previous,
        count,
        pending,
        finish,
        host_session_bound,
    })
}

/// Unsigned bytes, suitable only for a separately authorized receipt signer.
/// This does not claim origin or truth of the supplied recorded history.
pub(crate) fn prepare(
    log: &[u8],
    session: &str,
    release: &str,
    kind: ReceiptKind,
) -> Result<Vec<u8>, String> {
    decode::<32>(release)?;
    if session.is_empty() || session.len() > 256 {
        return Err("invalid session identity".into());
    }
    let state = inspect_log(log, session, release)?;
    if !state.host_session_bound {
        return Err("receipt preparation requires a recorded host session".into());
    }
    if kind == ReceiptKind::Terminal && !matches!(state.finish, Some((_, false))) {
        return Err("terminal receipt requires a finished, unpaused run".into());
    }
    let receipt = Receipt {
        format: "devlish-audit-receipt".into(),
        format_version: 1,
        kind,
        session_id: session.into(),
        release_manifest_sha256: release.into(),
        log_file_sha256: sha256(log),
        log_head_sha256: state.previous,
        record_count: state.count,
    };
    serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())
}
