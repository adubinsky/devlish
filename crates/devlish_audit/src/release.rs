//! Offline release admission against explicitly supplied operator expectations.
use crate::{sha256, verify, Purpose, Verification, MAX_METADATA_BYTES};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: String,
    pub format_version: u32,
    pub release_id: String,
    pub environment: String,
    pub target: String,
    pub sequence: u64,
    pub valid_from: u64,
    pub valid_until: u64,
    pub repository: String,
    pub commit: String,
    pub workflow: String,
    pub policy_id: String,
    pub policy_version: String,
    pub artifacts: Vec<Artifact>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub id: String,
    pub role: Role,
    pub sha256: String,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Runtime,
    Program,
    Compiler,
    Policy,
    ToolCatalog,
    Permissions,
    Containment,
    SourceClosure,
    BuildAttestation,
    Tool,
}
/// This document must come from the operator, never from the candidate release.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirements {
    pub format: String,
    pub format_version: u32,
    pub environment: String,
    pub target: String,
    pub repository: String,
    pub commit: String,
    pub workflow: String,
    pub policy_id: String,
    pub policy_version: String,
    pub minimum_sequence: u64,
    pub evaluated_at: u64,
    pub revocations_valid_from: u64,
    pub revocations_valid_until: u64,
    pub revoked_manifest_sha256: Vec<String>,
    pub authorized_release_keys: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct ReleaseVerification {
    #[serde(skip)]
    pub(crate) verified_scope_digest: String,
    #[serde(skip)]
    pub(crate) verified_sequence: u64,
    #[serde(skip)]
    pub(crate) verified_artifacts: Vec<(String, Role, String, Option<String>)>,
    #[serde(skip)]
    pub(crate) verified_permissions: Vec<(String, crate::controls::Permissions)>,
    #[serde(skip)]
    pub(crate) verified_manifest_digest: String,
    pub format: &'static str,
    pub format_version: u32,
    pub manifest_signature: Verification,
    pub requirements_sha256: String,
    pub release_id: String,
    pub sequence: u64,
    pub evaluated_at: u64,
    pub minimum_sequence: u64,
    pub artifact_count: usize,
    pub release_requirements_verified: bool,
    pub artifact_snapshots_verified: bool,
    pub build_provenance_verified: bool,
    pub execution_origin_verified: bool,
    pub policy_enforcement_verified: bool,
    pub explanation: &'static str,
}
fn digest(value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err("artifact and revocation digests must be lowercase SHA-256 hex".into());
    }
    Ok(())
}
/// Authenticate before requesting artifact bytes. The caller supplies a bounded
/// snapshot resolver; manifest IDs are identifiers, never filesystem paths.
pub fn verify_release(
    bytes: &[u8],
    signature: &[u8],
    trust: &[u8],
    requirements: &[u8],
    mut resolve: impl FnMut(&str) -> Result<Vec<u8>, String>,
) -> Result<ReleaseVerification, String> {
    if bytes.len() as u64 > MAX_METADATA_BYTES || requirements.len() as u64 > MAX_METADATA_BYTES {
        return Err("release metadata exceeds size limit".into());
    }
    let signed = verify(bytes, signature, trust, Purpose::ReleaseManifest)?;
    let m: Manifest =
        serde_json::from_slice(bytes).map_err(|e| format!("invalid manifest: {e}"))?;
    let r: Requirements = serde_json::from_slice(requirements)
        .map_err(|e| format!("invalid release requirements: {e}"))?;
    if m.format != "devlish-release-manifest"
        || m.format_version != 1
        || r.format != "devlish-release-requirements"
        || r.format_version != 1
    {
        return Err("unsupported release format".into());
    }
    if m.release_id.trim().is_empty() || m.sequence == 0 || m.sequence < r.minimum_sequence {
        return Err("invalid release identity or release rollback".into());
    }
    for (actual, expected) in [
        (&m.environment, &r.environment),
        (&m.target, &r.target),
        (&m.repository, &r.repository),
        (&m.commit, &r.commit),
        (&m.workflow, &r.workflow),
        (&m.policy_id, &r.policy_id),
        (&m.policy_version, &r.policy_version),
    ] {
        if expected.trim().is_empty() || actual != expected {
            return Err("release scope does not match operator requirements".into());
        }
    }
    if r.authorized_release_keys.is_empty()
        || r.authorized_release_keys
            .iter()
            .any(|id| id.trim().is_empty())
        || r.authorized_release_keys
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            != r.authorized_release_keys.len()
        || !r.authorized_release_keys.contains(&signed.signer_key_id)
    {
        return Err("signer is not an authorized release authority".into());
    }
    for (start, end) in [
        (m.valid_from, m.valid_until),
        (r.revocations_valid_from, r.revocations_valid_until),
    ] {
        if start >= end || r.evaluated_at < start || r.evaluated_at >= end {
            return Err("release or revocation information is not valid at evaluation time".into());
        }
    }
    for revoked in &r.revoked_manifest_sha256 {
        digest(revoked)?;
    }
    if r.revoked_manifest_sha256.contains(&signed.artifact_sha256) {
        return Err("release manifest is revoked".into());
    }
    let mut ids = BTreeSet::new();
    let mut roles = BTreeSet::new();
    for artifact in &m.artifacts {
        if artifact.id.is_empty()
            || artifact.id.len() > 128
            || !artifact
                .id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
            || !ids.insert(&artifact.id)
        {
            return Err("invalid or duplicate artifact ID".into());
        }
        digest(&artifact.sha256)?;
        roles.insert(artifact.role);
    }
    for required in [
        Role::Runtime,
        Role::Compiler,
        Role::Policy,
        Role::ToolCatalog,
        Role::Permissions,
        Role::Containment,
        Role::SourceClosure,
        Role::BuildAttestation,
    ] {
        if !roles.contains(&required) {
            return Err(format!("missing required artifact role: {required:?}"));
        }
    }
    let mut verified_artifacts = Vec::new();
    let mut verified_permissions = Vec::new();
    for artifact in &m.artifacts {
        let snapshot = resolve(&artifact.id)?;
        if snapshot.len() as u64 > crate::MAX_ARTIFACT_BYTES || sha256(&snapshot) != artifact.sha256
        {
            return Err(format!("artifact digest or size mismatch: {}", artifact.id));
        }
        if artifact.role == Role::Permissions {
            if let Some(permissions) = crate::controls::Permissions::parse(&snapshot) {
                verified_permissions.push((artifact.sha256.clone(), permissions));
            }
        }
        let canonical = if matches!(artifact.role, Role::Policy | Role::Program) {
            serde_json::from_slice::<serde_json::Value>(&snapshot)
                .ok()
                .map(|value| sha256(&serde_json::to_vec_pretty(&value).expect("JSON serializes")))
        } else {
            None
        };
        verified_artifacts.push((
            artifact.id.clone(),
            artifact.role,
            artifact.sha256.clone(),
            canonical,
        ));
    }
    Ok(ReleaseVerification {
        verified_scope_digest: crate::admission::scope_digest(&r),
        verified_sequence: m.sequence,
        verified_artifacts,
        verified_permissions,
        verified_manifest_digest: signed.artifact_sha256.clone(),
        format: "devlish-release-verification", format_version: 1,
        manifest_signature: signed, requirements_sha256: sha256(requirements),
        release_id: m.release_id, sequence: m.sequence, evaluated_at: r.evaluated_at,
        minimum_sequence: r.minimum_sequence, artifact_count: m.artifacts.len(),
        release_requirements_verified: true, artifact_snapshots_verified: true,
        build_provenance_verified: false, execution_origin_verified: false,
        policy_enforcement_verified: false,
        explanation: "Release signature, operator requirements, and supplied artifact snapshots agree at the supplied evaluation time. Requirements, clock, revocation freshness and rollback floor must be independently protected. This does not advance a persistent rollback floor, authenticate build attestations, interpret tool permissions, or prove that these bytes executed or enforced policy.",
    })
}
