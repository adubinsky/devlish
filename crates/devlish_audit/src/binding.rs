//! Cross-check recorded identities against a release verified in this process.
use crate::{
    receipt::{verify_receipt, ExpectedReceipt, ReceiptVerification},
    release::{ReleaseVerification, Role},
    sha256, MAX_ARTIFACT_BYTES,
};
use serde_json::{json, Value};

fn report(bytes: &[u8], kind: &str) -> Result<Value, String> {
    if bytes.len() as u64 > MAX_ARTIFACT_BYTES {
        return Err("report exceeds size limit".into());
    }
    let mut value: Value =
        serde_json::from_slice(bytes).map_err(|e| format!("invalid report: {e}"))?;
    if value["format"] != "devlish-compliance-report"
        || value["format_version"] != 1
        || value["kind"] != kind
        || value["passed"] != true
    {
        return Err("requires passing supported report".into());
    }
    let digest = value
        .as_object_mut()
        .ok_or("report must be object")?
        .remove("report_sha256")
        .ok_or("missing report digest")?;
    if digest != sha256(&serde_json::to_vec(&value).map_err(|e| e.to_string())?) {
        return Err("report digest mismatch".into());
    }
    Ok(value)
}
impl ReleaseVerification {
    fn matches(&self, role: Role, digest: &Value, canonical: bool) -> bool {
        digest.as_str().is_some_and(|digest| {
            self.verified_artifacts
                .iter()
                .any(|(_, r, exact, normalized)| {
                    *r == role
                        && if canonical {
                            normalized.as_deref() == Some(digest)
                        } else {
                            exact == digest
                        }
                })
        })
    }
    /// Reports remain assertions; this checks their hashes and release identities,
    /// not the truth of tests claimed by the report producer.
    pub fn bind_reports(&self, application: &[u8], policy: &[u8]) -> Result<Value, String> {
        let app = report(application, "application")?;
        let policy_report = report(policy, "policy")?;
        let files = app["details"]["files"]
            .as_array()
            .filter(|f| !f.is_empty())
            .ok_or("application report needs files")?;
        let mut seen = std::collections::BTreeSet::new();
        for file in files {
            let id = file["id"].as_str().ok_or("application file needs ID")?;
            if !seen.insert(id)
                || file["passed"] != true
                || file["actual_sha256"] != file["expected_sha256"]
            {
                return Err("invalid application file result".into());
            }
            let role = match file["role"].as_str() {
                Some("runtime") => Role::Runtime,
                Some("policy") => Role::Policy,
                Some("program") => Role::Program,
                Some("tool") => Role::Tool,
                _ => return Err(
                    "unsupported application binding role; use runtime, policy, program or tool"
                        .into(),
                ),
            };
            if !self
                .verified_artifacts
                .iter()
                .any(|(a, r, digest, _)| a == id && *r == role && file["actual_sha256"] == *digest)
            {
                return Err(format!("application file not approved by release: {id}"));
            }
        }
        if !files.iter().any(|f| f["role"] == "runtime")
            || !files.iter().any(|f| f["role"] == "policy")
        {
            return Err("application binding requires runtime and policy".into());
        }
        let policy_digest = &policy_report["details"]["policy_file_sha256"];
        if !self.matches(Role::Policy, policy_digest, false)
            || !files
                .iter()
                .any(|f| f["role"] == "policy" && f["actual_sha256"] == *policy_digest)
        {
            return Err("policy report does not match application and release".into());
        }
        Ok(
            json!({"format":"devlish-release-report-binding","format_version":1,
            "release_manifest_sha256":self.verified_manifest_digest,
            "application_report_file_sha256":sha256(application),"policy_report_file_sha256":sha256(policy),
            "reported_identities_match_release":true,"report_claims_independently_verified":false,
            "execution_origin_verified":false,"policy_enforcement_verified":false}),
        )
    }
    /// Call after receipt verification; additionally tie the reports to this run.
    pub fn bind_run_reports(
        &self,
        application: &[u8],
        policy: &[u8],
        log: &[u8],
    ) -> Result<Value, String> {
        let binding = self.bind_reports(application, policy)?;
        if log.len() as u64 > MAX_ARTIFACT_BYTES {
            return Err("log exceeds size limit".into());
        }
        let start: Value = serde_json::from_slice(
            log.split(|b| *b == b'\n')
                .next()
                .ok_or("missing log start")?,
        )
        .map_err(|e| e.to_string())?;
        let app = report(application, "application")?;
        let policy_report = report(policy, "policy")?;
        let record = &start["record"];
        if !app["details"]["files"]
            .as_array()
            .ok_or("missing files")?
            .iter()
            .any(|f| f["role"] == "runtime" && f["actual_sha256"] == record["runtime_file_sha256"])
            || !self
                .verified_artifacts
                .iter()
                .any(|(_, role, exact, canonical)| {
                    *role == Role::Policy
                        && policy_report["details"]["policy_file_sha256"] == *exact
                        && canonical
                            .as_deref()
                            .is_some_and(|digest| record["policy"]["artifact_sha256"] == digest)
                })
        {
            return Err("run identities differ from application or policy report".into());
        }
        Ok(binding)
    }

    fn check_recorded_release(&self, log: &[u8]) -> Result<(), String> {
        let start: Value = serde_json::from_slice(
            log.split(|b| *b == b'\n')
                .next()
                .ok_or("missing log start")?,
        )
        .map_err(|e| e.to_string())?;
        let record = &start["record"];
        if !self.matches(Role::Runtime, &record["runtime_file_sha256"], false)
            || !self.matches(Role::Policy, &record["policy"]["artifact_sha256"], true)
            || !self.matches(Role::Program, &record["program_sha256"], true)
        {
            return Err("log runtime, policy or program is not bound to verified release".into());
        }
        for line in log.split(|b| *b == b'\n').filter(|line| !line.is_empty()) {
            let envelope: Value = serde_json::from_slice(line).map_err(|e| e.to_string())?;
            if envelope["record"]["type"] == "effect_decision"
                && envelope["record"]["policy"] != record["policy"]
            {
                return Err("effect decision policy differs from run policy".into());
            }
        }
        Ok(())
    }

    /// Prepare unsigned receipt bytes after validating the recorded history and
    /// matching its identities to this verified release. Not signing authority.
    pub fn prepare_receipt(
        &self,
        log: &[u8],
        session: &str,
        kind: crate::receipt::ReceiptKind,
    ) -> Result<Vec<u8>, String> {
        let receipt = crate::receipt::prepare(log, session, &self.verified_manifest_digest, kind)?;
        self.check_recorded_release(log)?;
        Ok(receipt)
    }

    pub fn bind_receipt(
        &self,
        log: &[u8],
        receipt: &[u8],
        signature: &[u8],
        trust: &[u8],
        retained_digest: &str,
        session: &str,
    ) -> Result<ReceiptVerification, String> {
        let mut result = verify_receipt(
            log,
            receipt,
            signature,
            trust,
            ExpectedReceipt {
                sha256: retained_digest,
                session_id: session,
                release_sha256: &self.verified_manifest_digest,
            },
        )?;
        self.check_recorded_release(log)?;
        result.release_manifest_verified = true;
        result.explanation = "Receipt and log identities match the release verified in this process under supplied operator requirements. Recorded runtime, policy and program identities are signer assertions, not proof of actual execution. Replay, protected signing and execution are not verified.";
        Ok(result)
    }
}
