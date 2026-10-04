//! Shared signed-release admission and governed session execution.
use devlish_audit::{
    read_bounded,
    release::{verify_release, Manifest, Role},
    sha256, MAX_ARTIFACT_BYTES, MAX_METADATA_BYTES,
};
use devlish_vm::policy::EffectPolicy;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    format: String,
    format_version: u32,
    manifest: PathBuf,
    signature: PathBuf,
    trust: PathBuf,
    requirements: PathBuf,
    admission_state: PathBuf,
    artifacts: Vec<LocalArtifact>,
    runtime_id: String,
    program_id: String,
    policy_id: String,
    permissions_id: String,
    catalog_id: String,
    containment_id: String,
    #[serde(default)]
    allow_raw_evidence: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalArtifact {
    id: String,
    path: PathBuf,
}

/// An admitted, single-use session retaining the exact verified snapshots and
/// the durable rollback lock. Construction requires independently trusted
/// operator configuration; this type does not authenticate remote clients.
pub struct VerifiedSession {
    program: Value,
    policy: EffectPolicy,
    log_context: Value,
    instruction_limit: u64,
    allowed_effects: std::collections::BTreeSet<String>,
    llm_route: Option<devlish_llm::governed::ApprovedModel>,
    program_path: PathBuf,
    evidence: bool,
    admitted_at: u64,
    expires_at: u64,
    _admission: devlish_audit::admission::AdmissionGuard,
}
impl VerifiedSession {
    /// Profile and all referenced trust/state paths must be operator-controlled.
    /// Session IDs are correlation identifiers, not authenticated identities.
    /// Errors are operator diagnostics; do not forward them verbatim to clients.
    pub fn admit(path: &Path, session: &str, evidence: bool) -> Result<Self, String> {
        if session.is_empty()
            || session.len() > 128
            || !session
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        {
            return Err(
                "session ID must be 1-128 ASCII letters, digits, underscores or hyphens".into(),
            );
        }
        // Anchor once before any read; preserve symlink-based deployment paths.
        // Later cwd changes cannot retarget artifact or credential-lookup paths.
        let anchored = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(path)
        };
        let path = anchored.as_path();
        let profile_bytes = read_bounded(path, MAX_METADATA_BYTES)?;
        let profile: Profile = serde_json::from_slice(&profile_bytes)
            .map_err(|e| format!("invalid verified profile: {e}"))?;
        if profile.format != "devlish-verified-profile" || profile.format_version != 1 {
            return Err("unsupported verified profile".into());
        }
        if evidence && !profile.allow_raw_evidence {
            return Err("operator profile does not permit raw policy evidence".into());
        }
        let base = path.parent().unwrap_or(Path::new("."));
        let read = |p: &Path, limit| read_bounded(&base.join(p), limit);
        let manifest_bytes = read(&profile.manifest, MAX_METADATA_BYTES)?;
        let requirements_bytes = read(&profile.requirements, MAX_METADATA_BYTES)?;
        // Validate the original typed document before modifying evaluation time.
        let typed_requirements =
            serde_json::from_slice::<devlish_audit::release::Requirements>(&requirements_bytes)
                .map_err(|e| format!("invalid requirements: {e}"))?;
        let mut requirements: Value =
            serde_json::from_slice(&requirements_bytes).map_err(|e| e.to_string())?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs();
        requirements["evaluated_at"] = json!(now);
        let effective_requirements =
            serde_json::to_vec(&requirements).map_err(|e| e.to_string())?;
        let mut paths = BTreeMap::new();
        for artifact in &profile.artifacts {
            if paths
                .insert(artifact.id.clone(), base.join(&artifact.path))
                .is_some()
            {
                return Err("duplicate operator artifact mapping".into());
            }
        }
        // The approved runtime must be this executable, not a caller-chosen file.
        paths.insert(
            profile.runtime_id.clone(),
            std::env::current_exe().map_err(|e| e.to_string())?,
        );
        let mut retained = BTreeMap::new();
        let release = verify_release(
            &manifest_bytes,
            &read(&profile.signature, MAX_METADATA_BYTES)?,
            &read(&profile.trust, MAX_METADATA_BYTES)?,
            &effective_requirements,
            |id| {
                let bytes = read_bounded(
                    paths
                        .get(id)
                        .ok_or_else(|| format!("missing artifact mapping: {id}"))?,
                    MAX_ARTIFACT_BYTES,
                )?;
                if [
                    &profile.program_id,
                    &profile.policy_id,
                    &profile.permissions_id,
                    &profile.catalog_id,
                    &profile.containment_id,
                ]
                .iter()
                .any(|selected| selected.as_str() == id)
                {
                    retained.insert(id.to_string(), bytes.clone());
                }
                Ok(bytes)
            },
        )?;
        let manifest: Manifest =
            serde_json::from_slice(&manifest_bytes).map_err(|e| e.to_string())?;
        for (id, role) in [
            (&profile.runtime_id, Role::Runtime),
            (&profile.program_id, Role::Program),
            (&profile.policy_id, Role::Policy),
            (&profile.permissions_id, Role::Permissions),
            (&profile.catalog_id, Role::ToolCatalog),
            (&profile.containment_id, Role::Containment),
        ] {
            if !manifest
                .artifacts
                .iter()
                .any(|a| &a.id == id && a.role == role)
            {
                return Err(format!(
                    "selected artifact has incorrect release role: {id}"
                ));
            }
        }
        let permissions_bytes = retained
            .remove(&profile.permissions_id)
            .ok_or("missing permissions snapshot")?;
        let catalog_bytes = retained
            .remove(&profile.catalog_id)
            .ok_or("missing catalog snapshot")?;
        let containment_bytes = retained
            .remove(&profile.containment_id)
            .ok_or("missing containment snapshot")?;
        let controls = Controls::parse(&permissions_bytes, &catalog_bytes, &containment_bytes)?;
        let policy_bytes = retained
            .remove(&profile.policy_id)
            .ok_or("missing verified policy snapshot")?;
        let mut policy = EffectPolicy::new(
            serde_json::from_slice(&policy_bytes)
                .map_err(|e| format!("invalid verified policy: {e}"))?,
        )?;
        if policy.identity()["rule"]["id"] != manifest.policy_id
            || policy.identity()["rule"]["version"] != manifest.policy_version
        {
            return Err("compiled policy identity differs from approved release".into());
        }
        policy.set_file_digest(sha256(&policy_bytes));
        let program = serde_json::from_slice(
            &retained
                .remove(&profile.program_id)
                .ok_or("missing verified program snapshot")?,
        )
        .map_err(|e| format!("invalid verified program: {e}"))?;
        let runtime = manifest
            .artifacts
            .iter()
            .find(|a| a.id == profile.runtime_id)
            .ok_or("missing runtime")?;
        let admission = release.admit(&base.join(&profile.admission_state))?;
        Ok(Self {
            _admission: admission,
            program_path: paths
                .remove(&profile.program_id)
                .ok_or("missing program mapping")?,
            evidence,
            admitted_at: now,
            expires_at: manifest
                .valid_until
                .min(typed_requirements.revocations_valid_until),
            llm_route: controls.llm_route,
            instruction_limit: controls.instruction_limit,
            allowed_effects: controls.allowed_effects.clone(),
            program,
            policy,
            log_context: json!({
                "release_manifest_sha256":sha256(&manifest_bytes), "requirements_sha256":release.requirements_sha256,
                "operator_requirements_sha256":sha256(&requirements_bytes), "operator_profile_sha256":sha256(&profile_bytes),
                "evaluated_at":now, "sequence":manifest.sequence,"session_id":session,
                "runtime_file_sha256":runtime.sha256,
            "permissions_sha256":sha256(&permissions_bytes),"catalog_sha256":sha256(&catalog_bytes),"containment_sha256":sha256(&containment_bytes),
            "instruction_limit":controls.instruction_limit,"allowed_effects":controls.allowed_effects,
                "assurance":"in-process-verified-loading", "execution_origin_verified":false
            }),
        })
    }

    /// Trusted adapters may use this path only for operator credential lookup.
    /// The program is never reopened for execution.
    pub fn program_path(&self) -> &Path {
        &self.program_path
    }

    /// An immutable copy of the already approved model route for the host adapter.
    pub fn model_route(&self) -> Option<devlish_llm::governed::ApprovedModel> {
        self.llm_route.clone()
    }

    /// Consume the session, create its bound log, and execute through the shared
    /// guard. The caller must provide protected host adapters and log storage.
    /// Responses go through the approved host sink; no private VM envelope returns.
    pub fn execute(
        self,
        input: Value,
        log_path: &Path,
        host: &mut dyn devlish_vm::HostEffects,
    ) -> Result<crate::governed_run::Completion, String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs();
        check_dispatch_time(self.admitted_at, self.expires_at, now)?;
        let mut log = crate::policy_log::PolicyLog::create_for_bound_run(
            log_path,
            self.policy.identity(),
            &self.program,
            &input,
            false,
            self.evidence,
            Some(&self.log_context),
        )?;
        let run = crate::governed_run::GovernedRun::new(
            self.program,
            input,
            self.policy,
            self.instruction_limit,
            self.allowed_effects,
        )
        .map_err(|e| e.to_string())?;
        let run = if self.evidence {
            run.with_replay_evidence()
        } else {
            run
        };
        // _admission remains owned through execution and final durable recording.
        run.run(host, &mut log).map_err(|e| e.to_string())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Permissions {
    format: String,
    format_version: u32,
    allowed_effects: Vec<String>,
    instruction_limit: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    format: String,
    format_version: u32,
    host_effects: Vec<String>,
    #[serde(default)]
    llm_route: Option<Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Containment {
    format: String,
    format_version: u32,
    mode: String,
}
struct Controls {
    llm_route: Option<devlish_llm::governed::ApprovedModel>,
    allowed_effects: std::collections::BTreeSet<String>,
    instruction_limit: u64,
}
impl Controls {
    fn parse(permissions: &[u8], catalog: &[u8], containment: &[u8]) -> Result<Self, String> {
        let p: Permissions =
            serde_json::from_slice(permissions).map_err(|e| format!("invalid permissions: {e}"))?;
        let c: Catalog =
            serde_json::from_slice(catalog).map_err(|e| format!("invalid tool catalog: {e}"))?;
        let isolation: Containment =
            serde_json::from_slice(containment).map_err(|e| format!("invalid containment: {e}"))?;
        if p.format != "devlish-runtime-permissions"
            || p.format_version != 1
            || p.instruction_limit == 0
            || p.instruction_limit > 10_000_000
            || c.format != "devlish-tool-catalog"
            || c.format_version != 1
            || isolation.format != "devlish-containment-profile"
            || isolation.format_version != 1
            || isolation.mode != "in-process"
        {
            return Err("unsupported release enforcement controls".into());
        }
        let supported = [
            "write_file",
            "read_file",
            "call_service",
            "http_request",
            "respond",
            "http_download",
            "read_xlsx_rows",
            "file_copy",
            "file_move",
            "file_mkdir",
            "file_delete",
            "file_exists",
            "file_stat",
            "file_list",
            "file_glob",
            "llm_complete",
            "clock_now",
            "random_draw",
        ];
        let allowed_effects: std::collections::BTreeSet<_> =
            p.allowed_effects.iter().cloned().collect();
        let catalog: std::collections::BTreeSet<_> = c.host_effects.iter().cloned().collect();
        if allowed_effects.len() != p.allowed_effects.len()
            || catalog.len() != c.host_effects.len()
            || !allowed_effects.is_subset(&catalog)
            || catalog
                .iter()
                .any(|effect| !supported.contains(&effect.as_str()))
        {
            return Err("unknown, duplicate or uncatalogued release effect".into());
        }
        let llm_route = c
            .llm_route
            .as_ref()
            .map(devlish_llm::governed::ApprovedModel::parse)
            .transpose()?;
        if allowed_effects.contains("llm_complete") && llm_route.is_none() {
            return Err("verified model effects require an approved catalog route".into());
        }
        Ok(Self {
            llm_route,
            allowed_effects,
            instruction_limit: p.instruction_limit,
        })
    }
}

fn check_dispatch_time(admitted: u64, expires: u64, now: u64) -> Result<(), String> {
    if now < admitted || now >= expires {
        Err("admitted session is no longer within its verified time window".into())
    } else {
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn dispatch_rejects_clock_rollback_and_exclusive_expiry() {
        for now in [0, 99, 200, 201, u64::MAX] {
            assert!(super::check_dispatch_time(100, 200, now).is_err());
        }
        for now in [100, 150, 199] {
            assert!(super::check_dispatch_time(100, 200, now).is_ok());
        }
    }
}
