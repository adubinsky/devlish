//! Offline cryptographic boundary. No compiler, VM, model, or candidate execution.
use ring::{digest, signature};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs::OpenOptions, io::Read, path::Path};

pub mod receipt;

pub const MAX_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_METADATA_BYTES: u64 = 64 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum Purpose {
    ReleaseArtifact,
    AuditEvidence,
    AuditReceipt,
}
impl Purpose {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "release-artifact" => Ok(Self::ReleaseArtifact),
            "audit-evidence" => Ok(Self::AuditEvidence),
            "audit-receipt" => Ok(Self::AuditReceipt),
            _ => Err("purpose must be release-artifact, audit-evidence or audit-receipt".into()),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::ReleaseArtifact => "release-artifact",
            Self::AuditEvidence => "audit-evidence",
            Self::AuditReceipt => "audit-receipt",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Trust {
    format: String,
    format_version: u32,
    keys: Vec<TrustedKey>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustedKey {
    id: String,
    public_key_hex: String,
    purposes: Vec<Purpose>,
    revoked: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format: String,
    format_version: u32,
    algorithm: String,
    key_id: String,
    purpose: Purpose,
    signature_hex: String,
}

/// Fixed domain and purpose prefix followed by the exact artifact bytes.
/// This is a Devlish-specific detached-signature format, not a Sigstore bundle.
pub fn signing_message(purpose: Purpose, bytes: &[u8]) -> Vec<u8> {
    let mut message = b"devlish-detached-signature-v1\0".to_vec();
    message.extend_from_slice(purpose.name().as_bytes());
    message.push(0);
    message.extend_from_slice(bytes);
    message
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn decode<const N: usize>(value: &str) -> Result<[u8; N], String> {
    if value.len() != N * 2 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("expected {} hexadecimal characters", N * 2));
    }
    let mut out = [0; N];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16).map_err(|e| e.to_string())?;
    }
    Ok(out)
}
pub fn sha256(bytes: &[u8]) -> String {
    hex(digest::digest(&digest::SHA256, bytes).as_ref())
}

/// Reads one bounded snapshot. This does not prove a later process executes it.
pub fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Avoid blocking on a FIFO, including one substituted before open.
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .map_err(|e| format!("cannot open input: {e}"))?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > limit {
        return Err("input must be a regular file within the size limit".into());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("input exceeds the size limit".into());
    }
    Ok(bytes)
}

#[derive(Debug, Serialize)]
pub struct Verification {
    pub format: &'static str,
    pub format_version: u32,
    pub signature_verified: bool,
    pub artifact_sha256: String,
    pub trust_configuration_sha256: String,
    pub signer_key_id: String,
    pub signer_public_key_sha256: String,
    pub purpose: Purpose,
    pub assurance: &'static str,
    pub log_chain_verified: bool,
    pub external_anchor_verified: bool,
    pub replay_verified: bool,
    pub execution_origin_verified: bool,
    pub policy_enforcement_verified: bool,
    pub explanation: &'static str,
}

pub fn verify(
    bytes: &[u8],
    envelope: &[u8],
    trust: &[u8],
    purpose: Purpose,
) -> Result<Verification, String> {
    if bytes.len() as u64 > MAX_ARTIFACT_BYTES
        || envelope.len() as u64 > MAX_METADATA_BYTES
        || trust.len() as u64 > MAX_METADATA_BYTES
    {
        return Err("verification input exceeds the size limit".into());
    }
    let roots: Trust =
        serde_json::from_slice(trust).map_err(|e| format!("invalid trust configuration: {e}"))?;
    let envelope: Envelope =
        serde_json::from_slice(envelope).map_err(|e| format!("invalid signature envelope: {e}"))?;
    if roots.format != "devlish-audit-trust" || roots.format_version != 1 || roots.keys.is_empty() {
        return Err("unsupported or empty trust configuration".into());
    }
    if envelope.format != "devlish-detached-signature"
        || envelope.format_version != 1
        || envelope.algorithm != "ed25519"
    {
        return Err("unsupported signature format or algorithm".into());
    }
    let mut ids = BTreeSet::new();
    let mut public_keys = BTreeSet::new();
    for key in &roots.keys {
        let decoded = decode::<32>(&key.public_key_hex)?;
        if key.id.is_empty()
            || !ids.insert(&key.id)
            || !public_keys.insert(decoded)
            || key.purposes.is_empty()
            || key.purposes.iter().collect::<BTreeSet<_>>().len() != key.purposes.len()
        {
            return Err("invalid or duplicate trust key or purposes".into());
        }
    }
    if envelope.purpose != purpose {
        return Err("signature purpose does not match requested purpose".into());
    }
    let key = roots
        .keys
        .iter()
        .find(|key| key.id == envelope.key_id)
        .ok_or("signer is not trusted")?;
    if key.revoked || !key.purposes.contains(&purpose) {
        return Err("signer is revoked or not authorized for this purpose".into());
    }
    let public_key = decode::<32>(&key.public_key_hex)?;
    let signature_bytes = decode::<64>(&envelope.signature_hex)?;
    signature::UnparsedPublicKey::new(&signature::ED25519, public_key)
        .verify(&signing_message(purpose, bytes), &signature_bytes)
        .map_err(|_| "signature verification failed".to_string())?;
    Ok(Verification {
        format: "devlish-independent-verification", format_version: 1,
        signature_verified: true, artifact_sha256: sha256(bytes),
        trust_configuration_sha256: sha256(trust), signer_key_id: key.id.clone(),
        signer_public_key_sha256: sha256(&public_key), purpose,
        assurance: "signed-bytes-under-supplied-trust",
        log_chain_verified: false, external_anchor_verified: false, replay_verified: false,
        execution_origin_verified: false, policy_enforcement_verified: false,
        explanation: "These exact bytes were signed by a key authorized in the supplied trust configuration. Trust configuration authenticity and freshness are operator responsibilities. This does not establish log truth, replay agreement, actual execution, or policy enforcement.",
    })
}
