//! In-process receipt orchestration. Deployment must independently protect this
//! host, its configuration, recorder, storage and signing backend.
use devlish_audit::{
    issuance::ReceiptKey, receipt::ReceiptKind, release::ReleaseVerification, sha256,
    signing_message, Purpose, MAX_ARTIFACT_BYTES,
};
use devlish_vm::policy::{EffectPolicy, PolicyRecorder};
use serde_json::{json, Value};
use std::path::PathBuf;

/// Adapter to an operator-selected Ed25519 backend. There is deliberately no
/// implementation loading a software key or accepting a model-chosen endpoint.
pub trait ReceiptSigningBackend {
    fn available(&self) -> bool;
    fn sign(&mut self, key_id: &str, domain_message: &[u8]) -> Result<[u8; 64], String>;
}

pub struct ReceiptIssuer {
    policy: EffectPolicy,
    directory: PathBuf,
    tenant: String,
    key: ReceiptKey,
}

pub struct IssuedReceipt {
    pub receipt: Vec<u8>,
    pub signature: Vec<u8>,
    pub operation_id: String,
}

impl ReceiptIssuer {
    /// All arguments come from operator configuration, not the signing request.
    /// A digest pin detects substitution but does not authenticate configuration.
    pub fn new(
        policy_bytes: &[u8],
        expected_policy_sha256: &str,
        directory: PathBuf,
        tenant: String,
        key: ReceiptKey,
    ) -> Result<Self, String> {
        if policy_bytes.len() as u64 > MAX_ARTIFACT_BYTES
            || sha256(policy_bytes) != expected_policy_sha256
        {
            return Err("receipt authorization policy does not match its operator pin".into());
        }
        if tenant.is_empty()
            || tenant.len() > 128
            || !tenant
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        {
            return Err("invalid operator tenant identity".into());
        }
        let package = serde_json::from_slice(policy_bytes)
            .map_err(|_| "invalid receipt authorization policy")?;
        let mut policy = EffectPolicy::new(package)?;
        policy.set_file_digest(expected_policy_sha256.into());
        let directory = directory
            .canonicalize()
            .map_err(|_| "receipt storage unavailable")?;
        Ok(Self {
            policy,
            directory,
            tenant,
            key,
        })
    }

    fn authorize(
        &self,
        phase: &str,
        request: &Value,
        authority: &Value,
        recorder: &mut dyn PolicyRecorder,
    ) -> Result<(), String> {
        let (allow, reason) = self
            .policy
            .evaluate_with_authority(phase, request, authority)
            .unwrap_or_else(|error| (false, error));
        let diagnostic = sha256(reason.as_bytes());
        recorder.record(&json!({"type":"receipt_authorization","phase":phase,
            "policy":self.policy.identity(),"allow":allow,"reason_sha256":diagnostic,
            "request_sha256":sha256(&serde_json::to_vec(request).map_err(|_|"invalid request")?),
            "authority_sha256":sha256(&serde_json::to_vec(authority).map_err(|_|"invalid authority state")?)}))
            .map_err(|_| "receipt decision recording failed; signing blocked")?;
        if !allow {
            return Err(format!(
                "receipt authorization denied; diagnostic sha256: {diagnostic}"
            ));
        }
        Ok(())
    }

    /// The release, session, log and fresh trust snapshot must be supplied from
    /// host-owned state. Only request is untrusted caller JSON. No CLI/HTTP route
    /// exposes this function. A service must authenticate that separation.
    #[allow(clippy::too_many_arguments)]
    pub fn issue(
        &self,
        release: &ReleaseVerification,
        session: &str,
        log: &[u8],
        request: &Value,
        trust: &[u8],
        backend: &mut dyn ReceiptSigningBackend,
        recorder: &mut dyn PolicyRecorder,
    ) -> Result<IssuedReceipt, String> {
        let current_key = ReceiptKey::from_trust(&self.key.id, trust)?;
        if current_key.public_key_sha256 != self.key.public_key_sha256 {
            return Err("current receipt key differs from operator pin".into());
        }
        let receipt = release.prepare_receipt(log, session, ReceiptKind::Terminal)?;
        let parsed: Value =
            serde_json::from_slice(&receipt).map_err(|_| "invalid prepared receipt")?;
        let mut authority = json!({"tenant_id":self.tenant,"session_id":session,
            "release_sha256":parsed["release_manifest_sha256"],"receipt_sha256":sha256(&receipt),
            "key_id":self.key.id,"key_active":true,"signer_available":backend.available(),
            "terminal_ready":true,"reservation_state":"prepared","evidence_source":"verified-history",
            "assurance_profile":"recorded-history"});
        self.authorize("prepare_audit_receipt", request, &authority, recorder)?;
        let reservation = release.reserve_terminal_receipt(
            &self.directory,
            &self.tenant,
            session,
            &self.key,
            log,
        )?;
        if reservation.receipt() != receipt {
            return Err("reserved receipt changed after preflight; reconciliation required".into());
        }
        authority["reservation_state"] = json!("reserved");
        self.authorize("issue_audit_receipt", request, &authority, recorder)?;
        let operation_id = reservation.operation_id().to_owned();
        let signature_bytes = match backend.sign(
            reservation.key_id(),
            &signing_message(Purpose::AuditReceipt, reservation.receipt()),
        ) {
            Ok(signature) => signature,
            Err(error) => {
                recorder
                    .record(
                        &json!({"type":"receipt_signing_outcome","operation_id":operation_id,
                    "status":"uncertain","diagnostic_sha256":sha256(error.as_bytes())}),
                    )
                    .map_err(|_| {
                        "signing outcome uncertain and recording failed; reconciliation required"
                    })?;
                return Err("signing outcome uncertain; reconciliation required".into());
            }
        };
        let signature = serde_json::to_vec(
            &json!({"format":"devlish-detached-signature","format_version":1,
            "algorithm":"ed25519","key_id":self.key.id,"purpose":"audit-receipt",
            "signature_hex":devlish_audit::hex(&signature_bytes)}),
        )
        .map_err(|_| "cannot encode receipt signature")?;
        reservation.complete(&signature, trust)?;
        recorder.record(&json!({"type":"receipt_signing_outcome","operation_id":operation_id,
            "status":"completed","receipt_sha256":sha256(&receipt),"signature_sha256":sha256(&signature),
            "execution_origin_verified":false,"policy_enforcement_verified":false}))
            .map_err(|_|"receipt completed but outcome recording failed; reconcile stored completion")?;
        Ok(IssuedReceipt {
            receipt,
            signature,
            operation_id,
        })
    }
}
