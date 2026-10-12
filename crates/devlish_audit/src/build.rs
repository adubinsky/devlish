//! Scoped builder claims authenticated independently of release approval.
//! This is a Devlish exact-byte signature format, not Sigstore/CI attestation.
use crate::{
    release::{digest, Manifest, Role},
    verify, Purpose, Verification, MAX_METADATA_BYTES,
};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildRequirements {
    pub authorized_builder_keys: Vec<String>,
    pub toolchain: BuildInputs,
    pub policy: BuildInputs,
}
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BuildInputs {
    pub workflow: String,
    pub build_definition_sha256: String,
    pub dependencies_sha256: String,
    pub builder_image_sha256: String,
    pub options_sha256: String,
}
impl BuildInputs {
    fn validate(&self) -> Result<(), String> {
        if self.workflow.trim().is_empty() {
            return Err("builder workflow must be explicitly pinned".into());
        }
        for value in [
            &self.build_definition_sha256,
            &self.dependencies_sha256,
            &self.builder_image_sha256,
            &self.options_sha256,
        ] {
            digest(value)?;
        }
        Ok(())
    }
}
impl BuildRequirements {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.authorized_builder_keys.is_empty()
            || self
                .authorized_builder_keys
                .iter()
                .any(|id| id.trim().is_empty())
            || self
                .authorized_builder_keys
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != self.authorized_builder_keys.len()
        {
            return Err("build requirements need unique authorized builder keys".into());
        }
        self.toolchain.validate()?;
        self.policy.validate()
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bundle {
    format: String,
    format_version: u32,
    // String preserves exact signed UTF-8 bytes, including whitespace.
    statement: String,
    signature: crate::Envelope,
}
#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum Kind {
    Toolchain,
    Policy,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Statement {
    format: String,
    format_version: u32,
    kind: Kind,
    repository: String,
    commit: String,
    release_workflow: String,
    target: String,
    valid_from: u64,
    valid_until: u64,
    source_closure_sha256: String,
    #[serde(default)]
    compiler_sha256: Option<String>,
    inputs: BuildInputs,
    subjects: Vec<Subject>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Subject {
    id: String,
    sha256: String,
}

pub(crate) struct BuildVerification {
    pub signatures: Vec<Verification>,
    pub valid_until: u64,
}

pub(crate) fn verify_bundles(
    bundles: &[Vec<u8>],
    requirements: &BuildRequirements,
    manifest: &Manifest,
    release: &Verification,
    trust: &[u8],
    evaluated_at: u64,
) -> Result<BuildVerification, String> {
    if ![40, 64].contains(&manifest.commit.len())
        || !manifest
            .commit
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("authenticated builds require an immutable lowercase Git object ID".into());
    }
    let artifacts: BTreeMap<_, _> = manifest
        .artifacts
        .iter()
        .map(|a| (a.id.as_str(), a))
        .collect();
    let mut covered = BTreeSet::new();
    let mut verified = Vec::new();
    let mut valid_until = u64::MAX;
    for bytes in bundles {
        if bytes.len() as u64 > MAX_METADATA_BYTES {
            return Err("build bundle exceeds metadata limit".into());
        }
        let bundle: Bundle =
            serde_json::from_slice(bytes).map_err(|e| format!("invalid build bundle: {e}"))?;
        if bundle.format != "devlish-build-bundle" || bundle.format_version != 1 {
            return Err("unsupported build bundle".into());
        }
        let signature = verify(
            bundle.statement.as_bytes(),
            &serde_json::to_vec(&bundle.signature).map_err(|e| e.to_string())?,
            trust,
            Purpose::BuildStatement,
        )?;
        if !requirements
            .authorized_builder_keys
            .contains(&signature.signer_key_id)
        {
            return Err("builder is not authorized by operator requirements".into());
        }
        if signature.signer_public_key_sha256 == release.signer_public_key_sha256 {
            return Err("builder and release authority must use distinct keys".into());
        }
        let statement: Statement = serde_json::from_str(&bundle.statement)
            .map_err(|e| format!("invalid build statement: {e}"))?;
        if statement.format != "devlish-build-statement" || statement.format_version != 1 {
            return Err("unsupported build statement".into());
        }
        if statement.repository != manifest.repository
            || statement.commit != manifest.commit
            || statement.release_workflow != manifest.workflow
            || statement.target != manifest.target
        {
            return Err("build statement scope does not match approved release".into());
        }
        if statement.valid_from >= statement.valid_until
            || evaluated_at < statement.valid_from
            || evaluated_at >= statement.valid_until
        {
            return Err("build statement is not valid at evaluation time".into());
        }
        valid_until = valid_until.min(statement.valid_until);
        statement.inputs.validate()?;
        let expected = match statement.kind {
            Kind::Toolchain => &requirements.toolchain,
            Kind::Policy => &requirements.policy,
        };
        if statement.inputs != *expected {
            return Err("build inputs do not match operator requirements".into());
        }
        digest(&statement.source_closure_sha256)?;
        if !manifest
            .artifacts
            .iter()
            .any(|a| a.role == Role::SourceClosure && a.sha256 == statement.source_closure_sha256)
        {
            return Err("build source closure is absent from approved release".into());
        }
        match (&statement.kind, &statement.compiler_sha256) {
            (Kind::Toolchain, None) => {}
            (Kind::Policy, Some(hash)) => {
                digest(hash)?;
                if !manifest
                    .artifacts
                    .iter()
                    .any(|a| a.role == Role::Compiler && a.sha256 == *hash)
                {
                    return Err("policy builder used an unapproved compiler".into());
                }
            }
            _ => return Err("build statement has invalid compiler linkage".into()),
        }
        if statement.subjects.is_empty() {
            return Err("build statement has no subjects".into());
        }
        for subject in &statement.subjects {
            digest(&subject.sha256)?;
            let artifact = artifacts
                .get(subject.id.as_str())
                .ok_or("build subject is absent from approved release")?;
            let allowed = match statement.kind {
                Kind::Toolchain => matches!(
                    artifact.role,
                    Role::Runtime | Role::Compiler | Role::AuditVerifier
                ),
                Kind::Policy => matches!(artifact.role, Role::Policy | Role::Program),
            };
            if !allowed || artifact.sha256 != subject.sha256 || !covered.insert(subject.id.clone())
            {
                return Err("invalid, mismatched or duplicate build subject".into());
            }
        }
        verified.push(signature);
    }
    if manifest
        .artifacts
        .iter()
        .filter(|a| {
            matches!(
                a.role,
                Role::Runtime | Role::Compiler | Role::AuditVerifier | Role::Policy | Role::Program
            )
        })
        .any(|a| !covered.contains(&a.id))
    {
        return Err("approved release has outputs without authenticated builder statements".into());
    }
    Ok(BuildVerification {
        signatures: verified,
        valid_until,
    })
}
